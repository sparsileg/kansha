//! Edit > Find/Replace (REG-170): find register rows by field and
//! comparison, joined by And or Or, optionally within splits; Replace
//! All on the checked rows, audited, balanced, one undo for the batch.

use kansha_core::accounts::{AccountId, AccountType};
use kansha_core::categories::{CategoryId, CategoryKind};
use kansha_core::invest::{InvAction, InvInput};
use kansha_core::ledger::{
    self, Cleared, EntryLine, FindComparison, FindCondition, FindField, FindJoin, FindQuery,
    FindResult, FindSort, FoundKey, ReplaceResult, ReplaceSpec, Target, TxnId,
};
use kansha_core::persistence::audit::{self, AuditAction, AuditEntity};
use kansha_core::persistence::{categories, payees, tags};
use kansha_core::reconcile::{self, StartInput};
use kansha_core::testkit::Book;
use kansha_core::undo;
use kansha_core::{Error, Money};

use crate::fixture::date;

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

struct Fx {
    book: Book,
    chk: AccountId,
    visa: AccountId,
    sav: AccountId,
    fun: CategoryId,
    food: CategoryId,
    home: CategoryId,
    /// Netflix, checking, Feb.
    t1: TxnId,
    /// Netflix, Visa, Mar, tag Business.
    t2: TxnId,
    /// Netflix, checking, Apr, 17.99, cleared.
    t3: TxnId,
    /// Grocer split, check 1001: Food 60 (memo milk, tag Trip), Home 40.
    t4: TxnId,
    /// Transfer to savings, check 999.
    t5: TxnId,
    /// Netflix, voided.
    t6: TxnId,
    /// Salary deposit.
    t7: TxnId,
}

fn fx() -> Fx {
    let mut book = Book::new(date("2026-06-30")).unwrap();
    let chk = book.account("Checking", AccountType::Checking).unwrap();
    let visa = book.account("Visa", AccountType::CreditCard).unwrap();
    let sav = book.account("Savings", AccountType::Savings).unwrap();
    book.opening_balance(chk, date("2026-01-01"), m("1000.00"))
        .unwrap();
    let fun = book
        .category("Entertainment", CategoryKind::Expense)
        .unwrap();
    let food = book.category("Food", CategoryKind::Expense).unwrap();
    let home = book.category("Household", CategoryKind::Expense).unwrap();
    let salary = book.category("Salary", CategoryKind::Income).unwrap();
    let business = book.tag("Business").unwrap();
    let trip = book.tag("Trip").unwrap();

    let t1 = book
        .entry(chk, date("2026-02-10"))
        .payee("Netflix.com")
        .amount(m("-16.46"))
        .memo("Feb")
        .category(fun)
        .save()
        .unwrap()
        .id;
    let t2 = book
        .entry(visa, date("2026-03-10"))
        .payee("Netflix.com")
        .amount(m("-16.46"))
        .tag(business)
        .category(fun)
        .save()
        .unwrap()
        .id;
    let t3 = book
        .entry(chk, date("2026-04-10"))
        .payee("Netflix.com")
        .amount(m("-17.99"))
        .cleared(Cleared::Cleared)
        .category(fun)
        .save()
        .unwrap()
        .id;
    let mut milk = EntryLine::new(Target::Category(food), m("-60.00"));
    milk.memo = "milk".into();
    milk.tags = vec![trip];
    let mut soap = EntryLine::new(Target::Category(home), m("-40.00"));
    soap.memo = "soap".into();
    let t4 = book
        .entry(chk, date("2026-05-01"))
        .payee("Grocer")
        .check_num("1001")
        .amount(m("-100.00"))
        .line(milk)
        .line(soap)
        .save()
        .unwrap()
        .id;
    let t5 = book
        .entry(chk, date("2026-05-05"))
        .payee("Transfer")
        .check_num("999")
        .amount(m("-200.00"))
        .transfer(sav)
        .save()
        .unwrap()
        .id;
    let t6 = book
        .entry(chk, date("2026-05-10"))
        .payee("Netflix.com")
        .amount(m("-16.46"))
        .category(fun)
        .save()
        .unwrap()
        .id;
    book.write(|tx| ledger::void(tx, t6, false)).unwrap();
    let t7 = book
        .entry(chk, date("2026-06-01"))
        .payee("Employer")
        .amount(m("2000.00"))
        .category(salary)
        .save()
        .unwrap()
        .id;
    Fx {
        book,
        chk,
        visa,
        sav,
        fun,
        food,
        home,
        t1,
        t2,
        t3,
        t4,
        t5,
        t6,
        t7,
    }
}

fn cond(field: FindField, comparison: FindComparison, value: &str) -> FindCondition {
    FindCondition {
        field,
        comparison,
        value: value.into(),
    }
}

fn query(conditions: Vec<FindCondition>, join: FindJoin, within_splits: bool) -> FindQuery {
    FindQuery {
        conditions,
        join,
        within_splits,
        sort: FindSort::Date,
        descending: true,
    }
}

fn find(book: &Book, q: &FindQuery) -> FindResult {
    ledger::find(book.conn(), q).unwrap()
}

fn find_one(book: &Book, c: FindCondition) -> FindResult {
    find(book, &query(vec![c], FindJoin::And, true))
}

