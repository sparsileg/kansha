//! Per-computer preferences (SET-010, SET-020, SET-025, SET-070): theme,
//! font, font size, window sizes, and recent books. Kept in a JSON file in the OS
//! configuration folder, not in the book: the passphrase screen needs the
//! theme before any book is open.
//!
//! A missing or unreadable file gives the defaults; preferences are
//! never worth refusing to start over.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::security::write_atomic;

/// Base font size range in px (SET-020).
pub const FONT_SIZES: std::ops::RangeInclusive<i64> = 10..=24;

/// Recent books remembered.
const RECENT_MAX: usize = 8;

/// What the main window shows: the start page (the passphrase screen)
/// or the working window (a book open, setup, restore). Each has its
/// own size (SET-070).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum WindowMode {
    Start,
    Working,
}

impl WindowMode {
    /// The size before the user has chosen one on a screen.
    pub fn default_size(self) -> WindowSize {
        let (width, height) = match self {
            WindowMode::Start => (520, 760),
            WindowMode::Working => (1280, 800),
        };
        WindowSize {
            width,
            height,
            maximized: false,
        }
    }

    /// The smallest the window may be made, width and height.
    pub fn min_size(self) -> (u32, u32) {
        match self {
            WindowMode::Start => (460, 600),
            WindowMode::Working => (900, 600),
        }
    }
}

/// A main window size, in logical pixels (the same on screens of any
/// scale).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowSize {
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

impl WindowSize {
    /// No larger than the screen's free area (`area`, width and
    /// height, when known), and no smaller than `mode`'s minimum unless
    /// the screen is.
    pub fn fit(self, mode: WindowMode, area: Option<(u32, u32)>) -> WindowSize {
        let (min_w, min_h) = mode.min_size();
        let (mut width, mut height) = (self.width.max(min_w), self.height.max(min_h));
        if let Some((w, h)) = area {
            width = width.min(w);
            height = height.min(h);
        }
        WindowSize {
            width,
            height,
            ..self
        }
    }
}

/// The two sizes kept for one screen resolution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScreenWindows {
    pub start: Option<WindowSize>,
    pub working: Option<WindowSize>,
}

impl ScreenWindows {
    fn get(&mut self, mode: WindowMode) -> &mut Option<WindowSize> {
        match mode {
            WindowMode::Start => &mut self.start,
            WindowMode::Working => &mut self.working,
        }
    }
}

