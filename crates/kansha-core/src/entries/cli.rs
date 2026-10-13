//! `kansha add` and `kansha run` (CLI-030): check entries and write them
//! to the book's inbox, without opening the book or needing its key.
//! Names are checked against the names copy beside the book when there
//! is one (CLI-080). The app reads the inbox and lists the entries for
//! review (CLI-040).
//!
//! Exit codes: 0 accepted, 1 refused or quit, 2 usage.
//!
//! Output goes to the streams given, and the interactive questions to a
//! [`Prompter`], so tests drive all of it without a terminal.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::format::{
    self, Dates, EntryFile, FileEntry, LineError, MAX_FILE_BYTES, Spend, SplitLine,
};
use super::names::Names;
use super::sidecar;
use crate::book::BookFiles;
use crate::calc;
use crate::date::{Clock, Date};
use crate::local_config::LocalConfig;
use crate::money::Money;
use crate::security::write_atomic;
use crate::settings::DateFormat;

/// The sender when `--from` is not given.
pub const DEFAULT_SENDER: &str = "command line";

const USAGE: &str = "\
Kansha: enter spending from the command line. Entries wait in the book's
inbox until you approve them in Kansha (File > Review Entries…).

  kansha add                 ask for an entry field by field
  kansha add <entry>         one entry, e.g.
                             kansha add today Shell 45.20 Auto:Fuel acct \"Double Cash\"
  kansha run <file>          a kansha-entries 1 file
  kansha run -               the same, read from standard input
  kansha help                this text

Options (before or after the command):
  --book <path>   the book's .db file (else KANSHA_DB, else the last book opened)
  --from <name>   the sender shown at review (default: command line)

An entry is: <date> <payee> <amount> <category or split> acct <account>
             [memo \"<text>\"] [id <uuid>]
Dates: YYYY-MM-DD, today, yesterday, or the book's date format.
Quote any name with spaces (acct \"Checking 083\"); quoting any word is allowed.
Asked field by field, an amount may be a sum: 4.97+3.99, 3.99*3, 20-4.55, 10/3;
= works it out. On a split line, write the sum without spaces (3.99*2 Food).
";

/// The first word that makes Kansha run as a command, not a window.
pub fn is_command(first: &str) -> bool {
    matches!(first, "add" | "run" | "help" | "--help" | "-h")
}

/// Asks the interactive questions of `kansha add` (CLI-030).
pub trait Prompter {
    /// One answer to `prompt`; Tab completes from `choices`. `None` when
    /// the user quits (Ctrl+C, Ctrl+D).
    fn ask(&mut self, prompt: &str, choices: &[String]) -> Option<String>;
    /// [`Prompter::ask`] for an amount, which may be a sum (REG-035): a
    /// terminal works the sum out when `=` is typed ([`equals`]).
    fn ask_sum(&mut self, prompt: &str, choices: &[String]) -> Option<String> {
        self.ask(prompt, choices)
    }
    /// A line of output between questions.
    fn say(&mut self, text: &str);
}

/// Where the command looks for things.
pub struct Env<'a> {
    pub clock: &'a dyn Clock,
    /// The desktop app's config file (recent books).
    pub config: Option<PathBuf>,
    /// `KANSHA_DB`, when set.
    pub kansha_db: Option<PathBuf>,
}

/// The streams a command uses.
pub struct Io<'a> {
    pub stdin: &'a mut dyn Read,
    pub out: &'a mut dyn Write,
    pub err: &'a mut dyn Write,
}

struct Args {
    command: String,
    book: Option<PathBuf>,
    from: Option<String>,
    rest: Vec<String>,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut book = None;
    let mut from = None;
    let mut words = Vec::new();
    let mut it = args.iter();
    let mut options = true;
    while let Some(a) = it.next() {
        if options && a == "--" {
            options = false;
        } else if options && (a == "--book" || a == "--from") {
            let v = it.next().ok_or_else(|| format!("{a} needs a value"))?;
            if a == "--book" {
                book = Some(PathBuf::from(v));
            } else {
                from = Some(v.clone());
            }
        } else if options && let Some(v) = a.strip_prefix("--book=") {
            book = Some(PathBuf::from(v));
        } else if options && let Some(v) = a.strip_prefix("--from=") {
            from = Some(v.to_string());
        } else {
            words.push(a.clone());
        }
    }
    if words.is_empty() {
        return Err("no command".into());
    }
    let command = words.remove(0);
    Ok(Args {
        command,
        book,
        from,
        rest: words,
    })
}

