//! Matching entered account and category names (CLI-030, CLI-040).
//!
//! Case is ignored. A category resolves by its full path (`Food:Groceries`)
//! or by a last part no other category shares (`Groceries`). A name that
//! matches nothing, or more than one, is an error listing up to five close
//! matches: names starting with what was typed, then names within two
//! edits of it. Nothing is guessed.

use std::fmt;

use serde::Serialize;

use super::format::{EntryFile, LineError, NamesFile, Spend};

/// Close matches listed with an error.
const MAX_MATCHES: usize = 5;

/// Account or category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum NameKind {
    Account,
    Category,
}

impl NameKind {
    fn word(self) -> &'static str {
        match self {
            NameKind::Account => "account",
            NameKind::Category => "category",
        }
    }
}

/// A name that did not resolve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct NameError {
    pub kind: NameKind,
    pub name: String,
    /// More than one name fits; `matches` lists them.
    pub ambiguous: bool,
    /// Close matches, or for an ambiguous name every fit.
    pub matches: Vec<String>,
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = self.kind.word();
        if self.ambiguous {
            write!(
                f,
                "{kind} “{}” could be {}",
                self.name,
                self.matches.join(" or ")
            )
        } else if self.matches.is_empty() {
            write!(f, "no {kind} “{}”", self.name)
        } else {
            write!(
                f,
                "no {kind} “{}”; did you mean {}?",
                self.name,
                self.matches.join(", ")
            )
        }
    }
}

/// The names entries may use, as spelled in the book.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Names {
    pub accounts: Vec<String>,
    pub categories: Vec<String>,
}

impl From<&NamesFile> for Names {
    fn from(f: &NamesFile) -> Names {
        Names {
            accounts: f.accounts.clone(),
            categories: f.categories.clone(),
        }
    }
}

fn last_part(path: &str) -> &str {
    path.rsplit(':').next().unwrap_or(path)
}

impl Names {
    /// The account as spelled in the book.
    pub fn resolve_account(&self, name: &str) -> Result<String, NameError> {
        let want = name.trim().to_lowercase();
        if let Some(a) = self.accounts.iter().find(|a| a.to_lowercase() == want) {
            return Ok(a.clone());
        }
        Err(NameError {
            kind: NameKind::Account,
            name: name.to_string(),
            ambiguous: false,
            matches: close(&self.accounts, &want, false),
        })
    }

    /// The category's full path as spelled in the book.
    pub fn resolve_category(&self, name: &str) -> Result<String, NameError> {
        let want = name.trim().to_lowercase();
        if let Some(c) = self.categories.iter().find(|c| c.to_lowercase() == want) {
            return Ok(c.clone());
        }
        let fits: Vec<&String> = if want.contains(':') {
            Vec::new()
        } else {
            self.categories
                .iter()
                .filter(|c| last_part(c).to_lowercase() == want)
                .collect()
        };
        match fits[..] {
            [one] => Ok(one.clone()),
            [] => Err(NameError {
                kind: NameKind::Category,
                name: name.to_string(),
                ambiguous: false,
                matches: close(&self.categories, &want, true),
            }),
            _ => Err(NameError {
                kind: NameKind::Category,
                name: name.to_string(),
                ambiguous: true,
                matches: fits.into_iter().cloned().collect(),
            }),
        }
    }

    /// Resolve every name in `file`, spelling each as in the book. Errors
    /// carry the line the name is on.
    pub fn check(&self, file: &EntryFile) -> Result<EntryFile, Vec<LineError>> {
        let mut out = file.clone();
        let mut errors = Vec::new();
        for e in &mut out.entries {
            match self.resolve_account(&e.account) {
                Ok(a) => e.account = a,
                Err(err) => errors.push(LineError::new(e.line, err.to_string())),
            }
            match &mut e.spend {
                Spend::Category(c) => match self.resolve_category(c) {
                    Ok(r) => *c = r,
                    Err(err) => errors.push(LineError::new(e.line, err.to_string())),
                },
                Spend::Split(lines) => {
                    for l in lines {
                        match self.resolve_category(&l.category) {
                            Ok(r) => l.category = r,
                            Err(err) => errors.push(LineError::new(l.line, err.to_string())),
                        }
                    }
                }
            }
        }
        if errors.is_empty() {
            Ok(out)
        } else {
            errors.sort_by_key(|e| e.line);
            Err(errors)
        }
    }
}