/// The transactions found, in row order, each once.
fn txns(r: &FindResult) -> Vec<TxnId> {
    let mut out: Vec<TxnId> = Vec::new();
    for row in &r.rows {
        if !out.contains(&row.key.txn) {
            out.push(row.key.txn);
        }
    }
    out
}

fn keys(r: &FindResult) -> Vec<FoundKey> {
    r.rows.iter().map(|row| row.key).collect()
}

fn replace(
    book: &mut Book,
    rows: &[FoundKey],
    field: FindField,
    with: &str,
    confirmed: bool,
) -> kansha_core::Result<ReplaceResult> {
    let spec = ReplaceSpec {
        field,
        with: with.into(),
    };
    book.write(|tx| ledger::replace(tx, rows, &spec, confirmed))
}

fn clean(book: &Book) {
    let r = kansha_core::integrity::check(book.conn()).unwrap();
    assert!(r.is_clean(), "{r:?}");
}

fn tag_names(book: &Book, ids: &[kansha_core::categories::TagId]) -> Vec<String> {
    ids.iter()
        .map(|id| tags::get(book.conn(), *id).unwrap().fields.name)
        .collect()
}

// ---------------------------------------------------------------------------
// Find
// ---------------------------------------------------------------------------

#[test]
fn payee_contains_netflix_finds_every_live_one_newest_first() {
    let fx = fx();
    let r = find_one(
        &fx.book,
        cond(FindField::Payee, FindComparison::Contains, "netflix"),
    );
    // The voided one is not found.
    assert_eq!(txns(&r), vec![fx.t3, fx.t2, fx.t1]);
    assert_eq!(r.txns, 3);
    let row = &r.rows[1];
    assert_eq!(row.account_name, "Visa");
    assert_eq!(row.payee, "Netflix.com");
    assert_eq!(row.category, "Entertainment");
    assert_eq!(row.tags, "Business");
    assert_eq!(row.amount, m("-16.46"));
    assert!(!txns(&r).contains(&fx.t6));
}

#[test]
fn a_blank_value_finds_nothing_and_is_left_out_of_several_rows() {
    let fx = fx();
    let r = find_one(
        &fx.book,
        cond(FindField::Payee, FindComparison::Contains, "  "),
    );
    assert!(r.rows.is_empty());
    let r = find(
        &fx.book,
        &query(
            vec![
                cond(FindField::Payee, FindComparison::Exactly, "grocer"),
                cond(FindField::Memo, FindComparison::Contains, ""),
            ],
            FindJoin::And,
            false,
        ),
    );
    assert_eq!(txns(&r), vec![fx.t4]);
}

#[test]
fn amount_is_signed_and_compares_as_a_number() {
    let fx = fx();
    let r = find_one(
        &fx.book,
        cond(FindField::Amount, FindComparison::Exactly, "-16.46"),
    );
    assert_eq!(txns(&r), vec![fx.t2, fx.t1]);
    let r = find_one(
        &fx.book,
        cond(FindField::Amount, FindComparison::Exactly, "16.46"),
    );
    assert!(r.rows.is_empty(), "signed: a payment is negative");
    // 2,000 deposit; commas and $ are fine.
    let r = find_one(
        &fx.book,
        cond(FindField::Amount, FindComparison::GreaterOrEqual, "$1,000"),
    );
    assert_eq!(txns(&r), vec![fx.t7, fx.book_opening()]);
    let r = find_one(
        &fx.book,
        cond(FindField::Amount, FindComparison::Less, "-17.99"),
    );
    // The split (−100) and its lines (−60, −40), and the transfer's
    // checking side (−200); not 17.99 itself.
    assert_eq!(txns(&r), vec![fx.t5, fx.t4]);
    let r = find_one(
        &fx.book,
        cond(FindField::Amount, FindComparison::LessOrEqual, "-17.99"),
    );
    assert!(txns(&r).contains(&fx.t3));
    // Text comparisons use the amount as shown.
    let r = find_one(
        &fx.book,
        cond(FindField::Amount, FindComparison::EndsWith, ".99"),
    );
    assert_eq!(txns(&r), vec![fx.t3]);
    let r = find_one(
        &fx.book,
        cond(FindField::Amount, FindComparison::StartsWith, "-16"),
    );
    assert_eq!(txns(&r), vec![fx.t2, fx.t1]);
    // Greater than a payment includes deposits.
    let r = find_one(
        &fx.book,
        cond(FindField::Amount, FindComparison::Greater, "-16.46"),
    );
    assert!(txns(&r).contains(&fx.t7));
    assert!(!txns(&r).contains(&fx.t1));
}

impl Fx {
    /// The opening balance transaction (the first one).
    fn book_opening(&self) -> TxnId {
        TxnId(self.t1.0 - 1)
    }
}

