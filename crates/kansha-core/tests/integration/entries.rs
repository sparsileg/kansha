//! Entries from the command line and files (CLI-040): received whole or
//! not at all, ids seen once, reviewed outside the ledger, approved into
//! ordinary transactions with the right signs, rejected, and undone.

use kansha_core::accounts::{AccountId, AccountType};
use kansha_core::categories::{CategoryId, CategoryKind};
use kansha_core::entries::format::{Dates, parse_entries};
use kansha_core::entries::review::{self, EntryEdit, Received};
use kansha_core::ledger::{Target, TxnSource};
use kansha_core::persistence::audit::{self, AuditAction, AuditEntity};
use kansha_core::persistence::entries::{self as repo, EntrySource, PendingStatus};
use kansha_core::testkit::Book;
use kansha_core::undo::{self, Undo};
use kansha_core::{Money, Origin};

use crate::fixture::date;

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

struct Fx {
    book: Book,
    chk: AccountId,
    card: AccountId,
    groceries: CategoryId,
    household: CategoryId,
    tax: CategoryId,
}

fn fx() -> Fx {
    let mut book = Book::new(date("2026-10-12")).unwrap();
    let chk = book.account("Checking", AccountType::Checking).unwrap();
    let card = book
        .account("Double Cash", AccountType::CreditCard)
        .unwrap();
    book.account("Brokerage", AccountType::Brokerage).unwrap();
    let groceries = book
        .category("Food:Groceries", CategoryKind::Expense)
        .unwrap();
    let household = book.category("Household", CategoryKind::Expense).unwrap();
    let tax = book.category("Taxes:Sales", CategoryKind::Expense).unwrap();
    book.category("Salary", CategoryKind::Income).unwrap();
    Fx {
        book,
        chk,
        card,
        groceries,
        household,
        tax,
    }
}

const ID1: &str = "1b4e28ba-2fa1-4d2a-9c3e-0f6a8b7c5d21";
const ID2: &str = "2b4e28ba-2fa1-4d2a-9c3e-0f6a8b7c5d22";

fn sample() -> String {
    format!(
        "kansha-entries 1
from \"Pat's phone\"
2026-10-12 Kroger 45.20 groceries acct checking memo \"week\" id {ID1}
2026-10-12 Target 107.00 split acct \"Double Cash\" id {ID2}
  7.00 Taxes:Sales
  60.00 Groceries memo \"food\"
  40.00 Household
"
    )
}

fn receive(book: &mut Book, text: &str) -> kansha_core::Result<Received> {
    let file = parse_entries(text, Dates::Iso).unwrap();
    let clock = *book.clock();
    book.db_mut().write(&clock, Origin::System, |tx| {
        review::receive(
            tx,
            "entries.txt",
            text.as_bytes(),
            &file,
            EntrySource::File,
            None,
        )
    })
}

fn approve(book: &mut Book, ids: &[i64]) -> kansha_core::Result<(review::Approved, Undo)> {
    book.write(|tx| review::approve(tx, ids))
        .map(|(a, u)| (a, u.expect("undoable")))
}

fn pending_ids(book: &Book) -> Vec<i64> {
    review::pending(book.conn())
        .unwrap()
        .iter()
        .map(|v| v.id)
        .collect()
}

fn txn_count(book: &Book) -> i64 {
    book.conn()
        .query_row("SELECT count(*) FROM txn", [], |r| r.get(0))
        .unwrap()
}

fn clean(book: &Book) {
    let r = kansha_core::integrity::check(book.conn()).unwrap();
    assert!(r.is_clean(), "{r:?}");
}

#[test]
fn receiving_twice_adds_once() {
    let mut f = fx();
    let first = receive(&mut f.book, &sample()).unwrap();
    assert_eq!((first.added, first.skipped), (2, 0));
    let again = receive(&mut f.book, &sample()).unwrap();
    assert_eq!((again.added, again.skipped), (0, 2));
    assert_eq!(review::pending_count(f.book.conn()).unwrap(), 2);
    // Nothing reached the ledger.
    assert_eq!(txn_count(&f.book), 0);
}

