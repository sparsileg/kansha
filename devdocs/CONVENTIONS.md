# Kansha — Conventions

| | |
|---|---|
| **Applies to** | All Kansha work: code, tests, docs, and chat deliveries |
| **Companion doc** | `devdocs/kansha-spec.md` |
| **Last updated** | 2026-09-24 |

Rules here are binding. If this file and the spec disagree, fix one of them; do not guess.

---

## 1. Chat Workflow

- One fresh chat per phase (or sub-phase). Start with the kickoff template in spec §25.
- Spec and this file live in the Claude Project; do not paste them into chats.
- Stay within the current phase's scope. Out-of-scope ideas go in a "Deferred" list at the end of the reply.
- State assumptions explicitly before code that depends on them.
- Flag every change to the IPC command API or the database schema with a **⚠ API change** or **⚠ Schema change** line.
- End each delivery with the exact commands Stan runs, in order.
- Stan runs tests locally and pastes back only failing test names and assertion messages.
- At phase end, deliver `devdocs/phase-notes/phase-N.md`: files created, decisions made, known gaps.

## 2. Delivering Changes

### 2.1 Complete files vs. patches

- **Complete file** when the file is new, or changed so much that regenerating is cheaper than patching.
- **Otherwise one unified patch per request**, covering all modified files. Never several patches for one request.
- Never regenerate a whole file when a patch suffices.
- Deliver as downloadable files when file creation is available; otherwise as fenced code blocks.

### 2.2 Patch format

- Unified diff, paths `a/<path>` and `b/<path>` relative to the repo root.
- Stan downloads deliveries to `~/Downloads/`. Commands reference that path.
- Applied from the repo root:

  ```
  git apply --check -v ~/Downloads/<file>.patch
  git apply -v ~/Downloads/<file>.patch
  ```

- **Every hunk must have trailing context (≥1 unchanged line after the last change)** unless the hunk truly ends the file. A hunk without trailing context is anchored to EOF by git and fails.
- Hunk header line counts (`@@ -a,b +c,d @@`) must be exact. Line numbers may be approximate; git tolerates offsets.
- Leading context: at least 3 lines where the file allows.
- No trailing whitespace changes outside the intended edit.
- New files in a patch use `--- /dev/null` and `+++ b/<path>`.
- `--check` passing does not mean applied. Confirm with `git status` / `git diff`.

### 2.3 Commit messages

Delivered as **two separate fenced blocks** (for GitKraken):

1. One-line summary, imperative mood, ≤72 characters.
2. Body: what changed and why, wrapped at 72 columns; cite requirement IDs where relevant.

## 3. Rust (`crates/kansha-core`, `src-tauri`)

- All financial logic lives in `kansha-core`. The crate has **no Tauri dependency**.
- `src-tauri` contains command handlers only: argument mapping, call into core, map errors. No business logic.
- Modules interact only through public APIs; only `persistence` issues SQL (spec §17.2).
- `cargo fmt` and `cargo clippy -- -D warnings` must be clean.
- Errors: `thiserror` enums per module; no `unwrap()`/`expect()` outside tests except on invariants documented in a comment.
- No `unsafe`.

## 4. Money, Quantities, Prices, Dates

- **Money:** `i64` cents in the `Money` newtype.
- **Quantities and prices:** `i64` scaled to 6 decimal places (`Quantity`, `Price` newtypes).
- **No floats** (`f32`/`f64`, JS `number` arithmetic) anywhere in a money path.
- Intermediate math: `rust_decimal`; round to cents only at defined points; round-half-even unless a rule says otherwise.
- Allocations (basis splits, pro-rata adjustments) distribute remainders deterministically so parts sum exactly to the whole.
- **Dates:** date-only types in Rust; ISO `YYYY-MM-DD` strings across IPC and in TypeScript. No JS `Date` for financial dates. No time zones.
- **Showing dates:** every user-facing date, shown or typed, goes through `displayDate`/`parseDate` (and `datePattern`/`dateExample` for placeholders and messages) in `src/lib/format/date.ts`, which follow the date format setting (SET-030). Never hard-code `MM/DD/YYYY` or an example date. Logs and histories show timestamps as stored: `YYYY-MM-DDTHH:MM:SSZ` (UTC).
- **Statement sign:** reconciliation takes and returns amounts as a statement prints them (a credit card balance owed is positive); `reconcile::Sign` converts at that module's boundary. Everywhere else is ledger sign.
- **Clock:** "today" comes from an injected `Clock`; never read system time in the engine.
- **IPC:** amounts cross as integers or decimal strings, never floats.

