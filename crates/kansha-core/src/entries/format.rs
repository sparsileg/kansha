//! The `kansha-entries 1` and `kansha-names 1` text formats (CLI-020,
//! CLI-080).
//!
//! ```text
//! kansha-entries 1
//! from "Stan's phone"
//! 2026-10-12 Shell 45.20 Auto:Fuel acct "Double Cash" memo "Ohio trip"
//! 2026-10-12 Costco 182.40 split acct Checking id 1b4e28ba-2fa1-4d2a-9c3e-0f6a8b7c5d21
//!   120.00 Food:Groceries
//!   Household memo "paper goods"
//! ```
//!
//! Words are separated by spaces; `"…"` quotes a word with spaces (`\"`
//! and `\\` inside). A word starting with `#` outside quotes starts a
//! comment. An entry line is `<date> <payee> <amount> <category | split>`
//! then `acct <account>` (required), `memo "<text>"`, and `id <uuid>` in
//! any order. Split lines are indented: `<amount> <category> [memo
//! "<text>"]`, and add up to the entry's amount.
//! Amounts are read as text straight into cents.

use std::fmt;

use serde::Serialize;

use crate::date::Date;
use crate::error::{Error, Result};
use crate::money::Money;
use crate::settings::DateFormat;

/// The largest file read, in bytes (1 MB).
pub const MAX_FILE_BYTES: usize = 1024 * 1024;

const ENTRIES_HEADER: &str = "kansha-entries";
const NAMES_HEADER: &str = "kansha-names";
const VERSION: &str = "1";

/// Words with a meaning of their own; a name spelled like one is quoted.
const KEYWORDS: [&str; 5] = ["acct", "memo", "id", "split", "from"];

/// A problem on one line of a file; line 0 is the file as a whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LineError {
    pub line: u32,
    pub message: String,
}

impl LineError {
    pub fn new(line: u32, message: impl Into<String>) -> LineError {
        LineError {
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for LineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            f.write_str(&self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}

/// Which date forms are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dates {
    /// `YYYY-MM-DD` only: files read by the app (inbox, Import entries…).
    Iso,
    /// The command line: also `today`, `yesterday`, and the book's date
    /// format when known (CLI-020).
    Typed {
        today: Date,
        format: Option<DateFormat>,
    },
}

/// A parsed `kansha-entries` file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EntryFile {
    pub from: Option<String>,
    pub entries: Vec<FileEntry>,
}

/// One spending entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Line number in the file it was read from (0 when built in code).
    pub line: u32,
    pub date: Date,
    pub payee: String,
    /// What was spent; negative for a refund.
    pub amount: Money,
    pub spend: Spend,
    pub account: String,
    pub memo: String,
    /// A UUID, lower case; `None` until one is given (see [`new_id`]).
    pub id: Option<String>,
}

/// Where an entry's amount went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spend {
    Category(String),
    /// Two or more lines adding up to the entry's amount.
    Split(Vec<SplitLine>),
}

/// One line of a split.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitLine {
    pub line: u32,
    pub amount: Money,
    pub category: String,
    pub memo: String,
}

