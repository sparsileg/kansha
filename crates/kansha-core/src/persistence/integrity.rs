//! SQL for the integrity check (INT-030). Read-only.

use rusqlite::Connection;

use crate::error::Result;
use crate::integrity::{Check, Issue};

/// Run every check, in a fixed order.
pub fn run_all(conn: &Connection) -> Result<Vec<Issue>> {
    let mut issues = sqlite_integrity(conn)?;
    issues.extend(foreign_keys(conn)?);
    for (check, table, sql) in QUERIES {
        let mut stmt = conn.prepare_cached(sql)?;
        let rows = stmt.query_map([], |r| {
            Ok(Issue {
                check: *check,
                table: (*table).to_string(),
                id: r.get(0)?,
                detail: r.get(1)?,
            })
        })?;
        for row in rows {
            issues.push(row?);
        }
    }
    Ok(issues)
}

fn sqlite_integrity(conn: &Connection) -> Result<Vec<Issue>> {
    let mut stmt = conn.prepare("PRAGMA integrity_check")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut issues = Vec::new();
    for row in rows {
        let msg = row?;
        if msg != "ok" {
            issues.push(Issue {
                check: Check::SqliteIntegrity,
                table: String::new(),
                id: None,
                detail: msg,
            });
        }
    }
    Ok(issues)
}

