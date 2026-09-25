//! Securities, prices, and investment commands (SEC, PRC, INV, LOT, POS,
//! MIG-120).

use kansha_core::accounts::AccountId;
use kansha_core::invest::{
    self, Allocation, Holdings, IncomeReport, InvAction, InvInput, InvRegister, InvTxn, LotView,
    Performance, RealizedGain, SeedPreview,
};
use kansha_core::ledger::TxnId;
use kansha_core::persistence::imports::{self, ImportFormat};
use kansha_core::persistence::securities as repo;
use kansha_core::securities::{
    self, PriceImportPreview, PricePoint, PriceSource, Security, SecurityFields, SecurityId,
    SecurityType,
};
use kansha_core::{Date, Money, Origin, Price, Quantity};
use tauri::State;

use crate::state::{AppState, CmdResult};

// ---------------------------------------------------------------------------
// Securities and prices
// ---------------------------------------------------------------------------

/// Every security, hidden ones included, by name.
#[tauri::command]
#[specta::specta]
pub fn security_list(state: State<'_, AppState>) -> CmdResult<Vec<Security>> {
    state.read(|db, _| repo::list(db.conn()))
}

/// A new security's fields with the type's default asset class.
#[tauri::command]
#[specta::specta]
pub fn security_defaults(name: String, security_type: SecurityType) -> SecurityFields {
    SecurityFields::new(name, security_type)
}

#[tauri::command]
#[specta::specta]
pub fn security_create(state: State<'_, AppState>, fields: SecurityFields) -> CmdResult<Security> {
    state.write(|tx| repo::insert(tx, &fields))
}

#[tauri::command]
#[specta::specta]
pub fn security_update(
    state: State<'_, AppState>,
    id: SecurityId,
    fields: SecurityFields,
) -> CmdResult<Security> {
    state.write(|tx| repo::update(tx, id, &fields))
}

/// Only a security no transaction uses can be deleted (SEC-040).
#[tauri::command]
#[specta::specta]
pub fn security_delete(state: State<'_, AppState>, id: SecurityId) -> CmdResult<()> {
    state.write(|tx| repo::delete(tx, id))
}

/// A security's prices, newest first.
#[tauri::command]
#[specta::specta]
pub fn price_list(state: State<'_, AppState>, security: SecurityId) -> CmdResult<Vec<PricePoint>> {
    state.read(|db, _| repo::prices(db.conn(), security))
}

/// Enter or replace the closing price on a date (PRC-020).
#[tauri::command]
#[specta::specta]
pub fn price_set(
    state: State<'_, AppState>,
    security: SecurityId,
    date: Date,
    price: Price,
) -> CmdResult<()> {
    let point = PricePoint {
        security,
        date,
        price,
        source: PriceSource::Manual,
    };
    state.write(|tx| repo::set_price(tx, &point).map(|_| ()))
}

#[tauri::command]
#[specta::specta]
pub fn price_delete(state: State<'_, AppState>, security: SecurityId, date: Date) -> CmdResult<()> {
    state.write(|tx| repo::delete_price(tx, security, date))
}

/// Check a price CSV without writing anything (PRC-030).
#[tauri::command]
#[specta::specta]
pub fn price_import_preview(
    state: State<'_, AppState>,
    text: String,
) -> CmdResult<PriceImportPreview> {
    state.read(|db, _| securities::preview_prices(db.conn(), &text))
}

/// Import a price CSV, all or nothing. Returns the number of prices.
#[tauri::command]
#[specta::specta]
pub fn price_import(state: State<'_, AppState>, text: String) -> CmdResult<i64> {
    state.write(|tx| securities::commit_prices(tx, &text))
}

// ---------------------------------------------------------------------------
// Investment transactions
// ---------------------------------------------------------------------------

/// An investment account's register (INV-030).
#[tauri::command]
#[specta::specta]
pub fn inv_register(state: State<'_, AppState>, account: AccountId) -> CmdResult<InvRegister> {
    state.read(|db, today| invest::register(db.conn(), account, today))
}

#[tauri::command]
#[specta::specta]
pub fn inv_get(state: State<'_, AppState>, txn: TxnId) -> CmdResult<InvTxn> {
    state.read(|db, _| invest::get(db.conn(), txn))
}

/// The stored transaction as an input, to edit and send back.
#[tauri::command]
#[specta::specta]
pub fn inv_input(state: State<'_, AppState>, txn: TxnId) -> CmdResult<InvInput> {
    state.read(|db, _| invest::get(db.conn(), txn).map(|t| t.to_input()))
}

