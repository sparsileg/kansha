//! Transaction and posting repository (TXN-010 … TXN-070, INT-020).
//!
//! Rules that need other rows (open accounts, reconciled edits) are
//! checked by the `ledger` service before these run. Each write here is
//! one audited change.

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, Row, named_params, params};

use super::Tx;
use super::accounts::in_use_or;
use super::audit::{self, AuditAction, AuditEntity};
use crate::accounts::AccountId;
use crate::categories::TagId;
use crate::date::Date;
use crate::error::{Error, Result};
use crate::ledger::{
    AccountBalance, Cleared, Counterpart, Posting, PostingId, PostingInput, RegisterQuery,
    RegisterRow, RegisterSort, SearchHit, Target, Txn, TxnId, TxnInput, TxnSource,
};
use crate::money::Money;

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

fn source_from_row(r: &Row<'_>) -> rusqlite::Result<TxnSource> {
    let origin: String = r.get("origin")?;
    let batch: Option<i64> = r.get("import_batch_id")?;
    let schedule: Option<i64> = r.get("schedule_id")?;
    Ok(match (origin.as_str(), batch, schedule) {
        ("import", Some(batch), _) => TxnSource::Import { batch },
        ("schedule", _, Some(schedule)) => TxnSource::Schedule { schedule },
        ("reconcile", _, _) => TxnSource::Reconcile,
        ("system", _, _) => TxnSource::System,
        // 'manual', or a combination the schema's CHECKs rule out.
        _ => TxnSource::Manual,
    })
}

/// One transaction by ID, or `None`.
pub fn find(conn: &Connection, id: TxnId) -> Result<Option<Txn>> {
    let header = conn
        .prepare_cached(
            "SELECT id, txn_date, payee_id, check_num, memo, notes, status, origin,
                    import_batch_id, schedule_id, created_at
             FROM txn WHERE id = ?1",
        )?
        .query_row([id], |r| {
            Ok(Txn {
                id: r.get("id")?,
                date: r.get("txn_date")?,
                payee: r.get("payee_id")?,
                check_num: r.get("check_num")?,
                memo: r.get("memo")?,
                notes: r.get("notes")?,
                status: r.get("status")?,
                source: source_from_row(r)?,
                created_at: r.get("created_at")?,
                postings: Vec::new(),
            })
        })
        .optional()?;
    let Some(mut txn) = header else {
        return Ok(None);
    };

    let mut tags: HashMap<i64, Vec<TagId>> = HashMap::new();
    let mut stmt = conn.prepare_cached(
        "SELECT pt.posting_id, pt.tag_id FROM posting_tag pt
         JOIN posting p ON p.id = pt.posting_id
         WHERE p.txn_id = ?1 ORDER BY pt.tag_id",
    )?;
    let rows = stmt.query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, TagId>(1)?)))?;
    for row in rows {
        let (posting, tag) = row?;
        tags.entry(posting).or_default().push(tag);
    }

    let mut stmt = conn.prepare_cached(
        "SELECT id, line_no, account_id, category_id, amount, memo, cleared, reconciliation_id
         FROM posting WHERE txn_id = ?1 ORDER BY line_no",
    )?;
    let rows = stmt.query_map([id], |r| {
        let account: Option<AccountId> = r.get("account_id")?;
        let target = match account {
            Some(a) => Target::Account(a),
            None => Target::Category(r.get("category_id")?),
        };
        let pid: i64 = r.get("id")?;
        Ok(Posting {
            id: PostingId(pid),
            line_no: r.get("line_no")?,
            target,
            amount: r.get("amount")?,
            memo: r.get("memo")?,
            cleared: r.get("cleared")?,
            reconciliation_id: r.get("reconciliation_id")?,
            tags: Vec::new(),
        })
    })?;
    for row in rows {
        let mut p = row?;
        p.tags = tags.remove(&p.id.0).unwrap_or_default();
        txn.postings.push(p);
    }
    Ok(Some(txn))
}

/// One transaction by ID.
pub fn get(conn: &Connection, id: TxnId) -> Result<Txn> {
    find(conn, id)?.ok_or(Error::NotFound {
        entity: "txn",
        id: id.0,
    })
}

