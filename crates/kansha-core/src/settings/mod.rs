//! Book settings (SET-030 … SET-070), kept in the book's `setting` table
//! so they travel with the data and are restored with it. Per-computer
//! preferences (theme, font size, window geometry, recent books) are not
//! here; see [`crate::local_config`].
//!
//! Each setting is one row, stored as text. A missing or unreadable value
//! gives the default, so a bad row never keeps a book from opening;
//! [`save`] refuses values out of range.
//!
//! The backup status (last backup, last full verification) is kept in the
//! same table under `backup.*` keys; it is state, not a setting.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::accounts::LotMethod;
use crate::date::Timestamp;
use crate::error::{Error, Result};
use crate::persistence::{Tx, settings as repo};
use crate::securities::DEFAULT_STALE_DAYS;
use crate::text_enum::text_enum;

text_enum! {
    /// How dates are shown and typed (SET-030).
    pub enum DateFormat {
        Mdy = "mdy",
        Dmy = "dmy",
        Ymd = "ymd",
    }
}

text_enum! {
    /// First day of the week, for the calendar (SET-030).
    pub enum WeekStart {
        Sunday = "sunday",
        Monday = "monday",
    }
}

text_enum! {
    /// Which side the account list panel opens on.
    pub enum PanelSide {
        Left = "left",
        Right = "right",
    }
}

/// Every book setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Settings {
    pub date_format: DateFormat,
    pub week_start: WeekStart,
    /// "On startup open to:" (SET-060): a view, a window, or
    /// `account:<id>`. The UI owns the choices.
    pub startup: String,
    /// Run the integrity check after the book opens (SET-060).
    pub integrity_at_startup: bool,
    /// Navigation bar buttons as the UI's JSON list; `None` = default.
    pub nav_items: Option<String>,
    pub account_panel_open: bool,
    pub account_panel_side: PanelSide,
    /// The account list panel's width in pixels, as the user dragged it;
    /// 0 = the stock width.
    pub account_panel_width: i64,
    /// The Investments screen's named views, as the UI's JSON; `None` =
    /// default.
    pub invest_views: Option<String>,
    /// A price older than this many days is stale (SET-040), unless the
    /// security sets its own.
    pub stale_price_days: i64,
    /// Lot selection method a new investment account starts with
    /// (SET-040); each account and security keeps its own after that.
    pub default_lot_method: LotMethod,
    /// Price download from the internet is allowed (PRC-040, SECU-070);
    /// off until the user turns it on.
    pub price_download: bool,
    /// Days ahead the Due soon card lists scheduled items (CARD-020).
    pub upcoming_days: i64,
    /// Years the Net worth over time card shows (CARD-050).
    pub trend_years: i64,
    /// Rows a spending card shows before the rest scroll (CARD-060).
    pub spending_rows: i64,
    /// That card's money axis fits the data instead of reaching zero.
    pub trend_fitted: bool,
    /// Backup folder (SET-050, BAK-030); `None` = the Downloads folder.
    pub backup_folder: Option<String>,
    /// Retention (BAK-040): newest automatic backups kept …
    pub backup_keep_last: i64,
    /// … plus the newest one of each of this many months.
    pub backup_keep_months: i64,
    /// Minutes after the first change since the last backup, when a timed
    /// backup is made (SET-050); 0 = off.
    pub backup_timeout_minutes: i64,
    /// Reconciled register rows are shown gray (REG-070).
    pub gray_reconciled: bool,
    /// A memorized payee fills in its category, memo, and amount (REG-100).
    pub recall_payees: bool,
    /// Payee and category names are capitalized as they are entered (REG-110).
    pub capitalize_names: bool,
    /// A new payee is memorized from its first transaction (REG-100).
    pub auto_memorize_payees: bool,
    /// Memorized payees not used in this many months are removed when the
    /// book opens (REG-120); 0 = never.
    pub purge_payees_months: i64,
    /// Warn about a transaction dated over 7 days past or 30 days ahead
    /// (REG-130).
    pub warn_out_of_date: bool,
    /// Warn when a check number is used twice in an account (REG-140).
    pub warn_check_reuse: bool,
    /// Ask before saving a changed transaction (REG-150).
    pub confirm_save_change: bool,
    /// Warn when a line is in the top-level Uncategorized category
    /// (REG-160).
    pub warn_uncategorized: bool,
    /// The account bar shows cents; off, its balances, section totals,
    /// and net worth are rounded to whole dollars (ACCT-240).
    pub account_bar_cents: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            date_format: DateFormat::Mdy,
            week_start: WeekStart::Sunday,
            startup: "insights".into(),
            integrity_at_startup: false,
            nav_items: None,
            account_panel_open: true,
            account_panel_side: PanelSide::Left,
            account_panel_width: 0,
            invest_views: None,
            stale_price_days: DEFAULT_STALE_DAYS,
            default_lot_method: LotMethod::Fifo,
            price_download: false,
            upcoming_days: 14,
            trend_years: 1,
            spending_rows: 10,
            trend_fitted: false,
            backup_folder: None,
            backup_keep_last: 10,
            backup_keep_months: 12,
            backup_timeout_minutes: 5,
            gray_reconciled: true,
            recall_payees: true,
            capitalize_names: false,
            auto_memorize_payees: true,
            purge_payees_months: 0,
            warn_out_of_date: true,
            warn_check_reuse: true,
            confirm_save_change: false,
            warn_uncategorized: true,
            account_bar_cents: true,
        }
    }
}