#[test]
fn an_amount_that_is_not_a_number_is_an_error_for_ordered_comparisons() {
    let fx = fx();
    let q = query(
        vec![cond(FindField::Amount, FindComparison::Greater, "abc")],
        FindJoin::And,
        false,
    );
    let err = ledger::find(fx.book.conn(), &q).unwrap_err();
    assert!(err.to_string().contains("not an amount"), "{err}");
    let q = query(
        vec![cond(FindField::Date, FindComparison::Less, "03/05/2026")],
        FindJoin::And,
        false,
    );
    let err = ledger::find(fx.book.conn(), &q).unwrap_err();
    assert!(err.to_string().contains("not a date"), "{err}");
    // Contains takes any text.
    let r = find_one(
        &fx.book,
        cond(FindField::Amount, FindComparison::Contains, "abc"),
    );
    assert!(r.rows.is_empty());
}

#[test]
fn date_compares_as_a_date_or_as_shown() {
    let fx = fx();
    let r = find_one(
        &fx.book,
        cond(FindField::Date, FindComparison::Exactly, "2026-03-10"),
    );
    assert_eq!(txns(&r), vec![fx.t2]);
    let r = find_one(
        &fx.book,
        cond(FindField::Date, FindComparison::Greater, "2026-05-01"),
    );
    // The transfer shows in checking and in savings.
    assert_eq!(txns(&r), vec![fx.t7, fx.t5]);
    assert_eq!(r.rows.len(), 3);
    let r = find_one(
        &fx.book,
        cond(FindField::Date, FindComparison::LessOrEqual, "2026-02-10"),
    );
    assert_eq!(txns(&r), vec![fx.t1, fx.book_opening()]);
    // As the register shows it (MM/DD/YYYY by default).
    let r = find_one(
        &fx.book,
        cond(FindField::Date, FindComparison::StartsWith, "03/"),
    );
    assert_eq!(txns(&r), vec![fx.t2]);
    let r = find_one(
        &fx.book,
        cond(FindField::Date, FindComparison::Contains, "/10/"),
    );
    assert_eq!(txns(&r), vec![fx.t3, fx.t2, fx.t1]);
}

#[test]
fn check_numbers_compare_as_numbers() {
    let fx = fx();
    // As text "999" > "1001"; as numbers it is less.
    let r = find_one(
        &fx.book,
        cond(FindField::CheckNum, FindComparison::Greater, "999"),
    );
    assert_eq!(txns(&r), vec![fx.t4]);
    let r = find_one(
        &fx.book,
        cond(FindField::CheckNum, FindComparison::Less, "1001"),
    );
    // The transfer, in both registers. A blank check number is text,
    // and "" sorts before "1001".
    assert!(txns(&r).contains(&fx.t5) && txns(&r).contains(&fx.t1));
    assert!(!txns(&r).contains(&fx.t4));
    let r = find_one(
        &fx.book,
        cond(FindField::CheckNum, FindComparison::StartsWith, "10"),
    );
    assert_eq!(txns(&r), vec![fx.t4]);
}

#[test]
fn text_fields_ignore_case_and_order_alphabetically() {
    let fx = fx();
    let r = find_one(
        &fx.book,
        cond(FindField::Payee, FindComparison::Greater, "m"),
    );
    // Netflix.com and Transfer; not Employer or Grocer.
    assert!(txns(&r).contains(&fx.t1) && txns(&r).contains(&fx.t5));
    assert!(!txns(&r).contains(&fx.t4) && !txns(&r).contains(&fx.t7));
    let r = find_one(
        &fx.book,
        cond(FindField::Payee, FindComparison::EndsWith, ".COM"),
    );
    assert_eq!(txns(&r), vec![fx.t3, fx.t2, fx.t1]);
    let r = find_one(
        &fx.book,
        cond(FindField::Memo, FindComparison::Exactly, "FEB"),
    );
    assert_eq!(txns(&r), vec![fx.t1]);
    let r = find_one(
        &fx.book,
        cond(FindField::Category, FindComparison::StartsWith, "[sav"),
    );
    assert_eq!(txns(&r), vec![fx.t5]);
    assert_eq!(r.rows[0].key.account, fx.chk);
}

#[test]
fn cleared_status_compares_by_name() {
    let fx = fx();
    let r = find_one(
        &fx.book,
        cond(FindField::Cleared, FindComparison::Exactly, "cleared"),
    );
    assert_eq!(txns(&r), vec![fx.t3]);
    let r = find_one(
        &fx.book,
        cond(FindField::Cleared, FindComparison::Exactly, "Unmarked"),
    );
    assert!(txns(&r).contains(&fx.t1) && !txns(&r).contains(&fx.t3));
    assert_eq!(ledger::cleared_name(Cleared::Reconciled), "Reconciled");
}

#[test]
fn tag_matches_any_one_of_a_rows_tags() {
    let mut fx = fx();
    let extra = fx.book.tag("Subscription").unwrap();
    let business = fx.book.clone_tag("Business");
    // Give Netflix in Visa a second tag.
    let mut e = ledger::Entry::from_txn(&fx.book.txn(fx.t2).unwrap(), fx.visa).unwrap();
    e.tags = vec![business, extra];
    fx.book
        .write(|tx| ledger::update_entry(tx, fx.t2, &e, false))
        .unwrap();
    let r = find_one(
        &fx.book,
        cond(FindField::Tag, FindComparison::Exactly, "subscription"),
    );
    assert_eq!(txns(&r), vec![fx.t2]);
    assert_eq!(r.rows[0].tags, "Business, Subscription");
    let r = find_one(
        &fx.book,
        cond(FindField::Tag, FindComparison::Exactly, "business"),
    );
    assert_eq!(txns(&r), vec![fx.t2]);
}