#[test]
fn entries_without_ids_get_one_each() {
    let mut f = fx();
    let text = "kansha-entries 1\n2026-10-12 Shell 1.00 Household acct Checking\n";
    receive(&mut f.book, text).unwrap();
    receive(&mut f.book, text).unwrap();
    let all = repo::pending(f.book.conn()).unwrap();
    assert_eq!(all.len(), 2);
    assert_ne!(all[0].uid, all[1].uid);
}

#[test]
fn a_bad_file_adds_nothing() {
    let text = format!("{}2026-10-12 Shell ten Household acct Checking\n", sample());
    let refused = review::parse_file("bad.txt", text.as_bytes()).unwrap_err();
    assert_eq!(refused.errors.len(), 1);
    assert_eq!(refused.errors[0].line, 8);
    let not_text = review::parse_file("bin.txt", &[0xff, 0xfe, 0x00]).unwrap_err();
    assert_eq!(not_text.errors[0].message, "the file is not UTF-8 text");
}

#[test]
fn review_checks_names_against_the_book() {
    let mut f = fx();
    receive(
        &mut f.book,
        "kansha-entries 1
2026-10-12 Shell 1.00 Grocries acct Brokerage
2026-10-12 Shell 2.00 Salary acct Checking
",
    )
    .unwrap();
    let v = review::pending(f.book.conn()).unwrap();
    assert!(!v[0].ready);
    assert_eq!(
        v[0].account.error.as_deref(),
        Some("no account “Brokerage”")
    );
    assert_eq!(
        v[0].category.as_ref().unwrap().error.as_deref(),
        Some("no category “Grocries”; did you mean Food:Groceries?")
    );
    // Income categories are not offered.
    assert!(v[1].category.as_ref().unwrap().resolved.is_none());
}

#[test]
fn approve_makes_ordinary_transactions_with_the_right_signs() {
    let mut f = fx();
    receive(&mut f.book, &sample()).unwrap();
    let ids = pending_ids(&f.book);
    let views = review::pending(f.book.conn()).unwrap();
    assert!(views.iter().all(|v| v.ready));
    assert_eq!(views[0].sender, "Pat's phone");
    let (done, _) = approve(&mut f.book, &ids).unwrap();
    assert_eq!(done.txns.len(), 2);
    assert_eq!(done.account, Some(f.chk));

    let plain = f.book.txn(done.txns[0]).unwrap();
    assert_eq!(plain.source, TxnSource::Manual);
    assert_eq!(plain.memo, "week");
    assert_eq!(plain.posting_for(f.chk).unwrap().amount, m("-45.20"));
    let cat = plain
        .postings
        .iter()
        .find(|p| p.target == Target::Category(f.groceries))
        .unwrap();
    assert_eq!(cat.amount, m("45.20"));
    let payee = kansha_core::persistence::payees::get(f.book.conn(), plain.payee.unwrap()).unwrap();
    assert_eq!(payee.fields.name, "Kroger");

    // A credit card: the charge raises what is owed (negative posting).
    let split = f.book.txn(done.txns[1]).unwrap();
    assert_eq!(split.posting_for(f.card).unwrap().amount, m("-107.00"));
    let amount_to = |c: CategoryId| {
        split
            .postings
            .iter()
            .find(|p| p.target == Target::Category(c))
            .unwrap()
            .amount
    };
    assert_eq!(amount_to(f.tax), m("7.00"));
    assert_eq!(amount_to(f.groceries), m("60.00"));
    assert_eq!(amount_to(f.household), m("40.00"));
    assert_eq!(f.book.balance(f.chk).unwrap(), m("-45.20"));

    assert!(pending_ids(&f.book).is_empty());
    let e = repo::get(f.book.conn(), ids[0]).unwrap();
    assert_eq!(e.status, PendingStatus::Approved);
    assert_eq!(e.txn_id, Some(done.txns[0]));
    clean(&f.book);
}

