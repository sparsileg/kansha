//! Scenario runner (TEST-050, TEST-060).
//!
//! Discovers every `*.toml` file under `tests/scenarios/` (repo root) and
//! runs it. Set `KANSHA_SCENARIOS` to a file or directory (relative to the
//! repo root) to run just those — `just scenario <path>` does this.
//!
//! The file format is documented in `tests/scenarios/README.md`. Harness
//! actions (`add`, `subtract`) exercise the runner itself; ledger actions
//! (Phase 2) run against a fresh in-memory database built with
//! `kansha_core::testkit::Book`. Every scenario ends with the integrity
//! check (INT-030), which must be clean. Later phases extend `Action` and
//! `Expect` for their areas.

// Failure is large; fine for a test runner.
#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use kansha_core::accounts::{AccountFields, AccountId, AccountType};
use kansha_core::categories::{CategoryId, CategoryKind};
use kansha_core::ledger::{self, Cleared, Counterpart, EntryLine, Target, TxnId};
use kansha_core::persistence::{accounts, categories, payees, schedules, tags};
use kansha_core::schedule::{
    self, AmountType, End, EnterEdits, EntryMode, Frequency, Recurrence, ScheduleFields,
    ScheduleId, ScheduleLine, WeekendRule,
};
use kansha_core::testkit::Book;
use kansha_core::{Clock, Date, FixedClock, Money, integrity};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// File format
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Scenario {
    id: String,
    #[allow(dead_code)]
    description: String,
    requirements: Vec<String>,
    as_of: String,
    #[serde(default)]
    accounts: Vec<AccountSpec>,
    #[serde(default)]
    categories: Vec<CategorySpec>,
    #[serde(default)]
    actions: Vec<Action>,
    #[serde(default)]
    expect: Expect,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountSpec {
    name: String,
    #[serde(rename = "type")]
    account_type: String,
    /// Ledger sign; negative for money owed on a liability.
    opening_balance: Option<String>,
    opening_date: Option<String>,
    credit_limit: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CategorySpec {
    /// `"Parent:Child"`; missing parents are created with the same kind.
    path: String,
    kind: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    /// Harness only: add `amount` to the running total.
    Add {
        amount: String,
    },
    /// Harness only: subtract `amount` from the running total.
    Subtract {
        amount: String,
    },
    /// Enter a transaction in `account`'s register.
    Entry(EntrySpec),
    /// Replace the transaction named by `ref`, as seen from `account`.
    Edit(EntrySpec),
    Void(RefSpec),
    Delete(RefSpec),
    SetCleared {
        #[serde(rename = "ref")]
        reference: String,
        account: String,
        cleared: String,
        #[serde(default)]
        confirm: bool,
        expect_error: Option<String>,
    },
    CloseAccount {
        account: String,
        date: String,
        #[serde(default)]
        confirm: bool,
        expect_error: Option<String>,
    },
    ReopenAccount {
        account: String,
        expect_error: Option<String>,
    },
    DeleteAccount {
        account: String,
        expect_error: Option<String>,
    },
    MergeCategories(MergeSpec),
    MergePayees(MergeSpec),
    MergeTags(MergeSpec),
    /// Create a schedule (Phase 4).
    Schedule(ScheduleSpec),
    /// Enter the occurrence of schedule `ref` whose nominal date is `due`.
    EnterOccurrence {
        #[serde(rename = "ref")]
        reference: String,
        due: String,
        /// Name for the transaction it creates.
        txn_ref: Option<String>,
        date: Option<String>,
        amount: Option<String>,
        #[serde(default)]
        confirm: bool,
        expect_error: Option<String>,
    },
    SkipOccurrence {
        #[serde(rename = "ref")]
        reference: String,
        due: String,
        expect_error: Option<String>,
    },
    /// One-time date and/or amount for an occurrence; give neither to clear.
    OverrideOccurrence {
        #[serde(rename = "ref")]
        reference: String,
        due: String,
        date: Option<String>,
        amount: Option<String>,
        expect_error: Option<String>,
    },
    /// Run auto-entry as of `as_of`; optionally check how many it entered.
    AutoEnter {
        expect_entered: Option<usize>,
        expect_error: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScheduleSpec {
    #[serde(rename = "ref")]
    reference: Option<String>,
    account: String,
    payee: Option<String>,
    #[serde(default)]
    memo: String,
    /// The main account's amount, register sign.
    amount: String,
    /// Category or transfer taking the whole amount (or what `lines` leave).
    category: Option<String>,
    transfer: Option<String>,
    #[serde(default)]
    lines: Vec<LineSpec>,
    /// Tag on the (first) line.
    tag: Option<String>,
    frequency: String,
    interval: Option<i64>,
    day1: Option<i64>,
    day2: Option<i64>,
    weekday: Option<i64>,
    week_of_month: Option<i64>,
    start: String,
    /// none, previous, or next.
    weekend_rule: Option<String>,
    end_date: Option<String>,
    /// "# left".
    count: Option<i64>,
    #[serde(default)]
    remind_days: i64,
    /// remind or auto.
    mode: Option<String>,
    /// fixed or estimated.
    amount_type: Option<String>,
    expect_error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EntrySpec {
    /// Name for later actions (`edit`, `void`, ...) and register rows.
    #[serde(rename = "ref")]
    reference: Option<String>,
    date: String,
    account: String,
    /// This account's posting, ledger sign: − payment/charge, + deposit.
    amount: String,
    payee: Option<String>,
    #[serde(default)]
    check_num: String,
    #[serde(default)]
    memo: String,
    /// Category path for the whole amount (or what `lines` leave).
    category: Option<String>,
    /// Transfer account for the whole amount (or what `lines` leave).
    transfer: Option<String>,
    cleared: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    /// Split lines, same sign as `amount`.
    #[serde(default)]
    lines: Vec<LineSpec>,
    #[serde(default)]
    confirm: bool,
    expect_error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LineSpec {
    category: Option<String>,
    transfer: Option<String>,
    amount: String,
    #[serde(default)]
    memo: String,
    cleared: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RefSpec {
    #[serde(rename = "ref")]
    reference: String,
    #[serde(default)]
    confirm: bool,
    expect_error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MergeSpec {
    from: String,
    into: String,
    expect_error: Option<String>,
}

impl Action {
    fn kind(&self) -> &'static str {
        match self {
            Action::Add { .. } => "add",
            Action::Subtract { .. } => "subtract",
            Action::Entry(_) => "entry",
            Action::Edit(_) => "edit",
            Action::Void(_) => "void",
            Action::Delete(_) => "delete",
            Action::SetCleared { .. } => "set_cleared",
            Action::CloseAccount { .. } => "close_account",
            Action::ReopenAccount { .. } => "reopen_account",
            Action::DeleteAccount { .. } => "delete_account",
            Action::MergeCategories(_) => "merge_categories",
            Action::MergePayees(_) => "merge_payees",
            Action::MergeTags(_) => "merge_tags",
            Action::Schedule(_) => "schedule",
            Action::EnterOccurrence { .. } => "enter_occurrence",
            Action::SkipOccurrence { .. } => "skip_occurrence",
            Action::OverrideOccurrence { .. } => "override_occurrence",
            Action::AutoEnter { .. } => "auto_enter",
        }
    }

    fn expect_error(&self) -> Option<&str> {
        match self {
            Action::Add { .. } | Action::Subtract { .. } => None,
            Action::Entry(e) | Action::Edit(e) => e.expect_error.as_deref(),
            Action::Void(r) | Action::Delete(r) => r.expect_error.as_deref(),
            Action::MergeCategories(m) | Action::MergePayees(m) | Action::MergeTags(m) => {
                m.expect_error.as_deref()
            }
            Action::Schedule(sp) => sp.expect_error.as_deref(),
            Action::SetCleared { expect_error, .. }
            | Action::EnterOccurrence { expect_error, .. }
            | Action::SkipOccurrence { expect_error, .. }
            | Action::OverrideOccurrence { expect_error, .. }
            | Action::AutoEnter { expect_error, .. }
            | Action::CloseAccount { expect_error, .. }
            | Action::ReopenAccount { expect_error, .. }
            | Action::DeleteAccount { expect_error, .. } => expect_error.as_deref(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Expect {
    /// Harness only: expected running total.
    total: Option<String>,
    /// Expected `Clock::today()` (always `as_of`).
    today: Option<String>,
    /// Balance as of `as_of` (future-dated entries excluded).
    #[serde(default)]
    balances: BTreeMap<String, String>,
    /// Balance including future-dated entries.
    #[serde(default)]
    ending_balances: BTreeMap<String, String>,
    /// Cleared and reconciled postings only.
    #[serde(default)]
    cleared_balances: BTreeMap<String, String>,
    /// Σ postings to the category itself (expense +, income −).
    #[serde(default)]
    category_totals: BTreeMap<String, String>,
    /// Credit limit + balance as of `as_of`.
    #[serde(default)]
    available_credit: BTreeMap<String, String>,
    /// "open" or "closed".
    #[serde(default)]
    account_status: BTreeMap<String, String>,
    /// Number of transactions in the database.
    txn_count: Option<i64>,
    #[serde(default)]
    register: Vec<RegisterExpect>,
    /// State of schedules after the actions.
    #[serde(default)]
    schedules: Vec<ScheduleExpect>,
    /// Pending occurrences in a date range.
    #[serde(default)]
    occurrences: Vec<OccurrencesExpect>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScheduleExpect {
    #[serde(rename = "ref")]
    reference: String,
    /// Nominal date of the next occurrence, or "none".
    next_due: Option<String>,
    /// active or ended.
    status: Option<String>,
    /// "# left".
    left: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OccurrencesExpect {
    #[serde(rename = "ref")]
    reference: String,
    from: String,
    to: String,
    /// Due dates (weekend rule applied) of the pending occurrences from
    /// `from` to `to`, in order.
    dates: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegisterExpect {
    account: String,
    rows: Vec<RowExpect>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RowExpect {
    date: String,
    amount: String,
    balance: String,
    #[serde(rename = "ref")]
    reference: Option<String>,
    payee: Option<String>,
    /// Category path, `[Account]` for a transfer, `--Split--`, or "".
    counterpart: Option<String>,
    status: Option<String>,
    cleared: Option<String>,
    check_num: Option<String>,
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

enum Detail {
    Mismatch {
        field: String,
        expected: String,
        actual: String,
    },
    Error(String),
}

struct Failure {
    file: PathBuf,
    id: Option<String>,
    step: String,
    detail: Detail,
}

impl Failure {
    fn error(step: impl Into<String>, message: impl ToString) -> Self {
        Failure {
            file: PathBuf::new(),
            id: None,
            step: step.into(),
            detail: Detail::Error(message.to_string()),
        }
    }

    fn mismatch(step: &str, field: &str, expected: impl ToString, actual: impl ToString) -> Self {
        Failure {
            file: PathBuf::new(),
            id: None,
            step: step.into(),
            detail: Detail::Mismatch {
                field: field.into(),
                expected: expected.to_string(),
                actual: actual.to_string(),
            },
        }
    }

    fn render(&self, out: &mut String) {
        let id = self.id.as_deref().unwrap_or("?");
        let _ = writeln!(out, "FAIL {} [{id}]", self.file.display());
        let _ = writeln!(out, "  step:     {}", self.step);
        match &self.detail {
            Detail::Mismatch {
                field,
                expected,
                actual,
            } => {
                let _ = writeln!(out, "  field:    {field}");
                let _ = writeln!(out, "  expected: {expected}");
                let _ = writeln!(out, "  actual:   {actual}");
            }
            Detail::Error(msg) => {
                let _ = writeln!(
                    out,
                    "  error:    {}",
                    msg.trim_end().replace('\n', "\n            ")
                );
            }
        }
    }
}

#[derive(Default)]
struct Report {
    passed: Vec<String>,
    failures: Vec<Failure>,
}

impl Report {
    fn render(&self) -> String {
        let mut out = String::new();
        for f in &self.failures {
            f.render(&mut out);
            out.push('\n');
        }
        let _ = writeln!(
            out,
            "scenarios: {} passed, {} failed",
            self.passed.len(),
            self.failures.len()
        );
        out
    }
}

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root exists")
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        out.push(path.to_path_buf());
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|e| e == "toml") {
            out.push(p);
        }
    }
}

fn run_path(target: &Path) -> Report {
    let root = repo_root();
    let mut files = Vec::new();
    collect(target, &mut files);
    files.sort();

    let mut report = Report::default();
    let mut seen: BTreeMap<String, PathBuf> = BTreeMap::new();

    for file in files {
        let shown = file
            .canonicalize()
            .ok()
            .and_then(|p| p.strip_prefix(&root).ok().map(Path::to_path_buf))
            .unwrap_or_else(|| file.clone());

        let (id, result) = run_file(&file);
        let result = result.and_then(|()| match &id {
            Some(id) => match seen.get(id) {
                Some(first) => Err(Failure::error(
                    "validate",
                    format!("duplicate id; first used in {}", first.display()),
                )),
                None => Ok(()),
            },
            None => Ok(()),
        });
        if let Some(id) = &id {
            seen.entry(id.clone()).or_insert_with(|| shown.clone());
        }
        match result {
            Ok(()) => report.passed.push(id.unwrap_or_default()),
            Err(mut f) => {
                f.file = shown;
                f.id = id;
                report.failures.push(f);
            }
        }
    }
    report
}

fn run_file(path: &Path) -> (Option<String>, Result<(), Failure>) {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => return (None, Err(Failure::error("read", e))),
    };
    let scenario: Scenario = match toml::from_str(&text) {
        Ok(s) => s,
        Err(e) => return (None, Err(Failure::error("parse", e))),
    };
    let id = Some(scenario.id.clone());
    (id, execute(&scenario))
}

fn parse<T>(what: &str, s: &str) -> Result<T, ActErr>
where
    T: std::str::FromStr<Err = kansha_core::Error>,
{
    s.parse()
        .map_err(|e| ActErr::Harness(format!("{what}: {e}")))
}

fn money(step: &str, s: &str) -> Result<Money, Failure> {
    s.parse().map_err(|e| Failure::error(step, e))
}

/// Why an action failed: the scenario is wrong (always a failure), or the
/// engine refused (a pass if `expect_error` matches).
enum ActErr {
    Harness(String),
    Engine(kansha_core::Error),
}

impl From<kansha_core::Error> for ActErr {
    fn from(e: kansha_core::Error) -> Self {
        ActErr::Engine(e)
    }
}

/// State while a scenario runs.
struct Ctx {
    book: Book,
    total: Money,
    refs: BTreeMap<String, TxnId>,
    schedules: BTreeMap<String, ScheduleId>,
}

impl Ctx {
    fn account(&self, name: &str) -> Result<AccountId, ActErr> {
        accounts::list(self.book.conn())?
            .into_iter()
            .find(|a| a.fields.name.eq_ignore_ascii_case(name))
            .map(|a| a.id)
            .ok_or_else(|| ActErr::Harness(format!("unknown account {name:?}")))
    }

    fn category(&self, path: &str) -> Result<CategoryId, ActErr> {
        self.book
            .find_category(path)?
            .ok_or_else(|| ActErr::Harness(format!("unknown category {path:?}")))
    }

    fn txn(&self, reference: &str) -> Result<TxnId, ActErr> {
        self.refs
            .get(reference)
            .copied()
            .ok_or_else(|| ActErr::Harness(format!("unknown ref {reference:?}")))
    }

    fn tags(&mut self, names: &[String]) -> Result<Vec<kansha_core::categories::TagId>, ActErr> {
        names
            .iter()
            .map(|n| self.book.tag(n).map_err(ActErr::from))
            .collect()
    }

    fn target(
        &self,
        category: &Option<String>,
        transfer: &Option<String>,
    ) -> Result<Option<Target>, ActErr> {
        match (category, transfer) {
            (Some(_), Some(_)) => Err(ActErr::Harness(
                "give category or transfer, not both".into(),
            )),
            (Some(c), None) => Ok(Some(Target::Category(self.category(c)?))),
            (None, Some(a)) => Ok(Some(Target::Account(self.account(a)?))),
            (None, None) => Ok(None),
        }
    }

    fn entry(&mut self, spec: &EntrySpec) -> Result<ledger::Entry, ActErr> {
        let account = self.account(&spec.account)?;
        let mut entry = ledger::Entry::new(
            account,
            parse("date", &spec.date)?,
            parse("amount", &spec.amount)?,
        );
        if let Some(p) = &spec.payee {
            entry.payee = Some(self.book.payee(p)?);
        }
        entry.check_num = spec.check_num.clone();
        entry.memo = spec.memo.clone();
        if let Some(c) = &spec.cleared {
            entry.cleared = parse("cleared", c)?;
        }
        entry.tags = self.tags(&spec.tags)?;
        for l in &spec.lines {
            let target = self
                .target(&l.category, &l.transfer)?
                .ok_or_else(|| ActErr::Harness("a line needs a category or a transfer".into()))?;
            let mut line = EntryLine::new(target, parse("line amount", &l.amount)?);
            line.memo = l.memo.clone();
            if let Some(c) = &l.cleared {
                line.cleared = parse("cleared", c)?;
            }
            line.tags = self.tags(&l.tags)?;
            entry.lines.push(line);
        }
        if let Some(target) = self.target(&spec.category, &spec.transfer)? {
            let rest = entry.remainder()?;
            entry.lines.push(EntryLine::new(target, rest));
        }
        Ok(entry)
    }

    fn schedule_id(&self, reference: &str) -> Result<ScheduleId, ActErr> {
        self.schedules
            .get(reference)
            .copied()
            .ok_or_else(|| ActErr::Harness(format!("unknown schedule ref {reference:?}")))
    }

    fn schedule_fields(&mut self, spec: &ScheduleSpec) -> Result<ScheduleFields, ActErr> {
        let account = self.account(&spec.account)?;
        let amount: Money = parse("amount", &spec.amount)?;
        let tag = match &spec.tag {
            Some(t) => Some(self.book.tag(t)?),
            None => None,
        };
        let mut lines = Vec::new();
        for l in &spec.lines {
            let target = self
                .target(&l.category, &l.transfer)?
                .ok_or_else(|| ActErr::Harness("a line needs a category or a transfer".into()))?;
            let mut line = ScheduleLine {
                target,
                amount: parse("line amount", &l.amount)?,
                memo: l.memo.clone(),
                tag: None,
            };
            if let Some(t) = l.tags.first() {
                line.tag = Some(self.book.tag(t)?);
            }
            lines.push(line);
        }
        if let Some(target) = self.target(&spec.category, &spec.transfer)? {
            let rest = ledger::split_remainder(amount, lines.iter().map(|l| l.amount))?;
            lines.push(ScheduleLine {
                target,
                amount: rest,
                memo: String::new(),
                tag: None,
            });
        }
        if let Some(first) = lines.first_mut() {
            first.tag = first.tag.or(tag);
        }
        let mut rec = Recurrence::new(
            parse::<Frequency>("frequency", &spec.frequency)?,
            parse("start", &spec.start)?,
        );
        rec.interval = spec.interval.unwrap_or(1);
        rec.day1 = spec.day1;
        rec.day2 = spec.day2;
        rec.weekday = spec.weekday;
        rec.week_of_month = spec.week_of_month;
        if let Some(w) = &spec.weekend_rule {
            rec.weekend_rule = parse::<WeekendRule>("weekend_rule", w)?;
        }
        let end = match (&spec.end_date, spec.count) {
            (Some(_), Some(_)) => {
                return Err(ActErr::Harness("give end_date or count, not both".into()));
            }
            (Some(d), None) => End::OnDate {
                date: parse("end_date", d)?,
            },
            (None, Some(count)) => End::AfterCount { count },
            (None, None) => End::Never,
        };
        let payee = match &spec.payee {
            Some(p) => Some(self.book.payee(p)?),
            None => None,
        };
        Ok(ScheduleFields {
            account,
            payee,
            memo: spec.memo.clone(),
            amount_type: match &spec.amount_type {
                Some(t) => parse::<AmountType>("amount_type", t)?,
                None => AmountType::Fixed,
            },
            lines,
            recurrence: rec,
            end,
            remind_days: spec.remind_days,
            mode: match &spec.mode {
                Some(m) => parse::<EntryMode>("mode", m)?,
                None => EntryMode::Remind,
            },
        })
    }

    fn remember(&mut self, reference: &Option<String>, id: TxnId) {
        if let Some(r) = reference {
            self.refs.insert(r.clone(), id);
        }
    }

    fn run(&mut self, action: &Action) -> Result<(), ActErr> {
        match action {
            Action::Add { amount } => self.total += parse::<Money>("amount", amount)?,
            Action::Subtract { amount } => self.total -= parse::<Money>("amount", amount)?,
            Action::Entry(spec) => {
                let entry = self.entry(spec)?;
                let t = self.book.write(|tx| ledger::create_entry(tx, &entry))?;
                self.remember(&spec.reference, t.id);
            }
            Action::Edit(spec) => {
                let reference = spec
                    .reference
                    .as_deref()
                    .ok_or_else(|| ActErr::Harness("edit needs a ref".into()))?;
                let id = self.txn(reference)?;
                let entry = self.entry(spec)?;
                self.book
                    .write(|tx| ledger::update_entry(tx, id, &entry, spec.confirm))?;
            }
            Action::Void(r) => {
                let id = self.txn(&r.reference)?;
                self.book.write(|tx| ledger::void(tx, id, r.confirm))?;
            }
            Action::Delete(r) => {
                let id = self.txn(&r.reference)?;
                self.book.write(|tx| ledger::delete(tx, id, r.confirm))?;
            }
            Action::SetCleared {
                reference,
                account,
                cleared,
                confirm,
                ..
            } => {
                let id = self.txn(reference)?;
                let account = self.account(account)?;
                let cleared: Cleared = parse("cleared", cleared)?;
                self.book
                    .write(|tx| ledger::set_cleared(tx, id, account, cleared, *confirm))?;
            }
            Action::CloseAccount {
                account,
                date,
                confirm,
                ..
            } => {
                let account = self.account(account)?;
                let date: Date = parse("date", date)?;
                self.book
                    .write(|tx| ledger::close_account(tx, account, date, *confirm))?;
            }
            Action::ReopenAccount { account, .. } => {
                let account = self.account(account)?;
                self.book.write(|tx| accounts::reopen(tx, account))?;
            }
            Action::DeleteAccount { account, .. } => {
                let account = self.account(account)?;
                self.book.write(|tx| accounts::delete(tx, account))?;
            }
            Action::MergeCategories(m) => {
                let (from, into) = (self.category(&m.from)?, self.category(&m.into)?);
                self.book.write(|tx| categories::merge(tx, from, into))?;
            }
            Action::MergePayees(m) => {
                let find = |name: &str| -> Result<_, ActErr> {
                    payees::find_by_name(self.book.conn(), name)?
                        .map(|p| p.id)
                        .ok_or_else(|| ActErr::Harness(format!("unknown payee {name:?}")))
                };
                let (from, into) = (find(&m.from)?, find(&m.into)?);
                self.book.write(|tx| payees::merge(tx, from, into))?;
            }
            Action::Schedule(spec) => {
                let fields = self.schedule_fields(spec)?;
                let created = self.book.write(|tx| schedule::create(tx, &fields))?;
                if let Some(r) = &spec.reference {
                    self.schedules.insert(r.clone(), created.id);
                }
            }
            Action::EnterOccurrence {
                reference,
                due,
                txn_ref,
                date,
                amount,
                confirm,
                ..
            } => {
                let id = self.schedule_id(reference)?;
                let edits = EnterEdits {
                    date: date.as_deref().map(|d| parse("date", d)).transpose()?,
                    amount: amount.as_deref().map(|a| parse("amount", a)).transpose()?,
                };
                let due: Date = parse("due", due)?;
                let entered = self
                    .book
                    .write(|tx| schedule::enter(tx, id, due, &edits, *confirm))?;
                self.remember(txn_ref, entered.txn);
            }
            Action::SkipOccurrence { reference, due, .. } => {
                let id = self.schedule_id(reference)?;
                let due: Date = parse("due", due)?;
                self.book.write(|tx| schedule::skip(tx, id, due))?;
            }
            Action::OverrideOccurrence {
                reference,
                due,
                date,
                amount,
                ..
            } => {
                let id = self.schedule_id(reference)?;
                let due: Date = parse("due", due)?;
                let date: Option<Date> = date.as_deref().map(|d| parse("date", d)).transpose()?;
                let amount: Option<Money> =
                    amount.as_deref().map(|a| parse("amount", a)).transpose()?;
                self.book
                    .write(|tx| schedule::set_override(tx, id, due, date, amount))?;
            }
            Action::AutoEnter { expect_entered, .. } => {
                let clock = *self.book.clock();
                let report = schedule::auto_enter_due(self.book.db_mut(), &clock)?;
                if let Some(n) = expect_entered {
                    if report.entered.len() != *n {
                        return Err(ActErr::Harness(format!(
                            "auto_enter entered {}, expected {n}",
                            report.entered.len()
                        )));
                    }
                }
                if let Some(f) = report.failed.first() {
                    return Err(ActErr::Harness(format!("auto_enter failed: {}", f.reason)));
                }
            }
            Action::MergeTags(m) => {
                let all = tags::list(self.book.conn())?;
                let find = |name: &str| {
                    all.iter()
                        .find(|t| t.fields.name.eq_ignore_ascii_case(name))
                        .map(|t| t.id)
                        .ok_or_else(|| ActErr::Harness(format!("unknown tag {name:?}")))
                };
                let (from, into) = (find(&m.from)?, find(&m.into)?);
                self.book.write(|tx| tags::merge(tx, from, into))?;
            }
        }
        Ok(())
    }

    /// Category path by ID, for register counterparts.
    fn category_path(&self, id: CategoryId) -> Result<String, ActErr> {
        let all = categories::list(self.book.conn())?;
        let mut parts = Vec::new();
        let mut cursor = Some(id);
        while let Some(c) = cursor {
            let cat = all
                .iter()
                .find(|x| x.id == c)
                .ok_or_else(|| ActErr::Harness(format!("category {} missing", c.0)))?;
            parts.push(cat.fields.name.clone());
            cursor = cat.fields.parent;
        }
        parts.reverse();
        Ok(parts.join(":"))
    }

    fn counterpart(&self, c: Counterpart) -> Result<String, ActErr> {
        Ok(match c {
            Counterpart::None => String::new(),
            Counterpart::Split => "--Split--".into(),
            Counterpart::Category(id) => self.category_path(id)?,
            Counterpart::Transfer(id) => {
                format!("[{}]", accounts::get(self.book.conn(), id)?.fields.name)
            }
        })
    }
}

fn setup(s: &Scenario, clock: FixedClock) -> Result<Ctx, Failure> {
    let book = Book::with_clock(clock).map_err(|e| Failure::error("setup", e))?;
    let mut ctx = Ctx {
        book,
        total: Money::ZERO,
        refs: BTreeMap::new(),
        schedules: BTreeMap::new(),
    };
    for (i, a) in s.accounts.iter().enumerate() {
        let step = format!("accounts[{}] ({})", i + 1, a.name);
        let r = (|| -> Result<(), ActErr> {
            let t: AccountType = parse("type", &a.account_type)?;
            let mut f = AccountFields::new(a.name.clone(), t);
            if let Some(limit) = &a.credit_limit {
                f.credit_limit = Some(parse("credit_limit", limit)?);
            }
            let id = ctx.book.account_with(&f)?;
            match (&a.opening_balance, &a.opening_date) {
                (Some(amount), Some(date)) => {
                    ctx.book.opening_balance(
                        id,
                        parse("opening_date", date)?,
                        parse("opening_balance", amount)?,
                    )?;
                }
                (None, None) => {}
                _ => {
                    return Err(ActErr::Harness(
                        "opening_balance and opening_date go together".into(),
                    ));
                }
            }
            Ok(())
        })();
        r.map_err(|e| act_failure(&step, e))?;
    }
    for (i, c) in s.categories.iter().enumerate() {
        let step = format!("categories[{}] ({})", i + 1, c.path);
        let r = (|| -> Result<(), ActErr> {
            let kind: CategoryKind = parse("kind", &c.kind)?;
            ctx.book.category(&c.path, kind)?;
            Ok(())
        })();
        r.map_err(|e| act_failure(&step, e))?;
    }
    Ok(ctx)
}

fn act_failure(step: &str, e: ActErr) -> Failure {
    match e {
        ActErr::Harness(m) => Failure::error(step, m),
        ActErr::Engine(e) => Failure::error(step, e),
    }
}

fn execute(s: &Scenario) -> Result<(), Failure> {
    if s.id.trim().is_empty() {
        return Err(Failure::error("validate", "id is empty"));
    }
    if s.requirements.is_empty() {
        return Err(Failure::error(
            "validate",
            "requirements must list at least one requirement ID",
        ));
    }
    let as_of: Date = s.as_of.parse().map_err(|e| Failure::error("as_of", e))?;
    let clock = FixedClock::new(as_of);
    let mut ctx = setup(s, clock)?;

    for (i, action) in s.actions.iter().enumerate() {
        let step = format!("action {} ({})", i + 1, action.kind());
        match (ctx.run(action), action.expect_error()) {
            (Ok(()), None) => {}
            (Ok(()), Some(want)) => {
                return Err(Failure::mismatch(&step, "error", want, "(succeeded)"));
            }
            (Err(ActErr::Engine(e)), Some(want)) => {
                if !e.to_string().contains(want) {
                    return Err(Failure::mismatch(&step, "error", want, e));
                }
            }
            (Err(e), _) => return Err(act_failure(&step, e)),
        }
    }

    check_expectations(s, &ctx, as_of, &clock).map_err(|e| match e {
        Checked::Fail(f) => f,
        Checked::Err(step, e) => act_failure(&step, e),
    })?;

    let report = integrity::check(ctx.book.conn()).map_err(|e| Failure::error("integrity", e))?;
    if !report.is_clean() {
        let lines: Vec<String> = report
            .issues
            .iter()
            .map(|i| format!("{:?} {} {:?}: {}", i.check, i.table, i.id, i.detail))
            .collect();
        return Err(Failure::error("integrity", lines.join("\n")));
    }
    Ok(())
}

enum Checked {
    Fail(Failure),
    Err(String, ActErr),
}

fn compare(step: &str, field: &str, expected: &str, actual: &str) -> Result<(), Checked> {
    if expected == actual {
        Ok(())
    } else {
        Err(Checked::Fail(Failure::mismatch(
            step, field, expected, actual,
        )))
    }
}

/// Normalize an expected amount ("5" → "5.00") so text compares exactly.
fn norm_money(step: &str, s: &str) -> Result<String, Checked> {
    s.parse::<Money>()
        .map(|m| m.to_string())
        .map_err(|e| Checked::Err(step.into(), ActErr::Harness(e.to_string())))
}

fn check_expectations(
    s: &Scenario,
    ctx: &Ctx,
    as_of: Date,
    clock: &FixedClock,
) -> Result<(), Checked> {
    let e = &s.expect;
    if let Some(expected) = &e.total {
        let expected = money("expect.total", expected).map_err(Checked::Fail)?;
        if expected != ctx.total {
            return Err(Checked::Fail(Failure::mismatch(
                "expect.total",
                "total",
                expected,
                ctx.total,
            )));
        }
    }
    if let Some(expected) = &e.today {
        let expected: Date = expected
            .parse()
            .map_err(|e| Checked::Fail(Failure::error("expect.today", e)))?;
        if expected != clock.today() {
            return Err(Checked::Fail(Failure::mismatch(
                "expect.today",
                "today",
                expected,
                clock.today(),
            )));
        }
    }

    let conn = ctx.book.conn();
    let per_account = |step: &str,
                       map: &BTreeMap<String, String>,
                       f: &dyn Fn(AccountId) -> Result<String, ActErr>|
     -> Result<(), Checked> {
        for (name, expected) in map {
            let err = |e| Checked::Err(step.to_string(), e);
            let id = ctx.account(name).map_err(err)?;
            let actual = f(id).map_err(err)?;
            let expected = if expected == "none" || expected == "open" || expected == "closed" {
                expected.clone()
            } else {
                norm_money(step, expected)?
            };
            compare(step, name, &expected, &actual)?;
        }
        Ok(())
    };
    per_account("expect.balances", &e.balances, &|id| {
        Ok(ledger::balance(conn, id, Some(as_of))?.to_string())
    })?;
    per_account("expect.ending_balances", &e.ending_balances, &|id| {
        Ok(ledger::balance(conn, id, None)?.to_string())
    })?;
    per_account("expect.cleared_balances", &e.cleared_balances, &|id| {
        Ok(ledger::cleared_balance(conn, id, None)?.to_string())
    })?;
    per_account("expect.available_credit", &e.available_credit, &|id| {
        Ok(ledger::register_summary(conn, id, as_of)?
            .available_credit
            .map_or("none".into(), |m| m.to_string()))
    })?;
    per_account("expect.account_status", &e.account_status, &|id| {
        Ok(accounts::get(conn, id)?.status.to_string())
    })?;

    for (path, expected) in &e.category_totals {
        let step = "expect.category_totals";
        let err = |e| Checked::Err(step.to_string(), e);
        let id = ctx.category(path).map_err(err)?;
        let actual = ledger::category_total(conn, id, None)
            .map_err(|e| err(e.into()))?
            .to_string();
        compare(step, path, &norm_money(step, expected)?, &actual)?;
    }

    if let Some(expected) = e.txn_count {
        let actual: i64 = conn
            .query_row("SELECT count(*) FROM txn", [], |r| r.get(0))
            .map_err(|e| Checked::Err("expect.txn_count".into(), ActErr::Harness(e.to_string())))?;
        compare(
            "expect.txn_count",
            "txn_count",
            &expected.to_string(),
            &actual.to_string(),
        )?;
    }

    for sx in &e.schedules {
        let step = format!("expect.schedules ({})", sx.reference);
        let err = |e| Checked::Err(step.clone(), e);
        let id = ctx.schedule_id(&sx.reference).map_err(err)?;
        let sched = schedules::get(conn, id).map_err(|e| err(e.into()))?;
        if let Some(want) = &sx.next_due {
            let actual = sched.next_due.map_or("none".to_string(), |d| d.to_string());
            compare(&step, "next_due", want, &actual)?;
        }
        if let Some(want) = &sx.status {
            compare(&step, "status", want, sched.status.as_str())?;
        }
        if let Some(want) = sx.left {
            let actual = match sched.fields.end {
                End::AfterCount { count } => count.to_string(),
                _ => "none".into(),
            };
            compare(&step, "left", &want.to_string(), &actual)?;
        }
    }

    for ox in &e.occurrences {
        let step = format!("expect.occurrences ({})", ox.reference);
        let err = |e| Checked::Err(step.clone(), e);
        let id = ctx.schedule_id(&ox.reference).map_err(err)?;
        let from: Date = ox
            .from
            .parse()
            .map_err(|e: kansha_core::Error| err(ActErr::Harness(e.to_string())))?;
        let to: Date = ox
            .to
            .parse()
            .map_err(|e: kansha_core::Error| err(ActErr::Harness(e.to_string())))?;
        let views = schedule::occurrences_between(conn, from, to, as_of, None, false)
            .map_err(|e| err(e.into()))?;
        let actual: Vec<String> = views
            .iter()
            .filter(|v| v.schedule == id)
            .map(|v| v.date.to_string())
            .collect();
        compare(&step, "dates", &ox.dates.join(", "), &actual.join(", "))?;
    }

    for reg in &e.register {
        let step = format!("expect.register ({})", reg.account);
        let err = |e| Checked::Err(step.clone(), e);
        let id = ctx.account(&reg.account).map_err(err)?;
        let rows = ledger::register(conn, id).map_err(|e| err(e.into()))?;
        compare(
            &step,
            "row count",
            &reg.rows.len().to_string(),
            &rows.len().to_string(),
        )?;
        for (i, (want, got)) in reg.rows.iter().zip(&rows).enumerate() {
            let field = |name: &str| format!("row {} {name}", i + 1);
            compare(&step, &field("date"), &want.date, &got.date.to_string())?;
            compare(
                &step,
                &field("amount"),
                &norm_money(&step, &want.amount)?,
                &got.amount.to_string(),
            )?;
            compare(
                &step,
                &field("balance"),
                &norm_money(&step, &want.balance)?,
                &got.balance.to_string(),
            )?;
            if let Some(r) = &want.reference {
                let id = ctx.txn(r).map_err(err)?;
                let actual = if got.txn_id == id {
                    r.clone()
                } else {
                    format!("txn {}", got.txn_id.0)
                };
                compare(&step, &field("ref"), r, &actual)?;
            }
            if let Some(p) = &want.payee {
                let actual = match got.payee {
                    Some(pid) => {
                        payees::get(conn, pid)
                            .map_err(|e| err(e.into()))?
                            .fields
                            .name
                    }
                    None => String::new(),
                };
                compare(&step, &field("payee"), p, &actual)?;
            }
            if let Some(c) = &want.counterpart {
                let actual = ctx.counterpart(got.counterpart).map_err(err)?;
                compare(&step, &field("counterpart"), c, &actual)?;
            }
            if let Some(st) = &want.status {
                compare(&step, &field("status"), st, got.status.as_str())?;
            }
            if let Some(c) = &want.cleared {
                compare(&step, &field("cleared"), c, got.cleared.as_str())?;
            }
            if let Some(n) = &want.check_num {
                compare(&step, &field("check_num"), n, &got.check_num)?;
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn scenarios() {
    let root = repo_root();
    let target = match std::env::var_os("KANSHA_SCENARIOS") {
        Some(p) => root.join(p),
        None => root.join("tests/scenarios"),
    };
    let report = run_path(&target);
    let total = report.passed.len() + report.failures.len();
    assert!(
        total > 0,
        "no scenario files found under {}",
        target.display()
    );
    if !report.failures.is_empty() {
        panic!("\n\n{}", report.render());
    }
    println!("{}", report.render());
}

#[test]
fn failing_scenarios_are_reported_clearly() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/failing");
    let report = run_path(&dir);
    let text = report.render();
    assert_eq!(report.failures.len(), 2, "{text}");
    for needle in [
        // Wrong expected value: file, id, step, field, expected, actual.
        "HARNESS-901.toml [HARNESS-901]",
        "step:     expect.total",
        "field:    total",
        "expected: 10.00",
        "actual:   12.34",
        // Typo in a field name is caught, not ignored.
        "HARNESS-902.toml [?]",
        "step:     parse",
        "amout",
        "scenarios: 0 passed, 2 failed",
    ] {
        assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
    }
}