#[test]
fn within_splits_finds_and_shows_split_lines() {
    let fx = fx();
    // Off: the split shows --Split--; a line's category is not seen.
    let q = query(
        vec![cond(FindField::Category, FindComparison::Exactly, "Food")],
        FindJoin::And,
        false,
    );
    assert!(find(&fx.book, &q).rows.is_empty());
    // On: the Food line is a row of its own.
    let q = query(
        vec![cond(FindField::Category, FindComparison::Exactly, "Food")],
        FindJoin::And,
        true,
    );
    let r = find(&fx.book, &q);
    assert_eq!(r.rows.len(), 1);
    let line = &r.rows[0];
    assert_eq!(line.key.txn, fx.t4);
    assert!(line.key.line.is_some());
    assert_eq!(line.amount, m("-60.00"));
    assert_eq!(line.memo, "milk");
    assert_eq!(line.tags, "Trip");
    assert_eq!(line.payee, "Grocer");
    // The split row itself matches its own fields; its lines follow it.
    let r = find_one(
        &fx.book,
        cond(FindField::Payee, FindComparison::Exactly, "Grocer"),
    );
    assert_eq!(r.rows.len(), 3);
    assert_eq!(r.rows[0].category, "--Split--");
    assert_eq!(r.rows[0].key.line, None);
    assert_eq!(r.txns, 1);
    let q = query(
        vec![cond(FindField::Payee, FindComparison::Exactly, "Grocer")],
        FindJoin::And,
        false,
    );
    assert_eq!(find(&fx.book, &q).rows.len(), 1);
}

#[test]
fn and_needs_every_row_or_needs_one() {
    let fx = fx();
    let rows = vec![
        cond(FindField::Payee, FindComparison::Contains, "netflix"),
        cond(FindField::Amount, FindComparison::Exactly, "-17.99"),
    ];
    let r = find(&fx.book, &query(rows.clone(), FindJoin::And, false));
    assert_eq!(txns(&r), vec![fx.t3]);
    let rows2 = vec![
        cond(FindField::Payee, FindComparison::Exactly, "Employer"),
        cond(FindField::CheckNum, FindComparison::Exactly, "1001"),
    ];
    let r = find(&fx.book, &query(rows2, FindJoin::Or, false));
    assert_eq!(txns(&r), vec![fx.t7, fx.t4]);
    let r = find(&fx.book, &query(rows, FindJoin::Or, false));
    assert_eq!(txns(&r), vec![fx.t3, fx.t2, fx.t1]);
}

#[test]
fn investment_transactions_are_never_found() {
    let mut fx = fx();
    let brk = fx
        .book
        .account("Brokerage", AccountType::Brokerage)
        .unwrap();
    let mut cash = InvInput::new(brk, InvAction::CashIn, date("2026-06-15"));
    cash.amount = Some(m("500.00"));
    cash.counterpart = Some(Target::Account(fx.chk));
    fx.book.invest(&cash).unwrap();
    let r = find_one(
        &fx.book,
        cond(FindField::Date, FindComparison::Exactly, "2026-06-15"),
    );
    assert!(r.rows.is_empty(), "{:?}", r.rows);
}

#[test]
fn rows_sort_by_any_column() {
    let fx = fx();
    let mut q = query(
        vec![cond(FindField::Payee, FindComparison::Contains, "netflix")],
        FindJoin::And,
        false,
    );
    q.sort = FindSort::Amount;
    q.descending = false;
    let r = find(&fx.book, &q);
    // −17.99 first; the two −16.46 then newest first.
    assert_eq!(txns(&r), vec![fx.t3, fx.t2, fx.t1]);
    q.sort = FindSort::Account;
    let r = find(&fx.book, &q);
    assert_eq!(txns(&r), vec![fx.t3, fx.t1, fx.t2]);
    q.descending = true;
    let r = find(&fx.book, &q);
    assert_eq!(txns(&r), vec![fx.t2, fx.t3, fx.t1]);
    q.sort = FindSort::Date;
    q.descending = false;
    let r = find(&fx.book, &q);
    assert_eq!(txns(&r), vec![fx.t1, fx.t2, fx.t3]);
}

// ---------------------------------------------------------------------------
// Replace
// ---------------------------------------------------------------------------