/// Does the transaction carry investment detail (`investment_txn`)? Those
/// are changed only through the investments engine (Phase 6).
pub fn is_investment_txn(conn: &Connection, id: TxnId) -> Result<bool> {
    Ok(conn
        .prepare_cached("SELECT 1 FROM investment_txn WHERE txn_id = ?1")?
        .exists([id])?)
}

/// Σ postings to `account` dated on or before `as_of` (all when `None`);
/// only cleared and reconciled postings when `cleared_only`.
pub fn account_balance(
    conn: &Connection,
    account: AccountId,
    as_of: Option<Date>,
    cleared_only: bool,
) -> Result<Money> {
    Ok(conn
        .prepare_cached(
            "SELECT ifnull(sum(p.amount), 0) FROM posting p JOIN txn t ON t.id = p.txn_id
             WHERE p.account_id = :account
               AND (:as_of IS NULL OR t.txn_date <= :as_of)
               AND (:cleared_only = 0 OR p.cleared <> 'unmarked')",
        )?
        .query_row(
            named_params! {":account": account, ":as_of": as_of, ":cleared_only": cleared_only},
            |r| r.get(0),
        )?)
}

/// Σ postings to `category` dated on or before `as_of`.
pub fn category_total(
    conn: &Connection,
    category: crate::categories::CategoryId,
    as_of: Option<Date>,
) -> Result<Money> {
    Ok(conn
        .prepare_cached(
            "SELECT ifnull(sum(p.amount), 0) FROM posting p JOIN txn t ON t.id = p.txn_id
             WHERE p.category_id = :category AND (:as_of IS NULL OR t.txn_date <= :as_of)",
        )?
        .query_row(
            named_params! {":category": category, ":as_of": as_of},
            |r| r.get(0),
        )?)
}

/// Latest transaction date with a posting to `account`.
pub fn last_posting_date(conn: &Connection, account: AccountId) -> Result<Option<Date>> {
    Ok(conn
        .prepare_cached(
            "SELECT max(t.txn_date) FROM posting p JOIN txn t ON t.id = p.txn_id
             WHERE p.account_id = ?1",
        )?
        .query_row([account], |r| r.get(0))?)
}

