//! A small CSV reader for imports (PRC-030 prices, MIG-120 lot seeding).
//!
//! RFC 4180 fields: commas, double-quoted fields with `""` for a quote,
//! quoted line breaks, CRLF or LF line ends, a leading byte-order mark.
//! The first non-blank line is the header; columns are found by name, so
//! their order does not matter. Blank lines are skipped.
//!
//! Value helpers accept what brokerage exports print: `$1,234.56`,
//! `1,000.5`, and dates as `YYYY-MM-DD` or `M/D/YYYY`.

use crate::date::Date;
use crate::error::{Error, Result};
use crate::money::{Money, Price, Quantity};

/// A parsed file: header names and data rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub header: Vec<String>,
    /// Line the header is on (1-based).
    pub header_line: usize,
    pub rows: Vec<Record>,
}

/// One data row, with the line it started on (1-based) for messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub line: usize,
    pub fields: Vec<String>,
}

impl Record {
    /// Field `i`, trimmed; empty if the row is short.
    pub fn get(&self, i: usize) -> &str {
        self.fields.get(i).map_or("", |f| f.trim())
    }
}

impl Table {
    /// Index of the first column whose name matches one of `names`,
    /// ignoring case, spaces, and underscores.
    pub fn column(&self, names: &[&str]) -> Option<usize> {
        let key = |s: &str| -> String {
            s.chars()
                .filter(|c| !c.is_whitespace() && *c != '_')
                .flat_map(char::to_lowercase)
                .collect()
        };
        names.iter().find_map(|n| {
            let want = key(n);
            self.header.iter().position(|h| key(h) == want)
        })
    }

    /// Every line as data, for a file whose first line is not a header.
    pub fn all_rows(&self) -> Vec<Record> {
        let first = Record {
            line: self.header_line,
            fields: self.header.clone(),
        };
        std::iter::once(first)
            .chain(self.rows.iter().cloned())
            .collect()
    }

    /// Like [`column`](Self::column), but an error naming the column.
    pub fn require(&self, names: &[&str]) -> Result<usize> {
        self.column(names).ok_or_else(|| {
            Error::Invalid(format!(
                "the file has no {:?} column (found: {})",
                names[0],
                self.header.join(", ")
            ))
        })
    }
}

/// Parse CSV text.
pub fn parse(text: &str) -> Result<Table> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut records: Vec<Record> = Vec::new();
    let mut fields: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut line = 1;
    let mut start_line = 1;
    let mut chars = text.chars().peekable();

    let mut end_record = |fields: &mut Vec<String>, field: &mut String, start: usize| {
        fields.push(std::mem::take(field));
        let blank = fields.iter().all(|f| f.trim().is_empty());
        if !blank {
            records.push(Record {
                line: start,
                fields: std::mem::take(fields),
            });
        } else {
            fields.clear();
        }
    };

    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => in_quotes = false,
                '\n' => {
                    line += 1;
                    field.push('\n');
                }
                _ => field.push(c),
            }
            continue;
        }
        match c {
            '"' if field.trim().is_empty() => {
                field.clear();
                in_quotes = true;
            }
            ',' => fields.push(std::mem::take(&mut field)),
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                end_record(&mut fields, &mut field, start_line);
                line += 1;
                start_line = line;
            }
            _ => field.push(c),
        }
    }
    if in_quotes {
        return Err(Error::Invalid(format!(
            "line {start_line}: a quoted field is not closed"
        )));
    }
    end_record(&mut fields, &mut field, start_line);

    let mut rows = records.into_iter();
    let first = rows
        .next()
        .ok_or_else(|| Error::Invalid("the file is empty".into()))?;
    Ok(Table {
        header: first.fields.iter().map(|h| h.trim().to_string()).collect(),
        header_line: first.line,
        rows: rows.collect(),
    })
}

/// A number as exports print it: `$`, thousands commas, and spaces
/// removed. A leading `-` is kept.
fn clean_number(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '$' | ',') && !c.is_whitespace())
        .collect()
}

pub fn money(s: &str) -> Result<Money> {
    clean_number(s).parse()
}

pub fn quantity(s: &str) -> Result<Quantity> {
    clean_number(s).parse()
}

pub fn price(s: &str) -> Result<Price> {
    clean_number(s).parse()
}

/// `YYYY-MM-DD`, or `M/D/YYYY` (US order, as US brokerages print it).
pub fn date(s: &str) -> Result<Date> {
    let s = s.trim();
    if let Ok(d) = s.parse::<Date>() {
        return Ok(d);
    }
    let bad = || Error::Parse {
        kind: "date",
        input: s.to_string(),
        reason: "expected YYYY-MM-DD or M/D/YYYY".into(),
    };
    let parts: Vec<&str> = s.split('/').collect();
    let [m, d, y] = parts.as_slice() else {
        return Err(bad());
    };
    if y.len() != 4 {
        return Err(bad());
    }
    let num = |t: &str| t.parse::<u32>().map_err(|_| bad());
    let year = i32::try_from(num(y)?).map_err(|_| bad())?;
    Date::from_ymd(year, num(m)?, num(d)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quotes_line_ends_and_blank_lines() {
        let t = parse(
            "\u{feff}Name,Value\r\n\"Smith, J\",\"say \"\"hi\"\"\"\r\n\r\nb,\"two\nlines\"\nc,3",
        )
        .unwrap();
        assert_eq!(t.header, vec!["Name", "Value"]);
        assert_eq!(t.rows.len(), 3);
        assert_eq!(t.rows[0].fields, vec!["Smith, J", "say \"hi\""]);
        assert_eq!(t.rows[0].line, 2);
        assert_eq!(t.rows[1].fields, vec!["b", "two\nlines"]);
        assert_eq!(t.rows[1].line, 4);
        assert_eq!(t.rows[2].line, 6);
        assert_eq!(t.rows[2].get(1), "3");
        assert_eq!(t.rows[2].get(5), "");
    }

    #[test]
    fn finds_columns_by_loose_name() {
        let t = parse("Acquired Date,COST_BASIS\n").unwrap();
        assert_eq!(t.column(&["acquired_date"]), Some(0));
        assert_eq!(t.column(&["basis", "cost basis"]), Some(1));
        assert!(t.require(&["ticker"]).is_err());
    }

    #[test]
    fn rejects_unclosed_quote_and_empty_file() {
        assert!(parse("a,b\n\"x,1\n").is_err());
        assert!(parse("\n\n").is_err());
    }

    #[test]
    fn values_as_exports_print_them() {
        assert_eq!(money("$1,234.56").unwrap().cents(), 123456);
        assert_eq!(quantity("1,000.5").unwrap().to_string(), "1000.5");
        assert_eq!(price(" 12.345 ").unwrap().to_string(), "12.345");
        assert_eq!(date("2026-01-05").unwrap().to_string(), "2026-01-05");
        assert_eq!(date("1/5/2026").unwrap().to_string(), "2026-01-05");
        assert!(date("1/5/26").is_err());
        assert!(date("13/5/2026").is_err());
    }
}