pub const STALE_PRICE_DAYS: std::ops::RangeInclusive<i64> = 1..=365;
pub const UPCOMING_DAYS: std::ops::RangeInclusive<i64> = 1..=366;
pub const TREND_YEARS: std::ops::RangeInclusive<i64> = 1..=5;
pub const SPENDING_ROWS: std::ops::RangeInclusive<i64> = 3..=50;
pub const KEEP_LAST: std::ops::RangeInclusive<i64> = 1..=1000;
pub const KEEP_MONTHS: std::ops::RangeInclusive<i64> = 0..=120;
pub const TIMEOUT_MINUTES: std::ops::RangeInclusive<i64> = 0..=1440;
/// A dragged panel width, in pixels (or 0 for the stock width).
pub const PANEL_WIDTH: std::ops::RangeInclusive<i64> = 160..=800;
pub const PURGE_MONTHS: std::ops::RangeInclusive<i64> = 0..=120;

fn get<T: std::str::FromStr>(conn: &Connection, key: &str, default: T) -> Result<T> {
    Ok(repo::get(conn, key)?
        .and_then(|v| v.parse().ok())
        .unwrap_or(default))
}

fn get_in(
    conn: &Connection,
    key: &str,
    range: std::ops::RangeInclusive<i64>,
    default: i64,
) -> Result<i64> {
    let v = get(conn, key, default)?;
    Ok(if range.contains(&v) { v } else { default })
}

/// A JSON list; `None` when unset or unreadable.
fn get_text(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(repo::get(conn, key)?.filter(|v| !v.is_empty()))
}

/// The book's settings; defaults for anything unset.
pub fn load(conn: &Connection) -> Result<Settings> {
    let d = Settings::default();
    Ok(Settings {
        date_format: get(conn, "date_format", d.date_format)?,
        week_start: get(conn, "week_start", d.week_start)?,
        startup: get_text(conn, "startup")?.unwrap_or(d.startup),
        integrity_at_startup: get(conn, "integrity_at_startup", d.integrity_at_startup)?,
        nav_items: get_text(conn, "nav_items")?,
        account_panel_open: get(conn, "account_panel_open", d.account_panel_open)?,
        account_panel_side: get(conn, "account_panel_side", d.account_panel_side)?,
        account_panel_width: match get(conn, "account_panel_width", d.account_panel_width)? {
            w if PANEL_WIDTH.contains(&w) => w,
            _ => d.account_panel_width,
        },
        invest_views: get_text(conn, "invest_views")?,
        stale_price_days: get_in(
            conn,
            "stale_price_days",
            STALE_PRICE_DAYS,
            d.stale_price_days,
        )?,
        default_lot_method: get(conn, "default_lot_method", d.default_lot_method)?,
        price_download: get(conn, "price_download", d.price_download)?,
        upcoming_days: get_in(conn, "upcoming_days", UPCOMING_DAYS, d.upcoming_days)?,
        trend_years: get_in(conn, "trend_years", TREND_YEARS, d.trend_years)?,
        spending_rows: get_in(conn, "spending_rows", SPENDING_ROWS, d.spending_rows)?,
        trend_fitted: get(conn, "trend_fitted", d.trend_fitted)?,
        backup_folder: get_text(conn, "backup_folder")?,
        backup_keep_last: get_in(conn, "backup_keep_last", KEEP_LAST, d.backup_keep_last)?,
        backup_keep_months: get_in(
            conn,
            "backup_keep_months",
            KEEP_MONTHS,
            d.backup_keep_months,
        )?,
        backup_timeout_minutes: get_in(
            conn,
            "backup_timeout_minutes",
            TIMEOUT_MINUTES,
            d.backup_timeout_minutes,
        )?,
        gray_reconciled: get(conn, "gray_reconciled", d.gray_reconciled)?,
        recall_payees: get(conn, "recall_payees", d.recall_payees)?,
        capitalize_names: get(conn, "capitalize_names", d.capitalize_names)?,
        auto_memorize_payees: get(conn, "auto_memorize_payees", d.auto_memorize_payees)?,
        purge_payees_months: get_in(
            conn,
            "purge_payees_months",
            PURGE_MONTHS,
            d.purge_payees_months,
        )?,
        warn_out_of_date: get(conn, "warn_out_of_date", d.warn_out_of_date)?,
        warn_check_reuse: get(conn, "warn_check_reuse", d.warn_check_reuse)?,
        confirm_save_change: get(conn, "confirm_save_change", d.confirm_save_change)?,
        warn_uncategorized: get(conn, "warn_uncategorized", d.warn_uncategorized)?,
        account_bar_cents: get(conn, "account_bar_cents", d.account_bar_cents)?,
    })
}