/// Up to five names close to `want` (lower case): those starting with it
/// (or whose last part does), then those within two edits.
fn close(names: &[String], want: &str, paths: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let parts = |n: &str| -> Vec<String> {
        let full = n.to_lowercase();
        if paths && full.contains(':') {
            vec![last_part(&full).to_string(), full]
        } else {
            vec![full]
        }
    };
    if !want.is_empty() {
        for n in names {
            if parts(n).iter().any(|p| p.starts_with(want)) && !out.contains(n) {
                out.push(n.clone());
            }
        }
    }
    let mut near: Vec<(usize, &String)> = names
        .iter()
        .filter(|n| !out.contains(n))
        .filter_map(|n| {
            parts(n)
                .iter()
                .map(|p| edit_distance(p, want))
                .min()
                .filter(|d| *d <= 2)
                .map(|d| (d, n))
        })
        .collect();
    near.sort_by_key(|(d, _)| *d);
    out.extend(near.into_iter().map(|(_, n)| n.clone()));
    out.truncate(MAX_MATCHES);
    out
}

/// Levenshtein distance by characters.
fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != *cb);
            cur[j + 1] = sub.min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entries::format::{Dates, parse_entries};

    fn names() -> Names {
        Names {
            accounts: vec!["Checking".into(), "Double Cash".into(), "Savings".into()],
            categories: [
                "Auto",
                "Auto:Fuel",
                "Auto:Gas",
                "Food",
                "Food:Groceries",
                "Food:Dining",
                "Household",
                "Utilities",
                "Utilities:Gas",
            ]
            .map(String::from)
            .to_vec(),
        }
    }

    #[test]
    fn exact_ignoring_case() {
        let n = names();
        assert_eq!(n.resolve_account("double cash"), Ok("Double Cash".into()));
        assert_eq!(
            n.resolve_category("food:GROCERIES"),
            Ok("Food:Groceries".into())
        );
        assert_eq!(n.resolve_category("auto"), Ok("Auto".into()));
    }

    #[test]
    fn unique_last_part() {
        let n = names();
        assert_eq!(n.resolve_category("groceries"), Ok("Food:Groceries".into()));
        assert_eq!(n.resolve_category("Fuel"), Ok("Auto:Fuel".into()));
    }

    #[test]
    fn ambiguous_last_part() {
        let e = names().resolve_category("gas").unwrap_err();
        assert!(e.ambiguous);
        assert_eq!(e.matches, ["Auto:Gas", "Utilities:Gas"]);
        assert_eq!(
            e.to_string(),
            "category “gas” could be Auto:Gas or Utilities:Gas"
        );
    }

    #[test]
    fn close_matches() {
        let n = names();
        let e = n.resolve_category("Grocries").unwrap_err();
        assert!(!e.ambiguous);
        assert_eq!(e.matches, ["Food:Groceries"]);
        assert_eq!(
            e.to_string(),
            "no category “Grocries”; did you mean Food:Groceries?"
        );
        let e = n.resolve_category("Foo").unwrap_err();
        assert_eq!(e.matches, ["Food", "Food:Groceries", "Food:Dining"]);
        let e = n.resolve_account("Chekcing").unwrap_err();
        assert_eq!(e.matches, ["Checking"]);
        let e = n.resolve_account("Brokerage").unwrap_err();
        assert!(e.matches.is_empty());
        assert_eq!(e.to_string(), "no account “Brokerage”");
    }

    #[test]
    fn a_path_that_is_not_there_does_not_fall_back() {
        let e = names().resolve_category("Home:Groceries").unwrap_err();
        assert!(!e.ambiguous);
    }

    #[test]
    fn check_spells_names_as_in_the_book_and_reports_lines() {
        let file = parse_entries(
            "kansha-entries 1
2026-10-12 Shell 45.20 fuel acct checking
2026-10-12 Costco 30.00 split acct Chekcing
  10.00 groceries
  20.00 gas
",
            Dates::Iso,
        )
        .unwrap();
        let errs = names().check(&file).unwrap_err();
        let shown: Vec<String> = errs.iter().map(ToString::to_string).collect();
        assert_eq!(
            shown,
            [
                "line 3: no account “Chekcing”; did you mean Checking?",
                "line 5: category “gas” could be Auto:Gas or Utilities:Gas",
            ]
        );
        let ok = parse_entries(
            "kansha-entries 1\n2026-10-12 Shell 45.20 fuel acct checking\n",
            Dates::Iso,
        )
        .unwrap();
        let checked = names().check(&ok).unwrap();
        assert_eq!(checked.entries[0].account, "Checking");
        assert_eq!(
            checked.entries[0].spend,
            Spend::Category("Auto:Fuel".into())
        );
    }

    #[test]
    fn edit_distances() {
        assert_eq!(edit_distance("kitten", "sitting"), 3);
        assert_eq!(edit_distance("", "ab"), 2);
        assert_eq!(edit_distance("gas", "gas"), 0);
    }
}