#[test]
fn a_refund_posts_the_other_way() {
    let mut f = fx();
    receive(
        &mut f.book,
        "kansha-entries 1\n2026-10-14 Costco -24.99 Household acct \"Double Cash\"\n",
    )
    .unwrap();
    let ids = pending_ids(&f.book);
    let (done, _) = approve(&mut f.book, &ids).unwrap();
    let t = f.book.txn(done.txns[0]).unwrap();
    assert_eq!(t.posting_for(f.card).unwrap().amount, m("24.99"));
    let cat = t
        .postings
        .iter()
        .find(|p| p.target == Target::Category(f.household))
        .unwrap();
    assert_eq!(cat.amount, m("-24.99"));
}

#[test]
fn an_unresolved_name_blocks_approval_until_fixed() {
    let mut f = fx();
    receive(
        &mut f.book,
        "kansha-entries 1\n2026-10-12 Shell 5.00 Grocries acct Checking\n",
    )
    .unwrap();
    let id = pending_ids(&f.book)[0];
    let err = approve(&mut f.book, &[id]).unwrap_err();
    assert!(err.to_string().contains("fix it before approving"), "{err}");
    assert_eq!(txn_count(&f.book), 0);

    let edit = EntryEdit {
        date: date("2026-10-11"),
        payee: "Shell".into(),
        amount: m("5.50"),
        account: "Checking".into(),
        category: Some("Food:Groceries".into()),
        memo: "fixed".into(),
        split_categories: Vec::new(),
    };
    let view = f.book.write(|tx| review::edit(tx, id, &edit)).unwrap();
    assert!(view.ready);
    let (done, _) = approve(&mut f.book, &[id]).unwrap();
    let t = f.book.txn(done.txns[0]).unwrap();
    assert_eq!(t.date, date("2026-10-11"));
    assert_eq!(t.posting_for(f.chk).unwrap().amount, m("-5.50"));
}

#[test]
fn a_renamed_category_shows_as_unresolved() {
    let mut f = fx();
    receive(
        &mut f.book,
        "kansha-entries 1\n2026-10-12 Shell 5.00 Household acct Checking\n",
    )
    .unwrap();
    let mut fields = kansha_core::persistence::categories::get(f.book.conn(), f.household)
        .unwrap()
        .fields;
    fields.name = "Home".into();
    f.book
        .write(|tx| kansha_core::persistence::categories::update(tx, f.household, &fields))
        .unwrap();
    let v = review::pending(f.book.conn()).unwrap();
    assert!(!v[0].ready);
}

#[test]
fn a_split_amount_is_not_edited_in_review() {
    let mut f = fx();
    receive(&mut f.book, &sample()).unwrap();
    let id = pending_ids(&f.book)[1];
    let edit = EntryEdit {
        date: date("2026-10-12"),
        payee: "Target".into(),
        amount: m("100.00"),
        account: "Double Cash".into(),
        category: None,
        memo: String::new(),
        split_categories: vec!["Taxes:Sales".into(), "Household".into(), "Household".into()],
    };
    let err = f.book.write(|tx| review::edit(tx, id, &edit)).unwrap_err();
    assert!(err.to_string().contains("Approve and open"), "{err}");
}

#[test]
fn reject_leaves_the_ledger_untouched() {
    let mut f = fx();
    receive(&mut f.book, &sample()).unwrap();
    let ids = pending_ids(&f.book);
    f.book.write(|tx| review::reject(tx, &ids[..1])).unwrap();
    assert_eq!(txn_count(&f.book), 0);
    assert_eq!(pending_ids(&f.book), ids[1..].to_vec());
    assert_eq!(
        repo::get(f.book.conn(), ids[0]).unwrap().status,
        PendingStatus::Rejected
    );
    // Rejected ids are still seen: sending the file again adds nothing.
    let again = receive(&mut f.book, &sample()).unwrap();
    assert_eq!(again.added, 0);
    // Deciding twice is refused.
    assert!(f.book.write(|tx| review::reject(tx, &ids[..1])).is_err());
}

