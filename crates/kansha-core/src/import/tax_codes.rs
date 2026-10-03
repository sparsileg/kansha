//! Quicken tax codes (CAT-050, MIG-020).
//!
//! A QIF category list carries Quicken's tax code on each tax-related
//! category (`R4416` on `Tax:Real Estate`). [`CODES`] maps the codes Stan's
//! file uses to Kansha's built-in tax lines (migration 0003 names, which
//! are Quicken's). The import sets the line on a category it creates.

use std::collections::HashMap;

use rusqlite::Connection;

use crate::categories::{CategoryId, TaxLine, TaxLineId};
use crate::error::Result;
use crate::persistence::categories;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::qif::{self, QifCategory};
    use crate::persistence::reports;
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
}
