//! The command line (CLI-030): `kansha add`, `kansha run`, `kansha help`.
//! All of it is in `kansha_core::entries::cli`; this is the terminal:
//! arguments, streams, and rustyline for the questions of `kansha add`.
//! No history is kept, so no entry is ever written to a history file.

use std::io::IsTerminal;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use kansha_core::SystemClock;
use kansha_core::entries::cli::{self, Env, Io, Prompter};
use rustyline::completion::Completer;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::MemHistory;
use rustyline::validate::Validator;
use rustyline::{
    Cmd, CompletionType, ConditionalEventHandler, Config, Context, Editor, Event, EventContext,
    EventHandler, Helper, KeyEvent, Movement, RepeatCount,
};

/// Tab completes from the choices of the question being asked.
#[derive(Default)]
struct Choices(Vec<String>);

impl Completer for Choices {
    type Candidate = String;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<String>)> {
        Ok(cli::complete(line, pos, &self.0))
    }
}

impl Hinter for Choices {
    type Hint = String;
}
impl Highlighter for Choices {}
impl Validator for Choices {}
impl Helper for Choices {}

/// `=` at an amount question works out the sum before the cursor
/// (REG-035); elsewhere, or when it is not a sum, `=` is typed.
struct Equals(Arc<AtomicBool>);

impl ConditionalEventHandler for Equals {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        ctx: &EventContext,
    ) -> Option<Cmd> {
        if !self.0.load(Ordering::Relaxed) {
            return None;
        }
        let head = ctx.line().get(..ctx.pos())?;
        let total = cli::equals(head)?;
        Some(Cmd::Replace(Movement::BeginningOfLine, Some(total)))
    }
}

struct Terminal {
    editor: Editor<Choices, MemHistory>,
    /// True while an amount question is asked.
    sum: Arc<AtomicBool>,
}

impl Prompter for Terminal {
    fn ask(&mut self, prompt: &str, choices: &[String]) -> Option<String> {
        if let Some(h) = self.editor.helper_mut() {
            h.0 = choices.to_vec();
        }
        // Ctrl+C, Ctrl+D, or a terminal failure: quit, saving nothing.
        self.editor.readline(prompt).ok()
    }

    fn ask_sum(&mut self, prompt: &str, choices: &[String]) -> Option<String> {
        self.sum.store(true, Ordering::Relaxed);
        let answer = self.ask(prompt, choices);
        self.sum.store(false, Ordering::Relaxed);
        answer
    }

    fn say(&mut self, text: &str) {
        println!("{text}");
    }
}

fn terminal() -> Option<Terminal> {
    if !std::io::stdin().is_terminal() {
        return None;
    }
    let config = Config::builder()
        .auto_add_history(false)
        .completion_type(CompletionType::List)
        .build();
    let mut editor = Editor::with_history(config, MemHistory::new()).ok()?;
    editor.set_helper(Some(Choices::default()));
    let sum = Arc::new(AtomicBool::new(false));
    editor.bind_sequence(
        KeyEvent::from('='),
        EventHandler::Conditional(Box::new(Equals(Arc::clone(&sum)))),
    );
    Some(Terminal { editor, sum })
}

/// Run a command when the arguments name one; `None` starts the window.
pub fn main() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.iter().any(|a| cli::is_command(a)) {
        return None;
    }
    let clock = SystemClock;
    let env = Env {
        clock: &clock,
        config: kansha_core::local_config::default_path(),
        kansha_db: std::env::var_os("KANSHA_DB").map(Into::into),
    };
    let mut prompter = terminal();
    let code = cli::main(
        &args,
        &env,
        Io {
            stdin: &mut std::io::stdin().lock(),
            out: &mut std::io::stdout().lock(),
            err: &mut std::io::stderr().lock(),
        },
        prompter.as_mut().map(|t| t as &mut dyn Prompter),
    );
    Some(code)
}