fn foreign_keys(conn: &Connection) -> Result<Vec<Issue>> {
    let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
    let rows = stmt.query_map([], |r| {
        let table: String = r.get(0)?;
        let parent: String = r.get(2)?;
        Ok(Issue {
            check: Check::ForeignKeys,
            detail: format!("{table} row references a missing {parent} row"),
            table,
            id: r.get(1)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// (check, table, SQL returning (id, detail)).
const QUERIES: &[(Check, &str, &str)] = &[
    (
        Check::Unbalanced,
        "txn",
        "SELECT txn_id, 'postings sum to ' || total || ' cents' FROM unbalanced_txn ORDER BY txn_id",
    ),
    (
        Check::NoAccountPosting,
        "txn",
        "SELECT t.id, 'no posting to an account' FROM txn t
         WHERE NOT EXISTS (SELECT 1 FROM posting p WHERE p.txn_id = t.id AND p.account_id IS NOT NULL)
         ORDER BY t.id",
    ),
    (
        Check::DuplicateAccountPosting,
        "txn",
        "SELECT txn_id, 'account ' || account_id || ' posted ' || count(*) || ' times'
         FROM posting WHERE account_id IS NOT NULL AND security_id IS NULL
         GROUP BY txn_id, account_id HAVING count(*) > 1 ORDER BY txn_id",
    ),
    (
        Check::VoidWithAmount,
        "txn",
        "SELECT DISTINCT t.id, 'void transaction has a non-zero posting' FROM txn t
         JOIN posting p ON p.txn_id = t.id
         WHERE t.status = 'void' AND p.amount <> 0 ORDER BY t.id",
    ),
    (
        Check::PostingAfterClose,
        "posting",
        "SELECT p.id, 'account ' || a.id || ' closed ' || a.closed_date || '; posting dated ' || t.txn_date
         FROM posting p JOIN txn t ON t.id = p.txn_id JOIN account a ON a.id = p.account_id
         WHERE a.status = 'closed' AND t.txn_date > a.closed_date ORDER BY p.id",
    ),
    (
        Check::UnfinishedReconciliation,
        "posting",
        "SELECT p.id, 'reconciliation ' || r.id || ' is ' || r.status
         FROM posting p JOIN reconciliation r ON r.id = p.reconciliation_id
         WHERE r.status <> 'finished' ORDER BY p.id",
    ),
    (
        Check::ReconciledBalanceMismatch,
        "reconciliation",
        "SELECT r.id, 'account ' || r.account_id || ': reconciled postings total '
                || (SELECT ifnull(sum(p.amount), 0) FROM posting p
                    WHERE p.account_id = r.account_id AND p.cleared = 'reconciled')
                || ' cents; statement ' || r.statement_date || ' ended at '
                || r.statement_balance || ' cents'
         FROM reconciliation r
         WHERE r.status = 'finished'
           AND r.id = (SELECT max(r2.id) FROM reconciliation r2
                       WHERE r2.account_id = r.account_id AND r2.status = 'finished')
           AND (SELECT ifnull(sum(p.amount), 0) FROM posting p
                WHERE p.account_id = r.account_id AND p.cleared = 'reconciled')
               <> r.statement_balance
         ORDER BY r.id",
    ),
    (
        Check::CategoryCycle,
        "category",
        "WITH RECURSIVE up (start, cur, depth) AS (
             SELECT id, parent_id, 1 FROM category WHERE parent_id IS NOT NULL
             UNION ALL
             SELECT up.start, c.parent_id, up.depth + 1 FROM up JOIN category c ON c.id = up.cur
             WHERE up.cur <> up.start AND up.depth < 10000
         )
         SELECT DISTINCT start, 'category is its own ancestor' FROM up
         WHERE cur = start ORDER BY start",
    ),
    (
        Check::LotOverdrawn,
        "lot",
        "SELECT id, 'open shares ' || open_q || ' (×10⁻⁶), open basis ' || open_b || ' cents'
         FROM (SELECT l.id,
                      l.quantity
                        + ifnull((SELECT sum(quantity_delta) FROM lot_adjustment WHERE lot_id = l.id), 0)
                        - ifnull((SELECT sum(quantity) FROM lot_disposal WHERE lot_id = l.id), 0) AS open_q,
                      l.cost_basis
                        + ifnull((SELECT sum(basis_delta) FROM lot_adjustment WHERE lot_id = l.id), 0)
                        - ifnull((SELECT sum(basis) FROM lot_disposal WHERE lot_id = l.id), 0) AS open_b
               FROM lot l)
         WHERE open_q < 0 OR open_b < 0 OR (open_q = 0 AND open_b <> 0)
         ORDER BY id",
    ),
    (
        Check::ShareBalanceMismatch,
        "account",
        "WITH moves (account_id, security_id, q) AS (
             SELECT account_id, security_id,
                    CASE WHEN action IN ('buy', 'reinvest_dividend', 'reinvest_cg_short',
                                         'reinvest_cg_long', 'shares_added') THEN quantity
                         ELSE -quantity END
             FROM investment_txn
             WHERE action IN ('buy', 'reinvest_dividend', 'reinvest_cg_short', 'reinvest_cg_long',
                              'shares_added', 'sell', 'shares_removed', 'transfer_shares')
             UNION ALL
             SELECT to_account_id, security_id, quantity FROM investment_txn
             WHERE action = 'transfer_shares'
             UNION ALL
             SELECT l.account_id, l.security_id, a.quantity_delta
             FROM lot_adjustment a JOIN lot l ON l.id = a.lot_id
         ),
         held (account_id, security_id, q) AS (
             SELECT l.account_id, l.security_id,
                    l.quantity
                      + ifnull((SELECT sum(quantity_delta) FROM lot_adjustment WHERE lot_id = l.id), 0)
                      - ifnull((SELECT sum(quantity) FROM lot_disposal WHERE lot_id = l.id), 0)
             FROM lot l
         ),
         a AS (SELECT account_id, security_id, sum(q) AS q FROM moves GROUP BY 1, 2),
         b AS (SELECT account_id, security_id, sum(q) AS q FROM held GROUP BY 1, 2),
         k AS (SELECT account_id, security_id FROM a UNION SELECT account_id, security_id FROM b)
         SELECT k.account_id, 'security ' || k.security_id || ': transactions give '
                || ifnull(a.q, 0) || ', open lots hold ' || ifnull(b.q, 0) || ' (shares ×10⁻⁶)'
         FROM k
         LEFT JOIN a ON a.account_id = k.account_id AND a.security_id = k.security_id
         LEFT JOIN b ON b.account_id = k.account_id AND b.security_id = k.security_id
         WHERE ifnull(a.q, 0) <> ifnull(b.q, 0)
         ORDER BY k.account_id, k.security_id",
    ),
    (
        Check::LotBasisMismatch,
        "account",
        "WITH ledger (account_id, security_id, b) AS (
             SELECT account_id, security_id, sum(amount) FROM posting
             WHERE security_id IS NOT NULL GROUP BY 1, 2
         ),
         held (account_id, security_id, b) AS (
             SELECT l.account_id, l.security_id,
                    sum(l.cost_basis
                        + ifnull((SELECT sum(basis_delta) FROM lot_adjustment WHERE lot_id = l.id), 0)
                        - ifnull((SELECT sum(basis) FROM lot_disposal WHERE lot_id = l.id), 0))
             FROM lot l GROUP BY 1, 2
         ),
         k AS (SELECT account_id, security_id FROM ledger
               UNION SELECT account_id, security_id FROM held)
         SELECT k.account_id, 'security ' || k.security_id || ': postings carry '
                || ifnull(ledger.b, 0) || ' cents of basis, open lots '
                || ifnull(held.b, 0) || ' cents'
         FROM k
         LEFT JOIN ledger ON ledger.account_id = k.account_id AND ledger.security_id = k.security_id
         LEFT JOIN held ON held.account_id = k.account_id AND held.security_id = k.security_id
         WHERE ifnull(ledger.b, 0) <> ifnull(held.b, 0)
         ORDER BY k.account_id, k.security_id",
    ),
    (
        Check::LotQuantityMismatch,
        "txn",
        "SELECT i.txn_id, i.action || ' of ' || i.quantity || ' shares; lot records show '
                || CASE WHEN i.action IN ('sell', 'shares_removed', 'transfer_shares')
                        THEN ifnull((SELECT sum(quantity) FROM lot_disposal WHERE txn_id = i.txn_id), 0)
                        ELSE ifnull((SELECT sum(quantity) FROM lot WHERE origin_txn_id = i.txn_id), 0)
                   END
         FROM investment_txn i
         WHERE i.quantity IS NOT NULL
           AND i.quantity <> CASE
                 WHEN i.action IN ('sell', 'shares_removed', 'transfer_shares')
                   THEN ifnull((SELECT sum(quantity) FROM lot_disposal WHERE txn_id = i.txn_id), 0)
                 ELSE ifnull((SELECT sum(quantity) FROM lot WHERE origin_txn_id = i.txn_id), 0)
               END
         ORDER BY i.txn_id",
    ),
    (
        Check::CategoryKindMismatch,
        "category",
        "SELECT c.id, c.kind || ' category under ' || p.kind || ' category ' || p.id
         FROM category c JOIN category p ON p.id = c.parent_id
         WHERE c.kind <> p.kind ORDER BY c.id",
    ),
];
