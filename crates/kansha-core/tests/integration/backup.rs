//! Backup, restore, first-run setup, and the restore drill (BAK-010 …
//! BAK-080, SECU-010 … SECU-090, §20.3).

use std::path::Path;

use kansha_core::accounts::{AccountFields, AccountType};
use kansha_core::backup::{self, BackupKind};
use kansha_core::book::{self, BookFiles, BookState};
use kansha_core::categories::SystemCategory;
use kansha_core::ledger::{self, Entry, Target};
use kansha_core::persistence::{accounts, categories};
use kansha_core::security::Passphrase;
use kansha_core::{Db, Error, Origin};

use crate::encryption::{fingerprint, sample_db};
use crate::fixture::{clock, date};

fn pass(s: &str) -> Passphrase {
    Passphrase::for_tests(s)
}

/// A new encrypted book in `dir` filled with the sample data.
fn sample_book(dir: &Path) -> (BookFiles, book::OpenBook) {
    let files = BookFiles::in_folder(dir);
    // Build the sample unencrypted, then convert: the same path setup
    // takes for the prototype database.
    drop(sample_db(Some(&files.db)));
    assert_eq!(book::state(&files).unwrap(), BookState::Unencrypted);
    let open = book::convert(&files, &pass("secret"), &clock()).unwrap();
    (files, open)
}

fn backup_now(open: &book::OpenBook, folder: &Path, kind: BackupKind) -> backup::Written {
    backup::write(
        &open.db,
        &open.key_file,
        folder,
        "kansha",
        kind,
        "0.1.0",
        &clock(),
    )
    .unwrap()
}

/// One more transaction in the first account.
fn add_txn(db: &mut Db) {
    let clock = clock();
    let first = accounts::list(db.conn()).unwrap()[0].id;
    let opening = categories::system(db.conn(), SystemCategory::OpeningBalance)
        .unwrap()
        .id;
    let amount = "12.34".parse().unwrap();
    let entry =
        Entry::new(first, date("2026-06-29"), amount).line(Target::Category(opening), amount);
    db.write(&clock, Origin::Ui, |tx| ledger::create_entry(tx, &entry))
        .unwrap();
}

#[test]
fn setup_creates_an_encrypted_empty_book() {
    let dir = tempfile::tempdir().unwrap();
    let files = BookFiles::in_folder(dir.path());
    assert_eq!(book::state(&files).unwrap(), BookState::New);
    let open = book::create(&files, &pass("p"), &clock()).unwrap();
    assert!(!open.db.needs_migration().unwrap());
    drop(open);
    assert_eq!(book::state(&files).unwrap(), BookState::Locked);
    assert!(
        !std::fs::read(&files.db)
            .unwrap()
            .starts_with(b"SQLite format 3")
    );
    assert_eq!(
        book::unlock(&files, &pass("wrong")).unwrap_err(),
        Error::WrongPassphrase
    );
    book::unlock(&files, &pass("p")).unwrap();
    // Setup never runs over an existing book.
    assert!(book::create(&files, &pass("p"), &clock()).is_err());
}

#[test]
fn convert_keeps_every_row_and_removes_the_plain_file() {
    let dir = tempfile::tempdir().unwrap();
    let files = BookFiles::in_folder(dir.path());
    let expected = fingerprint(&sample_db(Some(&files.db)));
    let open = book::convert(&files, &pass("secret"), &clock()).unwrap();
    assert_eq!(fingerprint(&open.db), expected);
    drop(open);
    assert_eq!(book::state(&files).unwrap(), BookState::Locked);
    let raw = std::fs::read(&files.db).unwrap();
    assert!(!raw.starts_with(b"SQLite format 3"));
    assert!(!String::from_utf8_lossy(&raw).contains("Later Parent"));
    let names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| !n.ends_with("-wal") && !n.ends_with("-shm"))
        .collect();
    let mut names = names;
    names.sort();
    assert_eq!(names, vec!["kansha.db", "kansha.key"]);
    let again = book::unlock(&files, &pass("secret")).unwrap();
    assert_eq!(fingerprint(&again.db), expected);
}

