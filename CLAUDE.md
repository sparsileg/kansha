# Kansha — Claude Code Instructions

Kansha: local, single-user, cross-platform personal finance app replacing
Quicken 2013. Kubuntu primary; Windows later; macOS not precluded.
Tauri v2 + Rust core + Svelte 5 (Vite, not SvelteKit). SQLite via rusqlite
(`bundled-sqlcipher-vendored-openssl`). MSRV 1.85.

## Read first

- `devdocs/kansha-spec.md` — spec (requirement IDs, phases in Part VI).
- `devdocs/CONVENTIONS.md` — binding rules. §3–§8 apply as written.
  §1–§2 (chat workflow, zip/patch delivery) are replaced by "Workflow" below.
- `devdocs/phase-notes/phase-*.md` — what earlier phases built and left open.
- Schema: `crates/kansha-core/src/persistence/migrations/*.sql` (spec §18).

## Replying to Stan

- Caveman style: short sentences, essential words only, no intro, no
  praise, no filler. Answer, not explanation, unless asked.
- Count Stan's messages per session; announce at 15 and at 20.
- State assumptions before code that depends on them.
- Out-of-scope ideas → "Deferred" list at end of reply.

## Workflow

- Edit files in place. No zips, no patches, no `~/Downloads/`.
- Stay in current phase scope.
- Before calling work done: run `just check`; must be green
  (`cargo fmt`, `clippy -D warnings`, tests).
- Do not commit or push. Stan commits in GitKraken.
- End each piece of work with a commit message in two separate fenced
  blocks: (1) imperative summary ≤72 chars; (2) body wrapped at 72,
  citing requirement IDs.
- Flag **⚠ API change** (IPC command signatures) and
  **⚠ Schema change** (new migration) in reply and commit body.
- Command signature changed → run `just bindings`; commit the diff.
- Spec/conventions change → edit `devdocs/`, bump spec version, add
  Appendix A entry. Tell Stan so he syncs the Claude Project copy.
- Phase end → write `devdocs/phase-notes/phase-N.md`: files created,
  decisions made, known gaps.

## Hard rules (summary; CONVENTIONS wins on conflict)

- Money `i64` cents (`Money`); qty/price/rate `i64` ×1e6. No floats in
  any money path. Intermediate math `rust_decimal`; round-half-even;
  deterministic remainder allocation.
- Dates: date-only in Rust; ISO strings across IPC/TS. No JS `Date` for
  financial dates. "Today" from injected `Clock` only.
- All business logic in `kansha-core` (no Tauri dep). `src-tauri` =
  thin handlers. No logic or money math in TypeScript.
- Only `persistence` issues SQL. Writes via `Db::write`; audit entry in
  same transaction.
- Never edit a released migration; add a new one. Tables STRICT.
- No `unsafe`; no `unwrap`/`expect` outside tests except documented
  invariants. No let-chains (MSRV 1.85).
- Scenario TOML: amounts as strings; inline tables on one line only.
- Every engine bug fix adds a test that fails first.
- Do not bump specta or other pinned deps without asking.

## Schema quick facts

- Table `txn` (not `transaction`). Posting = account XOR category.
- Posting sign: + asset increase / expense; − liability / income.
  Postings sum to zero (`unbalanced_txn` view).
- `security_id` on investment-account posting = holding at cost basis.
- Lots immutable; open qty/basis derived from `lot_disposal` +
  `lot_adjustment`. Account type fixed at creation.

## Models

Opus for design and money logic (Phases 1, 2, 4, 6; hard debugging).
Sonnet for plumbing/UI.