/// The key a screen's sizes are kept under: its resolution in logical
/// pixels, e.g. "2560x1440". Screens alike share their sizes.
pub fn screen_key(width: u32, height: u32) -> String {
    format!("{width}x{height}")
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalConfig {
    /// Theme name; `None` follows the OS light/dark preference. The UI
    /// owns the list of themes.
    pub theme: Option<String>,
    /// Font name; `None` = the default (System). The UI owns the list.
    pub font: Option<String>,
    /// Base font size in px; `None` = the default.
    pub font_size: Option<i64>,
    /// Main window sizes by screen (`screen_key`). A file from before
    /// 0.7.41 kept one place and size in physical pixels (`window`);
    /// that is ignored.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub window_sizes: BTreeMap<String, ScreenWindows>,
    /// Database paths, most recent first.
    pub recent_books: Vec<String>,
}

impl LocalConfig {
    /// Read the file; defaults when it is missing or unreadable.
    pub fn load(path: &Path) -> LocalConfig {
        let mut cfg: LocalConfig = std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        if cfg.font_size.is_some_and(|s| !FONT_SIZES.contains(&s)) {
            cfg.font_size = None;
        }
        cfg
    }

    /// Write the file (creating its folder).
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        write_atomic(path, serde_json::to_string_pretty(self)?.as_bytes())
    }

    /// The size saved for `mode` on `screen`, if any.
    pub fn window_size(&self, screen: &str, mode: WindowMode) -> Option<WindowSize> {
        let sizes = self.window_sizes.get(screen)?;
        match mode {
            WindowMode::Start => sizes.start,
            WindowMode::Working => sizes.working,
        }
    }

    /// Remember the window's size for `mode` on `screen`. A maximized
    /// window keeps the size from before, to come back to.
    pub fn note_window(
        &mut self,
        screen: &str,
        mode: WindowMode,
        width: u32,
        height: u32,
        maximized: bool,
    ) {
        let slot = self
            .window_sizes
            .entry(screen.to_owned())
            .or_default()
            .get(mode);
        *slot = Some(if maximized {
            WindowSize {
                maximized: true,
                ..slot.unwrap_or(mode.default_size())
            }
        } else {
            WindowSize {
                width,
                height,
                maximized: false,
            }
        });
    }

    /// Put `book` first in the recent list.
    pub fn touch_book(&mut self, book: &str) {
        self.recent_books.retain(|b| b != book);
        self.recent_books.insert(0, book.to_owned());
        self.recent_books.truncate(RECENT_MAX);
    }

    /// Take `book` off the recent list (renamed, or chosen to forget).
    pub fn forget_book(&mut self, book: &str) {
        self.recent_books.retain(|b| b != book);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_damaged_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.json");
        assert_eq!(LocalConfig::load(&p), LocalConfig::default());
        std::fs::write(&p, "{ nope").unwrap();
        assert_eq!(LocalConfig::load(&p), LocalConfig::default());
    }

    #[test]
    fn round_trips_and_drops_bad_font_size() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sub").join("config.json");
        let mut cfg = LocalConfig {
            theme: Some("classic".into()),
            font: Some("courier".into()),
            font_size: Some(16),
            window_sizes: BTreeMap::new(),
            recent_books: vec![],
        };
        cfg.note_window("1920x1080", WindowMode::Working, 1200, 800, false);
        cfg.note_window("1920x1080", WindowMode::Start, 600, 700, false);
        cfg.touch_book("/a/kansha.db");
        cfg.save(&p).unwrap();
        assert_eq!(LocalConfig::load(&p), cfg);
        std::fs::write(&p, r#"{"font_size": 99, "theme": "dark"}"#).unwrap();
        let back = LocalConfig::load(&p);
        assert_eq!(back.font_size, None);
        assert_eq!(back.theme.as_deref(), Some("dark"));
    }

    #[test]
    fn a_config_from_before_fonts_loads_without_one() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.json");
        std::fs::write(&p, r#"{"font_size": 16, "theme": "matrix"}"#).unwrap();
        let cfg = LocalConfig::load(&p);
        assert_eq!(cfg.font, None);
        assert_eq!(cfg.font_size, Some(16));
        assert_eq!(cfg.theme.as_deref(), Some("matrix"));
    }

    #[test]
    fn each_screen_keeps_a_start_and_a_working_size() {
        let mut cfg = LocalConfig::default();
        cfg.note_window("1920x1080", WindowMode::Start, 500, 700, false);
        cfg.note_window("1920x1080", WindowMode::Working, 1400, 900, false);
        cfg.note_window("2560x1440", WindowMode::Working, 2000, 1300, false);
        let size = |w, h| WindowSize {
            width: w,
            height: h,
            maximized: false,
        };
        assert_eq!(
            cfg.window_size("1920x1080", WindowMode::Start),
            Some(size(500, 700))
        );
        assert_eq!(
            cfg.window_size("1920x1080", WindowMode::Working),
            Some(size(1400, 900))
        );
        assert_eq!(
            cfg.window_size("2560x1440", WindowMode::Working),
            Some(size(2000, 1300))
        );
        assert_eq!(cfg.window_size("2560x1440", WindowMode::Start), None);
        assert_eq!(cfg.window_size("1280x800", WindowMode::Working), None);
        // A later size replaces the earlier one.
        cfg.note_window("1920x1080", WindowMode::Start, 480, 650, false);
        assert_eq!(
            cfg.window_size("1920x1080", WindowMode::Start),
            Some(size(480, 650))
        );
    }

    #[test]
    fn a_maximized_window_keeps_its_size_from_before() {
        let mut cfg = LocalConfig::default();
        cfg.note_window("s", WindowMode::Working, 1400, 900, false);
        cfg.note_window("s", WindowMode::Working, 1920, 1050, true);
        let back = cfg.window_size("s", WindowMode::Working).unwrap();
        assert_eq!((back.width, back.height, back.maximized), (1400, 900, true));
        cfg.note_window("s", WindowMode::Working, 1300, 850, false);
        let back = cfg.window_size("s", WindowMode::Working).unwrap();
        assert_eq!(
            (back.width, back.height, back.maximized),
            (1300, 850, false)
        );
        // Maximized before any size: the default to come back to.
        cfg.note_window("s", WindowMode::Start, 1920, 1050, true);
        let back = cfg.window_size("s", WindowMode::Start).unwrap();
        assert_eq!((back.width, back.height, back.maximized), (520, 760, true));
    }

    #[test]
    fn a_size_fits_the_screen_and_the_minimum() {
        let s = |w, h| WindowSize {
            width: w,
            height: h,
            maximized: false,
        };
        // Taller than a smaller screen: cut to its free area.
        assert_eq!(
            s(1572, 1931).fit(WindowMode::Working, Some((1920, 1040))),
            s(1572, 1040)
        );
        // Under the minimum: raised to it.
        assert_eq!(
            s(300, 400).fit(WindowMode::Start, Some((1920, 1040))),
            s(460, 600)
        );
        assert_eq!(s(800, 700).fit(WindowMode::Working, None), s(900, 700));
        // A screen smaller than the minimum wins.
        assert_eq!(
            s(1280, 800).fit(WindowMode::Working, Some((800, 500))),
            s(800, 500)
        );
    }

    #[test]
    fn a_config_with_the_old_window_place_loads() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.json");
        std::fs::write(
            &p,
            r#"{"theme": "nordic", "window": {"x": 0, "y": 0, "width": 1572, "height": 1931, "maximized": false}}"#,
        )
        .unwrap();
        let cfg = LocalConfig::load(&p);
        assert_eq!(cfg.theme.as_deref(), Some("nordic"));
        assert!(cfg.window_sizes.is_empty());
    }

    #[test]
    fn recent_books_most_recent_first_without_duplicates() {
        let mut cfg = LocalConfig::default();
        cfg.touch_book("a");
        cfg.touch_book("b");
        cfg.touch_book("a");
        assert_eq!(cfg.recent_books, vec!["a", "b"]);
        for i in 0..20 {
            cfg.touch_book(&i.to_string());
        }
        assert_eq!(cfg.recent_books.len(), RECENT_MAX);
    }

    #[test]
    fn a_forgotten_book_leaves_the_recent_list() {
        let mut cfg = LocalConfig::default();
        cfg.touch_book("a");
        cfg.touch_book("b");
        cfg.forget_book("a");
        cfg.forget_book("missing");
        assert_eq!(cfg.recent_books, vec!["b"]);
    }
}
