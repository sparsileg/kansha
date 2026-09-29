//! Per-computer preferences (SET-010, SET-020, SET-025, SET-070): theme,
//! font, font size, window geometry, and recent books. Kept in a JSON file in the OS
//! configuration folder, not in the book: the passphrase screen needs the
//! theme before any book is open.
//!
//! A missing or unreadable file gives the defaults; preferences are
//! never worth refusing to start over.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::security::write_atomic;

/// Base font size range in px (SET-020).
pub const FONT_SIZES: std::ops::RangeInclusive<i64> = 10..=24;

/// Recent books remembered.
const RECENT_MAX: usize = 8;

/// The main window's last position and size, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowGeometry>,
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

    /// Put `book` first in the recent list.
    pub fn touch_book(&mut self, book: &str) {
        self.recent_books.retain(|b| b != book);
        self.recent_books.insert(0, book.to_owned());
        self.recent_books.truncate(RECENT_MAX);
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
            window: Some(WindowGeometry {
                x: 10,
                y: 20,
                width: 1200,
                height: 800,
                maximized: false,
            }),
            recent_books: vec![],
        };
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
}