#[test]
fn key_missing_is_its_own_state() {
    let dir = tempfile::tempdir().unwrap();
    let (files, open) = sample_book(dir.path());
    drop(open);
    std::fs::remove_file(&files.key).unwrap();
    assert_eq!(book::state(&files).unwrap(), BookState::KeyMissing);
    assert!(matches!(
        book::unlock(&files, &pass("secret")),
        Err(Error::Io(_))
    ));
}

#[test]
fn backup_round_trips_and_needs_the_passphrase() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let (_files, open) = sample_book(dir.path());
    let w = backup_now(&open, out.path(), BackupKind::Manual);
    assert_eq!(w.integrity_issues, 0);
    assert_eq!(w.path.parent(), Some(out.path()));
    assert_eq!(
        backup::parse_file_name("kansha", &w.path.file_name().unwrap().to_string_lossy()),
        Some((clock_now(), BackupKind::Manual))
    );
    assert_eq!(backup::read_manifest(&w.path).unwrap(), w.manifest);

    // Nothing financial readable in the file.
    let raw = std::fs::read(&w.path).unwrap();
    let text = String::from_utf8_lossy(&raw);
    assert!(!text.contains("Later Parent"));
    assert!(!text.contains("SQLite format 3"));
    assert!(!text.contains("AGE-SECRET-KEY"));

    assert_eq!(
        backup::open(&w.path, &pass("nope"), &clock()).unwrap_err(),
        Error::WrongPassphrase
    );
    let opened = backup::open(&w.path, &pass("secret"), &clock()).unwrap();
    assert!(opened.integrity.is_clean());
    assert_eq!(fingerprint(&opened.db), fingerprint(&open.db));
}

fn clock_now() -> kansha_core::Timestamp {
    use kansha_core::Clock;
    clock().now()
}

#[test]
fn backups_never_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let (_files, open) = sample_book(dir.path());
    let a = backup_now(&open, out.path(), BackupKind::Close);
    let b = backup_now(&open, out.path(), BackupKind::Close);
    assert_ne!(a.path, b.path);
    assert!(a.path.exists() && b.path.exists());
}

#[test]
fn missing_backup_folder_is_refused_and_nothing_is_created() {
    let dir = tempfile::tempdir().unwrap();
    let (_files, open) = sample_book(dir.path());
    let missing = dir.path().join("gone");
    let err = backup::write(
        &open.db,
        &open.key_file,
        &missing,
        "kansha",
        BackupKind::Close,
        "0.1.0",
        &clock(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::Io(_)));
    assert!(!missing.exists());
}

#[test]
fn damaged_backup_is_detected_without_the_passphrase() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let (_files, open) = sample_book(dir.path());
    let w = backup_now(&open, out.path(), BackupKind::Manual);
    let mut raw = std::fs::read(&w.path).unwrap();
    // Flip a byte in the middle: inside the encrypted database entry.
    let mid = raw.len() / 2;
    raw[mid] ^= 0xff;
    std::fs::write(&w.path, &raw).unwrap();
    assert!(matches!(
        backup::read_manifest(&w.path),
        Err(Error::Invalid(_))
    ));
    std::fs::write(&w.path, b"not a zip").unwrap();
    assert!(matches!(
        backup::read_manifest(&w.path),
        Err(Error::Invalid(_))
    ));
}