## 5. Database

- SQLite via `rusqlite` with `bundled-sqlcipher`.
- Numbered, forward-only migrations (`persistence/migrations/NNNN_name.sql`); `schema_version` table. Never edit a released migration; add a new one. The SQL files are the schema spec.
- Every table is `STRICT`. Money INTEGER cents; quantity/price/rate INTEGER × 10^6; dates TEXT with `CHECK (x IS date(x))`; timestamps UTC TEXT from the `Clock`, never `CURRENT_TIMESTAMP`.
- Enumerations: TEXT + CHECK list, mirrored by a `text_enum!` in Rust. A test inserts every Rust value.
- IDs: `INTEGER PRIMARY KEY AUTOINCREMENT` (never reused).
- Writes go through `Db::write`; every repository write records its audit entry in the same transaction.
- WAL mode, `synchronous=FULL`, `foreign_keys=ON`.
- Multi-record changes run in a single DB transaction.
- The ledger is the source of truth. Balances, positions, and gains are derived; caches are rebuildable and never authoritative.
- Prefer void/close over delete. Every create/edit/void/delete writes the audit log.

## 6. Frontend (`src/`)

- Svelte 5 + Vite. **Not SvelteKit.** State via runes in `.svelte.ts` modules; no external state library.
- `invoke` is called only in `src/lib/api/`.
- Types in `src/lib/types/` are generated from Rust; never hand-edited. `bindings.ts` is committed to the repo (so frontend-only CI and `npm run build` don't need the Rust toolchain) and regenerated with `just bindings` whenever a command's signature changes — check in the diff.
- Money/quantity/date formatting and parsing only in `src/lib/format/`.
- **No business logic or money arithmetic in TypeScript.** Throwaway prototype UI follows these rules too.
- **Colors come from theme variables** set on `.app` in `App.svelte` (`--bg`, `--bad`, `--good`, `--focus-*`, `--sel-*`, `--opt-*`); components do not hard-code colors. Every pair is readable in every theme, and color is never the only cue: Stan is red-green colorblind.
- **Focus (NFR-080):** one app-wide look for every field, dropdown, and keyboard-focused button: the theme's focus background and text color plus a ring drawn inside the edge. A focused field's selected text keeps the focus text color. Do not add per-component focus styles.
- **Fixed window:** the window itself never scrolls. The shell (`.app`) is pinned to it (`position: fixed; inset: 0`); a view that can outgrow its space scrolls inside its own area (`min-height: 0` plus `overflow: auto` on the flex child). Do not size anything with `100vh`.
- **Select on focus:** every text field selects its contents on focus, installed once on `document` by `App.svelte` (`src/lib/ui/selectOnFocus.ts`). Do not add it per field.

## 7. Testing

- `just test` runs everything; deterministic, offline, date-independent.
- Unit tests beside the code; integration tests on a fresh in-memory DB via the shared fixture.
- Scenario files: `tests/scenarios/<area>/*.toml`.
  - Amounts, quantities, and prices are **strings**.
  - Inline tables stay on **one line**; use `[section]` / `[[array]]` tables for anything longer. No multi-line inline tables.
  - Each scenario lists the requirement IDs it covers.
- Every engine bug fix adds a test that fails before the fix.
- Snapshot changes (`insta`) are reviewed before acceptance.

## 8. Documentation

- Spec requirement IDs are stable; never renumber or reuse. Withdrawn items are marked **[Withdrawn]**.
- Spec changes bump the version and add an Appendix A entry.
- Accepted recommendations change tag [R] → [S].