/// Run a command; returns the exit code. `prompter` is `None` when
/// standard input is not a terminal.
pub fn main(
    args: &[String],
    env: &Env<'_>,
    io: Io<'_>,
    prompter: Option<&mut dyn Prompter>,
) -> i32 {
    let Io { stdin, out, err } = io;
    let a = match parse_args(args) {
        Ok(a) => a,
        Err(m) => {
            let _ = writeln!(err, "kansha: {m}\n\n{USAGE}");
            return 2;
        }
    };
    let result = match a.command.as_str() {
        "help" | "--help" | "-h" => {
            let _ = write!(out, "{USAGE}");
            return 0;
        }
        "add" => add(&a, env, stdin, out, prompter),
        "run" => run(&a, env, stdin, out),
        other => Err(Failure::Usage(format!("unknown command {other:?}"))),
    };
    match result {
        Ok(()) => 0,
        Err(Failure::Usage(m)) => {
            let _ = writeln!(err, "kansha: {m}\n\n{USAGE}");
            2
        }
        Err(Failure::Refused(lines)) => {
            for l in lines {
                let _ = writeln!(err, "{l}");
            }
            let _ = writeln!(err, "Nothing was saved.");
            1
        }
    }
}

enum Failure {
    Usage(String),
    Refused(Vec<String>),
}

fn refused(m: impl Into<String>) -> Failure {
    Failure::Refused(vec![m.into()])
}

/// The book: `--book`, else `KANSHA_DB`, else the most recent book on
/// disk in the app's config.
fn book_files(a: &Args, env: &Env<'_>) -> Result<BookFiles, Failure> {
    let path = match (&a.book, &env.kansha_db) {
        (Some(p), _) | (None, Some(p)) => p.clone(),
        (None, None) => {
            let recent = env
                .config
                .as_deref()
                .map(LocalConfig::load)
                .map(|c| c.recent_books)
                .unwrap_or_default();
            recent
                .iter()
                .map(PathBuf::from)
                .find(|p| p.is_file())
                .ok_or_else(|| {
                    refused("No book found: open one in Kansha first, or use --book <path>.")
                })?
        }
    };
    if !path.is_file() {
        return Err(refused(format!("No book at {}.", path.display())));
    }
    Ok(BookFiles::at(path))
}

/// What the names copy gives: names to check and the date format.
struct Copy {
    names: Option<Names>,
    date_format: Option<DateFormat>,
    /// Why names are not checked, when they are not.
    note: Option<String>,
}

fn names_copy(files: &BookFiles) -> Copy {
    match sidecar::read_names_copy(files) {
        Some(Ok(n)) => Copy {
            names: Some(Names::from(&n)),
            date_format: n.date_format,
            note: None,
        },
        Some(Err(_)) => Copy {
            names: None,
            date_format: None,
            note: Some(format!(
                "names not checked: {} is damaged",
                sidecar::names_copy_path(files).display()
            )),
        },
        None => Copy {
            names: None,
            date_format: None,
            note: Some(
                "names not checked: there is no names copy yet (close the book in Kansha \
                 once to make one)"
                    .into(),
            ),
        },
    }
}

/// `add <entry words…>`: the words as one entry line, a word with spaces
/// or quotes quoted, so `acct "Double Cash"` works as typed in a shell.
/// Keywords stay keywords.
fn entry_text(words: &[String]) -> String {
    let line: Vec<String> = words
        .iter()
        .map(|w| {
            let plain = !w.is_empty()
                && !w.starts_with('#')
                && !w
                    .chars()
                    .any(|c| c.is_whitespace() || c == '"' || c == '\\');
            if plain { w.clone() } else { format::quote(w) }
        })
        .collect();
    format!("kansha-entries 1\n{}\n", line.join(" "))
}

fn line_errors(errors: Vec<LineError>, one_line: bool) -> Failure {
    Failure::Refused(
        errors
            .into_iter()
            .map(|e| if one_line { e.message } else { e.to_string() })
            .collect(),
    )
}

fn add(
    a: &Args,
    env: &Env<'_>,
    stdin: &mut dyn Read,
    out: &mut dyn Write,
    prompter: Option<&mut dyn Prompter>,
) -> Result<(), Failure> {
    let files = book_files(a, env)?;
    let copy = names_copy(&files);
    let dates = Dates::Typed {
        today: env.clock.today(),
        format: copy.date_format,
    };
    let mut file = if a.rest.is_empty() {
        let _ = stdin;
        let Some(p) = prompter else {
            return Err(Failure::Usage(
                "kansha add needs an entry when input is not a terminal; to pipe entries \
                 use kansha run -"
                    .into(),
            ));
        };
        let entry = interactive(p, dates, &copy)?;
        EntryFile {
            from: None,
            entries: vec![entry],
        }
    } else {
        format::parse_entries(&entry_text(&a.rest), dates).map_err(|e| line_errors(e, true))?
    };
    file.from = Some(a.from.clone().unwrap_or_else(|| DEFAULT_SENDER.into()));
    deliver(file, &files, &copy, true, out)
}

