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
    /// The Investments screen's named views, as the UI's JSON; `None` =
    /// default.
    pub invest_views: Option<String>,
    /// A price older than this many days is stale (SET-040), unless the
    /// security sets its own.
    pub stale_price_days: i64,
    /// Days ahead the dashboard lists scheduled items (DSH-020).
    pub upcoming_days: i64,
    /// Backup folder (SET-050, BAK-030); `None` = the Downloads folder.
    pub backup_folder: Option<String>,
    /// Retention (BAK-040): newest automatic backups kept …
    pub backup_keep_last: i64,
    /// … plus the newest one of each of this many months.
    pub backup_keep_months: i64,
    /// Minutes after the first change since the last backup, when a timed
    /// backup is made (SET-050); 0 = off.
    pub backup_timeout_minutes: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            date_format: DateFormat::Mdy,
            week_start: WeekStart::Sunday,
            startup: "dashboard".into(),
            integrity_at_startup: false,
            nav_items: None,
            account_panel_open: true,
            account_panel_side: PanelSide::Left,
            invest_views: None,
            stale_price_days: DEFAULT_STALE_DAYS,
            upcoming_days: 14,
            backup_folder: None,
            backup_keep_last: 10,
            backup_keep_months: 12,
            backup_timeout_minutes: 5,
        }
    }
}

pub const STALE_PRICE_DAYS: std::ops::RangeInclusive<i64> = 1..=365;
pub const UPCOMING_DAYS: std::ops::RangeInclusive<i64> = 1..=366;
pub const KEEP_LAST: std::ops::RangeInclusive<i64> = 1..=1000;
pub const KEEP_MONTHS: std::ops::RangeInclusive<i64> = 0..=120;
pub const TIMEOUT_MINUTES: std::ops::RangeInclusive<i64> = 0..=1440;

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
        invest_views: get_text(conn, "invest_views")?,
        stale_price_days: get_in(
            conn,
            "stale_price_days",
            STALE_PRICE_DAYS,
            d.stale_price_days,
        )?,
        upcoming_days: get_in(conn, "upcoming_days", UPCOMING_DAYS, d.upcoming_days)?,
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
    check_range("Backups to keep", s.backup_keep_last, KEEP_LAST)?;
    check_range("Months to keep", s.backup_keep_months, KEEP_MONTHS)?;
    check_range(
        "Minutes before a timed backup",
        s.backup_timeout_minutes,
        TIMEOUT_MINUTES,
    )?;
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
    put_text(tx, "invest_views", s.invest_views.as_deref())?;
    repo::set(tx, "stale_price_days", &s.stale_price_days.to_string())?;
    repo::set(tx, "upcoming_days", &s.upcoming_days.to_string())?;
    put_text(tx, "backup_folder", s.backup_folder.as_deref())?;
    repo::set(tx, "backup_keep_last", &s.backup_keep_last.to_string())?;
    repo::set(tx, "backup_keep_months", &s.backup_keep_months.to_string())?;
    repo::set(
        tx,
        "backup_timeout_minutes",
        &s.backup_timeout_minutes.to_string(),
    )?;
    Ok(())
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

/// The last backup and verification (BAK-080, DSH-030).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BackupStatus {
    pub last_at: Option<Timestamp>,
    pub last_path: Option<String>,
    /// Integrity problems in the last backup's snapshot.
    pub last_issues: i64,
    /// The last full decrypt-and-check (Verify backup…, restore drill).
    pub last_verified_at: Option<Timestamp>,
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

/// Forget the missing-folder warning (a new folder was chosen).
pub fn clear_folder_missing(tx: &Tx<'_>) -> Result<()> {
    repo::remove(tx, "backup.folder_missing")
}