#[test]
fn replace_tag_adds_it_creates_it_and_leaves_unchecked_rows() {
    let mut fx = fx();
    let r = find_one(
        &fx.book,
        cond(FindField::Payee, FindComparison::Contains, "netflix"),
    );
    // Uncheck the oldest.
    let checked: Vec<FoundKey> = keys(&r).into_iter().filter(|k| k.txn != fx.t1).collect();
    assert!(fx.book.find_tag("Subscription").is_none());
    let audits_before = audit::history(fx.book.conn(), AuditEntity::Txn, fx.t2.0)
        .unwrap()
        .len();
    let done = replace(
        &mut fx.book,
        &checked,
        FindField::Tag,
        "Subscription",
        false,
    )
    .unwrap();
    assert_eq!(done.changed, 2);
    let sub = fx.book.find_tag("Subscription").expect("created");
    let t2 = fx.book.txn(fx.t2).unwrap();
    let visa = t2.posting_for(fx.visa).unwrap();
    // Added; Business stays.
    assert_eq!(
        tag_names(&fx.book, &visa.tags),
        vec!["Business", "Subscription"]
    );
    assert!(
        fx.book
            .txn(fx.t3)
            .unwrap()
            .posting_for(fx.chk)
            .unwrap()
            .tags
            .contains(&sub)
    );
    // Unchecked: unchanged.
    assert!(
        fx.book
            .txn(fx.t1)
            .unwrap()
            .postings
            .iter()
            .all(|p| p.tags.is_empty())
    );
    // One audit entry per changed transaction, with before and after.
    let h = audit::history(fx.book.conn(), AuditEntity::Txn, fx.t2.0).unwrap();
    assert_eq!(h.len(), audits_before + 1);
    let last = h.last().unwrap();
    assert_eq!(last.action, AuditAction::Update);
    assert!(last.before_json.is_some() && last.after_json.is_some());
    // Find sees it now.
    let r = find_one(
        &fx.book,
        cond(FindField::Tag, FindComparison::Exactly, "Subscription"),
    );
    assert_eq!(txns(&r), vec![fx.t3, fx.t2]);
    // Again: already there, nothing changes.
    let again = replace(
        &mut fx.book,
        &checked,
        FindField::Tag,
        "subscription",
        false,
    )
    .unwrap();
    assert_eq!((again.changed, again.unchanged), (0, 2));
    clean(&fx.book);
}

#[test]
fn replace_tag_on_a_split_line_tags_that_line() {
    let mut fx = fx();
    let r = find_one(
        &fx.book,
        cond(FindField::Category, FindComparison::Exactly, "Household"),
    );
    replace(&mut fx.book, &keys(&r), FindField::Tag, "Trip", false).unwrap();
    let t = fx.book.txn(fx.t4).unwrap();
    let home = t
        .postings
        .iter()
        .find(|p| p.target == Target::Category(fx.home))
        .unwrap();
    assert_eq!(tag_names(&fx.book, &home.tags), vec!["Trip"]);
    assert!(t.posting_for(fx.chk).unwrap().tags.is_empty());
    clean(&fx.book);
}

#[test]
fn replace_category_creates_it_changes_splits_and_skips_transfers() {
    let mut fx = fx();
    // The split row, a split line, a simple row, and a transfer.
    let split_row = FoundKey {
        txn: fx.t4,
        account: fx.chk,
        line: None,
    };
    let simple = FoundKey {
        txn: fx.t1,
        account: fx.chk,
        line: None,
    };
    let transfer = FoundKey {
        txn: fx.t5,
        account: fx.chk,
        line: None,
    };
    let done = replace(
        &mut fx.book,
        &[split_row, simple, transfer],
        FindField::Category,
        "Fun:Streaming",
        false,
    )
    .unwrap();
    assert_eq!(done.changed, 2);
    assert_eq!(done.skipped_transfer, 1);
    let new = fx.book.find_category("Fun:Streaming").unwrap().unwrap();
    let cat = categories::get(fx.book.conn(), new).unwrap();
    assert_eq!(cat.fields.kind, CategoryKind::Expense);
    let t1 = fx.book.txn(fx.t1).unwrap();
    assert!(
        t1.postings
            .iter()
            .any(|p| p.target == Target::Category(new))
    );
    // Every category line of the split; amounts and memos kept.
    let t4 = fx.book.txn(fx.t4).unwrap();
    let lines: Vec<_> = t4
        .postings
        .iter()
        .filter(|p| p.target != Target::Account(fx.chk))
        .collect();
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().all(|p| p.target == Target::Category(new)));
    assert_eq!(lines[0].amount, m("60.00"));
    assert_eq!(lines[0].memo, "milk");
    // The transfer is still a transfer.
    let t5 = fx.book.txn(fx.t5).unwrap();
    assert!(t5.posting_for(fx.sav).is_some());
    clean(&fx.book);
}

#[test]
fn replace_category_on_one_split_line_changes_only_that_line() {
    let mut fx = fx();
    let r = find_one(
        &fx.book,
        cond(FindField::Memo, FindComparison::Exactly, "soap"),
    );
    assert_eq!(r.rows.len(), 1);
    replace(&mut fx.book, &keys(&r), FindField::Category, "Food", false).unwrap();
    let t4 = fx.book.txn(fx.t4).unwrap();
    let cats: Vec<_> = t4
        .postings
        .iter()
        .filter_map(|p| match p.target {
            Target::Category(c) => Some(c),
            Target::Account(_) => None,
        })
        .collect();
    assert_eq!(cats, vec![fx.food, fx.food]);
    clean(&fx.book);
}

#[test]
fn a_new_category_for_deposits_is_income() {
    let mut fx = fx();
    let row = FoundKey {
        txn: fx.t7,
        account: fx.chk,
        line: None,
    };
    replace(&mut fx.book, &[row], FindField::Category, "Bonus", false).unwrap();
    let id = fx.book.find_category("Bonus").unwrap().unwrap();
    assert_eq!(
        categories::get(fx.book.conn(), id).unwrap().fields.kind,
        CategoryKind::Income
    );
}

