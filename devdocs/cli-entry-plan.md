# Command line and file entry — build plan (#20)

Spec 0.7.83 §14.3, CLI-010 … CLI-080 (CLI-090 phone app is out of
scope). Written 2026-10-12. Four steps; Stan tries each in the app
before the next. Questions for Stan are in §7.

## 1. Decided

- Spending entries only: splits, refunds as negative amounts, memo,
  sender, per-entry id. No transfers, investments, or new categories.
- One executable: `main` checks the arguments first; with a command it
  runs core-only CLI code and exits, with none the desktop starts as
  now. Windows `kansha.com` stub comes with the Windows port.
- The CLI never opens the book. It writes to `<book>-inbox/` and
  checks names against `<book>-names.txt` when present.
- Inbox file names: `<UTC stamp>-<8 hex random>.txt`.
- Pending entries live outside the ledger until approved.
- Plain text throughout; encryption [Later].

## 2. Formats (step 1)

### `kansha-entries 1`

```
kansha-entries 1
from "Stan's phone"
2026-10-12 Shell 45.20 Auto:Fuel acct "Double Cash" memo "Ohio trip"
2026-10-12 Costco 182.40 split acct Checking id 1b4e28ba-2fa1-4d2a-9c3e-0f6a8b7c5d21
  120.00 Food:Groceries
  62.40 Household memo "paper goods"
2026-10-14 Costco -24.99 Household acct Checking memo "return"
```

- UTF-8; a leading BOM and CRLF line ends are accepted (phones,
  Windows). Files over 1 MB are refused.
- Words are separated by spaces; `"…"` quotes a word with spaces
  (`\"` and `\\` inside). `#` starts a comment outside quotes.
- Entry line: `<date> <payee> <amount> <category | split>`, then
  `acct <account>` (required), `memo "<text>"`, `id <uuid>` in any
  order. A payee spelled like a keyword is quoted.
- Split lines start with a space or tab: `<amount> <category>
  [memo "<text>"]`. At least two; they must add up to the entry's
  amount (signs may differ, e.g. a coupon). One line per entry may
  leave out its amount (`<category> [memo …]`): it takes the rest,
  which must not be zero.
- Dates: `YYYY-MM-DD` in the format. `kansha add` and `kansha run`
  also take `today`, `yesterday`, and the book's date format, read
  from the names copy (no copy: ISO, `today`, `yesterday` only), and
  write dates out before the inbox. The app's inbox read and Import
  entries… take ISO only.
- Amounts: optional `-`, optional `$`, thousands commas, at most two
  decimals; parsed as text into `Money` (no float). Zero is refused.
- Category: a path joined with `:`. Account and category names are
  matched later (§3), not by the parser.
- `id`: a UUID. An entry without one gets a v4 UUID made from
  `getrandom` (already a dependency; no new crate).
- Errors carry the line number; the parser reports every bad line,
  not just the first.

### `kansha-names 1`

```
kansha-names 1
book "barton2026"
date-format MM/DD/YYYY
acct Checking
acct "Double Cash"
cat Food
cat Food:Groceries
```

Expense categories with subcategories (full paths) and open
Checking, Savings, Cash, and Credit Card accounts. No payees.

### Code

New module `crates/kansha-core/src/entries/`:

- `format.rs`: tokenizer, `parse_entries(&str) -> Result<EntryFile,
  Vec<LineError>>`, `write_entries`, `parse_names`, `write_names`.
- `names.rs`: `Names::resolve_account`, `resolve_category`: case
  ignored; exact path, else a unique last part; else an error with up
  to five close matches (same prefix, then edit distance ≤ 2).
- Money and posting signs: an entry of amount A posts −A to the
  account and +A to the category (each split line its share). This
  holds for both asset and credit card accounts (spec §18 signs);
  a refund is the same with A negative.

Tests first (Opus): parser round-trips, every error with its line,
split sums, the rest line (one only, not zero), refund signs, CRLF and BOM, quoting, the resolver's
exact / last-part / ambiguous / close-match cases. Sample files in
`crates/kansha-core/tests/data/entries/`.

## 3. Pending table and review (step 2)

⚠ Schema change: migration `0019_pending_entries.sql`.