#[tauri::command]
#[specta::specta]
pub fn inv_create(state: State<'_, AppState>, input: InvInput) -> CmdResult<TxnId> {
    state.write(|tx| invest::create(tx, &input).map(|t| t.txn.id))
}

/// Replace an investment transaction; a reconciled cash posting fails
/// with `confirmation_required` until `confirmed`.
#[tauri::command]
#[specta::specta]
pub fn inv_update(
    state: State<'_, AppState>,
    txn: TxnId,
    input: InvInput,
    confirmed: bool,
) -> CmdResult<()> {
    state.write(|tx| invest::update(tx, txn, &input, confirmed).map(|_| ()))
}

#[tauri::command]
#[specta::specta]
pub fn inv_delete(state: State<'_, AppState>, txn: TxnId, confirmed: bool) -> CmdResult<()> {
    state.write(|tx| invest::delete(tx, txn, confirmed))
}

/// Shares × price ± commission for the entry form; the UI does no money
/// arithmetic.
#[tauri::command]
#[specta::specta]
pub fn inv_trade_amount(
    action: InvAction,
    quantity: Quantity,
    price: Price,
    commission: Money,
) -> CmdResult<Money> {
    Ok(invest::trade_amount(action, quantity, price, commission)?)
}

/// Holdings on `as_of` (today when left out) (POS-010).
#[tauri::command]
#[specta::specta]
pub fn inv_holdings(
    state: State<'_, AppState>,
    account: AccountId,
    as_of: Option<Date>,
) -> CmdResult<Holdings> {
    state.read(|db, today| invest::holdings(db.conn(), account, as_of.unwrap_or(today), None))
}

/// Open lots on `as_of` (today when left out), one security or all
/// (LOT-150; the lot picker for specific identification).
#[tauri::command]
#[specta::specta]
pub fn inv_lots(
    state: State<'_, AppState>,
    account: AccountId,
    security: Option<SecurityId>,
    as_of: Option<Date>,
) -> CmdResult<Vec<LotView>> {
    state.read(|db, today| invest::open_lots(db.conn(), account, security, as_of.unwrap_or(today)))
}

/// Realized gains in one account or all, between two dates (LOT-040).
#[tauri::command]
#[specta::specta]
pub fn inv_gains(
    state: State<'_, AppState>,
    account: Option<AccountId>,
    from: Option<Date>,
    to: Option<Date>,
) -> CmdResult<Vec<RealizedGain>> {
    state.read(|db, _| invest::realized_gains(db.conn(), account, from, to))
}

#[tauri::command]
#[specta::specta]
pub fn inv_income(
    state: State<'_, AppState>,
    account: AccountId,
    from: Option<Date>,
    to: Option<Date>,
) -> CmdResult<IncomeReport> {
    state.read(|db, _| invest::income(db.conn(), account, from, to))
}

/// Simple performance as of today (POS-030).
#[tauri::command]
#[specta::specta]
pub fn inv_performance(state: State<'_, AppState>, account: AccountId) -> CmdResult<Performance> {
    state.read(|db, today| invest::performance(db.conn(), account, today))
}

/// Asset allocation today across `accounts` (every open investment
/// account when empty) (POS-020).
#[tauri::command]
#[specta::specta]
pub fn inv_allocation(
    state: State<'_, AppState>,
    accounts: Vec<AccountId>,
) -> CmdResult<Allocation> {
    state.read(|db, today| invest::allocation(db.conn(), &accounts, today))
}

// ---------------------------------------------------------------------------
// Lot seeding (MIG-120)
// ---------------------------------------------------------------------------

/// Check a lot-seeding CSV without writing anything.
#[tauri::command]
#[specta::specta]
pub fn lot_seed_preview(
    state: State<'_, AppState>,
    text: String,
    date: Date,
) -> CmdResult<SeedPreview> {
    state.read(|db, _| invest::preview_seed(db.conn(), &text, date))
}

/// Seed lots from a CSV as one import batch, all or nothing. Returns the
/// number of lots created.
#[tauri::command]
#[specta::specta]
pub fn lot_seed(
    state: State<'_, AppState>,
    file_name: String,
    text: String,
    date: Date,
) -> CmdResult<i64> {
    let batch = state.write(|tx| imports::stage(tx, &file_name, ImportFormat::Csv))?;
    state.write_as(Origin::Import(batch.id), |tx| {
        let n = invest::commit_seed(tx, &text, date)?;
        imports::commit(tx, batch.id)?;
        Ok(n)
    })
}