#[test]
fn undo_of_approval_puts_entries_back() {
    let mut f = fx();
    receive(&mut f.book, &sample()).unwrap();
    let ids = pending_ids(&f.book);
    let (done, u) = approve(&mut f.book, &ids).unwrap();
    assert_eq!(u.label(), "Approve entries");
    assert!(undo::available(f.book.conn(), &u).unwrap());
    f.book.write(|tx| undo::apply(tx, &u, false)).unwrap();

    assert_eq!(txn_count(&f.book), 0);
    assert_eq!(pending_ids(&f.book), ids);
    let e = repo::get(f.book.conn(), ids[0]).unwrap();
    assert_eq!(
        (e.status, e.txn_id, e.decided_at),
        (PendingStatus::Pending, None, None)
    );
    // The payees approval created are gone again.
    assert!(
        kansha_core::persistence::payees::find_by_name(f.book.conn(), "Kroger")
            .unwrap()
            .is_none()
    );
    clean(&f.book);

    // Approving again makes fresh transactions.
    let (again, _) = approve(&mut f.book, &ids).unwrap();
    assert_eq!(again.txns.len(), 2);
    assert_eq!(txn_count(&f.book), 2);
    assert_eq!(done.txns.len(), 2);
    assert_eq!(f.book.balance(f.chk).unwrap(), m("-45.20"));
    clean(&f.book);
}

#[test]
fn undo_is_refused_after_another_change() {
    let mut f = fx();
    receive(&mut f.book, &sample()).unwrap();
    let ids = pending_ids(&f.book);
    let (_, u) = approve(&mut f.book, &ids[..1]).unwrap();
    assert_eq!(u.label(), "Approve entry");
    f.book.write(|tx| review::reject(tx, &ids[1..])).unwrap();
    assert!(!undo::available(f.book.conn(), &u).unwrap());
}

#[test]
fn deleting_an_approved_transaction_keeps_the_entry_seen() {
    let mut f = fx();
    receive(&mut f.book, &sample()).unwrap();
    let ids = pending_ids(&f.book);
    let (done, _) = approve(&mut f.book, &ids[..1]).unwrap();
    f.book
        .write(|tx| kansha_core::ledger::delete(tx, done.txns[0], false))
        .unwrap();
    let e = repo::get(f.book.conn(), ids[0]).unwrap();
    assert_eq!((e.status, e.txn_id), (PendingStatus::Approved, None));
    assert_eq!(receive(&mut f.book, &sample()).unwrap().added, 0);
}

#[test]
fn every_step_is_audited() {
    let mut f = fx();
    receive(&mut f.book, &sample()).unwrap();
    let ids = pending_ids(&f.book);
    approve(&mut f.book, &ids[..1]).unwrap();
    f.book.write(|tx| review::reject(tx, &ids[1..])).unwrap();
    let h = audit::history(f.book.conn(), AuditEntity::PendingEntry, ids[0]).unwrap();
    assert_eq!(h.len(), 2);
    assert_eq!(
        (h[0].action, h[0].origin.as_str()),
        (AuditAction::Create, "system")
    );
    assert!(h[0].after_json.as_deref().unwrap().contains("Pat's phone"));
    assert_eq!(
        (h[1].action, h[1].origin.as_str()),
        (AuditAction::Update, "ui")
    );
    assert!(h[1].after_json.as_deref().unwrap().contains("\"approved\""));
    let r = audit::history(f.book.conn(), AuditEntity::PendingEntry, ids[1]).unwrap();
    assert!(r[1].after_json.as_deref().unwrap().contains("\"rejected\""));
}

// ---------------------------------------------------------------------------
// Inbox and Import entries… (CLI-030)
// ---------------------------------------------------------------------------

mod inbox {
    use super::*;
    use kansha_core::book::BookFiles;
    use kansha_core::entries::cli::write_inbox;
    use kansha_core::entries::inbox;
    use kansha_core::entries::sidecar::inbox_folder;
    use kansha_core::import::{ArchiveTarget, archive_folder};
    use kansha_core::security::{KeyFile, Passphrase};

    struct Setup {
        dir: tempfile::TempDir,
        files: BookFiles,
        archive: ArchiveTarget,
    }