```
entry_file    id, file_name, sha256, sender, source ('cli' | 'file'),
              received_at, archive_path, entry_count
pending_entry id, file_id → entry_file, line_no, uid UNIQUE,
              date, payee, amount (cents), account_name,
              category_name (NULL for a split), memo,
              status ('pending' | 'approved' | 'rejected'),
              txn_id → txn (approved), decided_at
pending_split entry_id → pending_entry, line_no, amount,
              category_name, memo; PRIMARY KEY (entry_id, line_no)
```

- All STRICT. Names are kept as text and resolved at review, because
  categories and accounts may change between receipt and review.
- `uid UNIQUE` is the seen-id record: approved and rejected rows stay,
  so a file sent twice adds nothing (CLI-040).
- `audit_log` gains entity `pending_entry` (table rebuilt the way
  0013, 0014, and 0016 did). Receiving is audited with origin
  `system`; approving and rejecting with origin `ui`. Source, sender,
  and file go in the audit JSON (CLI-070), so `origin` and the txn
  `origin` CHECKs need no change: an approved transaction is `manual`.

Engine (`entries/review.rs`, SQL in `persistence/entries.rs`):

- `receive(tx, file, source)`: parse, skip seen uids, insert; all or
  nothing per file.
- `pending(conn) -> Vec<PendingView>`: each entry with its names
  resolved against the book, or the resolution error.
- `approve(tx, id, fixes)`: apply name fixes and edits, build a
  `TxnInput` (payee via `payees::find_or_insert`), create it with
  `ledger::create`, mark approved with `txn_id`. One transaction.
- `reject(tx, id)`: mark rejected.
- `unapprove(tx, id)`: Undo of an approval (UI-060): delete the
  transaction, set the entry back to pending, clear `txn_id` and
  `decided_at`; one transaction, audited. Offered only while nothing
  else has changed since, as other Undo.

IPC (⚠ API change, `just bindings`): `entries_pending`,
`entries_approve`, `entries_reject`, `entries_unapprove`, `entries_import` (step 3),
`entries_inbox_read` (step 3), `names_export` (step 4).

UI: a Review entries dialog (`src/lib/components/entries/`): one row
per entry (date, payee, amount, account, category or "Split (n)",
memo, sender), names that did not resolve marked with ⚠ and text,
not colour alone. Per row Approve, Edit (plain entries), Approve and
open (splits: approves, then opens the transaction in the register),
Reject; Approve all that resolve; Later. Opens by itself when the book opens with entries
pending; File > Review entries… (n) opens it any time. Entries that
arrive while the window is open do not open it: the menu count and a
status bar note change. An approval goes on the Undo slot
(`src/lib/ui/undoKey.ts`, Edit > Undo) as one change.

Tests: integration (`tests/integration/entries.rs`): receive twice
adds once; a bad file adds nothing; approve makes an ordinary
balanced transaction with the right signs for checking and credit
card, refund included; a renamed category shows as unresolved and
approves after a fix; reject leaves the ledger untouched; approve
then unapprove leaves the ledger as before and the entry pending;
approve again after unapprove makes a fresh transaction; audit rows.
Vitest for the dialog, the menu count, and the status bar note.

## 4. Ways in (step 3)

- **Inbox:** `entries::inbox::read(db, folder, book)`: each `*.txt`
  in `<book>-inbox/`, oldest first: receive, then copy into the
  import archive encrypted (as MIG-170, `import::archive_folder`)
  and delete it from the inbox. A file that fails stays in the inbox
  and is reported, with its line errors. Read when the book opens and
  when the window gets focus (`WindowEvent::Focused(true)` in
  `src-tauri/src/lib.rs`, emitting an event the frontend answers with
  `entries_inbox_read`).
- **File > Import entries…:** pick a file; same `receive`, names
  checked against the book; archived as above.