/// The restore drill (§20.3, BAK-070): back up, change the book, compare,
/// restore, and get exactly the backed-up data under the backup's
/// passphrase.
#[test]
fn restore_drill() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let (files, mut open) = sample_book(dir.path());
    let w = backup_now(&open, out.path(), BackupKind::Manual);
    let backed_up = fingerprint(&open.db);

    // Newer data in the current book: one more transaction, a renamed
    // account, and a new account.
    add_txn(&mut open.db);
    let first = accounts::list(open.db.conn()).unwrap()[0].clone();
    let mut renamed = first.fields.clone();
    renamed.name = "Renamed".into();
    open.db
        .write(&clock(), Origin::Ui, |tx| {
            accounts::update(tx, first.id, &renamed)?;
            accounts::insert(tx, &AccountFields::new("Newer", AccountType::Savings))
        })
        .unwrap();

    let opened = backup::open(&w.path, &pass("secret"), &clock()).unwrap();
    let cmp = backup::compare(&opened.db, opened.manifest.created_at, Some(&open.db)).unwrap();
    assert_eq!(cmp.backup_created_at, w.manifest.created_at);
    let row = cmp.rows.iter().find(|r| r.account == first.id).unwrap();
    let (b, c) = (row.backup.as_ref().unwrap(), row.current.as_ref().unwrap());
    assert!(row.differs);
    assert_eq!(c.txns, b.txns + 1);
    assert_eq!(
        c.value,
        b.value.checked_add("12.34".parse().unwrap()).unwrap()
    );
    assert_eq!(c.name, "Renamed");
    let last = cmp.rows.last().unwrap();
    assert!(last.backup.is_none());
    assert_eq!(last.current.as_ref().unwrap().name, "Newer");
    let unchanged = cmp.rows.iter().filter(|r| !r.differs).count();
    assert_eq!(unchanged, cmp.rows.len() - 2);

    // Restore: back up the current book first, close it, install.
    let before = backup_now(&open, out.path(), BackupKind::Restore);
    let current = fingerprint(&open.db);
    drop(open);
    let restored = book::install_restore(&files, &opened).unwrap();
    assert_eq!(fingerprint(&restored.db), backed_up);
    drop(restored);
    let again = book::unlock(&files, &pass("secret")).unwrap();
    assert_eq!(fingerprint(&again.db), backed_up);
    drop(again);

    // The pre-restore backup holds the newer data.
    let undo = backup::open(&before.path, &pass("secret"), &clock()).unwrap();
    assert_eq!(fingerprint(&undo.db), current);
}

#[test]
fn restore_on_another_computer_with_no_book() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let (_files, open) = sample_book(dir.path());
    let w = backup_now(&open, out.path(), BackupKind::Close);
    let expected = fingerprint(&open.db);

    let elsewhere = tempfile::tempdir().unwrap();
    let files = BookFiles::in_folder(elsewhere.path());
    assert_eq!(book::state(&files).unwrap(), BookState::New);
    let opened = backup::open(&w.path, &pass("secret"), &clock()).unwrap();
    let cmp = backup::compare(&opened.db, opened.manifest.created_at, None).unwrap();
    assert!(cmp.rows.iter().all(|r| r.current.is_none() && r.differs));
    assert_eq!(cmp.current_last_change, None);
    let restored = book::install_restore(&files, &opened).unwrap();
    assert_eq!(fingerprint(&restored.db), expected);
}

#[test]
fn restore_gives_a_new_database_key() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let (files, open) = sample_book(dir.path());
    let old_key = book::show_key(&open.key_file, &pass("secret")).unwrap();
    let w = backup_now(&open, out.path(), BackupKind::Manual);
    drop(open);
    let opened = backup::open(&w.path, &pass("secret"), &clock()).unwrap();
    let restored = book::install_restore(&files, &opened).unwrap();
    let new_key = book::show_key(&restored.key_file, &pass("secret")).unwrap();
    assert_ne!(old_key, new_key);
    assert!(new_key.starts_with("0x") && new_key.len() == 66);
}

