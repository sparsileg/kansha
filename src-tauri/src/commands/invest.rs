//! Securities, prices, and investment commands (SEC, PRC, INV, LOT, POS,
//! MIG-115, MIG-120).

use kansha_core::accounts::AccountId;
use kansha_core::backup::BackupKind;
use kansha_core::invest::{
    self, Allocation, Holdings, IncomeReport, InvAction, InvInput, InvRegister, InvTxn, LotView,
    Performance, Portfolio, RealizedGain, SeedPreview, TrueUpPreview,
};
use kansha_core::ledger::TxnId;
use kansha_core::persistence::imports::{self, ImportFormat};
use kansha_core::persistence::securities as repo;
use kansha_core::reports::{self, Chart, ChartSpan, SecurityChartKind, SecurityTxn};
use kansha_core::securities::download::{self, DownloadSummary, Fetched, Provider};
use kansha_core::securities::{
    self, PriceImportPreview, PricePoint, PriceSource, Security, SecurityFields, SecurityId,
    SecurityType,
};
use kansha_core::{Date, Money, Origin, Price, Quantity};
use tauri::State;

use crate::state::{AppState, CmdResult, IpcError};

// ---------------------------------------------------------------------------
// Securities and prices
// ---------------------------------------------------------------------------

/// The Security Details window's history: every transaction of the
/// security in every investment account, oldest first (SEC-060).
#[tauri::command]
#[specta::specta]
pub fn security_transactions(
    state: State<'_, AppState>,
    security: SecurityId,
) -> CmdResult<Vec<SecurityTxn>> {
    state.read(|db, today| reports::security_transactions(db.conn(), security, today))
}

/// The Security Details window's graph: market value or price over a span
/// ending today, or from `from` to `to` for a custom span (SEC-060).
/// `fitted` sizes the money axis to the data instead of reaching zero.
#[tauri::command]
#[specta::specta]
pub fn security_chart(
    state: State<'_, AppState>,
    security: SecurityId,
    kind: SecurityChartKind,
    span: ChartSpan,
    from: Option<Date>,
    to: Option<Date>,
    fitted: bool,
) -> CmdResult<Chart> {
    state.read(|db, today| {
        let (from, to) = reports::span_dates(span, today, from, to)?;
        reports::security_chart(db.conn(), security, kind, from, to, fitted)
    })
}

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

/// Check a price list without writing anything (PRC-030).
#[tauri::command]
#[specta::specta]
pub fn price_import_preview(
    state: State<'_, AppState>,
    text: String,
    date: Date,
) -> CmdResult<PriceImportPreview> {
    state.read(|db, _| securities::preview_prices(db.conn(), &text, date))
}

/// Import a price list (PRC-030), all or nothing; lines without a date
/// take `date`. Returns the number of prices.
#[tauri::command]
#[specta::specta]
pub fn price_import(state: State<'_, AppState>, text: String, date: Date) -> CmdResult<i64> {
    state.backup(BackupKind::Import)?;
    state.write(|tx| securities::commit_prices(tx, &text, date))
}

/// Download a price for every shown security with a ticker (PRC-040),
/// once the book's setting allows it (SECU-070): the latest when `date`
/// is today or later, else the close of the last trading day on or before
/// `date`. The fetching runs off the main thread; the prices are stored
/// in one transaction.
#[tauri::command]
#[specta::specta]
pub async fn prices_download(state: State<'_, AppState>, date: Date) -> CmdResult<DownloadSummary> {
    let (targets, today) = state.read(|db, today| Ok((download::targets(db.conn())?, today)))?;
    let on = (date < today).then_some(date);
    let fetched =
        tauri::async_runtime::spawn_blocking(move || fetch_all(Provider::Yahoo, targets, on))
            .await
            .map_err(|e| IpcError::internal(format!("price download stopped: {e}")))?;
    state.write(|tx| download::store(tx, &fetched))
}

