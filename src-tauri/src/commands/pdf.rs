//! Saving the report page as a PDF (RPT-050). On Linux the page goes
//! through WebKitGTK's print operation with the orientation set in code:
//! WebKitGTK's own print dialog prints blank pages in landscape (WebKitGTK
//! 2.52), and CSS `@page { size }` is ignored. Paper printing is left to
//! the PDF viewer. On Windows, WebView2's PrintToPdf with the same
//! settings set in code.

use serde::Deserialize;
use specta::Type;
use tauri::State;

use super::reports::{download_path, internal};
use crate::state::{AppState, CmdResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PageOrientation {
    Portrait,
    Landscape,
}

/// Save the window's page as a PDF in the Downloads folder and open it
/// in the PDF viewer; returns the file's path.
#[tauri::command]
#[specta::specta]
pub async fn report_save_pdf(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    title: String,
    orientation: PageOrientation,
) -> CmdResult<String> {
    let path = download_path(&app, &title, &state.today().to_string(), "pdf")?;
    platform::save_pdf(&window, orientation, &path).await?;
    let shown = path.display().to_string();
    {
        use tauri_plugin_opener::OpenerExt;
        // The PDF is saved either way; a missing viewer is not an error.
        if let Err(e) = app.opener().open_path(shown.clone(), None::<&str>) {
            eprintln!("could not open {shown}: {e}");
        }
    }
    Ok(shown)
}

#[cfg(target_os = "linux")]
mod platform {
    use std::path::Path;
    use std::sync::mpsc;

    use webkit2gtk::PrintOperationExt;

    use super::{CmdResult, PageOrientation, internal};

    /// Print to GTK's file printer through WebKit's PrintOperation on the
    /// main thread, and wait for it to finish.
    pub async fn save_pdf(
        window: &tauri::WebviewWindow,
        orientation: PageOrientation,
        path: &Path,
    ) -> CmdResult<()> {
        let uri =
            url_from(path).ok_or_else(|| internal(format!("bad file name {}", path.display())))?;
        let (done, wait) = mpsc::channel::<Result<(), String>>();
        window
            .with_webview(move |wv| {
                let op = webkit2gtk::PrintOperation::new(&wv.inner());
                let o = match orientation {
                    PageOrientation::Portrait => gtk::PageOrientation::Portrait,
                    PageOrientation::Landscape => gtk::PageOrientation::Landscape,
                };
                let setup = gtk::PageSetup::new();
                setup.set_orientation(o);
                let settings = gtk::PrintSettings::new();
                settings.set_orientation(o);
                // GTK's file printer; its settings name the file.
                settings.set_printer("Print to File");
                settings.set(gtk::PRINT_SETTINGS_OUTPUT_FILE_FORMAT.as_str(), Some("pdf"));
                settings.set(gtk::PRINT_SETTINGS_OUTPUT_URI.as_str(), Some(&uri));
                op.set_page_setup(&setup);
                op.set_print_settings(&settings);
                let ok = done.clone();
                op.connect_finished(move |_| {
                    let _ = ok.send(Ok(()));
                });
                op.connect_failed(move |_, e| {
                    let _ = done.send(Err(e.to_string()));
                });
                op.print();
            })
            .map_err(|e| internal(format!("could not save the PDF: {e}")))?;
        tauri::async_runtime::spawn_blocking(move || wait.recv())
            .await
            .map_err(|e| internal(format!("could not save the PDF: {e}")))?
            .map_err(|_| internal("saving the PDF stopped without finishing".into()))?
            .map_err(|e| internal(format!("could not save the PDF: {e}")))
    }

    /// A `file://` URI with the path's characters escaped.
    fn url_from(path: &Path) -> Option<String> {
        let text = path.to_str()?;
        let mut out = String::from("file://");
        for b in text.bytes() {
            if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
                out.push(b as char);
            } else {
                out.push_str(&format!("%{b:02X}"));
            }
        }
        Some(out)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn uri_escapes_spaces_and_parens() {
            let u = url_from(Path::new("/home/a/Net Worth 2026-09-28 (2).pdf"));
            assert_eq!(
                u.as_deref(),
                Some("file:///home/a/Net%20Worth%202026-09-28%20%282%29.pdf")
            );
        }
    }
}