#[test]
fn change_passphrase_keeps_old_backups_on_the_old_one() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let (files, open) = sample_book(dir.path());
    let old_backup = backup_now(&open, out.path(), BackupKind::Manual);
    let changed =
        book::change_passphrase(&files, &open.key_file, &pass("secret"), &pass("new one")).unwrap();
    let reopened = book::OpenBook {
        db: open.db,
        key_file: changed,
    };
    let new_backup = backup_now(&reopened, out.path(), BackupKind::Manual);
    drop(reopened);

    assert_eq!(
        book::unlock(&files, &pass("secret")).unwrap_err(),
        Error::WrongPassphrase
    );
    book::unlock(&files, &pass("new one")).unwrap();
    backup::open(&old_backup.path, &pass("secret"), &clock()).unwrap();
    assert_eq!(
        backup::open(&old_backup.path, &pass("new one"), &clock()).unwrap_err(),
        Error::WrongPassphrase
    );
    backup::open(&new_backup.path, &pass("new one"), &clock()).unwrap();
}

#[test]
fn backup_integrity_problems_are_counted_not_fatal() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let (_files, open) = sample_book(dir.path());
    // An unbalanced transaction, as an external edit could leave.
    open.db
        .conn()
        .execute(
            "UPDATE posting SET amount = amount + 1 WHERE id = (SELECT min(id) FROM posting)",
            [],
        )
        .unwrap();
    let w = backup_now(&open, out.path(), BackupKind::Close);
    assert!(w.integrity_issues > 0);
    let opened = backup::open(&w.path, &pass("secret"), &clock()).unwrap();
    assert!(!opened.integrity.is_clean());
}

/// BAK-030, BAK-040: the whole routine. A missing chosen folder sends the
/// backup to Downloads, creates nothing at the missing path, and is
/// recorded for the dashboard; choosing a folder clears it. Retention
/// prunes old automatic backups only.
#[test]
fn back_up_falls_back_to_downloads_and_records_status() {
    use kansha_core::settings;
    let dir = tempfile::tempdir().unwrap();
    let downloads = tempfile::tempdir().unwrap();
    let chosen = tempfile::tempdir().unwrap();
    let (_files, mut open) = sample_book(dir.path());
    let gone = dir.path().join("usb-drive");
    let set_folder = |db: &mut Db, folder: Option<String>| {
        db.write(&clock(), Origin::Ui, |tx| {
            let mut s = settings::load(tx.conn())?;
            s.backup_folder = folder;
            s.backup_keep_last = 1;
            s.backup_keep_months = 0;
            settings::save(tx, &s)
        })
        .unwrap();
    };

    set_folder(&mut open.db, Some(gone.display().to_string()));
    let done = backup::back_up(
        &mut open.db,
        &open.key_file,
        "kansha",
        Some(downloads.path()),
        BackupKind::Close,
        "0.1.0",
        &clock(),
    )
    .unwrap();
    assert!(done.folder_missing);
    assert_eq!(done.written.path.parent(), Some(downloads.path()));
    assert!(!gone.exists(), "nothing created at the missing path");
    let status = settings::backup_status(open.db.conn()).unwrap();
    assert!(status.folder_missing);
    assert_eq!(status.last_at, Some(done.written.manifest.created_at));

    set_folder(&mut open.db, Some(chosen.path().display().to_string()));
    let manual = backup::back_up(
        &mut open.db,
        &open.key_file,
        "kansha",
        Some(downloads.path()),
        BackupKind::Manual,
        "0.1.0",
        &clock(),
    )
    .unwrap();
    let a = backup::back_up(
        &mut open.db,
        &open.key_file,
        "kansha",
        Some(downloads.path()),
        BackupKind::Close,
        "0.1.0",
        &clock(),
    )
    .unwrap();
    let b = backup::back_up(
        &mut open.db,
        &open.key_file,
        "kansha",
        Some(downloads.path()),
        BackupKind::Close,
        "0.1.0",
        &clock(),
    )
    .unwrap();
    assert!(!b.folder_missing);
    assert!(
        !settings::backup_status(open.db.conn())
            .unwrap()
            .folder_missing
    );
    // Keep 1 automatic: the older close backup is pruned, the manual one
    // stays.
    assert_eq!(b.pruned, 1);
    assert!(manual.written.path.exists());
    assert!(!a.written.path.exists());
    assert!(b.written.path.exists());

    // No folder and no Downloads: an error, nothing written.
    set_folder(&mut open.db, None);
    assert!(
        backup::back_up(
            &mut open.db,
            &open.key_file,
            "kansha",
            None,
            BackupKind::Close,
            "0.1.0",
            &clock()
        )
        .is_err()
    );
}