/// Rows and matching count for a register query (REG-020, REG-040).
///
/// `base` numbers every posting to the account with a running balance in
/// date order; `shaped` adds the display columns; the filters, sort, and
/// paging apply last, so the balance never depends on them.
pub fn register_query(
    conn: &Connection,
    q: &RegisterQuery,
    today: Date,
) -> Result<(Vec<RegisterRow>, i64)> {
    let ctes = "WITH RECURSIVE
        cat_path (id, path) AS (
            SELECT id, name FROM category WHERE parent_id IS NULL
            UNION ALL
            SELECT c.id, cp.path || ':' || c.name FROM category c JOIN cat_path cp ON c.parent_id = cp.id
        ),
        sub (id) AS (
            SELECT id FROM category WHERE id = :category
            UNION ALL
            SELECT c.id FROM category c JOIN sub ON c.parent_id = sub.id
        ),
        base AS (
            SELECT t.id AS txn_id, t.txn_date, t.check_num, t.payee_id, t.memo, t.notes, t.status,
                   p.id AS posting_id, p.amount, p.cleared,
                   sum(p.amount) OVER (ORDER BY t.txn_date, t.id, p.id) AS balance
            FROM posting p JOIN txn t ON t.id = p.txn_id
            WHERE p.account_id = :account
        ),
        oc AS (
            SELECT txn_id, count(*) AS n FROM posting
            WHERE txn_id IN (SELECT txn_id FROM base) GROUP BY txn_id
        ),
        shaped AS (
            SELECT b.*, ifnull(py.name, '') AS payee_name, oc.n - 1 AS others,
                   o.account_id AS other_account, o.category_id AS other_category,
                   CASE oc.n - 1
                       WHEN 0 THEN ''
                       WHEN 1 THEN CASE WHEN o.account_id IS NOT NULL THEN '[' || oa.name || ']'
                                        ELSE cp.path END
                       ELSE '--Split--'
                   END AS category
            FROM base b
            LEFT JOIN payee py ON py.id = b.payee_id
            JOIN oc ON oc.txn_id = b.txn_id
            LEFT JOIN posting o ON o.id = (
                SELECT x.id FROM posting x
                WHERE x.txn_id = b.txn_id AND x.id <> b.posting_id
                ORDER BY x.line_no LIMIT 1)
            LEFT JOIN account oa ON oa.id = o.account_id
            LEFT JOIN cat_path cp ON cp.id = o.category_id
        )";
    let filter = "WHERE (:from IS NULL OR s.txn_date >= :from)
          AND (:to IS NULL OR s.txn_date <= :to)
          AND (:payee IS NULL OR s.payee_id = :payee)
          AND (:cleared IS NULL OR s.cleared = :cleared)
          AND (:category IS NULL OR EXISTS (
                SELECT 1 FROM posting o
                WHERE o.txn_id = s.txn_id AND o.category_id IN (SELECT id FROM sub)))
          AND (:tag IS NULL OR EXISTS (
                SELECT 1 FROM posting o JOIN posting_tag pt ON pt.posting_id = o.id
                WHERE o.txn_id = s.txn_id AND pt.tag_id = :tag))
          AND (:text IS NULL
               OR instr(lower(s.payee_name || char(31) || s.memo || char(31) || s.notes
                              || char(31) || s.check_num || char(31) || s.category),
                        lower(:text)) > 0
               OR EXISTS (SELECT 1 FROM posting o
                          LEFT JOIN cat_path cp ON cp.id = o.category_id
                          WHERE o.txn_id = s.txn_id
                            AND (instr(lower(o.memo), lower(:text)) > 0
                                 OR instr(lower(ifnull(cp.path, '')), lower(:text)) > 0)))";
    let dir = if q.descending { "DESC" } else { "ASC" };
    let order = match q.sort {
        RegisterSort::Date => format!("s.txn_date {dir}, s.txn_id {dir}, s.posting_id {dir}"),
        RegisterSort::CheckNum => {
            format!(
                "length(s.check_num) {dir}, s.check_num {dir}, s.txn_date, s.txn_id, s.posting_id"
            )
        }
        RegisterSort::Payee => {
            format!("s.payee_name COLLATE NOCASE {dir}, s.txn_date, s.txn_id, s.posting_id")
        }
        RegisterSort::Amount => format!("s.amount {dir}, s.txn_date, s.txn_id, s.posting_id"),
        RegisterSort::Category => {
            format!("s.category COLLATE NOCASE {dir}, s.txn_date, s.txn_id, s.posting_id")
        }
        RegisterSort::Memo => {
            format!("s.memo COLLATE NOCASE {dir}, s.txn_date, s.txn_id, s.posting_id")
        }
        RegisterSort::Cleared => format!("s.cleared {dir}, s.txn_date, s.txn_id, s.posting_id"),
        RegisterSort::Balance => format!("s.balance {dir}, s.txn_date, s.txn_id, s.posting_id"),
    };
    let text = q.text.as_deref().map(str::trim).filter(|t| !t.is_empty());
    let limit = q.limit.unwrap_or(-1);
    let offset = q.offset.max(0);

    let count_sql = format!("{ctes} SELECT count(*) FROM shaped s {filter}");
    let total: i64 = conn.prepare_cached(&count_sql)?.query_row(
        named_params! {
            ":account": q.account, ":from": q.date_from, ":to": q.date_to, ":payee": q.payee,
            ":cleared": q.cleared, ":category": q.category, ":tag": q.tag, ":text": text,
        },
        |r| r.get(0),
    )?;

    let sql = format!(
        "{ctes} SELECT s.txn_id, s.txn_date, s.check_num, s.payee_id, s.payee_name, s.memo,
                s.status, s.amount, s.cleared, s.others, s.other_account, s.other_category,
                s.category, s.balance,
                ifnull((SELECT group_concat(name, ', ') FROM (
                            SELECT DISTINCT tg.name AS name
                            FROM posting o
                            JOIN posting_tag pt ON pt.posting_id = o.id
                            JOIN tag tg ON tg.id = pt.tag_id
                            WHERE o.txn_id = s.txn_id ORDER BY tg.name)), '') AS tags
         FROM shaped s {filter} ORDER BY {order} LIMIT :limit OFFSET :offset"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt.query_map(
        named_params! {
            ":account": q.account, ":from": q.date_from, ":to": q.date_to, ":payee": q.payee,
            ":cleared": q.cleared, ":category": q.category, ":tag": q.tag, ":text": text,
            ":limit": limit, ":offset": offset,
        },
        |r| {
            let others: i64 = r.get("others")?;
            let counterpart = match others {
                0 => Counterpart::None,
                1 => match r.get::<_, Option<AccountId>>("other_account")? {
                    Some(a) => Counterpart::Transfer(a),
                    None => Counterpart::Category(r.get("other_category")?),
                },
                _ => Counterpart::Split,
            };
            let date: Date = r.get("txn_date")?;
            Ok(RegisterRow {
                txn_id: r.get("txn_id")?,
                date,
                check_num: r.get("check_num")?,
                payee: r.get("payee_id")?,
                payee_name: r.get("payee_name")?,
                memo: r.get("memo")?,
                status: r.get("status")?,
                amount: r.get("amount")?,
                cleared: r.get("cleared")?,
                counterpart,
                category: r.get("category")?,
                tags: r.get("tags")?,
                balance: r.get("balance")?,
                future: date > today,
            })
        },
    )?;
    Ok((rows.collect::<rusqlite::Result<Vec<_>>>()?, total))
}