impl FileEntry {
    /// Every category the entry names, with its amount.
    pub fn parts(&self) -> Vec<(&str, Money, &str)> {
        match &self.spend {
            Spend::Category(c) => vec![(c.as_str(), self.amount, "")],
            Spend::Split(lines) => lines
                .iter()
                .map(|l| (l.category.as_str(), l.amount, l.memo.as_str()))
                .collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Words
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct Word {
    text: String,
    quoted: bool,
}

impl Word {
    /// An unquoted keyword.
    fn is(&self, keyword: &str) -> bool {
        !self.quoted && self.text == keyword
    }
}

/// Split a line into words. `#` at the start of a word outside quotes
/// ends the line.
fn words(line: &str) -> std::result::Result<Vec<Word>, String> {
    let mut out = Vec::new();
    let mut chars = line.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let Some(&c) = chars.peek() else {
            return Ok(out);
        };
        if c == '#' {
            return Ok(out);
        }
        if c == '"' {
            chars.next();
            let mut text = String::new();
            loop {
                match chars.next() {
                    None => return Err("a quote is not closed".into()),
                    Some('"') => break,
                    Some('\\') => match chars.next() {
                        Some(e @ ('"' | '\\')) => text.push(e),
                        _ => return Err("inside quotes, \\ is followed only by \" or \\".into()),
                    },
                    Some(ch) => text.push(ch),
                }
            }
            if chars.peek().is_some_and(|c| !c.is_whitespace()) {
                return Err("put a space after a closing quote".into());
            }
            out.push(Word { text, quoted: true });
        } else {
            let mut text = String::new();
            while let Some(ch) = chars.next_if(|c| !c.is_whitespace()) {
                if ch == '"' {
                    return Err(format!(
                        "a quote inside the word {text}\"…; quote the whole word"
                    ));
                }
                text.push(ch);
            }
            out.push(Word {
                text,
                quoted: false,
            });
        }
    }
}

/// `text` as one word, quoted when it has to be.
pub(crate) fn quote(text: &str) -> String {
    let plain = !text.is_empty()
        && !text.starts_with('#')
        && !KEYWORDS.contains(&text)
        && !text
            .chars()
            .any(|c| c.is_whitespace() || c == '"' || c == '\\');
    if plain {
        return text.to_string();
    }
    let mut out = String::from('"');
    for c in text.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

/// An amount: optional `-`, optional `$`, thousands commas, at most two
/// decimals. Read as text into cents, never through a float.
pub fn parse_amount(text: &str) -> std::result::Result<Money, String> {
    let bad = || format!("“{text}” is not an amount");
    let mut rest = text;
    let mut negative = false;
    if let Some(r) = rest.strip_prefix('-') {
        negative = true;
        rest = r;
    }
    if let Some(r) = rest.strip_prefix('$') {
        rest = r;
    }
    if !negative && let Some(r) = rest.strip_prefix('-') {
        negative = true;
        rest = r;
    }
    let (int, frac) = match rest.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (rest, None),
    };
    let int = if int.contains(',') {
        let groups: Vec<&str> = int.split(',').collect();
        let first_ok = (1..=3).contains(&groups[0].len());
        if !first_ok || groups[1..].iter().any(|g| g.len() != 3) {
            return Err(format!("“{text}”: thousands commas are misplaced"));
        }
        groups.concat()
    } else if int.is_empty() && frac.is_some() {
        "0".to_string()
    } else {
        int.to_string()
    };
    if int.is_empty() || !int.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    if let Some(f) = frac {
        if f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad());
        }
        if f.len() > 2 {
            return Err(format!("“{text}” has more than two decimals"));
        }
    }
    let plain = match frac {
        Some(f) => format!("{}{int}.{f}", if negative { "-" } else { "" }),
        None => format!("{}{int}", if negative { "-" } else { "" }),
    };
    plain.parse::<Money>().map_err(|_| bad())
}

/// A date in one of the forms `dates` allows.
pub fn parse_date(text: &str, dates: Dates) -> std::result::Result<Date, String> {
    if let Ok(d) = text.parse::<Date>() {
        return Ok(d);
    }
    let Dates::Typed { today, format } = dates else {
        return Err(format!("“{text}” is not a date (use YYYY-MM-DD)"));
    };
    let lower = text.to_lowercase();
    if lower == "today" {
        return Ok(today);
    }
    if lower == "yesterday" {
        let day = today
            .naive()
            .pred_opt()
            .ok_or_else(|| format!("no day before {today}"))?;
        return Ok(Date::from_naive(day));
    }
    let shown = format.map_or("YYYY-MM-DD", date_format_text);
    let wrong = || format!("“{text}” is not a date (use {shown}, today, or yesterday)");
    let Some(format) = format.filter(|f| *f != DateFormat::Ymd) else {
        return Err(wrong());
    };
    let parts: Vec<&str> = text.split('/').collect();
    let [a, b, y] = parts[..] else {
        return Err(wrong());
    };
    let num = |s: &str, max_len: usize| -> Option<u32> {
        (!s.is_empty() && s.len() <= max_len && s.bytes().all(|c| c.is_ascii_digit()))
            .then(|| s.parse().ok())
            .flatten()
    };
    let (Some(a), Some(b), Some(year)) = (num(a, 2), num(b, 2), num(y, 4)) else {
        return Err(wrong());
    };
    if y.len() != 4 {
        return Err(wrong());
    }
    let (month, day) = match format {
        DateFormat::Mdy => (a, b),
        _ => (b, a),
    };
    Date::from_ymd(year as i32, month, day).map_err(|_| format!("“{text}”: no such date"))
}