#[test]
fn replace_amount_changes_simple_rows_only_and_stays_balanced() {
    let mut fx = fx();
    let r = find(
        &fx.book,
        &query(
            vec![
                cond(FindField::Payee, FindComparison::Exactly, "netflix.com"),
                cond(FindField::Payee, FindComparison::Exactly, "grocer"),
                cond(FindField::Payee, FindComparison::Exactly, "transfer"),
            ],
            FindJoin::Or,
            true,
        ),
    );
    let done = replace(&mut fx.book, &keys(&r), FindField::Amount, "-19.99", false).unwrap();
    // Three Netflix; the split (its row and lines) and the transfer
    // (both sides) skipped.
    assert_eq!(done.changed, 3);
    assert_eq!(done.skipped_split, 1);
    assert_eq!(done.skipped_transfer, 1);
    let t2 = fx.book.txn(fx.t2).unwrap();
    assert_eq!(t2.posting_for(fx.visa).unwrap().amount, m("-19.99"));
    let other = t2
        .postings
        .iter()
        .find(|p| p.target == Target::Category(fx.fun))
        .unwrap();
    assert_eq!(other.amount, m("19.99"));
    assert_eq!(
        fx.book
            .txn(fx.t4)
            .unwrap()
            .posting_for(fx.chk)
            .unwrap()
            .amount,
        m("-100.00")
    );
    assert_eq!(
        fx.book
            .txn(fx.t5)
            .unwrap()
            .posting_for(fx.chk)
            .unwrap()
            .amount,
        m("-200.00")
    );
    // A sum works (REG-035); a deposit is positive.
    let row = FoundKey {
        txn: fx.t7,
        account: fx.chk,
        line: None,
    };
    replace(&mut fx.book, &[row], FindField::Amount, "2000+50", false).unwrap();
    assert_eq!(
        fx.book
            .txn(fx.t7)
            .unwrap()
            .posting_for(fx.chk)
            .unwrap()
            .amount,
        m("2050.00")
    );
    clean(&fx.book);
}

#[test]
fn replace_date_asks_first() {
    let mut fx = fx();
    let r = find_one(
        &fx.book,
        cond(FindField::Payee, FindComparison::Contains, "netflix"),
    );
    let err = replace(
        &mut fx.book,
        &keys(&r),
        FindField::Date,
        "2026-06-15",
        false,
    )
    .unwrap_err();
    match err {
        Error::ConfirmationRequired(msg) => {
            assert!(msg.contains("moves 3 transactions to 2026-06-15"), "{msg}");
        }
        e => panic!("{e}"),
    }
    assert_eq!(fx.book.txn(fx.t1).unwrap().date, date("2026-02-10"));
    let done = replace(&mut fx.book, &keys(&r), FindField::Date, "2026-06-15", true).unwrap();
    assert_eq!(done.changed, 3);
    assert_eq!(fx.book.txn(fx.t1).unwrap().date, date("2026-06-15"));
    assert!(replace(&mut fx.book, &keys(&r), FindField::Date, "6/15", true).is_err());
}

fn reconcile_checking(fx: &mut Fx) {
    let balance = fx.book.balance(fx.chk).unwrap();
    let input = StartInput {
        account: fx.chk,
        statement_date: date("2026-06-30"),
        statement_balance: balance,
        interest: None,
        service_charge: None,
    };
    let rec = fx.book.write(|tx| reconcile::start(tx, &input)).unwrap();
    let s = reconcile::session(fx.book.conn(), rec.id).unwrap();
    let all: Vec<TxnId> = s
        .payments
        .iter()
        .chain(&s.deposits)
        .map(|i| i.txn_id)
        .collect();
    fx.book
        .write(|tx| reconcile::set_checked(tx, rec.id, &all, true))
        .unwrap();
    fx.book.write(|tx| reconcile::finish(tx, rec.id)).unwrap();
}

#[test]
fn reconciled_rows_need_one_confirmation_for_the_batch() {
    let mut fx = fx();
    reconcile_checking(&mut fx);
    let r = find_one(
        &fx.book,
        cond(FindField::Payee, FindComparison::Contains, "netflix"),
    );
    let err = replace(&mut fx.book, &keys(&r), FindField::Memo, "Streaming", false).unwrap_err();
    match err {
        Error::ConfirmationRequired(msg) => {
            assert_eq!(
                msg,
                "2 of the checked transactions are reconciled. Replace anyway?"
            );
        }
        e => panic!("{e}"),
    }
    let done = replace(&mut fx.book, &keys(&r), FindField::Memo, "Streaming", true).unwrap();
    assert_eq!(done.changed, 3);
    // Still reconciled, in the same reconciliation.
    let p = fx.book.txn(fx.t1).unwrap();
    let p = p.posting_for(fx.chk).unwrap();
    assert_eq!(p.cleared, Cleared::Reconciled);
    assert!(p.reconciliation_id.is_some());
    assert_eq!(fx.book.txn(fx.t1).unwrap().memo, "Streaming");
    clean(&fx.book);
}

