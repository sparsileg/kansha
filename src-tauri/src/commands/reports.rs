//! Reports, saved reports, tax lines, and what the Insights cards show
//! (RPT, CARD, CAT-050).

use std::path::PathBuf;

use kansha_core::Money;
use kansha_core::categories::TaxLine;
use kansha_core::persistence::reports as repo;
use kansha_core::reports::{
    self, CardData, Column, DateRange, PeriodChoice, Report, ReportFolder, ReportFolderId,
    ReportKind, ReportSettings, ResolvedRange, SavedReport, SavedReportId,
};
use tauri::{Manager, State};

use crate::state::{AppState, CmdResult, ErrorKind, IpcError};

/// A report's standard settings.
#[tauri::command]
#[specta::specta]
pub fn report_defaults(kind: ReportKind) -> ReportSettings {
    ReportSettings::defaults(kind)
}

/// The columns a report can show, for the Customize dialog.
#[tauri::command]
#[specta::specta]
pub fn report_columns(kind: ReportKind) -> Vec<Column> {
    reports::columns(kind)
}

/// A date range's dates today.
#[tauri::command]
#[specta::specta]
pub fn report_range(state: State<'_, AppState>, range: DateRange) -> CmdResult<ResolvedRange> {
    state.read(|db, today| reports::resolve(db.conn(), &range, today))
}

/// The periods offered for Monthly, Quarterly, or Yearly (RPT-040).
#[tauri::command]
#[specta::specta]
pub fn report_period_choices(
    state: State<'_, AppState>,
    range: DateRange,
) -> CmdResult<Vec<PeriodChoice>> {
    state.read(|_, today| reports::period_choices(&range, today))
}

#[tauri::command]
#[specta::specta]
pub fn report_run(state: State<'_, AppState>, settings: ReportSettings) -> CmdResult<Report> {
    state.read(|db, today| reports::run(db.conn(), &settings, today))
}

/// A file name from the report title: letters, digits, spaces, and
/// dashes only.
fn file_stem(title: &str) -> String {
    let clean: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                ' '
            }
        })
        .collect();
    let joined = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.is_empty() {
        "Report".into()
    } else {
        joined
    }
}

/// An internal error with a message.
pub(crate) fn internal(message: String) -> IpcError {
    IpcError {
        kind: ErrorKind::Internal,
        message,
    }
}

/// A new file in the Downloads folder (home if there is none), named
/// from the title and date. An existing file is never reused.
pub(crate) fn download_path(
    app: &tauri::AppHandle,
    title: &str,
    date: &str,
    ext: &str,
) -> CmdResult<PathBuf> {
    let dir: PathBuf = app
        .path()
        .download_dir()
        .or_else(|_| app.path().home_dir())
        .map_err(|e| internal(format!("no folder to save into: {e}")))?;
    let stem = format!("{} {date}", file_stem(title));
    let mut path = dir.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{stem} ({n}).{ext}"));
        n += 1;
    }
    Ok(path)
}

/// Write the report as CSV to the Downloads folder (RPT-050); returns
/// the file's path. An existing file is never overwritten.
#[tauri::command]
#[specta::specta]
pub fn report_export_csv(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    settings: ReportSettings,
) -> CmdResult<String> {
    let (report, today) =
        state.read(|db, today| Ok((reports::run(db.conn(), &settings, today)?, today)))?;
    let text = reports::to_csv(&report);
    let path = download_path(&app, &report.title, &today.to_string(), "csv")?;
    std::fs::write(&path, text)
        .map_err(|e| internal(format!("could not write {}: {e}", path.display())))?;
    Ok(path.display().to_string())
}

#[tauri::command]
#[specta::specta]
pub fn saved_report_list(state: State<'_, AppState>) -> CmdResult<Vec<SavedReport>> {
    state.read(|db, _| repo::saved_list(db.conn()))
}

#[tauri::command]
#[specta::specta]
pub fn saved_report_create(
    state: State<'_, AppState>,
    name: String,
    settings: ReportSettings,
) -> CmdResult<SavedReport> {
    state.write(|tx| repo::saved_insert(tx, &name, &settings))
}

#[tauri::command]
#[specta::specta]
pub fn saved_report_update(
    state: State<'_, AppState>,
    id: SavedReportId,
    name: String,
    settings: ReportSettings,
) -> CmdResult<SavedReport> {
    state.write(|tx| repo::saved_update(tx, id, &name, &settings))
}

#[tauri::command]
#[specta::specta]
pub fn saved_report_delete(state: State<'_, AppState>, id: SavedReportId) -> CmdResult<()> {
    state.write(|tx| repo::saved_delete(tx, id))
}

/// Move a saved report to another folder (RPT-020).
#[tauri::command]
#[specta::specta]
pub fn saved_report_move(
    state: State<'_, AppState>,
    id: SavedReportId,
    folder: ReportFolderId,
) -> CmdResult<SavedReport> {
    state.write(|tx| repo::saved_move(tx, id, folder))
}

/// Saved report folders, by name (RPT-020).
#[tauri::command]
#[specta::specta]
pub fn report_folder_list(state: State<'_, AppState>) -> CmdResult<Vec<ReportFolder>> {
    state.read(|db, _| repo::folder_list(db.conn()))
}

#[tauri::command]
#[specta::specta]
pub fn report_folder_create(state: State<'_, AppState>, name: String) -> CmdResult<ReportFolder> {
    state.write(|tx| repo::folder_insert(tx, &name))
}

#[tauri::command]
#[specta::specta]
pub fn report_folder_rename(
    state: State<'_, AppState>,
    id: ReportFolderId,
    name: String,
) -> CmdResult<ReportFolder> {
    state.write(|tx| repo::folder_rename(tx, id, &name))
}

/// Delete an empty folder; Unfiled stays.
#[tauri::command]
#[specta::specta]
pub fn report_folder_delete(state: State<'_, AppState>, id: ReportFolderId) -> CmdResult<()> {
    state.write(|tx| repo::folder_delete(tx, id))
}

/// Every tax line, in form and line order (CAT-050).
#[tauri::command]
#[specta::specta]
pub fn tax_line_list(state: State<'_, AppState>) -> CmdResult<Vec<TaxLine>> {
    state.read(|db, _| repo::tax_lines(db.conn()))
}

/// What the Insights cards show; scheduled items due within
/// `upcoming_days` (CARD-020).
#[tauri::command]
#[specta::specta]
pub fn card_data(state: State<'_, AppState>, upcoming_days: i64) -> CmdResult<CardData> {
    state.read(|db, today| reports::card_data(db.conn(), today, upcoming_days))
}

/// Net worth today, for the foot of the account list (ACCT-240).
#[tauri::command]
#[specta::specta]
pub fn net_worth(state: State<'_, AppState>) -> CmdResult<Money> {
    state.read(|db, today| reports::net_worth(db.conn(), today))
}

#[cfg(test)]
mod tests {
    use super::file_stem;

    #[test]
    fn file_names_are_safe() {
        assert_eq!(file_stem("Capital Gains"), "Capital Gains");
        assert_eq!(
            file_stem("Income/Expense by Category"),
            "Income Expense by Category"
        );
        assert_eq!(file_stem("../..:*?"), "Report");
    }
}