fn run(a: &Args, env: &Env<'_>, stdin: &mut dyn Read, out: &mut dyn Write) -> Result<(), Failure> {
    let [source] = &a.rest[..] else {
        return Err(Failure::Usage(
            "kansha run takes one file, or - for standard input".into(),
        ));
    };
    let files = book_files(a, env)?;
    let copy = names_copy(&files);
    let mut bytes = Vec::new();
    let limit = u64::try_from(MAX_FILE_BYTES + 1).unwrap_or(u64::MAX);
    let read = if source == "-" {
        stdin.take(limit).read_to_end(&mut bytes)
    } else {
        std::fs::File::open(source).and_then(|f| f.take(limit).read_to_end(&mut bytes))
    };
    read.map_err(|e| refused(format!("cannot read {source}: {e}")))?;
    let text =
        String::from_utf8(bytes).map_err(|_| refused(format!("{source} is not UTF-8 text")))?;
    let dates = Dates::Typed {
        today: env.clock.today(),
        format: copy.date_format,
    };
    let mut file = format::parse_entries(&text, dates).map_err(|e| line_errors(e, false))?;
    if file.from.is_none() || a.from.is_some() {
        file.from = Some(a.from.clone().unwrap_or_else(|| DEFAULT_SENDER.into()));
    }
    deliver(file, &files, &copy, false, out)
}

/// Check names, give each entry an id, and write the file to the inbox.
fn deliver(
    file: EntryFile,
    files: &BookFiles,
    copy: &Copy,
    one_line: bool,
    out: &mut dyn Write,
) -> Result<(), Failure> {
    let mut file = match &copy.names {
        Some(n) => n.check(&file).map_err(|e| line_errors(e, one_line))?,
        None => file,
    };
    for e in &mut file.entries {
        if e.id.is_none() {
            e.id = Some(format::new_id().map_err(|e| refused(e.to_string()))?);
        }
    }
    write_inbox(&sidecar::inbox_folder(files), &format::write_entries(&file))
        .map_err(|e| refused(e.to_string()))?;
    let n = file.entries.len();
    let _ = write!(
        out,
        "accepted (unverified): {n} entr{} for {}",
        if n == 1 { "y" } else { "ies" },
        files.name()
    );
    match &copy.note {
        Some(note) => {
            let _ = writeln!(out, "; {note}");
        }
        None => {
            let _ = writeln!(out);
        }
    }
    Ok(())
}

