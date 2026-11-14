//! Open dropdown lists in the theme (NFR-080, #18). On Linux, WebKitGTK
//! draws a `<select>`'s open list as a GTK popup (a window holding a tree
//! view) in the system's GTK colors and font size, ignoring the page's
//! CSS (checked in WebKitGTK 2.52). A GTK style
//! provider on the screen gives that popup the theme's option colors and
//! the app's font size; the UI sends them whenever the theme, font, or
//! size changes. Elsewhere the page's CSS already reaches the list, and
//! this does nothing.

use serde::Deserialize;
use specta::Type;

use crate::state::{CmdResult, IpcError};

/// The theme's colors for an open dropdown list, as CSS colors
/// (`#rrggbb` or `rgb()`/`rgba()`), and the app's font size in pixels.
#[derive(Debug, Clone, Deserialize, Type)]
pub struct PopupColors {
    pub bg: String,
    pub fg: String,
    pub sel_bg: String,
    pub sel_fg: String,
    pub font_px: u32,
}

/// A color the GTK style sheet may take: only `#`, letters, digits,
/// and `( ) , . %` and spaces, so nothing else can reach the CSS.
fn color(c: &str) -> CmdResult<&str> {
    let c = c.trim();
    let ok = !c.is_empty()
        && c.len() <= 40
        && c.chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "#(),.% ".contains(ch));
    if ok {
        Ok(c)
    } else {
        Err(IpcError::invalid(format!("not a color: {c:?}")))
    }
}

/// The GTK style sheet for WebKitGTK's dropdown popup.
fn style_sheet(p: &PopupColors) -> CmdResult<String> {
    let (bg, fg, sbg, sfg) = (
        color(&p.bg)?,
        color(&p.fg)?,
        color(&p.sel_bg)?,
        color(&p.sel_fg)?,
    );
    let px = p.font_px.clamp(8, 64);
    // The font size goes on every GTK tree view: WebKit measures the
    // popup's rows before the popup sits under the web view, so a size
    // set only there would leave the list too short and scrolling.
    Ok(format!(
        "treeview.view {{ font-size: {px}px; }}\n\
         webkitwebview window treeview.view {{ background-color: {bg}; color: {fg}; }}\n\
         webkitwebview window treeview.view:selected {{ background-color: {sbg}; color: {sfg}; }}\n\
         webkitwebview window scrolledwindow {{ border-color: {fg}; }}\n"
    ))
}

/// Give open dropdown lists the theme's colors (Linux; a no-op
/// elsewhere).
#[tauri::command]
#[specta::specta]
pub fn popup_colors(window: tauri::WebviewWindow, colors: PopupColors) -> CmdResult<()> {
    let css = style_sheet(&colors)?;
    platform::apply(&window, css)
}

#[cfg(target_os = "linux")]
mod platform {
    use std::cell::OnceCell;

    use gtk::prelude::CssProviderExt;

    use crate::state::{CmdResult, IpcError};

    thread_local! {
        /// One provider, added to the screen once and reloaded on each
        /// change; GTK lives on the main thread.
        static PROVIDER: OnceCell<gtk::CssProvider> = const { OnceCell::new() };
    }

    pub fn apply(window: &tauri::WebviewWindow, css: String) -> CmdResult<()> {
        window
            .run_on_main_thread(move || {
                PROVIDER.with(|cell| {
                    let provider = cell.get_or_init(|| {
                        let p = gtk::CssProvider::new();
                        if let Some(screen) = gtk::gdk::Screen::default() {
                            gtk::StyleContext::add_provider_for_screen(
                                &screen,
                                &p,
                                gtk::STYLE_PROVIDER_PRIORITY_USER + 1,
                            );
                        }
                        p
                    });
                    if let Err(e) = provider.load_from_data(css.as_bytes()) {
                        eprintln!("dropdown colors not applied: {e}");
                    }
                });
            })
            .map_err(|e| IpcError::internal(format!("dropdown colors: {e}")))
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use crate::state::CmdResult;

    pub fn apply(_window: &tauri::WebviewWindow, _css: String) -> CmdResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colors(bg: &str) -> PopupColors {
        PopupColors {
            bg: bg.into(),
            fg: "#00ff00".into(),
            sel_bg: "rgba(0, 255, 0, 0.25)".into(),
            sel_fg: "#000000".into(),
            font_px: 20,
        }
    }

    #[test]
    fn the_style_sheet_holds_the_theme_colors_and_size() {
        let css = style_sheet(&colors(" #000000 ")).unwrap();
        assert!(css.starts_with("treeview.view { font-size: 20px; }"));
        assert!(
            css.contains("window treeview.view { background-color: #000000; color: #00ff00; }")
        );
        assert!(
            css.contains(":selected { background-color: rgba(0, 255, 0, 0.25); color: #000000; }")
        );
    }

    #[test]
    fn anything_but_a_color_is_refused() {
        for bad in ["", "red; } * { color: red", "url(x)\"", "#000\n}"] {
            assert!(style_sheet(&colors(bad)).is_err(), "{bad:?}");
        }
        let mut huge = colors("#000");
        huge.font_px = 900;
        assert!(style_sheet(&huge).unwrap().contains("font-size: 64px"));
    }
}