    fn setup() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let files = BookFiles::at(dir.path().join("barton2026.db"));
        let key = KeyFile::create(&Passphrase::for_tests("pw"))
            .unwrap()
            .0
            .public_key()
            .unwrap();
        let archive = ArchiveTarget {
            folder: dir.path().to_path_buf(),
            book: "barton2026".into(),
            key,
        };
        Setup {
            dir,
            files,
            archive,
        }
    }

    fn count(dir: &std::path::Path) -> usize {
        std::fs::read_dir(dir).map_or(0, |d| d.count())
    }

    #[test]
    fn reading_the_inbox_receives_archives_and_empties_it() {
        let mut f = fx();
        let s = setup();
        let folder = inbox_folder(&s.files);
        write_inbox(&folder, &sample()).unwrap();
        let clock = *f.book.clock();
        let r = inbox::read(f.book.db_mut(), &clock, &s.files, &s.archive).unwrap();
        assert_eq!(r.added(), 2);
        assert!(r.refused.is_empty());
        assert_eq!(count(&folder), 0);
        let archived = archive_folder(s.dir.path(), "barton2026");
        assert_eq!(count(&archived), 1);
        // Encrypted: the payee is not readable in the copy.
        let copy = std::fs::read_dir(&archived)
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        let bytes = std::fs::read(copy.path()).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("Kroger"));
        let e = &repo::pending(f.book.conn()).unwrap()[0];
        assert_eq!(e.source, EntrySource::Cli);

        // The same entries again: nothing added, the file leaves the inbox,
        // and no second copy is kept.
        write_inbox(&folder, &sample()).unwrap();
        let r = inbox::read(f.book.db_mut(), &clock, &s.files, &s.archive).unwrap();
        assert_eq!((r.added(), r.received[0].skipped), (0, 2));
        assert_eq!(count(&folder), 0);
        assert_eq!(count(&archived), 1);
    }

    #[test]
    fn a_bad_file_stays_in_the_inbox_and_is_reported() {
        let mut f = fx();
        let s = setup();
        let folder = inbox_folder(&s.files);
        write_inbox(
            &folder,
            "kansha-entries 1\ntoday Shell 1 Household acct Checking\n",
        )
        .unwrap();
        let clock = *f.book.clock();
        let r = inbox::read(f.book.db_mut(), &clock, &s.files, &s.archive).unwrap();
        assert_eq!(r.refused.len(), 1);
        assert_eq!(
            r.refused[0].errors[0].to_string(),
            "line 2: “today” is not a date (use YYYY-MM-DD)"
        );
        assert_eq!(count(&folder), 1);
        assert_eq!(review::pending_count(f.book.conn()).unwrap(), 0);
    }

    #[test]
    fn a_file_being_written_is_not_read() {
        let mut f = fx();
        let s = setup();
        let folder = inbox_folder(&s.files);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(".20261012T100000Z-0a1b2c3d.txt"), sample()).unwrap();
        let clock = *f.book.clock();
        let r = inbox::read(f.book.db_mut(), &clock, &s.files, &s.archive).unwrap();
        assert_eq!(r, inbox::InboxReport::default());
        // No inbox at all is not an error.
        std::fs::remove_dir_all(&folder).unwrap();
        assert!(inbox::read(f.book.db_mut(), &clock, &s.files, &s.archive).is_ok());
    }

    #[test]
    fn import_entries_leaves_the_chosen_file_where_it_is() {
        let mut f = fx();
        let s = setup();
        let path = s.dir.path().join("from-signal.txt");
        std::fs::write(&path, sample()).unwrap();
        let clock = *f.book.clock();
        let r = inbox::import_file(f.book.db_mut(), &clock, &path, &s.archive).unwrap();
        assert_eq!(r.added(), 2);
        assert!(path.is_file());
        let e = &repo::pending(f.book.conn()).unwrap()[0];
        assert_eq!(
            (e.source, e.file_name.as_str()),
            (EntrySource::File, "from-signal.txt")
        );
    }
}