/// The typed text as a number of cents ("184.23", "$1,000", "-5"), if it
/// is one; matches a posting of that size in either direction.
fn amount_cents(text: &str) -> Option<i64> {
    let t: String = text
        .chars()
        .filter(|c| !matches!(c, ',' | '$') && !c.is_whitespace())
        .collect();
    let t = t.strip_prefix('-').unwrap_or(&t);
    if t.is_empty() || !t.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    t.parse::<Money>().ok().map(|m| m.cents())
}

/// Rows whose payee, memo, notes, check number, line memo, category, or
/// amount matches `text`, one per (transaction, account) posting, newest
/// first, and the count of all matches (search box).
pub fn search(
    conn: &Connection,
    text: &str,
    account: Option<AccountId>,
    limit: i64,
) -> Result<(Vec<SearchHit>, i64)> {
    let ctes = "WITH RECURSIVE
        cat_path (id, path) AS (
            SELECT id, name FROM category WHERE parent_id IS NULL
            UNION ALL
            SELECT c.id, cp.path || ':' || c.name FROM category c JOIN cat_path cp ON c.parent_id = cp.id
        ),
        base AS (
            SELECT t.id AS txn_id, t.txn_date, t.check_num, t.payee_id, t.memo, t.notes, t.status,
                   p.id AS posting_id, p.account_id, p.amount
            FROM posting p JOIN txn t ON t.id = p.txn_id
            WHERE p.account_id IS NOT NULL AND (:account IS NULL OR p.account_id = :account)
        ),
        oc AS (
            SELECT txn_id, count(*) AS n FROM posting
            WHERE txn_id IN (SELECT txn_id FROM base) GROUP BY txn_id
        ),
        shaped AS (
            SELECT b.*, ifnull(py.name, '') AS payee_name,
                   CASE oc.n - 1
                       WHEN 0 THEN ''
                       WHEN 1 THEN CASE WHEN o.account_id IS NOT NULL THEN '[' || oa.name || ']'
                                        ELSE cp.path END
                       ELSE '--Split--'
                   END AS category
            FROM base b
            LEFT JOIN payee py ON py.id = b.payee_id
            JOIN oc ON oc.txn_id = b.txn_id
            LEFT JOIN posting o ON o.id = (
                SELECT x.id FROM posting x
                WHERE x.txn_id = b.txn_id AND x.id <> b.posting_id
                ORDER BY x.line_no LIMIT 1)
            LEFT JOIN account oa ON oa.id = o.account_id
            LEFT JOIN cat_path cp ON cp.id = o.category_id
        )";
    let filter = "WHERE instr(lower(s.payee_name || char(31) || s.memo || char(31) || s.notes
                              || char(31) || s.check_num || char(31) || s.category),
                        lower(:text)) > 0
               OR EXISTS (SELECT 1 FROM posting o
                          LEFT JOIN cat_path cp ON cp.id = o.category_id
                          WHERE o.txn_id = s.txn_id
                            AND (instr(lower(o.memo), lower(:text)) > 0
                                 OR instr(lower(ifnull(cp.path, '')), lower(:text)) > 0))
               OR (:cents IS NOT NULL AND EXISTS (
                          SELECT 1 FROM posting o
                          WHERE o.txn_id = s.txn_id AND abs(o.amount) = :cents))";
    let cents = amount_cents(text);
    let total: i64 = conn
        .prepare_cached(&format!("{ctes} SELECT count(*) FROM shaped s {filter}"))?
        .query_row(
            named_params! {":account": account, ":text": text, ":cents": cents},
            |r| r.get(0),
        )?;
    let sql = format!(
        "{ctes} SELECT s.txn_id, s.account_id, s.txn_date, s.payee_name, s.memo, s.status,
                s.amount, s.category
         FROM shaped s {filter}
         ORDER BY s.txn_date DESC, s.txn_id DESC, s.posting_id DESC LIMIT :limit"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt.query_map(
        named_params! {":account": account, ":text": text, ":cents": cents, ":limit": limit},
        |r| {
            Ok(SearchHit {
                txn_id: r.get("txn_id")?,
                account: r.get("account_id")?,
                date: r.get("txn_date")?,
                payee_name: r.get("payee_name")?,
                memo: r.get("memo")?,
                status: r.get("status")?,
                amount: r.get("amount")?,
                category: r.get("category")?,
            })
        },
    )?;
    Ok((rows.collect::<rusqlite::Result<Vec<_>>>()?, total))
}

