//! The restore comparison window (BAK-075): per account, the backup's
//! figures beside the current database's, so the user sees whether a
//! restore would replace newer data.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::accounts::{AccountId, AccountType};
use crate::date::{Date, Timestamp};
use crate::error::Result;
use crate::ledger;
use crate::money::Money;
use crate::persistence::{Db, accounts, backup as repo};

/// One database's figures for one account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AccountSide {
    pub name: String,
    pub account_type: AccountType,
    /// Transactions with a posting in the account, voided ones included.
    pub txns: i64,
    /// Final balance: every transaction, future-dated included. An
    /// investment account's market value instead: cash plus holdings at
    /// the database's latest prices. Ledger sign (a liability owed is
    /// negative), as in the account list.
    pub value: Money,
}

/// One account in either database, matched by ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ComparisonRow {
    pub account: AccountId,
    pub backup: Option<AccountSide>,
    pub current: Option<AccountSide>,
    /// Only in one database, or a figure differs.
    pub differs: bool,
}

/// The whole comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Comparison {
    pub backup_created_at: Timestamp,
    /// Last change in each database, from its audit log.
    pub backup_last_change: Option<Timestamp>,
    pub current_last_change: Option<Timestamp>,
    /// Accounts in both databases first (in the backup's account order),
    /// then those only in the backup, then those only in the current one.
    pub rows: Vec<ComparisonRow>,
}

fn sides(db: &Db) -> Result<Vec<(AccountId, AccountSide)>> {
    let conn = db.conn();
    // Far enough ahead to take every posting and the latest price.
    let end = Date::from_ymd(9999, 12, 31)?;
    let values: BTreeMap<AccountId, Money> = ledger::account_balances(conn, end)?
        .into_iter()
        .map(|b| (b.account, b.ending))
        .collect();
    let counts: BTreeMap<AccountId, i64> = repo::txn_counts(conn)?.into_iter().collect();
    Ok(accounts::list(conn)?
        .into_iter()
        .map(|a| {
            let side = AccountSide {
                name: a.fields.name.clone(),
                account_type: a.fields.account_type,
                txns: counts.get(&a.id).copied().unwrap_or(0),
                value: values.get(&a.id).copied().unwrap_or(Money::ZERO),
            };
            (a.id, side)
        })
        .collect())
}

/// Compare a backup's database with the current one (`None` when there
/// is none, as at first start).
pub fn compare(
    backup: &Db,
    backup_created_at: Timestamp,
    current: Option<&Db>,
) -> Result<Comparison> {
    let theirs = sides(backup)?;
    let mut ours: BTreeMap<AccountId, AccountSide> = match current {
        Some(db) => sides(db)?.into_iter().collect(),
        None => BTreeMap::new(),
    };
    let mut both = Vec::new();
    let mut backup_only = Vec::new();
    for (id, b) in theirs {
        match ours.remove(&id) {
            Some(c) => {
                let differs = b.txns != c.txns || b.value != c.value || b.name != c.name;
                both.push(ComparisonRow {
                    account: id,
                    backup: Some(b),
                    current: Some(c),
                    differs,
                });
            }
            None => backup_only.push(ComparisonRow {
                account: id,
                backup: Some(b),
                current: None,
                differs: true,
            }),
        }
    }
    let current_only = ours.into_iter().map(|(id, c)| ComparisonRow {
        account: id,
        backup: None,
        current: Some(c),
        differs: true,
    });
    let mut rows = both;
    rows.extend(backup_only);
    rows.extend(current_only);
    Ok(Comparison {
        backup_created_at,
        backup_last_change: repo::last_change(backup.conn())?,
        current_last_change: match current {
            Some(db) => repo::last_change(db.conn())?,
            None => None,
        },
        rows,
    })
}
