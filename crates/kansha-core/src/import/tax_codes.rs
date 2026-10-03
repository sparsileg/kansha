//! Quicken tax codes (CAT-050, MIG-020).
//!
//! A QIF category list carries Quicken's tax code on each tax-related
//! category (`R4416` on `Tax:Real Estate`). [`CODES`] maps the codes Stan's
//! file uses to Kansha's built-in tax lines (migration 0003 names, which
//! are Quicken's). The import sets the line on a category it creates;
//! [`plan`] and [`apply`] set it once on a book's existing categories,
//! never overwriting a line already set. Built-in categories are never
//! changed.

use std::collections::HashMap;

use rusqlite::Connection;
use serde::Serialize;

use super::qif::QifCategory;
use crate::categories::{CategoryId, TaxLine, TaxLineId};
use crate::error::Result;
use crate::persistence::{Tx, categories, reports};

/// Quicken tax code → built-in (form, line).
const CODES: &[(u32, &str, &str)] = &[
    (4112, "Form 1040", "Other income, misc."),
    (8336, "Form 1040", "Fed. estimated tax, quarterly"),
    (4416, "Schedule A", "Real estate taxes"),
    (8560, "Schedule A", "Personal property taxes"),
    (4480, "Schedule A", "Cash charity contributions"),
    (7760, "Schedule A", "Non-cash charity contributions"),
    (8352, "Schedule A", "State estimated tax, quarterly"),
    (4400, "Schedule A", "State income taxes"),
    (4592, "Schedule B", "Interest income"),
    (4576, "Schedule B", "Dividend income"),
    (8512, "1099-R", "IRA federal tax withheld"),
    (8528, "1099-R", "IRA state tax withheld"),
    (4160, "1099-G", "State and local tax refunds"),
    (7664, "1099-G", "Unemployment compensation"),
    (4256, "SSA-1099", "Net social security benefits"),
    (9776, "SSA-1099", "Federal tax withheld"),
    (7808, "1099-DIV", "Total capital gain distr."),
    (7360, "W-2", "Salary or wages"),
    (7376, "W-2", "Federal tax withheld"),
    (7392, "W-2", "Social Security tax withheld"),
    (7424, "W-2", "State tax withheld"),
    (7680, "W-2", "Medicare tax withheld"),
];

/// The built-in (form, line) a Quicken tax code stands for.
pub fn form_line(code: u32) -> Option<(&'static str, &'static str)> {
    CODES
        .iter()
        .find(|(c, _, _)| *c == code)
        .map(|(_, form, line)| (*form, *line))
}

/// The book's tax line for a Quicken tax code, if the code maps.
pub(crate) fn tax_line_id(lines: &[TaxLine], code: u32) -> Option<TaxLineId> {
    let (form, line) = form_line(code)?;
    lines
        .iter()
        .find(|t| t.form == form && t.line == line)
        .map(|t| t.id)
}

/// The book's categories by full path (`Parent:Child`), lower-cased.
pub(crate) fn category_paths(conn: &Connection) -> Result<HashMap<String, CategoryId>> {
    let mut paths: HashMap<CategoryId, String> = HashMap::new();
    let mut out = HashMap::new();
    for c in categories::list(conn)? {
        let path = match c.fields.parent.and_then(|p| paths.get(&p)) {
            Some(parent) => format!("{parent}:{}", c.fields.name),
            None => c.fields.name.clone(),
        };
        out.insert(path.to_lowercase(), c.id);
        paths.insert(c.id, path);
    }
    Ok(out)
}

/// What setting tax lines from a QIF does to one of its categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum TaxLinePlanStatus {
    /// The category has no tax line and gets the code's.
    Set,
    /// The category already has a tax line; it is kept.
    Kept,
    /// Kansha has no line for the code.
    Unmapped,
    /// No category of that path in the book.
    Missing,
    /// A built-in category; its tax line is left alone.
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TaxLinePlanItem {
    /// As the file writes it (`Tax:Real Estate`).
    pub qif_name: String,
    pub code: u32,
    /// The book's category of that path.
    pub category: Option<CategoryId>,
    /// `Set`: the line it gets. `Kept`, `System`: the line it has.
    /// `Missing`: the code's line. `Unmapped`: none.
    pub tax_line: Option<TaxLineId>,
    pub status: TaxLinePlanStatus,
}