#[test]
fn replace_cleared_sets_unmarked_or_cleared_never_reconciled() {
    let mut fx = fx();
    let r = find_one(
        &fx.book,
        cond(FindField::Payee, FindComparison::Contains, "netflix"),
    );
    replace(
        &mut fx.book,
        &keys(&r),
        FindField::Cleared,
        "Cleared",
        false,
    )
    .unwrap();
    assert_eq!(
        fx.book
            .txn(fx.t1)
            .unwrap()
            .posting_for(fx.chk)
            .unwrap()
            .cleared,
        Cleared::Cleared
    );
    replace(
        &mut fx.book,
        &keys(&r),
        FindField::Cleared,
        "unmarked",
        false,
    )
    .unwrap();
    assert_eq!(
        fx.book
            .txn(fx.t3)
            .unwrap()
            .posting_for(fx.chk)
            .unwrap()
            .cleared,
        Cleared::Unmarked
    );
    let err = replace(
        &mut fx.book,
        &keys(&r),
        FindField::Cleared,
        "Reconciled",
        false,
    )
    .unwrap_err();
    assert!(err.to_string().contains("only reconciling"), "{err}");
}

#[test]
fn replace_payee_check_and_memo() {
    let mut fx = fx();
    let r = find_one(
        &fx.book,
        cond(FindField::Payee, FindComparison::Contains, "netflix"),
    );
    replace(&mut fx.book, &keys(&r), FindField::Payee, "Netflix", false).unwrap();
    let p = payees::find_by_name(fx.book.conn(), "Netflix")
        .unwrap()
        .expect("created");
    assert_eq!(fx.book.txn(fx.t2).unwrap().payee, Some(p.id));
    replace(&mut fx.book, &keys(&r), FindField::CheckNum, "EFT", false).unwrap();
    assert_eq!(fx.book.txn(fx.t3).unwrap().check_num, "EFT");
    // A blank memo clears it; on a split line it is the line's memo.
    replace(&mut fx.book, &keys(&r), FindField::Memo, "", false).unwrap();
    assert_eq!(fx.book.txn(fx.t1).unwrap().memo, "");
    let r = find_one(
        &fx.book,
        cond(FindField::Memo, FindComparison::Exactly, "milk"),
    );
    replace(&mut fx.book, &keys(&r), FindField::Memo, "bread", false).unwrap();
    let t4 = fx.book.txn(fx.t4).unwrap();
    assert!(t4.postings.iter().any(|p| p.memo == "bread"));
    assert_eq!(t4.memo, "");
    // A blank payee clears it.
    let row = FoundKey {
        txn: fx.t7,
        account: fx.chk,
        line: None,
    };
    replace(&mut fx.book, &[row], FindField::Payee, " ", false).unwrap();
    assert_eq!(fx.book.txn(fx.t7).unwrap().payee, None);
    clean(&fx.book);
}

#[test]
fn closed_and_voided_transactions_are_skipped() {
    let mut fx = fx();
    let r = find_one(
        &fx.book,
        cond(FindField::Payee, FindComparison::Contains, "netflix"),
    );
    let mut rows = keys(&r);
    rows.push(FoundKey {
        txn: fx.t6,
        account: fx.chk,
        line: None,
    });
    // Close Visa: pay it off first.
    fx.book
        .entry(fx.chk, date("2026-06-20"))
        .amount(m("-16.46"))
        .transfer(fx.visa)
        .save()
        .unwrap();
    fx.book
        .write(|tx| ledger::close_account(tx, fx.visa, date("2026-06-30"), false))
        .unwrap();
    let done = replace(&mut fx.book, &rows, FindField::Memo, "x", false).unwrap();
    assert_eq!(done.changed, 2);
    assert_eq!(done.skipped_closed, 1);
    assert_eq!(done.skipped_gone, 1);
    assert_eq!(fx.book.txn(fx.t2).unwrap().memo, "");
}

#[test]
fn nothing_checked_or_nothing_to_replace_with_is_an_error() {
    let mut fx = fx();
    assert!(replace(&mut fx.book, &[], FindField::Memo, "x", false).is_err());
    let row = FoundKey {
        txn: fx.t1,
        account: fx.chk,
        line: None,
    };
    for field in [
        FindField::Tag,
        FindField::Category,
        FindField::Amount,
        FindField::Date,
    ] {
        let err = replace(&mut fx.book, &[row], field, "  ", true).unwrap_err();
        assert!(
            err.to_string().contains("to replace with"),
            "{field}: {err}"
        );
    }
}

#[test]
fn one_undo_puts_back_every_replaced_transaction() {
    let mut fx = fx();
    let r = find_one(
        &fx.book,
        cond(FindField::Payee, FindComparison::Contains, "netflix"),
    );
    let before: Vec<_> = [fx.t1, fx.t2, fx.t3]
        .iter()
        .map(|id| fx.book.txn(*id).unwrap())
        .collect();
    let rows = keys(&r);
    let ids = vec![fx.t1, fx.t2, fx.t3];
    let spec = ReplaceSpec {
        field: FindField::Tag,
        with: "Subscription".into(),
    };
    let u = fx
        .book
        .write(|tx| {
            let b = undo::before_many(tx.conn(), &ids)?;
            ledger::replace(tx, &rows, &spec, false)?;
            undo::after_many(tx.conn(), b, "Replace All")
        })
        .unwrap()
        .expect("undoable");
    assert_eq!(u.label(), "Replace All");
    assert_eq!(u.len(), 3);
    assert!(undo::available(fx.book.conn(), &u).unwrap());
    fx.book.write(|tx| undo::apply(tx, &u, false)).unwrap();
    let after: Vec<_> = [fx.t1, fx.t2, fx.t3]
        .iter()
        .map(|id| fx.book.txn(*id).unwrap())
        .collect();
    assert_eq!(after, before);
    assert!(!undo::available(fx.book.conn(), &u).unwrap());
    clean(&fx.book);
}

