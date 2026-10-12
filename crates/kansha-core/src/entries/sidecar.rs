//! Files beside a book for entry from outside the app: the inbox
//! folder (`<book>-inbox/`, CLI-030) and the plain-text names copy
//! (`<book>-names.txt`, CLI-080) that `kansha add` checks names against.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::format::{self, LineError, NamesFile};
use super::review::book_names;
use crate::book::BookFiles;
use crate::error::Result;
use crate::security::write_atomic;
use crate::settings;

/// `<book>-inbox/` beside the book.
pub fn inbox_folder(files: &BookFiles) -> PathBuf {
    files.folder().join(format!("{}-inbox", files.name()))
}

/// `<book>-names.txt` beside the book.
pub fn names_copy_path(files: &BookFiles) -> PathBuf {
    files.folder().join(format!("{}-names.txt", files.name()))
}

/// The book's names for phones and the command line (CLI-080): open
/// spending accounts and visible expense categories, the book's date
/// format; no payees.
pub fn names_file(conn: &Connection, book: &str) -> Result<NamesFile> {
    let names = book_names(conn)?.names;
    Ok(NamesFile {
        book: book.to_string(),
        date_format: Some(settings::load(conn)?.date_format),
        accounts: names.accounts,
        categories: names.categories,
    })
}

/// Write the names as `kansha-names 1` to `path`, replacing it whole.
pub fn write_names(conn: &Connection, book: &str, path: &Path) -> Result<()> {
    let text = format::write_names(&names_file(conn, book)?);
    write_atomic(path, text.as_bytes())
}

/// Write the names copy beside the book (on close and after each
/// export); returns its path.
pub fn write_names_copy(conn: &Connection, files: &BookFiles) -> Result<PathBuf> {
    let path = names_copy_path(files);
    write_names(conn, &files.name(), &path)?;
    Ok(path)
}

/// The names copy beside the book: `None` when there is none.
pub fn read_names_copy(
    files: &BookFiles,
) -> Option<std::result::Result<NamesFile, Vec<LineError>>> {
    let text = std::fs::read_to_string(names_copy_path(files)).ok()?;
    Some(format::parse_names(&text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::{AccountStatus, AccountType};
    use crate::categories::CategoryKind;
    use crate::date::Date;
    use crate::persistence::accounts;
    use crate::settings::DateFormat;
    use crate::testkit::Book;

    fn d(s: &str) -> Date {
        s.parse().unwrap()
    }

    #[test]
    fn paths_sit_beside_the_book() {
        let files = BookFiles::at(PathBuf::from("/x/barton2026.db"));
        assert_eq!(inbox_folder(&files), PathBuf::from("/x/barton2026-inbox"));
        assert_eq!(
            names_copy_path(&files),
            PathBuf::from("/x/barton2026-names.txt")
        );
    }

    #[test]
    fn lists_spending_accounts_and_expense_categories_only() {
        let mut book = Book::new(d("2026-10-12")).unwrap();
        book.account("Checking", AccountType::Checking).unwrap();
        book.account("Double Cash", AccountType::CreditCard)
            .unwrap();
        book.account("Brokerage", AccountType::Brokerage).unwrap();
        let old = book.account("Old Savings", AccountType::Savings).unwrap();
        book.write(|tx| accounts::close(tx, old, d("2026-01-01")))
            .unwrap();
        assert_eq!(
            accounts::get(book.conn(), old).unwrap().status,
            AccountStatus::Closed
        );
        book.category("Food:Groceries", CategoryKind::Expense)
            .unwrap();
        book.category("Salary", CategoryKind::Income).unwrap();
        book.payee("Kroger").unwrap();

        let n = names_file(book.conn(), "barton2026").unwrap();
        assert_eq!(n.book, "barton2026");
        assert_eq!(n.date_format, Some(DateFormat::Mdy));
        assert_eq!(n.accounts, ["Checking", "Double Cash"]);
        assert_eq!(n.categories, ["Food", "Food:Groceries"]);
        assert!(!format::write_names(&n).contains("Kroger"));
    }

    #[test]
    fn writes_and_reads_the_copy() {
        let dir = tempfile::tempdir().unwrap();
        let files = BookFiles::at(dir.path().join("b.db"));
        assert!(read_names_copy(&files).is_none());
        let mut book = Book::new(d("2026-10-12")).unwrap();
        book.account("Checking", AccountType::Checking).unwrap();
        let path = write_names_copy(book.conn(), &files).unwrap();
        assert!(path.ends_with("b-names.txt"));
        let back = read_names_copy(&files).unwrap().unwrap();
        assert_eq!(back.book, "b");
        assert_eq!(back.accounts, ["Checking"]);
    }
}