/// The QIF's coded categories against the book (MIG-020).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TaxLinePlan {
    /// In the file's order.
    pub items: Vec<TaxLinePlanItem>,
}

impl TaxLinePlan {
    /// How many categories get a tax line.
    pub fn set(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.status == TaxLinePlanStatus::Set)
            .count()
    }
}

/// What setting tax lines from these QIF categories would do. Matches by
/// full path, ignoring case. Categories without a tax code are left out.
pub fn plan(conn: &Connection, qif: &[QifCategory]) -> Result<TaxLinePlan> {
    let lines = reports::tax_lines(conn)?;
    let paths = category_paths(conn)?;
    let mut items = Vec::new();
    for q in qif {
        let Some(code) = q.tax_code else { continue };
        let mapped = tax_line_id(&lines, code);
        let found = match paths.get(&q.name.trim().to_lowercase()) {
            Some(id) => Some(categories::get(conn, *id)?),
            None => None,
        };
        let (tax_line, status) = match (&found, mapped) {
            (Some(c), _) if c.system.is_some() => (c.fields.tax_line, TaxLinePlanStatus::System),
            (_, None) => (None, TaxLinePlanStatus::Unmapped),
            (None, Some(t)) => (Some(t), TaxLinePlanStatus::Missing),
            (Some(c), Some(t)) => match c.fields.tax_line {
                Some(have) => (Some(have), TaxLinePlanStatus::Kept),
                None => (Some(t), TaxLinePlanStatus::Set),
            },
        };
        items.push(TaxLinePlanItem {
            qif_name: q.name.trim().to_string(),
            code,
            category: found.map(|c| c.id),
            tax_line,
            status,
        });
    }
    Ok(TaxLinePlan { items })
}

