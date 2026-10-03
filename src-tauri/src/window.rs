//! The main window's size and place. The window opens small for the
//! start screen (`tauri.conf.json`); the working size and place from
//! last time come back once the book is open (`window_restore`).

use kansha_core::local_config::WindowGeometry;

use crate::state::AppState;

/// Narrowest working window, in logical pixels; matches `FULL.minWidth`
/// in `src/lib/shell/windowsize.ts`. Anything narrower is the start
/// screen's compact window.
const WORKING_MIN_WIDTH: f64 = 900.0;

/// Whether a window this wide (physical pixels at `scale`) is the
/// working window rather than the compact start screen.
fn is_working_size(width: u32, scale: f64) -> bool {
    f64::from(width) / scale >= WORKING_MIN_WIDTH
}

/// Remember the main window's place for next time. The compact start
/// screen is not remembered: it would replace the working size.
pub fn save_geometry(window: &tauri::Window, state: &AppState) {
    let maximized = window.is_maximized().unwrap_or(false);
    let mut cfg = state.load_config();
    if maximized {
        // Keep the unmaximized size and place from before.
        if let Some(g) = cfg.window.as_mut() {
            g.maximized = true;
        }
    } else {
        let (Ok(pos), Ok(size)) = (window.outer_position(), window.inner_size()) else {
            return;
        };
        let scale = window.scale_factor().unwrap_or(1.0);
        if !is_working_size(size.width, scale) {
            return;
        }
        cfg.window = Some(WindowGeometry {
            x: pos.x,
            y: pos.y,
            width: size.width,
            height: size.height,
            maximized: false,
        });
    }
    if let Err(e) = state.save_config(&cfg) {
        eprintln!("could not save the window position: {}", e.message);
    }
}

/// Put the main window where it was last closed. False when nothing
/// was saved (the caller picks a default size).
pub fn restore_geometry(window: &tauri::Window, state: &AppState) -> bool {
    let Some(g) = state.load_config().window else {
        return false;
    };
    let _ = window.set_size(tauri::PhysicalSize::new(g.width, g.height));
    let _ = window.set_position(tauri::PhysicalPosition::new(g.x, g.y));
    if g.maximized {
        let _ = window.maximize();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_start_screen_is_not_the_working_size() {
        assert!(!is_working_size(520, 1.0));
        assert!(is_working_size(900, 1.0));
        assert!(is_working_size(1280, 1.0));
    }

    #[test]
    fn working_size_is_judged_in_logical_pixels() {
        // 1040 physical at 2× is a 520-wide compact window.
        assert!(!is_working_size(1040, 2.0));
        assert!(is_working_size(1800, 2.0));
        assert!(!is_working_size(1349, 1.5));
        assert!(is_working_size(1350, 1.5));
    }
}