/// How a date format is written in a names file.
pub fn date_format_text(f: DateFormat) -> &'static str {
    match f {
        DateFormat::Mdy => "MM/DD/YYYY",
        DateFormat::Dmy => "DD/MM/YYYY",
        DateFormat::Ymd => "YYYY-MM-DD",
    }
}

fn parse_date_format(text: &str) -> Option<DateFormat> {
    DateFormat::ALL
        .iter()
        .copied()
        .find(|f| date_format_text(*f) == text)
}

/// A UUID, lower-cased.
pub fn parse_id(text: &str) -> std::result::Result<String, String> {
    let b = text.as_bytes();
    let ok = b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        });
    if ok {
        Ok(text.to_ascii_lowercase())
    } else {
        Err(format!("“{text}” is not an id (a UUID)"))
    }
}

/// A new random (version 4) UUID.
pub fn new_id() -> Result<String> {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).map_err(|e| Error::Io(format!("no random numbers: {e}")))?;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

// ---------------------------------------------------------------------------
// kansha-entries
// ---------------------------------------------------------------------------

/// Lines of `text` with their numbers: a leading BOM dropped, CRLF and
/// LF both ending a line. Refuses a file over [`MAX_FILE_BYTES`].
fn numbered_lines(text: &str) -> std::result::Result<Vec<(u32, &str)>, LineError> {
    if text.len() > MAX_FILE_BYTES {
        return Err(LineError::new(0, "the file is larger than 1 MB"));
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    Ok(text
        .lines()
        .enumerate()
        .map(|(i, l)| (u32::try_from(i + 1).unwrap_or(u32::MAX), l))
        .collect())
}

/// The first line holding words: it must be `<header> 1`. Returns the
/// index of the line after it.
fn read_header(
    lines: &[(u32, &str)],
    header: &str,
    what: &str,
) -> std::result::Result<usize, LineError> {
    for (i, (no, line)) in lines.iter().enumerate() {
        let w = words(line).map_err(|m| LineError::new(*no, m))?;
        if w.is_empty() {
            continue;
        }
        if w.len() == 2 && w[0].is(header) {
            if w[1].is(VERSION) {
                return Ok(i + 1);
            }
            return Err(LineError::new(
                *no,
                format!("{header} {} is newer than this Kansha reads (1)", w[1].text),
            ));
        }
        return Err(LineError::new(
            *no,
            format!("not {what}: the first line must be “{header} 1”"),
        ));
    }
    Err(LineError::new(0, format!("not {what}: the file is empty")))
}

/// An entry being read, before its split lines are checked.
struct Open {
    entry: FileEntry,
    /// Split lines as written: line number, amount, category, memo.
    lines: Vec<(u32, Money, String, String)>,
    split: bool,
    /// A split line was refused: its error stands alone, with no sum
    /// error on top.
    bad: bool,
}

/// Parse a `kansha-entries 1` file. Every bad line is reported, not just
/// the first.
pub fn parse_entries(text: &str, dates: Dates) -> std::result::Result<EntryFile, Vec<LineError>> {
    let lines = numbered_lines(text).map_err(|e| vec![e])?;
    let start =
        read_header(&lines, ENTRIES_HEADER, "a Kansha entries file").map_err(|e| vec![e])?;
    let mut file = EntryFile::default();
    let mut errors = Vec::new();
    let mut open: Option<Open> = None;
    // An entry line was bad: its split lines are skipped.
    let mut skipping = false;
    let mut seen_entry = false;
    for &(no, line) in &lines[start..] {
        let indented = line.starts_with([' ', '\t']);
        let w = match words(line) {
            Ok(w) => w,
            Err(m) => {
                errors.push(LineError::new(no, m));
                if !indented {
                    open = None;
                    skipping = true;
                }
                continue;
            }
        };
        if w.is_empty() {
            continue;
        }
        if indented {
            if skipping {
                continue;
            }
            match open.as_mut() {
                Some(o) if o.split => match split_line(&w) {
                    Ok((amount, category, memo)) => o.lines.push((no, amount, category, memo)),
                    Err(m) => {
                        o.bad = true;
                        errors.push(LineError::new(no, m));
                    }
                },
                Some(_) => errors.push(LineError::new(
                    no,
                    "an indented line follows an entry that is not a split",
                )),
                None => errors.push(LineError::new(no, "an indented line with no entry above")),
            }
            continue;
        }
        if let Some(o) = open.take() {
            finish(o, &mut file, &mut errors);
        }
        skipping = false;
        if w[0].is("from") {
            if seen_entry || file.from.is_some() {
                errors.push(LineError::new(
                    no,
                    "“from” comes once, before the first entry",
                ));
            } else if w.len() != 2 {
                errors.push(LineError::new(no, "write from \"<sender>\""));
            } else {
                file.from = Some(w[1].text.clone());
            }
            continue;
        }
        seen_entry = true;
        match entry_line(no, &w, dates) {
            Ok(o) => open = Some(o),
            Err(m) => {
                errors.push(LineError::new(no, m));
                skipping = true;
            }
        }
    }
    if let Some(o) = open.take() {
        finish(o, &mut file, &mut errors);
    }
    let mut ids: Vec<(&str, u32)> = Vec::new();
    for e in &file.entries {
        if let Some(id) = &e.id {
            if let Some((_, first)) = ids.iter().find(|(i, _)| i == id) {
                errors.push(LineError::new(
                    e.line,
                    format!("id {id} is also on line {first}"),
                ));
            } else {
                ids.push((id, e.line));
            }
        }
    }
    if errors.is_empty() && file.entries.is_empty() {
        errors.push(LineError::new(0, "the file has no entries"));
    }
    if errors.is_empty() {
        Ok(file)
    } else {
        errors.sort_by_key(|e| e.line);
        Err(errors)
    }
}

/// `<date> <payee> <amount> <category | split>` then keywords.
fn entry_line(no: u32, w: &[Word], dates: Dates) -> std::result::Result<Open, String> {
    if w.len() < 4 {
        return Err(
            "an entry is <date> <payee> <amount> <category or split> acct <account>".into(),
        );
    }
    let date = parse_date(&w[0].text, dates)?;
    let payee = w[1].text.trim().to_string();
    if payee.is_empty() {
        return Err("the payee is empty".into());
    }
    let amount = parse_amount(&w[2].text)?;
    if amount.is_zero() {
        return Err("the amount is zero".into());
    }
    let split = w[3].is("split");
    let category = w[3].text.clone();
    if !split && category.trim().is_empty() {
        return Err("the category is empty".into());
    }
    let mut account = None;
    let mut memo = None;
    let mut id = None;
    let mut rest = w[4..].iter();
    while let Some(k) = rest.next() {
        let slot = if k.is("acct") {
            &mut account
        } else if k.is("memo") {
            &mut memo
        } else if k.is("id") {
            &mut id
        } else {
            return Err(format!(
                "“{}” is not acct, memo, or id (quote a name with spaces)",
                k.text
            ));
        };
        let Some(value) = rest.next() else {
            return Err(format!("{} needs a value", k.text));
        };
        if slot.is_some() {
            return Err(format!("{} is given twice", k.text));
        }
        *slot = Some(value.text.clone());
    }
    let Some(account) = account.filter(|a| !a.trim().is_empty()) else {
        return Err("acct <account> is required".into());
    };
    let id = id.map(|i| parse_id(&i)).transpose()?;
    Ok(Open {
        entry: FileEntry {
            line: no,
            date,
            payee,
            amount,
            spend: Spend::Category(category),
            account,
            memo: memo.unwrap_or_default(),
            id,
        },
        lines: Vec::new(),
        split,
        bad: false,
    })
}

/// `<amount> <category> [memo "<text>"]`.
type RawSplit = (Money, String, String);

fn split_line(w: &[Word]) -> std::result::Result<RawSplit, String> {
    let (head, memo) = match w.iter().position(|x| x.is("memo")) {
        Some(i) if i + 2 == w.len() => (&w[..i], w[i + 1].text.clone()),
        Some(_) => return Err("memo \"<text>\" comes last on a split line".into()),
        None => (w, String::new()),
    };
    match head {
        [_] => Err("a split line needs an amount: <amount> <category>".into()),
        [amount, category] => {
            let a = parse_amount(&amount.text)?;
            if a.is_zero() {
                return Err("a split line of zero".into());
            }
            Ok((a, category.text.clone(), memo))
        }
        _ => Err("a split line is <amount> <category> [memo \"<text>\"]".into()),
    }
}

/// Check an entry's split lines and add it to the file.
fn finish(o: Open, file: &mut EntryFile, errors: &mut Vec<LineError>) {
    let Open {
        mut entry,
        lines,
        split,
        bad,
    } = o;
    if bad {
        return;
    }
    if !split {
        file.entries.push(entry);
        return;
    }
    let no = entry.line;
    if lines.len() < 2 {
        errors.push(LineError::new(no, "a split needs at least two lines"));
        return;
    }
    let mut given = Money::ZERO;
    for l in &lines {
        match given.checked_add(l.1) {
            Some(s) => given = s,
            None => {
                errors.push(LineError::new(l.0, "the amounts are too large"));
                return;
            }
        }
    }
    let Some(left) = entry.amount.checked_sub(given) else {
        errors.push(LineError::new(no, "the amounts are too large"));
        return;
    };
    if !left.is_zero() {
        errors.push(LineError::new(
            no,
            format!(
                "the split lines add up to {given}, not {}; {left} is left",
                entry.amount
            ),
        ));
        return;
    }
    entry.spend = Spend::Split(
        lines
            .into_iter()
            .map(|(line, amount, category, memo)| SplitLine {
                line,
                amount,
                category,
                memo,
            })
            .collect(),
    );
    file.entries.push(entry);
}

/// Write `file` as `kansha-entries 1`. Every split line gets its amount;
/// dates are written out.
pub fn write_entries(file: &EntryFile) -> String {
    let mut out = format!("{ENTRIES_HEADER} {VERSION}\n");
    if let Some(from) = &file.from {
        out.push_str(&format!("from {}\n", quote_always(from)));
    }
    for e in &file.entries {
        let category = match &e.spend {
            Spend::Category(c) => quote(c),
            Spend::Split(_) => "split".to_string(),
        };
        out.push_str(&format!(
            "{} {} {} {} acct {}",
            e.date,
            quote(&e.payee),
            e.amount,
            category,
            quote(&e.account)
        ));
        if !e.memo.is_empty() {
            out.push_str(&format!(" memo {}", quote_always(&e.memo)));
        }
        if let Some(id) = &e.id {
            out.push_str(&format!(" id {id}"));
        }
        out.push('\n');
        if let Spend::Split(lines) = &e.spend {
            for l in lines {
                out.push_str(&format!("  {} {}", l.amount, quote(&l.category)));
                if !l.memo.is_empty() {
                    out.push_str(&format!(" memo {}", quote_always(&l.memo)));
                }
                out.push('\n');
            }
        }
    }
    out
}

fn quote_always(text: &str) -> String {
    let q = quote(text);
    if q.starts_with('"') {
        q
    } else {
        format!("\"{q}\"")
    }
}

// ---------------------------------------------------------------------------
// kansha-names
// ---------------------------------------------------------------------------

/// A parsed `kansha-names 1` file: the names entries may use (CLI-080).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NamesFile {
    pub book: String,
    pub date_format: Option<DateFormat>,
    pub accounts: Vec<String>,
    pub categories: Vec<String>,
}