/// Write `text` into the inbox as `<UTC stamp>-<8 hex>.txt`: first as a
/// hidden temporary file, then renamed, so the app never reads half a
/// file.
pub fn write_inbox(folder: &Path, text: &str) -> crate::Result<PathBuf> {
    std::fs::create_dir_all(folder)?;
    let mut rnd = [0u8; 4];
    getrandom::getrandom(&mut rnd)
        .map_err(|e| crate::Error::Io(format!("no random numbers: {e}")))?;
    let hex: String = rnd.iter().map(|b| format!("{b:02x}")).collect();
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let name = format!("{stamp}-{hex}.txt");
    let tmp = folder.join(format!(".{name}"));
    write_atomic(&tmp, text.as_bytes())?;
    let path = folder.join(&name);
    std::fs::rename(&tmp, &path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    Ok(path)
}

// ---------------------------------------------------------------------------
// Interactive add
// ---------------------------------------------------------------------------

/// Where Tab completion starts in `line` (cursor at `pos`), and the
/// choices that fit. A split line's leading amount is skipped, so its
/// category completes too. Case is ignored.
pub fn complete(line: &str, pos: usize, choices: &[String]) -> (usize, Vec<String>) {
    let head = &line[..pos.min(line.len())];
    let start = match head.split_once(' ') {
        Some((first, _)) if calc::eval_amount(first).is_ok() => {
            first.len() + head[first.len()..].len() - head[first.len()..].trim_start().len()
        }
        _ => 0,
    };
    let typed = head[start..].to_lowercase();
    let fits = choices
        .iter()
        .filter(|c| c.to_lowercase().starts_with(&typed))
        .cloned()
        .collect();
    (start, fits)
}

struct Quit;

impl From<Quit> for Failure {
    fn from(_: Quit) -> Failure {
        refused("Quit.")
    }
}

/// What `=` puts in place of the text before the cursor at an amount
/// question: the sum worked out (REG-035). `None` when it is not a sum,
/// and `=` is typed as itself.
pub fn equals(head: &str) -> Option<String> {
    calc::eval_amount(head).ok().map(|m| m.to_string())
}

/// An amount answer: a plain amount as in files, or a sum (REG-035),
/// which may end in `=`. True for a sum, whose total is shown.
fn amount_or_sum(s: &str) -> Result<(Money, bool), String> {
    if let Ok(m) = format::parse_amount(s) {
        return Ok((m, false));
    }
    let sum = s.strip_suffix('=').unwrap_or(s);
    calc::eval_amount(sum)
        .map(|m| (m, true))
        .map_err(|e| format!("{e}."))
}

/// Ask until `check` accepts the answer; its error is shown and the
/// question asked again.
fn ask_until<T>(
    p: &mut dyn Prompter,
    prompt: &str,
    choices: &[String],
    check: impl FnMut(&str) -> Result<T, String>,
) -> Result<T, Quit> {
    ask_loop(p, prompt, choices, false, check)
}

/// [`ask_until`] for an amount, which may be a sum.
fn ask_sum_until<T>(
    p: &mut dyn Prompter,
    prompt: &str,
    choices: &[String],
    check: impl FnMut(&str) -> Result<T, String>,
) -> Result<T, Quit> {
    ask_loop(p, prompt, choices, true, check)
}

fn ask_loop<T>(
    p: &mut dyn Prompter,
    prompt: &str,
    choices: &[String],
    sum: bool,
    mut check: impl FnMut(&str) -> Result<T, String>,
) -> Result<T, Quit> {
    loop {
        let answer = if sum {
            p.ask_sum(prompt, choices)
        } else {
            p.ask(prompt, choices)
        }
        .ok_or(Quit)?;
        match check(answer.trim()) {
            Ok(v) => return Ok(v),
            Err(m) => p.say(&m),
        }
    }
}

fn interactive(p: &mut dyn Prompter, dates: Dates, copy: &Copy) -> Result<FileEntry, Failure> {
    if let Some(note) = &copy.note {
        p.say(&format!("Note: {note}."));
    }
    let names = copy.names.as_ref();
    let accounts = names.map(|n| n.accounts.clone()).unwrap_or_default();
    let mut categories = names.map(|n| n.categories.clone()).unwrap_or_default();
    let Dates::Typed { today, .. } = dates else {
        return Err(refused("interactive add needs today's date"));
    };
    let date: Date = ask_until(p, &format!("Date [{today}]: "), &[], |s| {
        if s.is_empty() {
            Ok(today)
        } else {
            format::parse_date(s, dates)
        }
    })?;
    let payee = ask_until(p, "Payee: ", &[], |s| {
        if s.is_empty() {
            Err("The payee is required.".into())
        } else {
            Ok(s.to_string())
        }
    })?;
    let (amount, sum) = ask_sum_until(p, "Amount: ", &[], |s| match amount_or_sum(s) {
        Ok((m, _)) if m.is_zero() => Err("The amount is zero.".into()),
        other => other,
    })?;
    if sum {
        p.say(&format!("= {amount}"));
    }
    let account = ask_until(p, "Account: ", &accounts, |s| {
        resolve(s, |n| names.map(|x| x.resolve_account(n)))
    })?;
    categories.push("split".into());
    let category = ask_until(p, "Category (or split): ", &categories, |s| {
        if s.eq_ignore_ascii_case("split") {
            return Ok(None);
        }
        resolve(s, |n| names.map(|x| x.resolve_category(n))).map(Some)
    })?;
    categories.pop();
    let spend = match category {
        Some(c) => Spend::Category(c),
        None => Spend::Split(split_lines(p, amount, &categories, names)?),
    };
    let memo = p.ask("Memo (Enter for none): ", &[]).ok_or(Quit)?;
    Ok(FileEntry {
        line: 0,
        date,
        payee,
        amount,
        spend,
        account,
        memo: memo.trim().to_string(),
        id: None,
    })
}

/// A name as the book spells it; as typed when there is no names copy.
fn resolve(
    s: &str,
    find: impl Fn(&str) -> Option<Result<String, super::NameError>>,
) -> Result<String, String> {
    if s.is_empty() {
        return Err("A name is required.".into());
    }
    match find(s) {
        None => Ok(s.to_string()),
        Some(Ok(name)) => Ok(name),
        Some(Err(e)) => Err(format!("{e}.")),
    }
}

/// Split lines until nothing is left. Each takes an amount and a
/// category: `<amount> <category>` at once, or the amount alone and the
/// category at the next question.
fn split_lines(
    p: &mut dyn Prompter,
    total: Money,
    categories: &[String],
    names: Option<&Names>,
) -> Result<Vec<SplitLine>, Failure> {
    let mut lines: Vec<SplitLine> = Vec::new();
    let mut left = total;
    p.say("Split lines: <amount> <category>, or the amount, then the category.");
    while !left.is_zero() {
        let n = lines.len() + 1;
        let prompt = format!("  Line {n} amount ({left} left): ");
        let first = lines.is_empty();
        let resolve_cat = |c: &str| resolve(c, |x| names.map(|v| v.resolve_category(x)));
        let (amount, sum, category) = ask_sum_until(p, &prompt, categories, |s| {
            let (amount, category) = match s.split_once(' ') {
                Some((a, rest)) => (a, Some(rest.trim())),
                None => (s, None),
            };
            let op = |c: char| matches!(c, '+' | '-' | '*' | '/');
            if category.is_some_and(|c| c.starts_with(op)) || amount.ends_with(op) {
                return Err("A sum before a category has no spaces: 3.99*2 Groceries \
                     (or press = to work it out)."
                    .into());
            }
            let (amount, sum) = match amount_or_sum(amount) {
                Ok(a) => a,
                Err(e)
                    if amount.starts_with(|c: char| c.is_ascii_digit() || ".$-+".contains(c)) =>
                {
                    return Err(e);
                }
                Err(_) => {
                    return Err(
                        "Enter the amount first: <amount> <category>, or the amount alone.".into(),
                    );
                }
            };
            if amount.is_zero() {
                return Err("A split line of zero.".into());
            }
            if first && amount == total {
                return Err("A split needs at least two lines: enter less than the total.".into());
            }
            let category = category.map(resolve_cat).transpose()?;
            Ok((amount, sum, category))
        })?;
        if sum {
            p.say(&format!("  = {amount}"));
        }
        let category = match category {
            Some(c) => c,
            None => ask_until(
                p,
                &format!("  Line {n} category ({amount}): "),
                categories,
                |s| resolve_cat(s),
            )?,
        };
        left = left
            .checked_sub(amount)
            .ok_or_else(|| refused("the amounts are too large"))?;
        lines.push(SplitLine {
            line: 0,
            amount,
            category,
            memo: String::new(),
        });
        if !left.is_zero() {
            p.say(&format!("  {left} left"));
        }
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date::FixedClock;
    use crate::entries::format::{NamesFile, parse_entries};

    fn d(s: &str) -> Date {
        s.parse().unwrap()
    }

    /// A book file (any file will do: the command never opens it) with
    /// an optional names copy beside it.
    fn book(dir: &Path, names: bool) -> PathBuf {
        let db = dir.join("barton2026.db");
        std::fs::write(&db, b"not opened").unwrap();
        if names {
            let n = NamesFile {
                book: "barton2026".into(),
                date_format: Some(DateFormat::Mdy),
                accounts: vec!["Checking".into(), "Double Cash".into()],
                categories: [
                    "Auto",
                    "Auto:Fuel",
                    "Food",
                    "Food:Groceries",
                    "Household",
                    "Taxes:Sales",
                ]
                .map(String::from)
                .to_vec(),
            };
            std::fs::write(dir.join("barton2026-names.txt"), format::write_names(&n)).unwrap();
        }
        db
    }

    struct Run {
        code: i32,
        out: String,
        err: String,
    }

    fn cli(args: &[&str], stdin: &str, prompter: Option<&mut dyn Prompter>) -> Run {
        let clock = FixedClock::new(d("2026-10-12"));
        let env = Env {
            clock: &clock,
            config: None,
            kansha_db: None,
        };
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let mut input = stdin.as_bytes();
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = main(
            &args,
            &env,
            Io {
                stdin: &mut input,
                out: &mut out,
                err: &mut err,
            },
            prompter,
        );
        Run {
            code,
            out: String::from_utf8(out).unwrap(),
            err: String::from_utf8(err).unwrap(),
        }
    }

    fn inbox(dir: &Path) -> Vec<EntryFile> {
        let folder = dir.join("barton2026-inbox");
        let mut names: Vec<PathBuf> = std::fs::read_dir(&folder)
            .map(|d| d.map(|e| e.unwrap().path()).collect())
            .unwrap_or_default();
        names.sort();
        names
            .iter()
            .map(|p| {
                let n = p.file_name().unwrap().to_string_lossy().into_owned();
                assert!(!n.starts_with('.'), "temporary file left: {n}");
                assert!(n.ends_with(".txt") && n.len() == "20261012T101010Z-0a1b2c3d.txt".len());
                parse_entries(&std::fs::read_to_string(p).unwrap(), Dates::Iso).unwrap()
            })
            .collect()
    }

    #[test]
    fn add_one_entry_with_names_checked() {
        let dir = tempfile::tempdir().unwrap();
        let db = book(dir.path(), true);
        let r = cli(
            &[
                "add",
                "--book",
                db.to_str().unwrap(),
                "today",
                "Shell",
                "45.20",
                "fuel",
                "acct",
                "Double Cash",
            ],
            "",
            None,
        );
        assert_eq!((r.code, r.err.as_str()), (0, ""));
        assert_eq!(r.out, "accepted (unverified): 1 entry for barton2026\n");
        let files = inbox(dir.path());
        let e = &files[0].entries[0];
        assert_eq!(files[0].from.as_deref(), Some("command line"));
        assert_eq!(e.date, d("2026-10-12"));
        assert_eq!(e.spend, Spend::Category("Auto:Fuel".into()));
        assert_eq!(e.account, "Double Cash");
        assert!(e.id.is_some());
    }

    #[test]
    fn a_bad_name_lists_close_matches_and_saves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let db = book(dir.path(), true);
        let r = cli(
            &[
                "--book",
                db.to_str().unwrap(),
                "add",
                "today",
                "Kroger",
                "12",
                "Grocries",
                "acct",
                "Checking",
            ],
            "",
            None,
        );
        assert_eq!(r.code, 1);
        assert_eq!(
            r.err,
            "no category “Grocries”; did you mean Food:Groceries?\nNothing was saved.\n"
        );
        assert!(inbox(dir.path()).is_empty());
    }

    #[test]
    fn without_a_names_copy_names_are_not_checked_and_the_answer_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let db = book(dir.path(), false);
        let r = cli(
            &[
                "add",
                "--book",
                db.to_str().unwrap(),
                "2026-10-11",
                "Kroger",
                "12",
                "Anything",
                "acct",
                "Whatever",
            ],
            "",
            None,
        );
        assert_eq!(r.code, 0, "{}", r.err);
        assert!(
            r.out
                .contains("names not checked: there is no names copy yet")
        );
        assert_eq!(inbox(dir.path())[0].entries[0].account, "Whatever");
        // No date format either: only ISO, today, and yesterday.
        let r = cli(
            &[
                "add",
                "--book",
                db.to_str().unwrap(),
                "10/11/2026",
                "K",
                "1",
                "A",
                "acct",
                "B",
            ],
            "",
            None,
        );
        assert_eq!(r.code, 1);
    }

    #[test]
    fn run_reads_a_file_with_book_dates_and_writes_iso() {
        let dir = tempfile::tempdir().unwrap();
        let db = book(dir.path(), true);
        let src = dir.path().join("in.txt");
        std::fs::write(
            &src,
            "kansha-entries 1
yesterday Kroger 10.00 Groceries acct Checking
10/3/2026 Target 107.00 split acct Checking
  7.00 Taxes:Sales
  100 Household
",
        )
        .unwrap();
        let r = cli(
            &[
                "run",
                src.to_str().unwrap(),
                "--book",
                db.to_str().unwrap(),
                "--from",
                "Pat",
            ],
            "",
            None,
        );
        assert_eq!(r.code, 0, "{}", r.err);
        assert_eq!(r.out, "accepted (unverified): 2 entries for barton2026\n");
        let f = &inbox(dir.path())[0];
        assert_eq!(f.from.as_deref(), Some("Pat"));
        assert_eq!(f.entries[0].date, d("2026-10-11"));
        assert_eq!(f.entries[1].date, d("2026-10-03"));
        let text = std::fs::read_to_string(
            std::fs::read_dir(dir.path().join("barton2026-inbox"))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path(),
        )
        .unwrap();
        assert!(text.contains("2026-10-03 Target 107.00 split"), "{text}");
        assert!(text.contains("  100.00 Household"), "{text}");
    }

    #[test]
    fn run_from_stdin_and_errors_by_line() {
        let dir = tempfile::tempdir().unwrap();
        let db = book(dir.path(), true);
        let ok = cli(
            &["run", "-", "--book", db.to_str().unwrap()],
            "kansha-entries 1\ntoday Shell 5 Fuel acct Checking\n",
            None,
        );
        assert_eq!(ok.code, 0, "{}", ok.err);
        let bad = cli(
            &["run", "-", "--book", db.to_str().unwrap()],
            "kansha-entries 1\ntoday Shell 5 Fuel acct Checking\ntoday Shell five Fuel acct Chk\n",
            None,
        );
        assert_eq!(bad.code, 1);
        assert_eq!(
            bad.err,
            "line 3: “five” is not an amount\nNothing was saved.\n"
        );
        assert_eq!(inbox(dir.path()).len(), 1);
    }

    #[test]
    fn usage_and_book_errors() {
        let r = cli(&["run"], "", None);
        assert_eq!(r.code, 2);
        assert!(r.err.contains("kansha run takes one file"));
        let r = cli(&["help"], "", None);
        assert_eq!(r.code, 0);
        assert!(r.out.contains("kansha add"));
        let r = cli(
            &[
                "add",
                "--book",
                "/no/such/book.db",
                "today",
                "x",
                "1",
                "y",
                "acct",
                "z",
            ],
            "",
            None,
        );
        assert_eq!(r.code, 1);
        assert!(r.err.starts_with("No book at /no/such/book.db."));
        let r = cli(&["add", "today", "x", "1", "y", "acct", "z"], "", None);
        assert_eq!(r.code, 1);
        assert!(r.err.contains("No book found"));
    }

    #[test]
    fn add_without_a_terminal_points_to_run() {
        let dir = tempfile::tempdir().unwrap();
        let db = book(dir.path(), true);
        let r = cli(&["add", "--book", db.to_str().unwrap()], "", None);
        assert_eq!(r.code, 2);
        assert!(r.err.contains("use kansha run -"));
    }

    #[test]
    fn the_book_comes_from_kansha_db_then_the_config() {
        let dir = tempfile::tempdir().unwrap();
        let db = book(dir.path(), true);
        let config = dir.path().join("config.json");
        let cfg = LocalConfig {
            recent_books: vec!["/gone.db".into(), db.display().to_string()],
            ..LocalConfig::default()
        };
        cfg.save(&config).unwrap();
        let clock = FixedClock::new(d("2026-10-12"));
        let env = Env {
            clock: &clock,
            config: Some(config),
            kansha_db: None,
        };
        let a = parse_args(&["add".to_string()]).unwrap();
        assert_eq!(book_files(&a, &env).ok().unwrap().db, db);
        let env = Env {
            kansha_db: Some(PathBuf::from("/other.db")),
            ..env
        };
        assert!(book_files(&a, &env).is_err());
    }

    /// Scripted answers; `None` quits.
    struct Script {
        answers: Vec<Option<&'static str>>,
        asked: Vec<String>,
        /// Questions asked as amounts (`=` works the sum out).
        sums: Vec<String>,
        said: Vec<String>,
    }

    impl Script {
        fn new(answers: &[Option<&'static str>]) -> Script {
            Script {
                answers: answers.iter().rev().copied().collect(),
                asked: Vec::new(),
                sums: Vec::new(),
                said: Vec::new(),
            }
        }
    }

    impl Prompter for Script {
        fn ask(&mut self, prompt: &str, _choices: &[String]) -> Option<String> {
            self.asked.push(prompt.to_string());
            self.answers.pop().expect("an answer").map(String::from)
        }
        fn ask_sum(&mut self, prompt: &str, choices: &[String]) -> Option<String> {
            self.sums.push(prompt.to_string());
            self.ask(prompt, choices)
        }
        fn say(&mut self, text: &str) {
            self.said.push(text.to_string());
        }
    }

    #[test]
    fn interactive_add_asks_again_and_every_line_has_an_amount() {
        let dir = tempfile::tempdir().unwrap();
        let db = book(dir.path(), true);
        let mut s = Script::new(&[
            Some(""), // date: today
            Some("Target"),
            Some("107"),
            Some("Chekcing"), // wrong: asked again
            Some("checking"),
            Some("split"),
            Some("107"), // the whole total: asked again
            Some("7 Taxes:Sales"),
            Some("60"), // amount alone: its category is asked next
            Some("groceries"),
            Some("Household"), // no amount: asked again
            Some("40 Household"),
            Some("weekly"),
        ]);
        let r = cli(&["add", "--book", db.to_str().unwrap()], "", Some(&mut s));
        assert_eq!(r.code, 0, "{}", r.err);
        assert!(
            s.said
                .contains(&"no account “Chekcing”; did you mean Checking?.".to_string())
        );
        assert!(
            s.asked
                .contains(&"  Line 2 amount (100.00 left): ".to_string())
        );
        assert!(s.asked.contains(&"  Line 2 category (60.00): ".to_string()));
        assert!(
            s.asked
                .contains(&"  Line 3 amount (40.00 left): ".to_string())
        );
        assert!(
            s.said
                .iter()
                .any(|t| t.starts_with("Enter the amount first"))
        );
        let e = &inbox(dir.path())[0].entries[0];
        assert_eq!(e.date, d("2026-10-12"));
        assert_eq!(e.account, "Checking");
        assert_eq!(e.memo, "weekly");
        let Spend::Split(lines) = &e.spend else {
            panic!()
        };
        let got: Vec<(Money, &str)> = lines
            .iter()
            .map(|l| (l.amount, l.category.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                (Money::from_cents(700), "Taxes:Sales"),
                (Money::from_cents(6000), "Food:Groceries"),
                (Money::from_cents(4000), "Household"),
            ]
        );
    }

    /// The entry `kansha add` saved after the scripted answers.
    fn add_scripted(answers: &[Option<&'static str>]) -> (Script, FileEntry) {
        let dir = tempfile::tempdir().unwrap();
        let db = book(dir.path(), true);
        let mut s = Script::new(answers);
        let r = cli(&["add", "--book", db.to_str().unwrap()], "", Some(&mut s));
        assert_eq!(r.code, 0, "{}", r.err);
        let e = inbox(dir.path())[0].entries[0].clone();
        (s, e)
    }

    #[test]
    fn amount_takes_a_sum_and_shows_the_total() {
        let (s, e) = add_scripted(&[
            Some(""),
            Some("Target"),
            Some("4.97+"), // malformed: asked again
            Some("5/0"),
            Some("abc"),
            Some("1-1"), // zero
            Some("4.97+3.99+14.47"),
            Some("Checking"),
            Some("Food"),
            Some(""),
        ]);
        assert_eq!(e.amount, Money::from_cents(2343));
        assert_eq!(s.sums, vec!["Amount: "; 5]);
        assert!(s.said.contains(&"= 23.43".to_string()));
        assert!(s.said.contains(&"The amount is zero.".to_string()));
        assert!(s.said.iter().any(|t| t.contains("divide by zero")));
        assert!(s.said.iter().any(|t| t.contains("\"abc\"")));
        assert!(s.said.iter().any(|t| t.contains("missing at the end")));
    }

    #[test]
    fn amount_sums_minus_times_divide_and_round_half_even() {
        let total = |a: &'static str| {
            let (s, e) = add_scripted(&[
                Some(""),
                Some("Target"),
                Some(a),
                Some("Checking"),
                Some("Food"),
                Some(""),
            ]);
            (e.amount.to_string(), s.said)
        };
        assert_eq!(total("3.99*3").0, "11.97");
        assert_eq!(total("20-4.55").0, "15.45");
        assert_eq!(total("1/3").0, "0.33");
        assert_eq!(total("0.125*1").0, "0.12");
        assert_eq!(total("4.97+3.99=").0, "8.96"); // a trailing = is fine
        assert_eq!(total("-3-2").0, "-5.00"); // a refund
        // Plain amounts as before, with no total shown.
        for (typed, want) in [
            ("12.30", "12.30"),
            ("-5", "-5.00"),
            ("$-5", "-5.00"),
            ("$1,234.50", "1234.50"),
        ] {
            let (got, said) = total(typed);
            assert_eq!(got, want, "{typed}");
            assert!(!said.iter().any(|t| t.starts_with("= ")), "{typed}");
        }
    }

    #[test]
    fn split_lines_take_sums() {
        let (s, e) = add_scripted(&[
            Some(""),
            Some("Target"),
            Some("23.43"),
            Some("Checking"),
            Some("split"),
            Some("3.99 * 2 Groceries"), // spaces inside a sum: asked again
            Some("3.99*2 Groceries"),
            Some("20-4.55"), // amount alone: category next
            Some("Household"),
            Some(""),
        ]);
        assert!(
            s.sums
                .contains(&"  Line 1 amount (23.43 left): ".to_string())
        );
        assert!(
            s.asked
                .contains(&"  Line 2 amount (15.45 left): ".to_string())
        );
        assert!(s.said.iter().any(|t| t.contains("no spaces")));
        assert!(s.said.contains(&"  = 7.98".to_string()));
        assert!(s.said.contains(&"  15.45 left".to_string()));
        let Spend::Split(lines) = &e.spend else {
            panic!()
        };
        let got: Vec<(Money, &str)> = lines
            .iter()
            .map(|l| (l.amount, l.category.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                (Money::from_cents(798), "Food:Groceries"),
                (Money::from_cents(1545), "Household"),
            ]
        );
    }

    #[test]
    fn equals_works_out_the_sum_before_the_cursor() {
        assert_eq!(equals("4.97+3.99+14.47").as_deref(), Some("23.43"));
        assert_eq!(equals("3.99 * 2").as_deref(), Some("7.98"));
        assert_eq!(equals("10-4/2").as_deref(), Some("8.00"));
        assert_eq!(equals("12.3").as_deref(), Some("12.30"));
        assert_eq!(equals("4.97+"), None);
        assert_eq!(equals("5/0"), None);
        assert_eq!(equals("abc"), None);
        assert_eq!(equals(""), None);
    }

    #[test]
    fn files_still_take_plain_amounts_only() {
        assert!(format::parse_amount("3.99*3").is_err());
        assert!(format::parse_amount("4.97+3.99").is_err());
    }

    #[test]
    fn quitting_saves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let db = book(dir.path(), true);
        let mut s = Script::new(&[Some("today"), Some("Shell"), None]);
        let r = cli(&["add", "--book", db.to_str().unwrap()], "", Some(&mut s));
        assert_eq!(r.code, 1);
        assert_eq!(r.err, "Quit.\nNothing was saved.\n");
        assert!(inbox(dir.path()).is_empty());
    }

    #[test]
    fn completion_skips_a_leading_amount() {
        let choices: Vec<String> = ["Food", "Food:Groceries", "Household"]
            .map(String::from)
            .to_vec();
        assert_eq!(
            complete("fo", 2, &choices),
            (0, vec!["Food".into(), "Food:Groceries".into()])
        );
        assert_eq!(
            complete("7.00 ho", 7, &choices),
            (5, vec!["Household".into()])
        );
        assert_eq!(
            complete("3.99*3 ho", 9, &choices),
            (7, vec!["Household".into()])
        );
        assert_eq!(
            complete("Food:G", 6, &choices),
            (0, vec!["Food:Groceries".into()])
        );
    }

    #[test]
    fn is_command_words() {
        assert!(is_command("add") && is_command("run") && is_command("--help"));
        assert!(!is_command("") && !is_command("--book"));
    }
}
