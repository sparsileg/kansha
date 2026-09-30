# Phase 9 — Quicken import (MIG), part 1: QIF import

Spec: 0.6. App version unchanged (0.7.0). `just check` green.

Built from the decisions in `import-proposal.md` Part A. **Not built
yet:** the lot true-up (A4, MIG-115, needs a migration) and
verification report layouts (MIG-100, P-02). Automated tests use
synthetic QIF; Stan's first real trial is below.

**⚠ API change:** new commands `pick_import_file`, `import_open`,
`import_preview`, `import_run`, `import_cancel`, `import_batches`,
`import_rollback`; types `ImportOptions`, `ImportPreview`,
`ImportResult`, `RollbackResult`, `ImportBatch` (now with
`rolled_back_at`, `source_sha256`, `archive_path`, `txns`) and the
choice enums. `just bindings` run. **No schema change** (uses
`import_batch`'s existing `source_sha256` and `archive_path`).

New dependency: `sha2 =0.10.9` in kansha-core (already in the tree
under `age`).

## Files

- `crates/kansha-core/src/import/qif.rs`: the parser. Text in,
  records out, no database; never fails (problems stay on their record
  or become a file note). `decode` (UTF-8, else Windows-1252),
  `parse(text, default_account, order)`, `read_date`, `split_target`.
- `import/plan.rs`: QIF records + mapping → a `Plan` (reads the book,
  writes nothing): account/category/security/tag choices and their
  checks, records → items, transfer matching, totals, preview.
- `import/commit.rs`: writes a plan through the engine in one
  transaction (each record in a savepoint); `rollback`.
- `import/mod.rs`: IPC types; `Staged` (file bytes, text, SHA-256;
  `preview`, `run`); the archive (`archive_folder`, encrypted copy).
- `persistence/mod.rs`: `Db::write_import` (stages the batch inside
  the import's own transaction, origin `Import(batch)`), `Tx::savepoint`.
- `persistence/imports.rs`: `NewBatch`, `list`,
  `find_committed_by_sha`, `mark_rolled_back`, `txns_newest_first`,
  `reconciled_in_kansha`, `created`; `ImportBatch` gains fields.
- `security.rs`: `PrivateKey::generate` is `pub(crate)` (archive test).
- `src-tauri/src/commands/import.rs`; `AppState.pending_import`,
  `note_import` (timed backup due, undo dropped).
- `src/lib/components/import/ImportModal.svelte` (+ test): choose file,
  preview, mapping tables, notes and errors, Test import, Import, Past
  imports with Roll back. File > Import… is enabled.
- Tests: `tests/integration/import.rs` (12 tests) on
  `tests/fixtures/qif/whole.qif` (synthetic whole-file export) and
  inline files; parser unit tests in `qif.rs`; archive and hash tests
  in `import/mod.rs`.

## Decisions made in the build (for Stan to confirm)

1. **Hidden accounts:** QIF has no hidden flag, so nothing starts
   unticked by itself (A3 said hidden ones start unchecked). Default:
   an account with transactions in the file is created; Stan sets
   Skip for dead ones. Their transfers go to Opening Balance.
2. **Transfers to skipped accounts, and to the account itself**
   (Quicken's opening balance) post to the built-in **Opening
   Balance** (equity) category: balances stay right and reports leave
   them out.
3. **Cash between two investment accounts** (e.g. `XOut` Brokerage →
   `XIn` IRA): the engine has no single transaction for it (Cash In/Out
   refuse an investment account as the other side). Each side becomes
   Cash In/Out against Opening Balance, with a note. Balances are
   right; the link between the two is lost. A proper form would be an
   engine change.
4. **Which side keeps a matched transfer:** the side with more lines
   (a split), else the first in the file; an investment record always
   (only the investments engine posts to investment accounts). The
   other side's payee, memo, and check number are lost; its cleared
   status is kept on the posting.
5. **One-sided transfers:** imported from the side seen; a note when
   the other account had records in the file (its balance will then
   differ from Quicken's by that amount).
6. **Bad records:** any stops the whole import unless "Import the rest
   and leave these out" is ticked; mapping problems always stop it.
7. **Order:** date; within a day, sales and `ShrsOut` after the rest;
   then file order. Quicken holds shares by the day and may write a
   sale before that day's reinvestment (Stan's Fidelity IRA: a full
   sale listed before the same day's `ReinvDiv`).
8. **New investment accounts** keep their own cash and hold money
   market funds as securities (QIF has no money-market type, and
   Quicken treats them as securities).
9. **Security types:** from the QIF `T` (`Stock`, `Mutual Fund`,
   `Bond`, `CD`, …); unknown or missing → Other; asset class by type.
   Changeable in the mapping.
10. **`ShrsIn`:** basis `T`, else shares × price, else zero with a
    note; acquisition date = transaction date (QIF has no lot dates).
11. **`StkSplit`:** `Q` is new shares per 10 old (GnuCash reads it the
    same way). To check against Stan's file.
12. **Investment payees** go into the memo (`payee — memo`).
13. **Tax lines:** Quicken's `R` tax codes are not mapped; only the
    tax-related flag is.
14. **Rollback:** deletes the batch's transactions newest first, then
    each account, category, payee, tag, and security the import
    created that nothing uses. Prices it added to securities that stay
    are kept. Refused while any imported posting has been reconciled in
    Kansha, or when a later entry stands in the way (a sale after an
    imported buy); nothing changes then.
15. **Archive:** `<book>-imports/<UTC stamp>-<file>.age` beside the
    book, encrypted to the backup public key (decrypt with the backup
    passphrase, as a backup). Written before the import, removed if it
    fails. Dry runs write none.
16. **Same file twice:** a warning in the preview (by SHA-256), not a
    refusal. Records already in the book are not detected (MIG-200).

## First real trial (2026-09-30)

- Stan imported a real checking account (1,000+ transactions) in the
  dev build (`just dev`, Windows): the balance matched Quicken's
  exactly.
- Earlier the same import in the app gave "database error: out of
  memory". Not reproduced since, neither in the dev build nor in tests
  (`imports_into_an_encrypted_book_after_a_backup`). Watch for it in a
  release build. Errors now name the step, and a database failure on a
  record names its line. `import::a_file_from_disk` (ignored,
  `KANSHA_QIF=<absolute path>`) runs a real file the app's way in a
  throwaway book.
- Still to trial: the whole-file export, investment accounts,
  category totals by year against Quicken.
- Per-account investment exports: Fidelity HSA imported. Fidelity IRA
  first left a sold fund held and cash short by the sale (same-day sale
  before reinvestment, decision 7); fixed. A book on the pCloud drive
  (`P:`) gave "database disk image is malformed" mid-import; clean
  after reopening. Books belong on a local disk (WAL needs working
  shared memory and locks); the earlier "out of memory" may be the
  same cause.

## Known gaps

- Lot true-up (MIG-115): not built; needs the broker CSV layouts and a
  migration (new lot adjustment kind, riding with the tithing-column
  drop).
- MIG-100: only the per-account before/after/expected table in the
  import result. Matching report layouts wait for P-02.
- MIG-150 (scheduled transactions): nothing; P-04 open.
- Not measured on Stan's file. Synthetic timing (release, this
  Windows machine; `import::large_file_timing`, ignored): 7,680
  transactions over 20 years with 960 matched transfers, preview
  23 ms, import 0.92 s, so about 12 s per 100,000. Every record goes
  through the engine with its audit entry; prices are one audited
  write each (a daily price history for many securities is many rows;
  PRC-060 thinning is Later).
- Existing investment accounts with linked cash: Cash In/Out from the
  import are refused by the engine (listed as bad records).
- Actions not handled: `Exercise`, `Expire`, `Grant`, `Vest`,
  `ShtSell`, `CvrShrt`, bond `Buy`s priced per 100, and anything else
  Quicken 2013 may write; they are listed as bad records. Stan's
  samples showed only handled ones.
- The mapping UI is a prototype: long lists in scrolling tables, no
  search; every change re-runs the preview.
- `Staged` stays in memory until imported or cancelled; switching
  books does not drop it.
