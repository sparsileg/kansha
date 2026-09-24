//! Scenario runner (TEST-050, TEST-060).
//!
//! Discovers every `*.toml` file under `tests/scenarios/` (repo root) and
//! runs it. Set `KANSHA_SCENARIOS` to a file or directory (relative to the
//! repo root) to run just those — `just scenario <path>` does this.
//!
//! Phase 0 supports only the harness actions (`add`, `subtract`) and
//! expectations (`total`, `today`). Each later phase extends `Action` and
//! `Expect` for its area.

// Failure is large; fine for a test runner.
#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use kansha_core::{Clock, Date, FixedClock, Money};
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
    actions: Vec<Action>,
    #[serde(default)]
    expect: Expect,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    /// Harness only: add `amount` to the running total.
    Add { amount: String },
    /// Harness only: subtract `amount` from the running total.
    Subtract { amount: String },
}

impl Action {
    fn kind(&self) -> &'static str {
        match self {
            Action::Add { .. } => "add",
            Action::Subtract { .. } => "subtract",
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

fn money(step: &str, s: &str) -> Result<Money, Failure> {
    s.parse().map_err(|e| Failure::error(step, e))
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

    let mut total = Money::ZERO;
    for (i, action) in s.actions.iter().enumerate() {
        let step = format!("action {} ({})", i + 1, action.kind());
        match action {
            Action::Add { amount } => total += money(&step, amount)?,
            Action::Subtract { amount } => total -= money(&step, amount)?,
        }
    }

    if let Some(expected) = &s.expect.total {
        let expected = money("expect.total", expected)?;
        if expected != total {
            return Err(Failure::mismatch("expect.total", "total", expected, total));
        }
    }
    if let Some(expected) = &s.expect.today {
        let expected: Date = expected
            .parse()
            .map_err(|e| Failure::error("expect.today", e))?;
        if expected != clock.today() {
            return Err(Failure::mismatch(
                "expect.today",
                "today",
                expected,
                clock.today(),
            ));
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