- **CLI** (`entries::cli`, called from `src-tauri/src/main.rs`):
  - `kansha add` with no entry: interactive, field by field
    (CLI-030). Date (Enter: today), payee, amount, account, category
    or `split`; for a split, lines until the amount left is zero,
    showing what is left after each, a category alone taking the
    rest; then memo. Each answer is resolved at once against the
    names copy and asked again on error, with the close matches.
    Ctrl+C or Ctrl+D quits without saving.
    - The prompt logic is in `kansha-core` behind a small `Prompter`
      trait (read a line with a completion list, print), so tests
      drive it with scripted answers.
    - The terminal side is `rustyline` (MIT; approved 2026-10-12,
      pinned `=18.0.1`, `default-features = false`: no history file,
      so no entry is ever written to one; `custom-bindings` on since
      0.7.91 for `=` in amounts). It lives in `src-tauri`,
      not in `kansha-core`. Tab completes account and category names
      from the names copy.
    - Not a terminal (piped input): `kansha add` with no entry is an
      error that points to `kansha run -`.
  - `kansha add <entry words…>`: rebuilds one entry line from the
    arguments (quoting any that hold spaces), so
    `kansha add today Shell 45.20 Auto:Fuel acct "Double Cash"`
    works as typed.
  - `kansha run <file>`; `kansha run -` reads standard input.
  - `--book <path>`, else `KANSHA_DB`, else the first recent book in
    the local config. The config path is found without Tauri:
    `$XDG_CONFIG_HOME` or `~/.config` on Linux, `%APPDATA%` on
    Windows, plus `tools.astryx.kansha/config.json` (a helper in
    `local_config`, checked against Tauri's path by a test).
  - `--from <name>`: the sender; default `command line`.
  - Checks format, then names against `<book>-names.txt` if present;
    writes the inbox file as a temp file renamed into place, so the
    app never reads half a file.
  - Answers `accepted (unverified): N entries for barton2026` and
    exits 0; errors one per line (`line 3: …`) and exit 1; usage
    exit 2. `kansha help` lists the commands.
  - Logic lives in `kansha-core` and takes its output stream as a
    parameter, so integration tests run it without the binary.

Tests: the interactive add with scripted answers (default date,
a wrong name asked again, amount left after each split line, a
category alone taking the rest, quitting saves nothing); CLI
integration (add, run, stdin, `--book`, no names copy, `today`
/ `yesterday` / book date format in `run` written out as ISO,
bad name with close matches, half-written file never read), inbox
read and archive, focus read in vitest.

## 5. Names export (step 4)

- `names::write(conn, settings) -> String` (`kansha-names 1`).
- File > Export names for phones…: save dialog, default
  `<book>-names-for-phones.txt` in Downloads.
- The copy `<book>-names.txt` beside the book: written in
  `AppState::close_book` (before the close backup) and after each
  export. Failure to write it is reported on stderr and does not stop
  the close.

Tests: the export lists expense categories and open spending accounts
only, no payees, closed accounts left out; close writes the copy.

## 6. Order and size

| Step | Content | Schema | API | Size |
|---|---|---|---|---|
| 1 | Formats, resolver | — | — | M |
| 2 | Pending table, review dialog | 0019 | yes | L |
| 3 | Inbox, Import entries…, CLI (interactive add) | — | yes | L |
| 4 | Names export and copy | — | yes | S |

Step 4 could come before step 3 so the CLI's name check can be tried
at once; it is small and independent.

## 7. Questions for Stan (all decided)

1. **Edit before approving.** Plain entries can be edited in the
   review row (date, payee, amount, account, category, memo). For a
   split: edit its lines in the dialog too, or "Approve and open",
   which approves it and opens it in the register to edit there?
   Decided 2026-10-12: Approve and open.
2. **Undo.** Should Ctrl+Z undo an approval (transaction removed,
   entry back in the list)? Decided 2026-10-12: yes.
3. **Splits on the command line.** Decided 2026-10-12: interactive
   `kansha add` (above); no one-line split form. `kansha run -` stays
   for pasting and piping.
4. **Dates in `kansha run`.** Accept `today`, `yesterday`, and the
   book's date format there too, as `kansha add` does? The CLI writes
   dates out before the inbox, so the app and phone files still see
   only `YYYY-MM-DD`. Decided 2026-10-12: yes.
5. **When the dialog opens.** Opens by itself when the book opens
   with entries pending; when new entries arrive while the window is
   open, only the menu count and a status bar note change. Decided
   2026-10-12: as written.

## 8. Not in this plan

- Phone app (CLI-090).
- Encrypted entry and names files.
- Matching against bank imports (MIG-200).