/// Current and ending balance of every account.
pub fn account_balances(conn: &Connection, today: Date) -> Result<Vec<AccountBalance>> {
    let mut stmt = conn.prepare_cached(
        "SELECT a.id,
                ifnull(sum(CASE WHEN t.txn_date <= ?1 THEN p.amount END), 0),
                ifnull(sum(p.amount), 0)
         FROM account a
         LEFT JOIN posting p ON p.account_id = a.id
         LEFT JOIN txn t ON t.id = p.txn_id
         GROUP BY a.id ORDER BY a.id",
    )?;
    let rows = stmt.query_map([today], |r| {
        Ok(AccountBalance {
            account: r.get(0)?,
            current: r.get(1)?,
            ending: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ---------------------------------------------------------------------------
// Writes
// ---------------------------------------------------------------------------

/// Create a transaction (TXN-070: immutable ID, creation timestamp, origin).
pub fn insert(tx: &Tx<'_>, source: TxnSource, input: &TxnInput) -> Result<Txn> {
    let (origin, batch, schedule) = match source {
        TxnSource::Manual => ("manual", None, None),
        TxnSource::Import { batch } => ("import", Some(batch), None),
        TxnSource::Schedule { schedule } => ("schedule", None, Some(schedule)),
        TxnSource::Reconcile => ("reconcile", None, None),
        TxnSource::System => ("system", None, None),
    };
    tx.conn().execute(
        "INSERT INTO txn (txn_date, payee_id, check_num, memo, notes, origin,
             import_batch_id, schedule_id, created_at)
         VALUES (:date, :payee, :check_num, :memo, :notes, :origin, :batch, :schedule, :created_at)",
        named_params! {
            ":date": input.date,
            ":payee": input.payee,
            ":check_num": input.check_num.trim(),
            ":memo": input.memo,
            ":notes": input.notes,
            ":origin": origin,
            ":batch": batch,
            ":schedule": schedule,
            ":created_at": tx.now(),
        },
    )?;
    let id = TxnId(tx.conn().last_insert_rowid());
    insert_postings(tx, id, &input.postings, &HashMap::new())?;
    let txn = get(tx.conn(), id)?;
    audit::record::<(), _>(
        tx,
        AuditEntity::Txn,
        id.0,
        AuditAction::Create,
        None,
        Some(&txn),
    )?;
    Ok(txn)
}

/// Replace a transaction's header and postings. An account posting that
/// stays reconciled keeps its reconciliation link.
pub fn update(tx: &Tx<'_>, id: TxnId, input: &TxnInput) -> Result<Txn> {
    let before = get(tx.conn(), id)?;
    let links: HashMap<AccountId, Option<i64>> = before
        .postings
        .iter()
        .filter(|p| p.cleared == Cleared::Reconciled)
        .filter_map(|p| match p.target {
            Target::Account(a) => Some((a, p.reconciliation_id)),
            Target::Category(_) => None,
        })
        .collect();
    tx.conn().execute(
        "UPDATE txn SET txn_date = :date, payee_id = :payee, check_num = :check_num,
             memo = :memo, notes = :notes
         WHERE id = :id",
        named_params! {
            ":id": id,
            ":date": input.date,
            ":payee": input.payee,
            ":check_num": input.check_num.trim(),
            ":memo": input.memo,
            ":notes": input.notes,
        },
    )?;
    tx.conn()
        .execute("DELETE FROM posting WHERE txn_id = ?1", [id])?;
    insert_postings(tx, id, &input.postings, &links)?;
    let after = get(tx.conn(), id)?;
    if after != before {
        audit::record(
            tx,
            AuditEntity::Txn,
            id.0,
            AuditAction::Update,
            Some(&before),
            Some(&after),
        )?;
    }
    Ok(after)
}

fn insert_postings(
    tx: &Tx<'_>,
    id: TxnId,
    postings: &[PostingInput],
    links: &HashMap<AccountId, Option<i64>>,
) -> Result<()> {
    let mut insert = tx.conn().prepare_cached(
        "INSERT INTO posting (txn_id, line_no, account_id, category_id, amount, memo, cleared,
             reconciliation_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    let mut insert_tag = tx
        .conn()
        .prepare_cached("INSERT INTO posting_tag (posting_id, tag_id) VALUES (?1, ?2)")?;
    for (i, p) in postings.iter().enumerate() {
        let (account, category) = match p.target {
            Target::Account(a) => (Some(a), None),
            Target::Category(c) => (None, Some(c)),
        };
        let link = match (account, p.cleared) {
            (Some(a), Cleared::Reconciled) => links.get(&a).copied().flatten(),
            _ => None,
        };
        let line_no = i64::try_from(i + 1).map_err(|_| Error::Overflow("posting line_no"))?;
        insert.execute(params![
            id, line_no, account, category, p.amount, p.memo, p.cleared, link
        ])?;
        let posting_id = tx.conn().last_insert_rowid();
        let mut tags = p.tags.clone();
        tags.sort();
        tags.dedup();
        for tag in tags {
            insert_tag.execute(params![posting_id, tag])?;
        }
    }
    Ok(())
}

/// Void: every posting's amount becomes zero and the transaction is
/// marked void; the original amounts are in the audit entry (TXN-040).
pub fn void(tx: &Tx<'_>, id: TxnId) -> Result<Txn> {
    let before = get(tx.conn(), id)?;
    tx.conn()
        .execute("UPDATE posting SET amount = 0 WHERE txn_id = ?1", [id])?;
    tx.conn()
        .execute("UPDATE txn SET status = 'void' WHERE id = ?1", [id])?;
    let after = get(tx.conn(), id)?;
    audit::record(
        tx,
        AuditEntity::Txn,
        id.0,
        AuditAction::Void,
        Some(&before),
        Some(&after),
    )?;
    Ok(after)
}

/// Delete a transaction and its postings. One entered from a schedule
/// occurrence is `InUse`.
pub fn delete(tx: &Tx<'_>, id: TxnId) -> Result<()> {
    let before = get(tx.conn(), id)?;
    tx.conn()
        .execute("DELETE FROM txn WHERE id = ?1", [id])
        .map_err(|e| in_use_or(e.into(), "txn", id.0))?;
    audit::record::<_, ()>(
        tx,
        AuditEntity::Txn,
        id.0,
        AuditAction::Delete,
        Some(&before),
        None,
    )?;
    Ok(())
}

/// Set the cleared status of the transaction's posting to `account`.
/// Leaving `Reconciled` drops the reconciliation link.
pub fn set_cleared(tx: &Tx<'_>, id: TxnId, account: AccountId, cleared: Cleared) -> Result<Txn> {
    let before = get(tx.conn(), id)?;
    let changed = tx.conn().execute(
        "UPDATE posting SET cleared = ?3,
             reconciliation_id = CASE WHEN ?3 = 'reconciled' THEN reconciliation_id END
         WHERE txn_id = ?1 AND account_id = ?2",
        params![id, account, cleared],
    )?;
    if changed == 0 {
        return Err(Error::Invalid(format!(
            "transaction {} has no posting to account {}",
            id.0, account.0
        )));
    }
    let after = get(tx.conn(), id)?;
    if after != before {
        audit::record(
            tx,
            AuditEntity::Txn,
            id.0,
            AuditAction::Update,
            Some(&before),
            Some(&after),
        )?;
    }
    Ok(after)
}