/// Set the tax lines `plan` marks `Set` (each an audited category update)
/// and mark those categories tax-related. Returns how many were set. A
/// category that got a line since the plan was made keeps it.
pub fn apply(tx: &Tx<'_>, plan: &TaxLinePlan) -> Result<usize> {
    let mut n = 0;
    for item in &plan.items {
        if item.status != TaxLinePlanStatus::Set {
            continue;
        }
        let (Some(id), Some(line)) = (item.category, item.tax_line) else {
            continue;
        };
        let c = categories::get(tx.conn(), id)?;
        if c.system.is_some() || c.fields.tax_line.is_some() {
            continue;
        }
        let mut f = c.fields;
        f.tax_line = Some(line);
        f.tax_related = true;
        categories::update(tx, id, &f)?;
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::categories::{CategoryKind, SystemCategory};
    use crate::import::qif;
    use crate::testkit::Book;

    /// Category records as Stan's Quicken export writes them.
    const QIF: &str = "!Type:Cat
NTax:Real Estate
T
R4416
E
^
NTAX:PERSONAL PROPERTY
T
R8560
E
^
NSalary
T
R7360
I
^
NSalary Spouse
T
R8096
I
^
NInterest Inc
T
R4592
I
^
NCharity:Cash
T
R4480
E
^
NFood
E
^
NDividends
T
R4576
I
^
";

    fn book() -> Book {
        Book::new("2026-06-30".parse().unwrap()).unwrap()
    }

    fn line(b: &Book, form: &str, l: &str) -> TaxLineId {
        reports::tax_lines(b.conn())
            .unwrap()
            .into_iter()
            .find(|t| t.form == form && t.line == l)
            .unwrap()
            .id
    }

    fn cats() -> Vec<QifCategory> {
        qif::parse(QIF, "x", None).categories
    }

    #[test]
    fn every_code_names_a_built_in_line() {
        let b = book();
        let lines = reports::tax_lines(b.conn()).unwrap();
        for (code, form, l) in CODES {
            assert!(
                tax_line_id(&lines, *code).is_some(),
                "{code}: {form} / {l} is not a built-in line"
            );
        }
    }

    #[test]
    fn the_parser_keeps_the_tax_code() {
        let c = cats();
        assert_eq!(c[0].tax_code, Some(4416));
        assert_eq!(c[6].name, "Food");
        assert_eq!(c[6].tax_code, None);
    }

    #[test]
    fn plan_sets_keeps_and_reports() {
        let mut b = book();
        let real_estate = b
            .category("Tax:Real Estate", CategoryKind::Expense)
            .unwrap();
        let property = b
            .category("Tax:Personal Property", CategoryKind::Expense)
            .unwrap();
        let salary = b.category("Salary", CategoryKind::Income).unwrap();
        b.category("Salary Spouse", CategoryKind::Income).unwrap();
        let interest = b.category("Interest Inc", CategoryKind::Income).unwrap();
        // Already on a line: kept, even though the code says otherwise.
        let other = line(&b, "Form 1040", "Other income, misc.");
        b.write(|tx| {
            let mut f = categories::get(tx.conn(), interest)?.fields;
            f.tax_line = Some(other);
            categories::update(tx, interest, &f).map(|_| ())
        })
        .unwrap();

        let p = plan(b.conn(), &cats()).unwrap();
        let st: Vec<(&str, TaxLinePlanStatus)> = p
            .items
            .iter()
            .map(|i| (i.qif_name.as_str(), i.status))
            .collect();
        use TaxLinePlanStatus as S;
        assert_eq!(
            st,
            [
                ("Tax:Real Estate", S::Set),
                ("TAX:PERSONAL PROPERTY", S::Set),
                ("Salary", S::Set),
                ("Salary Spouse", S::Unmapped),
                ("Interest Inc", S::Kept),
                ("Charity:Cash", S::Missing),
                ("Dividends", S::System),
            ]
        );
        assert_eq!(p.set(), 3);
        let item = |n: &str| p.items.iter().find(|i| i.qif_name == n).unwrap();
        assert_eq!(item("Tax:Real Estate").category, Some(real_estate));
        assert_eq!(item("TAX:PERSONAL PROPERTY").category, Some(property));
        assert_eq!(
            item("Salary").tax_line,
            Some(line(&b, "W-2", "Salary or wages"))
        );
        assert_eq!(item("Interest Inc").tax_line, Some(other));
        assert_eq!(item("Salary Spouse").tax_line, None);
        assert_eq!(
            item("Charity:Cash").tax_line,
            Some(line(&b, "Schedule A", "Cash charity contributions"))
        );

        let before = count_audit(&b);
        let n = b.write(|tx| apply(tx, &p)).unwrap();
        assert_eq!(n, 3);
        assert_eq!(count_audit(&b), before + 3, "one audited update each");
        let got = |id| categories::get(b.conn(), id).unwrap().fields;
        assert_eq!(
            got(real_estate).tax_line,
            Some(line(&b, "Schedule A", "Real estate taxes"))
        );
        assert!(got(real_estate).tax_related);
        assert_eq!(
            got(property).tax_line,
            Some(line(&b, "Schedule A", "Personal property taxes"))
        );
        assert!(got(salary).tax_related);
        assert_eq!(got(interest).tax_line, Some(other), "never overwritten");
        let div = categories::system(b.conn(), SystemCategory::Dividends).unwrap();
        assert_eq!(
            div.fields.tax_line,
            Some(line(&b, "Schedule B", "Dividend income")),
            "built-in categories are left alone"
        );

        // A second run has nothing left to set.
        let again = plan(b.conn(), &cats()).unwrap();
        assert_eq!(again.set(), 0);
        assert_eq!(b.write(|tx| apply(tx, &again)).unwrap(), 0);
    }

    #[test]
    fn apply_skips_a_category_set_since_the_plan() {
        let mut b = book();
        let salary = b.category("Salary", CategoryKind::Income).unwrap();
        let p = plan(b.conn(), &cats()).unwrap();
        let other = line(&b, "Form 1040", "Other income, misc.");
        b.write(|tx| {
            let mut f = categories::get(tx.conn(), salary)?.fields;
            f.tax_line = Some(other);
            categories::update(tx, salary, &f).map(|_| ())
        })
        .unwrap();
        assert_eq!(b.write(|tx| apply(tx, &p)).unwrap(), 0);
        assert_eq!(
            categories::get(b.conn(), salary).unwrap().fields.tax_line,
            Some(other)
        );
    }

    fn count_audit(b: &Book) -> i64 {
        b.conn()
            .query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0))
            .unwrap()
    }
}