/// SET-050, BAK-040: a timeout backup is temporary. Only the newest is
/// kept, and it goes as soon as any backup of another kind is newer; the
/// backups made in the old name style still count.
#[test]
fn timeout_backups_are_temporary() {
    use kansha_core::{Clock, FixedClock, Timestamp};
    let dir = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let (_files, mut open) = sample_book(dir.path());
    let at = |h, m| {
        FixedClock::with_now(
            date("2026-06-30"),
            Timestamp::from_ymd_hms(2026, 6, 30, h, m, 0).unwrap(),
        )
    };
    let run = |db: &mut Db, key: &kansha_core::security::KeyFile, kind, c: &FixedClock| {
        backup::back_up(
            db,
            key,
            "kansha",
            Some(folder.path()),
            kind,
            "0.7.0",
            c as &dyn Clock,
        )
        .unwrap()
    };

    let first = run(
        &mut open.db,
        &open.key_file,
        BackupKind::Timeout,
        &at(10, 0),
    );
    let second = run(
        &mut open.db,
        &open.key_file,
        BackupKind::Timeout,
        &at(10, 5),
    );
    assert_eq!(second.pruned, 1, "the older timeout goes at once");
    assert!(!first.written.path.exists());
    assert!(second.written.path.exists());
    assert!(
        second
            .written
            .path
            .to_string_lossy()
            .ends_with("kansha-20260630-100500Z-timeout.zip")
    );

    let close = run(&mut open.db, &open.key_file, BackupKind::Close, &at(10, 30));
    assert_eq!(
        close.pruned, 1,
        "a close backup makes the timeout redundant"
    );
    assert!(!second.written.path.exists());
    assert!(close.written.path.exists());

    let third = run(
        &mut open.db,
        &open.key_file,
        BackupKind::Timeout,
        &at(11, 0),
    );
    let manual = run(
        &mut open.db,
        &open.key_file,
        BackupKind::Manual,
        &at(11, 30),
    );
    assert!(!third.written.path.exists(), "so does a manual one");
    assert!(manual.written.path.exists());
}

/// SET-050: the new setting is stored, defaults to 5, and is range-checked.
#[test]
fn timed_backup_delay_is_a_setting() {
    use kansha_core::settings;
    let dir = tempfile::tempdir().unwrap();
    let (_files, mut open) = sample_book(dir.path());
    assert_eq!(
        settings::load(open.db.conn())
            .unwrap()
            .backup_timeout_minutes,
        5
    );
    open.db
        .write(&clock(), Origin::Ui, |tx| {
            let mut s = settings::load(tx.conn())?;
            s.backup_timeout_minutes = 0;
            settings::save(tx, &s)
        })
        .unwrap();
    assert_eq!(
        settings::load(open.db.conn())
            .unwrap()
            .backup_timeout_minutes,
        0
    );
    let bad = open.db.write(&clock(), Origin::Ui, |tx| {
        let mut s = settings::load(tx.conn())?;
        s.backup_timeout_minutes = 1441;
        settings::save(tx, &s)
    });
    assert!(bad.is_err());
}