/// Parse a `kansha-names 1` file.
pub fn parse_names(text: &str) -> std::result::Result<NamesFile, Vec<LineError>> {
    let lines = numbered_lines(text).map_err(|e| vec![e])?;
    let start = read_header(&lines, NAMES_HEADER, "a Kansha names file").map_err(|e| vec![e])?;
    let mut names = NamesFile::default();
    let mut errors = Vec::new();
    for &(no, line) in &lines[start..] {
        let w = match words(line) {
            Ok(w) => w,
            Err(m) => {
                errors.push(LineError::new(no, m));
                continue;
            }
        };
        match &w[..] {
            [] => {}
            [k, v] if k.is("book") => names.book = v.text.clone(),
            [k, v] if k.is("date-format") => match parse_date_format(&v.text) {
                Some(f) => names.date_format = Some(f),
                None => errors.push(LineError::new(
                    no,
                    format!("“{}” is not a date format", v.text),
                )),
            },
            [k, v] if k.is("acct") => names.accounts.push(v.text.clone()),
            [k, v] if k.is("cat") => names.categories.push(v.text.clone()),
            _ => errors.push(LineError::new(
                no,
                "expected book, date-format, acct, or cat and one value",
            )),
        }
    }
    if errors.is_empty() {
        Ok(names)
    } else {
        Err(errors)
    }
}

