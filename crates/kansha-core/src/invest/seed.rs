//! Lot seeding from CSV (MIG-120): parse → preview → commit.
//!
//! One row per lot: `account`, `security` (ticker or name), `acquired`
//! (original acquisition date), `quantity`, `cost basis`. Each row
//! becomes a Shares Added transaction dated the seeding date, whose lot
//! keeps the original acquisition date. Nothing is written unless every
//! row is good. Commit runs as an import (origin `Import`, MIG-080).

use std::collections::BTreeMap;

use rusqlite::Connection;
use serde::Serialize;

use super::service::plan;
use super::{InvAction, InvInput, create};
use crate::accounts::AccountId;
use crate::csv;
use crate::date::Date;
use crate::error::{Error, Result};
use crate::money::{Money, Quantity};
use crate::persistence::{Origin, Tx, accounts, securities};
use crate::securities::SecurityId;

/// One line of the file, checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SeedRow {
    pub line: i64,
    pub account_label: String,
    pub account: Option<AccountId>,
    pub security_label: String,
    pub security: Option<SecurityId>,
    pub acquired: Option<Date>,
    pub quantity: Option<Quantity>,
    pub basis: Option<Money>,
    pub error: Option<String>,
}

/// Lots, shares, and basis per account and security (MIG-050 totals).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SeedTotal {
    pub account: AccountId,
    pub security: SecurityId,
    pub lots: i64,
    pub quantity: Quantity,
    pub basis: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SeedPreview {
    /// The date the Shares Added transactions get.
    pub date: Date,
    pub rows: Vec<SeedRow>,
    pub totals: Vec<SeedTotal>,
    pub good: i64,
    pub errors: i64,
}

fn input_for(row: &SeedRow, date: Date) -> Option<InvInput> {
    let mut i = InvInput::new(row.account?, InvAction::SharesAdded, date);
    i.security = row.security;
    i.quantity = row.quantity;
    i.amount = row.basis;
    i.acquired = row.acquired;
    i.memo = "Lot seeding".into();
    Some(i)
}

/// Check every row of `text` as of seeding date `date`.
pub fn preview_seed(conn: &Connection, text: &str, date: Date) -> Result<SeedPreview> {
    let table = csv::parse(text)?;
    let acct_col = table.require(&["account"])?;
    let sec_col = table.require(&["security", "ticker", "symbol"])?;
    let acq_col = table.require(&[
        "acquired",
        "acquired date",
        "date acquired",
        "acquisition date",
        "open date",
    ])?;
    let qty_col = table.require(&["quantity", "shares"])?;
    let basis_col = table.require(&["cost basis", "basis", "cost"])?;
    let all_accounts = accounts::list(conn)?;

    let mut rows = Vec::with_capacity(table.rows.len());
    let mut totals: BTreeMap<(AccountId, SecurityId), SeedTotal> = BTreeMap::new();
    for rec in &table.rows {
        let mut row = SeedRow {
            line: i64::try_from(rec.line).unwrap_or(i64::MAX),
            account_label: rec.get(acct_col).to_string(),
            account: None,
            security_label: rec.get(sec_col).to_string(),
            security: None,
            acquired: None,
            quantity: None,
            basis: None,
            error: None,
        };
        let mut problems: Vec<String> = Vec::new();
        match all_accounts
            .iter()
            .find(|a| a.fields.name.eq_ignore_ascii_case(&row.account_label))
        {
            Some(a) => row.account = Some(a.id),
            None => problems.push(format!("unknown account {:?}", row.account_label)),
        }
        match securities::find_by_label(conn, &row.security_label)? {
            Some(s) => row.security = Some(s.id),
            None => problems.push(format!("unknown security {:?}", row.security_label)),
        }
        match csv::date(rec.get(acq_col)) {
            Ok(d) => row.acquired = Some(d),
            Err(e) => problems.push(e.to_string()),
        }
        match csv::quantity(rec.get(qty_col)) {
            Ok(q) => row.quantity = Some(q),
            Err(e) => problems.push(e.to_string()),
        }
        match csv::money(rec.get(basis_col)) {
            Ok(m) => row.basis = Some(m),
            Err(e) => problems.push(e.to_string()),
        }
        if problems.is_empty() {
            if let Some(input) = input_for(&row, date) {
                if let Err(e) = plan(conn, &input, None) {
                    problems.push(e.to_string());
                }
            }
        }
        if problems.is_empty() {
            if let (Some(a), Some(s), Some(q), Some(b)) =
                (row.account, row.security, row.quantity, row.basis)
            {
                let t = totals.entry((a, s)).or_insert(SeedTotal {
                    account: a,
                    security: s,
                    lots: 0,
                    quantity: Quantity::ZERO,
                    basis: Money::ZERO,
                });
                t.lots += 1;
                t.quantity = t
                    .quantity
                    .checked_add(q)
                    .ok_or(Error::Overflow("seed total"))?;
                t.basis = t
                    .basis
                    .checked_add(b)
                    .ok_or(Error::Overflow("seed total"))?;
            }
        } else {
            row.error = Some(problems.join("; "));
        }
        rows.push(row);
    }
    let errors = rows.iter().filter(|r| r.error.is_some()).count();
    Ok(SeedPreview {
        date,
        good: i64::try_from(rows.len() - errors).unwrap_or(i64::MAX),
        errors: i64::try_from(errors).unwrap_or(i64::MAX),
        rows,
        totals: totals.into_values().collect(),
    })
}

/// Create a Shares Added transaction for every row of `text`, all or
/// nothing. Must run as an import. Returns the number of lots created.
pub fn commit_seed(tx: &Tx<'_>, text: &str, date: Date) -> Result<i64> {
    if !matches!(tx.origin(), Origin::Import(_)) {
        return Err(Error::Invalid("lot seeding runs as an import".into()));
    }
    let preview = preview_seed(tx.conn(), text, date)?;
    if let Some(bad) = preview.rows.iter().find(|r| r.error.is_some()) {
        return Err(Error::Invalid(format!(
            "line {}: {}; nothing was imported",
            bad.line,
            bad.error.as_deref().unwrap_or("")
        )));
    }
    for row in &preview.rows {
        if let Some(input) = input_for(row, date) {
            create(tx, &input)?;
        }
    }
    Ok(preview.good)
}