/// DSH-040: the dashboard's card choice is a stored setting; clearing it
/// returns to the default.
#[test]
fn dashboard_cards_is_a_setting() {
    use kansha_core::settings;
    let dir = tempfile::tempdir().unwrap();
    let (_files, mut open) = sample_book(dir.path());
    assert_eq!(
        settings::load(open.db.conn()).unwrap().dashboard_cards,
        None
    );
    let set = |db: &mut Db, v: Option<&str>| {
        db.write(&clock(), Origin::Ui, |tx| {
            let mut s = settings::load(tx.conn())?;
            s.dashboard_cards = v.map(String::from);
            settings::save(tx, &s)
        })
        .unwrap();
    };
    set(&mut open.db, Some(r#"{"order":["upcoming"],"hidden":[]}"#));
    assert_eq!(
        settings::load(open.db.conn()).unwrap().dashboard_cards,
        Some(r#"{"order":["upcoming"],"hidden":[]}"#.into())
    );
    set(&mut open.db, None);
    assert_eq!(
        settings::load(open.db.conn()).unwrap().dashboard_cards,
        None
    );
}

/// `path` with `suffix` appended to its name.
fn with_suffix(path: &Path, suffix: &str) -> std::path::PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    s.into()
}

#[test]
fn recover_finishes_a_swap_stopped_after_the_database_was_renamed() {
    // Book A is being replaced by book B's files; the crash came after
    // the database was renamed, before the key file was.
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let a = BookFiles::in_folder(dir_a.path());
    let b = BookFiles::in_folder(dir_b.path());
    drop(book::create(&a, &pass("old"), &clock()).unwrap());
    drop(book::create(&b, &pass("new"), &clock()).unwrap());
    std::fs::copy(&b.db, &a.db).unwrap();
    std::fs::copy(&b.key, with_suffix(&a.key, ".new")).unwrap();
    assert!(book::unlock(&a, &pass("old")).is_err());

    assert_eq!(book::recover(&a).unwrap(), book::Recovery::Finished);
    assert!(!with_suffix(&a.key, ".new").exists());
    book::unlock(&a, &pass("new")).unwrap();
    assert_eq!(book::recover(&a).unwrap(), book::Recovery::Nothing);
}

#[test]
fn recover_drops_staged_files_when_the_swap_never_started() {
    let dir = tempfile::tempdir().unwrap();
    let files = BookFiles::in_folder(dir.path());
    drop(book::create(&files, &pass("p"), &clock()).unwrap());
    let db_new = with_suffix(&files.db, ".new");
    let key_new = with_suffix(&files.key, ".new");

    // Both staged: the old book stays.
    std::fs::write(&db_new, b"staged database").unwrap();
    std::fs::write(&key_new, b"staged key").unwrap();
    assert_eq!(book::recover(&files).unwrap(), book::Recovery::Discarded);
    assert!(!db_new.exists() && !key_new.exists());
    book::unlock(&files, &pass("p")).unwrap();

    // Staging stopped after the database: likewise.
    std::fs::write(&db_new, b"staged database").unwrap();
    assert_eq!(book::recover(&files).unwrap(), book::Recovery::Discarded);
    assert!(!db_new.exists());
    book::unlock(&files, &pass("p")).unwrap();
}

#[test]
fn book_names_are_safe_file_names() {
    for good in [
        "kansha",
        "barton2026",
        "carol",
        "barton-2026",
        "a_b",
        "9lives",
    ] {
        book::validate_name(good).unwrap();
    }
    let long = "x".repeat(41);
    for bad in [
        "",
        "-x",
        "_x",
        "a b",
        "a.b",
        "a/b",
        "a:b",
        "ä",
        long.as_str(),
    ] {
        assert!(book::validate_name(bad).is_err(), "{bad:?}");
    }
    let dir = tempfile::tempdir().unwrap();
    let files = BookFiles::named(dir.path(), "carol").unwrap();
    assert_eq!(files.db, dir.path().join("carol.db"));
    assert_eq!(files.key, dir.path().join("carol.key"));
    assert_eq!(files.name(), "carol");
    assert!(BookFiles::named(dir.path(), "a b").is_err());
}

#[test]
fn a_new_book_never_takes_over_an_existing_key_file() {
    let dir = tempfile::tempdir().unwrap();
    let files = BookFiles::named(dir.path(), "carol").unwrap();
    std::fs::write(&files.key, b"someone's key").unwrap();
    assert!(book::create(&files, &pass("p"), &clock()).is_err());
    assert_eq!(std::fs::read(&files.key).unwrap(), b"someone's key");
}

#[test]
fn backups_carry_the_book_name() {
    let dir = tempfile::tempdir().unwrap();
    let backups = tempfile::tempdir().unwrap();
    let files = BookFiles::named(dir.path(), "carol").unwrap();
    let mut open = book::create(&files, &pass("p"), &clock()).unwrap();
    let done = backup::back_up(
        &mut open.db,
        &open.key_file,
        &files.name(),
        Some(backups.path()),
        BackupKind::Manual,
        "0.8.0",
        &clock(),
    )
    .unwrap();
    let name = done
        .written
        .path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(name.starts_with("carol-"), "{name}");
    assert_eq!(done.written.manifest.book.as_deref(), Some("carol"));
    assert_eq!(
        backup::read_manifest(&done.written.path)
            .unwrap()
            .book
            .as_deref(),
        Some("carol")
    );
    // A manifest from before books had names still reads.
    let old: backup::Manifest = serde_json::from_str(
        r#"{"format":1,"created_at":"2026-09-29T18:30:12Z","app_version":"0.7.0",
            "schema_version":4,"kind":"close"}"#,
    )
    .unwrap();
    assert_eq!(old.book, None);
}

#[test]
fn renaming_a_book_moves_both_files() {
    let dir = tempfile::tempdir().unwrap();
    let files = BookFiles::in_folder(dir.path());
    drop(book::create(&files, &pass("p"), &clock()).unwrap());
    let renamed = book::rename(&files, "barton2026").unwrap();
    assert_eq!(renamed, BookFiles::named(dir.path(), "barton2026").unwrap());
    assert!(!files.db.exists() && !files.key.exists());
    assert!(!dir.path().join("barton2026.rename").exists());
    book::unlock(&renamed, &pass("p")).unwrap();

    // Taken names and bad names are refused; nothing moves.
    let other = BookFiles::named(dir.path(), "carol").unwrap();
    drop(book::create(&other, &pass("q"), &clock()).unwrap());
    assert!(book::rename(&renamed, "carol").is_err());
    assert!(book::rename(&renamed, "bad name").is_err());
    assert!(book::rename(&renamed, "barton2026").is_err());
    book::unlock(&renamed, &pass("p")).unwrap();
}

#[test]
fn recover_finishes_an_interrupted_rename() {
    // The crash came after the marker and the database's rename.
    let dir = tempfile::tempdir().unwrap();
    let old = BookFiles::in_folder(dir.path());
    drop(book::create(&old, &pass("p"), &clock()).unwrap());
    let new = BookFiles::named(dir.path(), "barton2026").unwrap();
    std::fs::write(dir.path().join("barton2026.rename"), "kansha").unwrap();
    std::fs::rename(&old.db, &new.db).unwrap();

    assert_eq!(book::recover(&new).unwrap(), book::Recovery::Finished);
    assert!(!old.key.exists());
    assert!(!dir.path().join("barton2026.rename").exists());
    book::unlock(&new, &pass("p")).unwrap();

    // Before either file moved: both move.
    let dir = tempfile::tempdir().unwrap();
    let old = BookFiles::in_folder(dir.path());
    drop(book::create(&old, &pass("p"), &clock()).unwrap());
    let new = BookFiles::named(dir.path(), "carol").unwrap();
    std::fs::write(dir.path().join("carol.rename"), "kansha").unwrap();
    assert_eq!(book::recover(&new).unwrap(), book::Recovery::Finished);
    book::unlock(&new, &pass("p")).unwrap();
}
