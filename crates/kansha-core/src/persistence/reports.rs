//! Report reads (RPT-100 … RPT-145), tax lines (CAT-050), and saved
//! report definitions (RPT-020).

use rusqlite::{Connection, OptionalExtension, Row, named_params, params};

use super::Tx;
use super::audit::{self, AuditAction, AuditEntity};
use crate::accounts::AccountId;
use crate::categories::{CategoryId, PayeeId, TagId, TaxLine};
use crate::date::Date;
use crate::error::{Error, Result};
use crate::invest::{ConversionTax, InvAction};
use crate::ledger::{Cleared, TxnId};
use crate::money::{Money, Quantity};
use crate::reports::{
    FilterList, ReportFolder, ReportFolderId, ReportKind, ReportSettings, SavedReport,
    SavedReportId,
};
use crate::securities::SecurityId;

// ---------------------------------------------------------------------------
// Tax lines
// ---------------------------------------------------------------------------

/// Every tax line, in form and line order.
pub fn tax_lines(conn: &Connection) -> Result<Vec<TaxLine>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, form, line, sort_order FROM tax_line ORDER BY sort_order, id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(TaxLine {
            id: r.get(0)?,
            form: r.get(1)?,
            line: r.get(2)?,
            sort_order: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ---------------------------------------------------------------------------
// Posting facts
// ---------------------------------------------------------------------------

/// One posting with its transaction's header and investment detail: the
/// raw material of the category, payee, and tax reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostingRow {
    pub txn: TxnId,
    pub date: Date,
    pub payee: Option<PayeeId>,
    pub payee_name: String,
    pub check_num: String,
    pub txn_memo: String,
    pub line_no: i64,
    pub account: Option<AccountId>,
    pub category: Option<CategoryId>,
    pub security: Option<SecurityId>,
    pub amount: Money,
    pub memo: String,
    pub cleared: Cleared,
    pub tags: Vec<TagId>,
    /// Investment transactions: the investment account, action, shares,
    /// and security.
    pub inv_account: Option<AccountId>,
    pub inv_action: Option<InvAction>,
    pub inv_quantity: Option<Quantity>,
    pub inv_security: Option<SecurityId>,
    /// Share transfers and Roth conversions: the receiving account.
    pub inv_to_account: Option<AccountId>,
    /// Roth conversions (INV-070).
    pub inv_conversion: Option<ConversionTax>,
}

fn tags_from(text: Option<String>) -> rusqlite::Result<Vec<TagId>> {
    let mut out = Vec::new();
    for part in text.as_deref().unwrap_or("").split(',') {
        if part.is_empty() {
            continue;
        }
        let id = part.parse::<i64>().map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?;
        out.push(TagId(id));
    }
    out.sort();
    Ok(out)
}

/// Postings of normal (not void) transactions dated `from` (open when
/// `None`) through `to`, in date, transaction, and line order.
pub fn postings(conn: &Connection, from: Option<Date>, to: Date) -> Result<Vec<PostingRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT t.id, t.txn_date, t.payee_id, ifnull(py.name, ''), t.check_num, t.memo,
                p.line_no, p.account_id, p.category_id, p.security_id, p.amount, p.memo,
                p.cleared,
                (SELECT group_concat(pt.tag_id) FROM posting_tag pt WHERE pt.posting_id = p.id),
                i.account_id, i.action, i.quantity, i.security_id, i.to_account_id,
                i.nontaxable, i.withheld_federal, i.withheld_state
         FROM txn t
         JOIN posting p ON p.txn_id = t.id
         LEFT JOIN payee py ON py.id = t.payee_id
         LEFT JOIN investment_txn i ON i.txn_id = t.id
         WHERE t.status = 'normal'
           AND (:from IS NULL OR t.txn_date >= :from)
           AND t.txn_date <= :to
         ORDER BY t.txn_date, t.id, p.line_no",
    )?;
    let rows = stmt.query_map(named_params! {":from": from, ":to": to}, |r| {
        Ok(PostingRow {
            txn: r.get(0)?,
            date: r.get(1)?,
            payee: r.get(2)?,
            payee_name: r.get(3)?,
            check_num: r.get(4)?,
            txn_memo: r.get(5)?,
            line_no: r.get(6)?,
            account: r.get(7)?,
            category: r.get(8)?,
            security: r.get(9)?,
            amount: r.get(10)?,
            memo: r.get(11)?,
            cleared: r.get(12)?,
            tags: tags_from(r.get(13)?)?,
            inv_account: r.get(14)?,
            inv_action: r.get(15)?,
            inv_quantity: r.get(16)?,
            inv_security: r.get(17)?,
            inv_to_account: r.get(18)?,
            inv_conversion: match (r.get(19)?, r.get(20)?, r.get(21)?) {
                (Some(nontaxable), Some(withheld_federal), Some(withheld_state)) => {
                    Some(ConversionTax {
                        nontaxable,
                        withheld_federal,
                        withheld_state,
                    })
                }
                _ => None,
            },
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Date of the earliest transaction, for "all dates".
pub fn first_txn_date(conn: &Connection) -> Result<Option<Date>> {
    Ok(conn
        .prepare_cached("SELECT min(txn_date) FROM txn")?
        .query_row([], |r| r.get(0))?)
}

/// Accounts with a finished reconciliation: the latest statement date of
/// each.
pub fn last_statement_dates(conn: &Connection) -> Result<Vec<(AccountId, Date)>> {
    let mut stmt = conn.prepare_cached(
        "SELECT account_id, max(statement_date) FROM reconciliation
         WHERE status = 'finished' GROUP BY account_id",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Accounts with an uncleared posting dated on or before `before`.
pub fn accounts_with_old_uncleared(conn: &Connection, before: Date) -> Result<Vec<AccountId>> {
    let mut stmt = conn.prepare_cached(
        "SELECT DISTINCT p.account_id FROM posting p JOIN txn t ON t.id = p.txn_id
         WHERE p.account_id IS NOT NULL AND p.security_id IS NULL
           AND p.cleared = 'unmarked' AND t.status = 'normal' AND p.amount <> 0
           AND t.txn_date <= ?1
         ORDER BY p.account_id",
    )?;
    let rows = stmt.query_map([before], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Accounts with a transaction dated after the last date they were
/// reconciled to (any, if never), each with that date: the latest
/// finished statement, or the latest reconciled transaction (imported
/// ones have no reconciliation row, MIG-090). `None` when never.
pub fn last_reconciled(conn: &Connection) -> Result<Vec<(AccountId, Option<Date>)>> {
    // Both dates per account, then compared: each account is read once.
    // A subquery's date compared inside EXISTS was worked out again for
    // every transaction (NFR-040).
    let mut stmt = conn.prepare_cached(
        "WITH l AS MATERIALIZED (
           SELECT a.id,
             nullif(max(
               coalesce((SELECT max(r.statement_date) FROM reconciliation r
                         WHERE r.account_id = a.id AND r.status = 'finished'), ''),
               coalesce((SELECT max(t.txn_date) FROM posting p JOIN txn t ON t.id = p.txn_id
                         WHERE p.account_id = a.id AND p.cleared = 'reconciled'), '')), '') AS last,
             (SELECT max(t.txn_date) FROM posting p JOIN txn t ON t.id = p.txn_id
              WHERE p.account_id = a.id AND t.status = 'normal') AS latest
           FROM account a)
         SELECT id, last FROM l
         WHERE latest > coalesce(last, '')
         ORDER BY id",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Transactions with a line in the top-level `Uncategorized` category
/// (where the import puts lines with none, MIG-020), counted by each
/// account they touch.
pub fn uncategorized_by_account(conn: &Connection) -> Result<Vec<(AccountId, i64)>> {
    let mut stmt = conn.prepare_cached(
        "SELECT pa.account_id, count(DISTINCT t.id)
         FROM category c
         JOIN posting pc ON pc.category_id = c.id
         JOIN txn t ON t.id = pc.txn_id
         JOIN posting pa ON pa.txn_id = t.id AND pa.account_id IS NOT NULL
         WHERE c.name = 'Uncategorized' AND c.parent_id IS NULL
           AND t.status = 'normal'
         GROUP BY pa.account_id ORDER BY pa.account_id",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ---------------------------------------------------------------------------
// Saved reports (RPT-020)
// ---------------------------------------------------------------------------

type SavedRow = (SavedReportId, String, ReportKind, String, ReportFolderId);

fn saved_from_row(r: &Row<'_>) -> rusqlite::Result<SavedRow> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
}

fn decode((id, name, kind, json, folder): SavedRow) -> Result<SavedReport> {
    let mut settings: ReportSettings = serde_json::from_str(&json)
        .map_err(|e| Error::Database(format!("saved report {}: {e}", id.0)))?;
    settings.kind = kind;
    Ok(SavedReport {
        id,
        name,
        settings,
        folder,
    })
}

const SAVED_COLUMNS: &str = "id, name, report_type, settings_json, folder_id";

/// One saved report by ID.
pub fn saved_get(conn: &Connection, id: SavedReportId) -> Result<SavedReport> {
    let sql = format!("SELECT {SAVED_COLUMNS} FROM saved_report WHERE id = ?1");
    let raw = conn
        .prepare_cached(&sql)?
        .query_row([id], saved_from_row)
        .optional()?
        .ok_or(Error::NotFound {
            entity: "saved report",
            id: id.0,
        })?;
    decode(raw)
}

/// Every saved report, by name.
pub fn saved_list(conn: &Connection) -> Result<Vec<SavedReport>> {
    let sql = format!("SELECT {SAVED_COLUMNS} FROM saved_report ORDER BY name COLLATE NOCASE, id");
    let mut stmt = conn.prepare_cached(&sql)?;
    let raw = stmt
        .query_map([], saved_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    raw.into_iter().map(decode).collect()
}

fn clean_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Invalid("a saved report needs a name".into()));
    }
    Ok(name)
}

fn name_taken(conn: &Connection, name: &str, except: Option<SavedReportId>) -> Result<()> {
    let clash = conn
        .prepare_cached(
            "SELECT 1 FROM saved_report WHERE name = ?1 COLLATE NOCASE AND id IS NOT ?2",
        )?
        .exists(params![name, except])?;
    if clash {
        return Err(Error::Invalid(format!(
            "a saved report named {name:?} already exists"
        )));
    }
    Ok(())
}

/// Save a new named report, in the Unfiled folder.
pub fn saved_insert(tx: &Tx<'_>, name: &str, settings: &ReportSettings) -> Result<SavedReport> {
    let name = clean_name(name)?;
    name_taken(tx.conn(), name, None)?;
    tx.conn().execute(
        "INSERT INTO saved_report (name, report_type, settings_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
        params![
            name,
            settings.kind,
            serde_json::to_string(settings)?,
            tx.now()
        ],
    )?;
    let saved = saved_get(tx.conn(), SavedReportId(tx.conn().last_insert_rowid()))?;
    audit::record::<(), _>(
        tx,
        AuditEntity::SavedReport,
        saved.id.0,
        AuditAction::Create,
        None,
        Some(&saved),
    )?;
    Ok(saved)
}

/// Rename a saved report or replace its settings.
pub fn saved_update(
    tx: &Tx<'_>,
    id: SavedReportId,
    name: &str,
    settings: &ReportSettings,
) -> Result<SavedReport> {
    let before = saved_get(tx.conn(), id)?;
    let name = clean_name(name)?;
    name_taken(tx.conn(), name, Some(id))?;
    tx.conn().execute(
        "UPDATE saved_report SET name = ?2, report_type = ?3, settings_json = ?4, updated_at = ?5
         WHERE id = ?1",
        params![
            id,
            name,
            settings.kind,
            serde_json::to_string(settings)?,
            tx.now()
        ],
    )?;
    let after = saved_get(tx.conn(), id)?;
    if after != before {
        audit::record(
            tx,
            AuditEntity::SavedReport,
            id.0,
            AuditAction::Update,
            Some(&before),
            Some(&after),
        )?;
    }
    Ok(after)
}

/// Keep saved reports' filters pointing at live records: after a merge
/// `from` becomes `to`; after a delete (`to` is `None`) it is dropped.
/// Each changed report gets an audit entry.
pub(crate) fn saved_replace_id(
    tx: &Tx<'_>,
    list: FilterList,
    from: i64,
    to: Option<i64>,
) -> Result<()> {
    for before in saved_list(tx.conn())? {
        let mut settings = before.settings.clone();
        if !settings.replace_filter_id(list, from, to) {
            continue;
        }
        tx.conn().execute(
            "UPDATE saved_report SET settings_json = ?2, updated_at = ?3 WHERE id = ?1",
            params![before.id, serde_json::to_string(&settings)?, tx.now()],
        )?;
        let after = saved_get(tx.conn(), before.id)?;
        audit::record(
            tx,
            AuditEntity::SavedReport,
            before.id.0,
            AuditAction::Update,
            Some(&before),
            Some(&after),
        )?;
    }
    Ok(())
}

/// Move a saved report to another folder.
pub fn saved_move(tx: &Tx<'_>, id: SavedReportId, folder: ReportFolderId) -> Result<SavedReport> {
    let before = saved_get(tx.conn(), id)?;
    folder_get(tx.conn(), folder)?;
    if before.folder == folder {
        return Ok(before);
    }
    tx.conn().execute(
        "UPDATE saved_report SET folder_id = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, folder, tx.now()],
    )?;
    let after = saved_get(tx.conn(), id)?;
    audit::record(
        tx,
        AuditEntity::SavedReport,
        id.0,
        AuditAction::Update,
        Some(&before),
        Some(&after),
    )?;
    Ok(after)
}

/// Delete a saved report.
pub fn saved_delete(tx: &Tx<'_>, id: SavedReportId) -> Result<()> {
    let before = saved_get(tx.conn(), id)?;
    tx.conn()
        .execute("DELETE FROM saved_report WHERE id = ?1", [id])?;
    audit::record::<_, ()>(
        tx,
        AuditEntity::SavedReport,
        id.0,
        AuditAction::Delete,
        Some(&before),
        None,
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Saved report folders (RPT-020)
// ---------------------------------------------------------------------------

fn folder_from_row(r: &Row<'_>) -> rusqlite::Result<ReportFolder> {
    let id: ReportFolderId = r.get(0)?;
    Ok(ReportFolder {
        id,
        name: r.get(1)?,
        permanent: id == ReportFolderId::UNFILED,
    })
}

/// One folder by ID.
pub fn folder_get(conn: &Connection, id: ReportFolderId) -> Result<ReportFolder> {
    conn.prepare_cached("SELECT id, name FROM report_folder WHERE id = ?1")?
        .query_row([id], folder_from_row)
        .optional()?
        .ok_or(Error::NotFound {
            entity: "report folder",
            id: id.0,
        })
}

/// Every folder, by name.
pub fn folder_list(conn: &Connection) -> Result<Vec<ReportFolder>> {
    let mut stmt =
        conn.prepare_cached("SELECT id, name FROM report_folder ORDER BY name COLLATE NOCASE, id")?;
    let rows = stmt.query_map([], folder_from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn clean_folder_name(
    conn: &Connection,
    name: &str,
    except: Option<ReportFolderId>,
) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Invalid("a folder needs a name".into()));
    }
    let clash = conn
        .prepare_cached(
            "SELECT 1 FROM report_folder WHERE name = ?1 COLLATE NOCASE AND id IS NOT ?2",
        )?
        .exists(params![name, except])?;
    if clash {
        return Err(Error::Invalid(format!(
            "a folder named {name:?} already exists"
        )));
    }
    Ok(name.to_string())
}

fn permanent_refused(folder: &ReportFolder, what: &str) -> Result<()> {
    if folder.permanent {
        return Err(Error::Invalid(format!(
            "the {} folder cannot be {what}",
            folder.name
        )));
    }
    Ok(())
}

/// Make a new, empty folder.
pub fn folder_insert(tx: &Tx<'_>, name: &str) -> Result<ReportFolder> {
    let name = clean_folder_name(tx.conn(), name, None)?;
    tx.conn().execute(
        "INSERT INTO report_folder (name, created_at) VALUES (?1, ?2)",
        params![name, tx.now()],
    )?;
    let folder = folder_get(tx.conn(), ReportFolderId(tx.conn().last_insert_rowid()))?;
    audit::record::<(), _>(
        tx,
        AuditEntity::ReportFolder,
        folder.id.0,
        AuditAction::Create,
        None,
        Some(&folder),
    )?;
    Ok(folder)
}

/// Rename a folder. Unfiled keeps its name.
pub fn folder_rename(tx: &Tx<'_>, id: ReportFolderId, name: &str) -> Result<ReportFolder> {
    let before = folder_get(tx.conn(), id)?;
    permanent_refused(&before, "renamed")?;
    let name = clean_folder_name(tx.conn(), name, Some(id))?;
    tx.conn().execute(
        "UPDATE report_folder SET name = ?2 WHERE id = ?1",
        params![id, name],
    )?;
    let after = folder_get(tx.conn(), id)?;
    if after != before {
        audit::record(
            tx,
            AuditEntity::ReportFolder,
            id.0,
            AuditAction::Update,
            Some(&before),
            Some(&after),
        )?;
    }
    Ok(after)
}

/// Delete an empty folder. Unfiled stays.
pub fn folder_delete(tx: &Tx<'_>, id: ReportFolderId) -> Result<()> {
    let before = folder_get(tx.conn(), id)?;
    permanent_refused(&before, "deleted")?;
    let used = tx
        .conn()
        .prepare_cached("SELECT 1 FROM saved_report WHERE folder_id = ?1")?
        .exists([id])?;
    if used {
        return Err(Error::Invalid(format!(
            "the {} folder has saved reports; move or delete them first",
            before.name
        )));
    }
    tx.conn()
        .execute("DELETE FROM report_folder WHERE id = ?1", [id])?;
    audit::record::<_, ()>(
        tx,
        AuditEntity::ReportFolder,
        id.0,
        AuditAction::Delete,
        Some(&before),
        None,
    )?;
    Ok(())
}