#[test]
fn undo_leaves_out_transactions_the_replace_did_not_change() {
    let mut fx = fx();
    let r = find_one(
        &fx.book,
        cond(FindField::Payee, FindComparison::Contains, "netflix"),
    );
    let rows = keys(&r);
    let ids = vec![fx.t1, fx.t2, fx.t3];
    let spec = ReplaceSpec {
        field: FindField::Tag,
        with: "Business".into(),
    };
    let u = fx
        .book
        .write(|tx| {
            let b = undo::before_many(tx.conn(), &ids)?;
            ledger::replace(tx, &rows, &spec, false)?;
            undo::after_many(tx.conn(), b, "Replace All")
        })
        .unwrap()
        .expect("undoable");
    // Visa's Netflix already had Business.
    assert_eq!(u.len(), 2);
    let unchanged = fx
        .book
        .write(|tx| {
            let b = undo::before_many(tx.conn(), &ids)?;
            ledger::replace(tx, &rows, &spec, false)?;
            undo::after_many(tx.conn(), b, "Replace All")
        })
        .unwrap();
    assert!(unchanged.is_none());
}

/// Replace and keep its undo, with what it created.
fn replace_undoable(
    book: &mut Book,
    rows: &[FoundKey],
    field: FindField,
    with: &str,
) -> undo::Undo {
    let mut ids: Vec<TxnId> = rows.iter().map(|k| k.txn).collect();
    ids.sort();
    ids.dedup();
    let spec = ReplaceSpec {
        field,
        with: with.into(),
    };
    book.write(|tx| {
        let b = undo::before_many(tx.conn(), &ids)?;
        let (_, created) = ledger::replace_tracked(tx, rows, &spec, true)?;
        Ok(undo::after_many(tx.conn(), b, "Replace All")?.map(|u| u.with_created(created)))
    })
    .unwrap()
    .expect("undoable")
}

#[test]
fn undo_deletes_the_tag_payee_and_categories_replace_created() {
    let mut fx = fx();
    let r = find_one(
        &fx.book,
        cond(FindField::Payee, FindComparison::Contains, "netflix"),
    );
    let rows = keys(&r);
    let tag = replace_undoable(&mut fx.book, &rows, FindField::Tag, "Subscription");
    let sub = fx.book.find_tag("Subscription").unwrap();
    assert_eq!(tag.created(), &[undo::Created::Tag(sub)]);
    fx.book.write(|tx| undo::apply(tx, &tag, false)).unwrap();
    assert!(fx.book.find_tag("Subscription").is_none());
    let h = audit::history(fx.book.conn(), AuditEntity::Tag, sub.0).unwrap();
    assert_eq!(h.last().unwrap().action, AuditAction::Delete);

    // Two levels: the child goes before its parent.
    let cat = replace_undoable(&mut fx.book, &rows, FindField::Category, "Fun:Streaming");
    assert_eq!(cat.created().len(), 2);
    fx.book.write(|tx| undo::apply(tx, &cat, false)).unwrap();
    assert!(fx.book.find_category("Fun").unwrap().is_none());
    assert!(
        fx.book
            .txn(fx.t1)
            .unwrap()
            .postings
            .iter()
            .any(|p| p.target == Target::Category(fx.fun))
    );

    let payee = replace_undoable(&mut fx.book, &rows, FindField::Payee, "Netflix");
    assert_eq!(payee.created().len(), 1);
    fx.book.write(|tx| undo::apply(tx, &payee, false)).unwrap();
    assert!(
        payees::find_by_name(fx.book.conn(), "Netflix")
            .unwrap()
            .is_none()
    );
    assert!(
        payees::find_by_name(fx.book.conn(), "Netflix.com")
            .unwrap()
            .is_some()
    );
    clean(&fx.book);
}

#[test]
fn undo_keeps_a_tag_or_category_that_already_existed() {
    let mut fx = fx();
    let r = find_one(
        &fx.book,
        cond(FindField::Payee, FindComparison::Contains, "netflix"),
    );
    let rows = keys(&r);
    let u = replace_undoable(&mut fx.book, &rows, FindField::Tag, "trip");
    assert!(u.created().is_empty());
    fx.book.write(|tx| undo::apply(tx, &u, false)).unwrap();
    assert!(fx.book.find_tag("Trip").is_some());
    // A new child under an existing parent: only the child is created.
    let u = replace_undoable(&mut fx.book, &rows, FindField::Category, "Food:Snacks");
    assert_eq!(u.created().len(), 1);
    fx.book.write(|tx| undo::apply(tx, &u, false)).unwrap();
    assert!(fx.book.find_category("Food:Snacks").unwrap().is_none());
    assert_eq!(fx.book.find_category("Food").unwrap(), Some(fx.food));
    clean(&fx.book);
}