/// The one place Kansha uses `unsafe` (CONVENTIONS §3): every WebView2
/// COM method is an `unsafe fn` in `webview2-com`. Each call is on a
/// live interface Tauri hands over, on the main thread, with owned
/// arguments; nothing here dereferences a raw pointer.
#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
mod platform {
    use std::path::Path;
    use std::sync::mpsc;

    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PRINT_ORIENTATION_LANDSCAPE, COREWEBVIEW2_PRINT_ORIENTATION_PORTRAIT,
        ICoreWebView2_7, ICoreWebView2Environment6,
    };
    use webview2_com::PrintToPdfCompletedHandler;
    use windows_core::{HSTRING, Interface};

    use super::{CmdResult, PageOrientation, internal};

    /// Print to a file through WebView2's PrintToPdf on the main thread,
    /// and wait for it to finish.
    pub async fn save_pdf(
        window: &tauri::WebviewWindow,
        orientation: PageOrientation,
        path: &Path,
    ) -> CmdResult<()> {
        let file = HSTRING::from(path);
        let (done, wait) = mpsc::channel::<Result<(), String>>();
        window
            .with_webview(move |wv| {
                let fail = done.clone();
                if let Err(e) = start(&wv, orientation, &file, done) {
                    let _ = fail.send(Err(e.message()));
                }
            })
            .map_err(|e| internal(format!("could not save the PDF: {e}")))?;
        tauri::async_runtime::spawn_blocking(move || wait.recv())
            .await
            .map_err(|e| internal(format!("could not save the PDF: {e}")))?
            .map_err(|_| internal("saving the PDF stopped without finishing".into()))?
            .map_err(|e| internal(format!("could not save the PDF: {e}")))
    }

    /// Set up the print settings and start PrintToPdf; `done` hears how
    /// it ended.
    fn start(
        wv: &tauri::webview::PlatformWebview,
        orientation: PageOrientation,
        file: &HSTRING,
        done: mpsc::Sender<Result<(), String>>,
    ) -> windows_core::Result<()> {
        let o = match orientation {
            PageOrientation::Portrait => COREWEBVIEW2_PRINT_ORIENTATION_PORTRAIT,
            PageOrientation::Landscape => COREWEBVIEW2_PRINT_ORIENTATION_LANDSCAPE,
        };
        let env: ICoreWebView2Environment6 = wv.environment().cast()?;
        // SAFETY: COM calls on live interfaces from Tauri, main thread.
        let core: ICoreWebView2_7 = unsafe { wv.controller().CoreWebView2() }?.cast()?;
        // SAFETY: as above; the settings object is ours until PrintToPdf.
        let settings = unsafe { env.CreatePrintSettings() }?;
        // SAFETY: as above. Backgrounds on and no header or footer, as
        // WebKitGTK prints on Linux.
        unsafe {
            settings.SetOrientation(o)?;
            settings.SetShouldPrintBackgrounds(true)?;
            settings.SetShouldPrintHeaderAndFooter(false)?;
        }
        let handler = PrintToPdfCompletedHandler::create(Box::new(move |hr, ok| {
            let _ = done.send(match hr {
                Err(e) => Err(e.message()),
                Ok(()) if !ok => Err("WebView2 did not write the file".into()),
                Ok(()) => Ok(()),
            });
            Ok(())
        }));
        // SAFETY: as above; `file` outlives the call, which copies it.
        unsafe { core.PrintToPdf(file, &settings, &handler) }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
mod platform {
    use std::path::Path;

    use super::{CmdResult, PageOrientation, internal};

    /// Not built yet on macOS.
    pub async fn save_pdf(
        _window: &tauri::WebviewWindow,
        _orientation: PageOrientation,
        _path: &Path,
    ) -> CmdResult<()> {
        Err(internal(
            "Save PDF works on Linux and Windows only for now".into(),
        ))
    }
}