fn check_range(what: &str, v: i64, range: std::ops::RangeInclusive<i64>) -> Result<()> {
    if range.contains(&v) {
        Ok(())
    } else {
        Err(Error::Invalid(format!(
            "{what} must be from {} to {}",
            range.start(),
            range.end()
        )))
    }
}

fn put_text(tx: &Tx<'_>, key: &str, v: Option<&str>) -> Result<()> {
    match v.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => repo::set(tx, key, v),
        None => repo::remove(tx, key),
    }
}

/// Store every setting. Out-of-range numbers are refused.
pub fn save(tx: &Tx<'_>, s: &Settings) -> Result<()> {
    check_range("Stale price days", s.stale_price_days, STALE_PRICE_DAYS)?;
    check_range("Upcoming days", s.upcoming_days, UPCOMING_DAYS)?;
    check_range("Net worth years", s.trend_years, TREND_YEARS)?;
    check_range(
        "Rows shown on spending cards",
        s.spending_rows,
        SPENDING_ROWS,
    )?;
    check_range("Backups to keep", s.backup_keep_last, KEEP_LAST)?;
    check_range("Months to keep", s.backup_keep_months, KEEP_MONTHS)?;
    check_range(
        "Minutes before a timed backup",
        s.backup_timeout_minutes,
        TIMEOUT_MINUTES,
    )?;
    check_range(
        "Months before a payee is removed",
        s.purge_payees_months,
        PURGE_MONTHS,
    )?;
    if s.account_panel_width != 0 {
        check_range("Account list width", s.account_panel_width, PANEL_WIDTH)?;
    }
    if s.startup.trim().is_empty() {
        return Err(Error::Invalid("the startup choice is required".into()));
    }
    repo::set(tx, "date_format", s.date_format.as_str())?;
    repo::set(tx, "week_start", s.week_start.as_str())?;
    repo::set(tx, "startup", s.startup.trim())?;
    repo::set(
        tx,
        "integrity_at_startup",
        &s.integrity_at_startup.to_string(),
    )?;
    put_text(tx, "nav_items", s.nav_items.as_deref())?;
    repo::set(tx, "account_panel_open", &s.account_panel_open.to_string())?;
    repo::set(tx, "account_panel_side", s.account_panel_side.as_str())?;
    repo::set(
        tx,
        "account_panel_width",
        &s.account_panel_width.to_string(),
    )?;
    put_text(tx, "invest_views", s.invest_views.as_deref())?;
    repo::set(tx, "stale_price_days", &s.stale_price_days.to_string())?;
    repo::set(tx, "default_lot_method", s.default_lot_method.as_str())?;
    repo::set(tx, "price_download", &s.price_download.to_string())?;
    repo::set(tx, "upcoming_days", &s.upcoming_days.to_string())?;
    repo::set(tx, "trend_years", &s.trend_years.to_string())?;
    repo::set(tx, "spending_rows", &s.spending_rows.to_string())?;
    put_text(tx, "backup_folder", s.backup_folder.as_deref())?;
    repo::set(tx, "backup_keep_last", &s.backup_keep_last.to_string())?;
    repo::set(tx, "backup_keep_months", &s.backup_keep_months.to_string())?;
    repo::set(
        tx,
        "backup_timeout_minutes",
        &s.backup_timeout_minutes.to_string(),
    )?;
    for (key, v) in [
        ("gray_reconciled", s.gray_reconciled),
        ("recall_payees", s.recall_payees),
        ("capitalize_names", s.capitalize_names),
        ("auto_memorize_payees", s.auto_memorize_payees),
        ("warn_out_of_date", s.warn_out_of_date),
        ("warn_check_reuse", s.warn_check_reuse),
        ("confirm_save_change", s.confirm_save_change),
        ("warn_uncategorized", s.warn_uncategorized),
        ("account_bar_cents", s.account_bar_cents),
        ("trend_fitted", s.trend_fitted),
    ] {
        repo::set(tx, key, &v.to_string())?;
    }
    repo::set(
        tx,
        "purge_payees_months",
        &s.purge_payees_months.to_string(),
    )?;
    Ok(())
}