/// One request per security, 15 seconds each at most.
fn fetch_all(provider: Provider, targets: Vec<download::Target>, on: Option<Date>) -> Vec<Fetched> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent(concat!("Kansha/", env!("CARGO_PKG_VERSION")))
        .build();
    targets
        .into_iter()
        .map(|target| {
            let result = agent
                .get(&provider.url(&target.ticker, on))
                .call()
                .map_err(|e| match e {
                    ureq::Error::Status(404, r) => r
                        .into_string()
                        .ok()
                        .and_then(|b| provider.parse(&b, on).err())
                        .unwrap_or_else(|| "not found".into()),
                    ureq::Error::Status(code, _) => format!("the server answered {code}"),
                    ureq::Error::Transport(t) => format!("no connection ({})", t.kind()),
                })
                .and_then(|r| r.into_string().map_err(|e| e.to_string()))
                .and_then(|body| provider.parse(&body, on));
            Fetched { target, result }
        })
        .collect()
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
    state.write_undoable(
        None,
        "New transaction",
        |tx| invest::create(tx, &input).map(|t| t.txn.id),
        |id| *id,
    )
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
    state.write_undoable(
        Some(txn),
        "Edit",
        |tx| invest::update(tx, txn, &input, confirmed).map(|_| ()),
        |()| txn,
    )
}

#[tauri::command]
#[specta::specta]
pub fn inv_delete(state: State<'_, AppState>, txn: TxnId, confirmed: bool) -> CmdResult<()> {
    state.write_undoable(
        Some(txn),
        "Delete",
        |tx| invest::delete(tx, txn, confirmed),
        |()| txn,
    )
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

/// The investments overview on `as_of` (today when left out): `accounts`
/// in the order given, their positions and lots, day changes, and totals;
/// `securities` limits it to those (all when left out); `closed` adds
/// each lot's sales and the securities sold out (POS-010, POS-040,
/// LOT-150).
#[tauri::command]
#[specta::specta]
pub fn inv_portfolio(
    state: State<'_, AppState>,
    accounts: Vec<AccountId>,
    securities: Option<Vec<SecurityId>>,
    as_of: Option<Date>,
    closed: bool,
) -> CmdResult<Portfolio> {
    state.read(|db, today| {
        invest::portfolio(
            db.conn(),
            &accounts,
            securities.as_deref(),
            as_of.unwrap_or(today),
            closed,
        )
    })
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
    state.backup(BackupKind::Import)?;
    let batch = state.write(|tx| imports::stage(tx, &file_name, ImportFormat::Csv))?;
    state.write_as(Origin::Import(batch.id), |tx| {
        let n = invest::commit_seed(tx, &text, date)?;
        imports::commit(tx, batch.id)?;
        Ok(n)
    })
}

// ---------------------------------------------------------------------------
// Lot true-up (MIG-115)
// ---------------------------------------------------------------------------

/// The broker's lot list in `text` (CSV) for `security`'s rows.
fn broker_lots(
    db: &kansha_core::Db,
    security: SecurityId,
    text: &str,
) -> kansha_core::Result<Vec<invest::TrueUpLot>> {
    let sec = repo::get(db.conn(), security)?;
    invest::parse_true_up(text, sec.fields.ticker.as_deref())
}

/// Compare a holding's lots on `date` with the broker's list.
#[tauri::command]
#[specta::specta]
pub fn true_up_preview(
    state: State<'_, AppState>,
    account: AccountId,
    security: SecurityId,
    date: Date,
    text: String,
) -> CmdResult<TrueUpPreview> {
    state.read(|db, _| {
        let lots = broker_lots(db, security, &text)?;
        invest::preview_true_up(db.conn(), account, security, date, &lots)
    })
}

/// Set a holding's lots to the broker's list as of `date` (a backup is
/// made first); later sales choose their lots again.
#[tauri::command]
#[specta::specta]
pub fn true_up(
    state: State<'_, AppState>,
    account: AccountId,
    security: SecurityId,
    date: Date,
    text: String,
    memo: String,
) -> CmdResult<InvTxn> {
    state.backup(BackupKind::Bulk)?;
    state.write(|tx| {
        let sec = repo::get(tx.conn(), security)?;
        let lots = invest::parse_true_up(&text, sec.fields.ticker.as_deref())?;
        invest::true_up(tx, account, security, date, &lots, &memo)
    })
}
