//! The main window's size (SET-070). It has two: the start page's and
//! the working window's (a book open, setup, restore), each kept for
//! every screen resolution it has been used on, in logical pixels. A
//! size is saved half a second after the user stops resizing, and on
//! close. The window's place is not kept: Wayland neither tells nor
//! lets an app set it, so the window opens centred where it can.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use kansha_core::local_config::{WindowMode, screen_key};
use tauri::{LogicalSize, Manager};

use crate::state::AppState;

/// How long resizing must stop before the size is saved.
const SETTLE: Duration = Duration::from_millis(500);

/// What the main window shows, and a pending save.
#[derive(Default)]
pub struct WindowTracker {
    /// `None` until the start page or a book first sets it; resizes
    /// before then are not saved.
    mode: Mutex<Option<WindowMode>>,
    /// When a pending save is due; `None` when none is.
    due: Mutex<Option<Instant>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// The window's screen: its key and free area (logical pixels).
fn screen(window: &tauri::Window) -> Option<(String, (u32, u32))> {
    let m = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())?;
    let scale = m.scale_factor();
    let size = m.size().to_logical::<u32>(scale);
    let area = m.work_area().size.to_logical::<u32>(scale);
    Some((
        screen_key(size.width, size.height),
        (area.width, area.height),
    ))
}

/// Save the window's size now for what it shows. Holds the mode while
/// saving, so a switch cannot land between reading the size and
/// writing it.
fn save_now(window: &tauri::Window, state: &AppState) {
    let mode = lock(&state.window.mode);
    let Some(mode) = *mode else { return };
    let (Some((key, _)), Ok(size), Ok(scale)) =
        (screen(window), window.inner_size(), window.scale_factor())
    else {
        return;
    };
    let size = size.to_logical::<u32>(scale);
    let maximized = window.is_maximized().unwrap_or(false);
    let mut cfg = state.load_config();
    cfg.note_window(&key, mode, size.width, size.height, maximized);
    if let Err(e) = state.save_config(&cfg) {
        eprintln!("could not save the window size: {}", e.message);
    }
}

/// Drop a pending save.
fn cancel(state: &AppState) {
    *lock(&state.window.due) = None;
}

/// The window was resized: save its size once resizing stops.
pub fn resized(window: &tauri::Window, state: &AppState) {
    if lock(&state.window.mode).is_none() {
        return;
    }
    let mut due = lock(&state.window.due);
    let waiting = due.is_some();
    *due = Some(Instant::now() + SETTLE);
    if waiting {
        return;
    }
    drop(due);
    let window = window.clone();
    std::thread::spawn(move || {
        loop {
            let state = window.state::<AppState>();
            let wait = {
                let mut due = lock(&state.window.due);
                let Some(at) = *due else { return };
                let now = Instant::now();
                if now >= at {
                    *due = None;
                    None
                } else {
                    Some(at - now)
                }
            };
            match wait {
                Some(d) => std::thread::sleep(d),
                None => return save_now(&window, &state),
            }
        }
    });
}

/// The window is closing: save its size now.
pub fn closing(window: &tauri::Window, state: &AppState) {
    cancel(state);
    save_now(window, state);
}

/// Show the start page or the working window: keep the size of what
/// was showing, then take the size saved for `mode` on this screen
/// (else the default), fitted to the screen. Nothing happens when
/// `mode` is showing already.
pub fn set_mode(window: &tauri::Window, state: &AppState, mode: WindowMode) {
    {
        let shown = *lock(&state.window.mode);
        if shown == Some(mode) {
            return;
        }
        cancel(state);
        save_now(window, state);
        *lock(&state.window.mode) = Some(mode);
    }
    let (key, area) = screen(window).unzip();
    let size = key
        .and_then(|k| state.load_config().window_size(&k, mode))
        .unwrap_or(mode.default_size())
        .fit(mode, area);
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
    }
    let (min_w, min_h) = mode.min_size();
    let _ = window.set_min_size(Some(LogicalSize::new(min_w, min_h)));
    let _ = window.set_size(LogicalSize::new(size.width, size.height));
    let _ = window.center();
    if size.maximized {
        let _ = window.maximize();
    }
}