/// Upper-case the first letter of each word (REG-110). The rest of a word
/// stays as typed, so "IBM" and "McDonald" survive.
pub fn capitalize_words(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut start = true;
    for c in name.chars() {
        if start {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
        start = c.is_whitespace() || c == ':';
    }
    out
}

/// A payee or category name as the setting wants it stored.
pub fn tidy_name(conn: &Connection, name: &str) -> Result<String> {
    let on = get(
        conn,
        "capitalize_names",
        Settings::default().capitalize_names,
    )?;
    Ok(if on {
        capitalize_words(name)
    } else {
        name.to_string()
    })
}

/// The stale-price threshold alone (SET-040), for valuations.
pub fn stale_price_days(conn: &Connection) -> Result<i64> {
    get_in(
        conn,
        "stale_price_days",
        STALE_PRICE_DAYS,
        DEFAULT_STALE_DAYS,
    )
}

/// The last backup and verification (BAK-080, CARD-030).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BackupStatus {
    pub last_at: Option<Timestamp>,
    pub last_path: Option<String>,
    /// Integrity problems in the last backup's snapshot.
    pub last_issues: i64,
    /// The last full decrypt-and-check that passed (Verify backup…,
    /// restore drill).
    pub last_verified_at: Option<Timestamp>,
    /// When the last backup was last fully checked at startup (BAK-080),
    /// passed or failed. Verify backup… does not count.
    pub startup_checked_at: Option<Timestamp>,
    /// The file that check was of.
    pub startup_path: Option<String>,
    /// Why that check failed; `None` when it passed.
    pub startup_error: Option<String>,
    /// The backup folder was missing at the last backup, which went to
    /// Downloads instead (BAK-030). Cleared when a folder is chosen or a
    /// backup reaches it.
    pub folder_missing: bool,
}

pub fn backup_status(conn: &Connection) -> Result<BackupStatus> {
    Ok(BackupStatus {
        last_at: get_text(conn, "backup.last_at")?.and_then(|v| v.parse().ok()),
        last_path: get_text(conn, "backup.last_path")?,
        last_issues: get(conn, "backup.last_issues", 0)?,
        last_verified_at: get_text(conn, "backup.last_verified_at")?.and_then(|v| v.parse().ok()),
        startup_checked_at: get_text(conn, "backup.startup_at")?.and_then(|v| v.parse().ok()),
        startup_path: get_text(conn, "backup.startup_path")?,
        startup_error: get_text(conn, "backup.startup_error")?,
        folder_missing: get(conn, "backup.folder_missing", false)?,
    })
}

/// Record a backup just written.
pub fn record_backup(
    tx: &Tx<'_>,
    at: Timestamp,
    path: &str,
    issues: usize,
    folder_missing: bool,
) -> Result<()> {
    repo::set(tx, "backup.last_at", &at.to_string())?;
    repo::set(tx, "backup.last_path", path)?;
    repo::set(tx, "backup.last_issues", &issues.to_string())?;
    repo::set(tx, "backup.folder_missing", &folder_missing.to_string())
}

/// Record a full verification that passed.
pub fn record_verified(tx: &Tx<'_>, at: Timestamp) -> Result<()> {
    repo::set(tx, "backup.last_verified_at", &at.to_string())
}

/// Record the full check of the last backup at startup (BAK-080):
/// `error` is why it failed, `None` when it passed.
pub fn record_startup_check(
    tx: &Tx<'_>,
    at: Timestamp,
    path: &str,
    error: Option<&str>,
) -> Result<()> {
    repo::set(tx, "backup.startup_at", &at.to_string())?;
    repo::set(tx, "backup.startup_path", path)?;
    match error {
        Some(e) => repo::set(tx, "backup.startup_error", e),
        None => repo::remove(tx, "backup.startup_error"),
    }
}

/// Forget the missing-folder warning (a new folder was chosen).
pub fn clear_folder_missing(tx: &Tx<'_>) -> Result<()> {
    repo::remove(tx, "backup.folder_missing")
}