/// Write `names` as `kansha-names 1`.
pub fn write_names(names: &NamesFile) -> String {
    let mut out = format!("{NAMES_HEADER} {VERSION}\n");
    out.push_str(&format!("book {}\n", quote_always(&names.book)));
    if let Some(f) = names.date_format {
        out.push_str(&format!("date-format {}\n", date_format_text(f)));
    }
    for a in &names.accounts {
        out.push_str(&format!("acct {}\n", quote(a)));
    }
    for c in &names.categories {
        out.push_str(&format!("cat {}\n", quote(c)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(s: &str) -> Money {
        s.parse().unwrap()
    }

    fn d(s: &str) -> Date {
        s.parse().unwrap()
    }

    fn parse(text: &str) -> EntryFile {
        parse_entries(text, Dates::Iso).unwrap()
    }

    fn errors(text: &str) -> Vec<String> {
        parse_entries(text, Dates::Iso)
            .unwrap_err()
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    const SAMPLE: &str = "kansha-entries 1
from \"Stan's phone\"
2026-10-12 Shell 45.20 Auto:Fuel acct \"Double Cash\" memo \"Ohio trip\"
2026-10-12 Costco 182.40 split acct Checking id 1B4E28BA-2fa1-4d2a-9c3e-0f6a8b7c5d21
  120.00 Food:Groceries
  62.40 Household memo \"paper goods\"
2026-10-14 Costco -24.99 Household acct Checking memo \"return\"
";

    #[test]
    fn reads_the_sample() {
        let f = parse(SAMPLE);
        assert_eq!(f.from.as_deref(), Some("Stan's phone"));
        assert_eq!(f.entries.len(), 3);
        let e = &f.entries[0];
        assert_eq!(
            (e.line, e.date, e.payee.as_str(), e.amount),
            (3, d("2026-10-12"), "Shell", m("45.20"))
        );
        assert_eq!(e.spend, Spend::Category("Auto:Fuel".into()));
        assert_eq!(
            (e.account.as_str(), e.memo.as_str()),
            ("Double Cash", "Ohio trip")
        );
        assert_eq!(e.id, None);
        let s = &f.entries[1];
        assert_eq!(
            s.id.as_deref(),
            Some("1b4e28ba-2fa1-4d2a-9c3e-0f6a8b7c5d21")
        );
        let Spend::Split(lines) = &s.spend else {
            panic!("split expected")
        };
        assert_eq!(lines.len(), 2);
        assert_eq!((lines[1].line, lines[1].amount), (6, m("62.40")));
        assert_eq!(lines[1].memo, "paper goods");
        assert_eq!(f.entries[2].amount, m("-24.99"));
    }

    #[test]
    fn round_trips() {
        let f = parse(SAMPLE);
        let again = parse(&write_entries(&f));
        let strip = |f: &EntryFile| -> Vec<FileEntry> {
            f.entries
                .iter()
                .map(|e| {
                    let mut e = e.clone();
                    e.line = 0;
                    if let Spend::Split(l) = &mut e.spend {
                        for x in l {
                            x.line = 0;
                        }
                    }
                    e
                })
                .collect()
        };
        assert_eq!(strip(&f), strip(&again));
        assert_eq!(f.from, again.from);
    }

    #[test]
    fn writer_quotes_what_needs_it() {
        let f = EntryFile {
            from: Some("me".into()),
            entries: vec![FileEntry {
                line: 0,
                date: d("2026-01-02"),
                payee: "split".into(),
                amount: m("-1.00"),
                spend: Spend::Category("Say \"hi\" \\ there".into()),
                account: "#1 Card".into(),
                memo: "x".into(),
                id: None,
            }],
        };
        let text = write_entries(&f);
        assert!(text.contains(
            "2026-01-02 \"split\" -1.00 \"Say \\\"hi\\\" \\\\ there\" acct \"#1 Card\" memo \"x\""
        ));
        assert!(text.contains("from \"me\""));
        let back = parse(&text);
        assert_eq!(back.entries[0].payee, "split");
        assert_eq!(
            back.entries[0].spend,
            Spend::Category("Say \"hi\" \\ there".into())
        );
        assert_eq!(back.entries[0].account, "#1 Card");
    }

    #[test]
    fn every_split_line_needs_an_amount() {
        assert_eq!(
            errors(
                "kansha-entries 1
2026-10-12 Target 107.00 split acct Checking
  7.00 Taxes:Sales
  60.00 Clothing
  Household memo \"rest\"
"
            ),
            ["line 5: a split line needs an amount: <amount> <category>"]
        );
    }

    #[test]
    fn split_sums_are_checked() {
        assert_eq!(
            errors(
                "kansha-entries 1
2026-10-12 Target 10.00 split acct Checking
  4.00 Food
  5.00 Household
"
            ),
            ["line 2: the split lines add up to 9.00, not 10.00; 1.00 is left"]
        );
        assert_eq!(
            errors(
                "kansha-entries 1
2026-10-12 Target 10.00 split acct Checking
  10.00 Food
"
            ),
            ["line 2: a split needs at least two lines"]
        );
        // A coupon: signs may differ.
        let f = parse(
            "kansha-entries 1
2026-10-12 Target 10.00 split acct Checking
  12.00 Food
  -2.00 Discounts
",
        );
        assert_eq!(f.entries.len(), 1);
    }

    #[test]
    fn every_bad_line_is_reported() {
        let e = errors(
            "kansha-entries 1
2026-13-01 Shell 1.00 Auto acct Checking
2026-10-12 Shell 1.001 Auto acct Checking
2026-10-12 Shell 0 Auto acct Checking
2026-10-12 Shell 1.00 Auto
2026-10-12 Shell 1.00 Auto acct Checking acct Savings
2026-10-12 Shell 1.00 Auto acct Checking color red
2026-10-12 Shell 1.00 Auto acct Checking
  1.00 Food
2026-10-12 Shell 1.00 Auto acct \"Checking
  1.00 Food
2026-10-12 Shell
",
        );
        assert_eq!(
            e,
            [
                "line 2: “2026-13-01” is not a date (use YYYY-MM-DD)",
                "line 3: “1.001” has more than two decimals",
                "line 4: the amount is zero",
                "line 5: acct <account> is required",
                "line 6: acct is given twice",
                "line 7: “color” is not acct, memo, or id (quote a name with spaces)",
                "line 9: an indented line follows an entry that is not a split",
                "line 10: a quote is not closed",
                "line 12: an entry is <date> <payee> <amount> <category or split> acct <account>",
            ]
        );
    }

    #[test]
    fn split_lines_under_a_bad_entry_are_skipped() {
        let e = errors(
            "kansha-entries 1
2026-10-12 Target ten split acct Checking
  4.00 Food
  Household
",
        );
        assert_eq!(e, ["line 2: “ten” is not an amount"]);
    }

    #[test]
    fn header_is_required() {
        assert_eq!(
            errors("2026-10-12 Shell 1.00 Auto acct Checking\n"),
            ["line 1: not a Kansha entries file: the first line must be “kansha-entries 1”"]
        );
        assert_eq!(
            errors("kansha-entries 2\n"),
            ["line 1: kansha-entries 2 is newer than this Kansha reads (1)"]
        );
        assert_eq!(errors(""), ["not a Kansha entries file: the file is empty"]);
        assert_eq!(
            errors("kansha-entries 1\n# none\n"),
            ["the file has no entries"]
        );
    }

    #[test]
    fn crlf_bom_comments_and_blank_lines() {
        let f = parse(
            "\u{feff}# phone export\r\nkansha-entries 1\r\n\r\n2026-10-12 Shell 1.00 Auto acct Checking # gas\r\n",
        );
        assert_eq!(f.entries.len(), 1);
        assert_eq!(f.entries[0].line, 4);
        assert_eq!(f.entries[0].account, "Checking");
    }

    #[test]
    fn too_large_is_refused() {
        let big = format!("kansha-entries 1\n#{}\n", "x".repeat(MAX_FILE_BYTES));
        assert_eq!(errors(&big), ["the file is larger than 1 MB"]);
    }

    #[test]
    fn from_rules() {
        assert_eq!(
            errors(
                "kansha-entries 1
2026-10-12 Shell 1.00 Auto acct Checking
from \"late\"
"
            ),
            ["line 3: “from” comes once, before the first entry"]
        );
    }

    #[test]
    fn duplicate_ids_in_one_file() {
        let id = "1b4e28ba-2fa1-4d2a-9c3e-0f6a8b7c5d21";
        let e = errors(&format!(
            "kansha-entries 1
2026-10-12 Shell 1.00 Auto acct Checking id {id}
2026-10-12 Shell 2.00 Auto acct Checking id {id}
"
        ));
        assert_eq!(e, [format!("line 3: id {id} is also on line 2")]);
        assert_eq!(
            errors(
                "kansha-entries 1
2026-10-12 Shell 1.00 Auto acct Checking id 12
"
            ),
            ["line 2: “12” is not an id (a UUID)"]
        );
    }

    #[test]
    fn amounts() {
        for (text, cents) in [
            ("45.20", 4520),
            ("$45.2", 4520),
            ("-$1,234.56", -123_456),
            ("$-3", -300),
            ("1,000,000", 100_000_000),
            (".50", 50),
        ] {
            assert_eq!(parse_amount(text), Ok(Money::from_cents(cents)), "{text}");
        }
        for text in [
            "1,00", "12,3456", "abc", "1.2.3", "$", "--1", "1e3", "1.", "",
        ] {
            assert!(parse_amount(text).is_err(), "{text}");
        }
    }

    #[test]
    fn typed_dates() {
        let today = d("2026-03-01");
        let us = Dates::Typed {
            today,
            format: Some(DateFormat::Mdy),
        };
        assert_eq!(parse_date("today", us), Ok(today));
        assert_eq!(parse_date("Yesterday", us), Ok(d("2026-02-28")));
        assert_eq!(parse_date("2/3/2026", us), Ok(d("2026-02-03")));
        assert_eq!(parse_date("2026-02-03", us), Ok(d("2026-02-03")));
        let eu = Dates::Typed {
            today,
            format: Some(DateFormat::Dmy),
        };
        assert_eq!(parse_date("2/3/2026", eu), Ok(d("2026-03-02")));
        assert!(parse_date("2/3/26", us).is_err());
        assert!(parse_date("2/30/2026", us).is_err());
        let none = Dates::Typed {
            today,
            format: None,
        };
        assert_eq!(parse_date("today", none), Ok(today));
        assert_eq!(
            parse_date("2/3/2026", none),
            Err("“2/3/2026” is not a date (use YYYY-MM-DD, today, or yesterday)".into())
        );
        assert!(parse_date("today", Dates::Iso).is_err());
    }

    #[test]
    fn typed_dates_in_a_file() {
        let f = parse_entries(
            "kansha-entries 1\ntoday Shell 1.00 Auto acct Checking\n",
            Dates::Typed {
                today: d("2026-10-12"),
                format: None,
            },
        )
        .unwrap();
        assert_eq!(f.entries[0].date, d("2026-10-12"));
        assert!(write_entries(&f).contains("2026-10-12 Shell"));
    }

    #[test]
    fn new_ids_are_v4_and_differ() {
        let a = new_id().unwrap();
        let b = new_id().unwrap();
        assert_ne!(a, b);
        assert_eq!(parse_id(&a), Ok(a.clone()));
        assert_eq!(&a[14..15], "4");
        assert!(matches!(&a[19..20], "8" | "9" | "a" | "b"));
    }

    #[test]
    fn names_round_trip() {
        let n = NamesFile {
            book: "barton2026".into(),
            date_format: Some(DateFormat::Mdy),
            accounts: vec!["Checking".into(), "Double Cash".into()],
            categories: vec!["Food".into(), "Food:Groceries".into()],
        };
        let text = write_names(&n);
        assert_eq!(
            text,
            "kansha-names 1
book \"barton2026\"
date-format MM/DD/YYYY
acct Checking
acct \"Double Cash\"
cat Food
cat Food:Groceries
"
        );
        assert_eq!(parse_names(&text), Ok(n));
    }

    #[test]
    fn names_errors() {
        let e = parse_names("kansha-names 1\ndate-format soon\nshoe x\n").unwrap_err();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].to_string(), "line 2: “soon” is not a date format");
        assert!(parse_names("kansha-entries 1\n").is_err());
    }
}
