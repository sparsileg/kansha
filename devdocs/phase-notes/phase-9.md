# Phase 9 — Quicken import (MIG), part 1: QIF import

Spec: 0.6. App version unchanged (0.7.0). `just check` green.

Built from the decisions in `import-proposal.md` Part A. The lot
true-up (A4, MIG-115) followed in spec 0.7 (section below). **Not
built yet:** verification report layouts (MIG-100, P-02). Automated tests use
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
- Securities the import creates that no account holds afterwards
  (Stan's IRA: Floating Rate) are created hidden (MIG-140), so price
  download skips them. Books imported before this keep theirs shown:
  hide them in Tools > Securities, or roll back and import again.
- Per-account exports carry no `!Type:Security` list, so tickers and
  types come in empty/Other; type them in the mapping step or export
  with Security lists ticked.

## Alongside Phase 9 (2026-09-30, spec 0.6.1 … 0.6.4)

UI and investment work Stan asked for during the import trials; not
MIG scope.

Files created:
- `src/lib/components/GearButton.svelte`: the gear used on screens'
  bars.
- `src/lib/components/invest/SecurityDetailsModal.svelte` (+ test):
  SEC-060.
- `src/lib/components/shell/ArrangeAccountsModal.svelte` (+ test):
  ACCT-240.
- `src/lib/dashboard/cards.ts` (+ test): dashboard card IDs and the
  stored layout (DSH-040); `CustomizeDashboardModal.svelte`.
- `src/lib/invest/securityTypes.ts`, `src/lib/reports/axis.ts` (+ test).
- `crates/kansha-core/src/reports/security.rs`: security transactions,
  graph, spans.
- Migrations `0005_other_group.sql` (account group `other`),
  `0006_donor_advised_fund.sql` (security type). Both rebuild a table
  others reference, so `Migration` gained `foreign_keys_off`: the
  runner turns foreign keys off around that migration and still runs
  `foreign_key_check` before commit.

Decisions:
1. **Sheets and cards:** `.sheet` and `.card` in `base.css`; surfaces
   use the theme's row color (white in Classic; flat in dark themes).
   Reports stay forced light (paper).
2. **Account list:** groups stay seven in the data; the list shows six
   sections, Assets & Debt = Assets + Liabilities (the Net Worth report
   keeps them apart). Arrange stores each account's place in the whole
   arrangement as `sort_order`, runs of one group at a time, so a mixed
   section keeps its order.
3. **Net Worth** at the list's foot: `reports::net_worth`, the
   dashboard's figure, reloaded with balances.
4. **Show closed lots:** `portfolio(.., closed)` adds `lot_disposal`
   sales (kind `sale`) up to the date under each position, and
   sold-out positions (zero shares, market value 0, in no total).
   Transfers out are not shown as sales. Kept per view (0.6.5:
   `ViewDef.showClosed`, inside the `invest_views` JSON).
5. **Security Details graph:** points are the security's price dates in
   the span (at most 250), market value = shares in every account ×
   latest price (MMF $1.00); prices drawn on the money axis rounded to
   cents. 0.6.5: a "Fit graph to data" box (on to start) asks Rust for
   `chart::build_fitted`: the axis follows the data's min..max with
   steps down to a cent and room above and below; off, the old axis
   from zero. `security_chart` gained `fitted` (API change).
6. **Date axis:** Rust picks the unit (`Chart.x_unit`); the UI labels
   the first point of each day/month/year, at most twelve.
7. **Import order** (decision 7 above): sales after the day's other
   records.
8. **Reconcile and Reminders** (0.6.4) on sheets like the other views:
   shaded sticky headings, striped rows; a ticked reconcile row keeps
   its tint over the stripe.
9. **Needs Attention** (DSH-030, 0.6.4): investment accounts with old
   uncleared transactions are one line naming none (they are not
   reconciled); banking, credit, and asset accounts are still named.
10. **Convention (Stan):** a view's title sits in a shaded band at the
    top of the view's sheet (`.view-sheet`, `.view-title`,
    `.view-body` in `base.css`). Built for the Dashboard only (0.6.5);
    the other views keep their title in WindowFrame's band until Stan
    says to move them.
11. **DSH-040** (0.6.5): `Settings.dashboard_cards`, JSON
    `{order, hidden}` of card IDs; `null` = default. Unknown IDs drop;
    a card the list lacks shows last, so a new card appears without a
    migration. To add a card: `CardId`, `CARDS`, a body in
    `Dashboard.svelte`. Gear in the title band > Customize.
12. **Importer** (0.6.5): (a) `ShrsIn` with no shares and Quicken's
    `Cash` with no amount (its empty opening entry) are warnings and
    skipped; (b) in a per-account export the file-name account
    (`FidelityIRA510`) and the name transfers use (`Fidelity IRA 510`)
    are one account, named as transfers name it (same letters and
    digits, undefined account with records); the book's account is
    found the same way when only one matches; (c)
    `ImportOptions.show_securities`: a "Keep shown" box per new
    security keeps it from being created hidden when sold out.

Gaps:
- Calendar, Investments, dashboard: checked by Stan in the app only by
  eye; no visual tests.
- Security Details opens only from the Investments screen.
- Only the Dashboard has its title in its sheet (decision 10).
- Importer fixes (decision 12) were tested with synthetic records
  shaped like Stan's Fidelity and Vanguard exports, not re-run on the
  real files.

## Known gaps

- MIG-100: only the per-account before/after/expected table in the
  import result. Matching report layouts wait for P-02.
- MIG-150 (scheduled transactions): closed, not needed (spec 0.7.2);
  Stan re-entered his schedules by hand.
- From 2026-10-01 Stan's imported book is his production book. Fixes
  and dry runs go on a scratch copy; a new migration is tested on a
  copy of his book first.
- Not measured on Stan's file. Synthetic timing (release, this
  Windows machine; `import::large_file_timing`, ignored): 7,680
  transactions over 20 years with 960 matched transfers, preview
  23 ms, import 0.92 s, so about 12 s per 100,000. Every record goes
  through the engine with its audit entry. Prices are no longer
  audited (spec 0.7), so a daily price history costs only its own
  rows; PRC-060 thinning is Later.
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
- **Resolved (2026-10-01): per-account imports.** Solved in Stan's
  book on Windows by recategorizing three transfers to Opening
  Balance; no code change. The original note: importing one QIF per
  account into one book gave wrong balances (Checking, Fidelity IRA
  cash, a VBS-Cash cash account at about -$40k that cannot be deleted
  because its transfers tie it to Savings and Checking). Transfers are
  matched only within one file (MIG-070), so separate files may
  double-count them or post them against Opening Balance. The designed
  path is one whole-file export (all five data types) imported once,
  with accounts to leave out unticked in the mapping step. Cause in
  Stan's book not yet found; the next session inspects a copy of
  `J:\Kansha\import.db`. Still open: whether a later import should
  match a transfer against transactions already in the book.

## Lot true-up (MIG-115), spec 0.7, 2026-10-01

QIF has no lot IDs, so replaying the investment history relieves every
sale by the account's method (FIFO). Stan picked lots (Vanguard SpecID,
later MinTax), so Kansha's open lots and gains differed. On a copy of
his book: shares matched Vanguard exactly; basis was high by $164,634
(VTI, account 448) and $5,010 (VTSAX, 140).

**⚠ Schema change:** migration 0008: investment action and disposal
kind `true_up`; `category.tithable` and `giving` dropped (table
rebuild); price audit entries deleted (trigger dropped and recreated).
**⚠ API change:** commands `true_up_preview`, `true_up`; types
`TrueUpPreview`, `TrueUpLine`, `TrueUpLot`, `TrueUpReplay`,
`TrueUpStatus`; `InvAction`/`DisposalKind` gain `true_up`;
`CategoryFields` loses `tithable`, `giving`. `just bindings` run.

### Files

- `persistence/migrations/0008_lot_true_up.sql`; `migrate.rs` entry.
- `invest/true_up.rs`: `parse_true_up`, `preview_true_up`, `true_up`,
  `delete` (with later events taken out and put back).
- `invest/service.rs`: `replan`, `source`; update refuses a true-up
  (memo only); delete routes to `true_up::delete`; `plan` refuses
  `TrueUp` input.
- `persistence/invest.rs`: `events_after`.
- `invest/period.rs`: a true-up that changes shares is a flow.
- `persistence/integrity.rs`: `share_balance_mismatch` counts true-up
  lots.
- `persistence/securities.rs`: `set_price`, `delete_price` not audited.
- `src-tauri/src/commands/invest.rs`: `true_up_preview`, `true_up`
  (backup kind `bulk` first).
- `src/lib/components/invest/TrueUpModal.svelte`, button in
  Securities; `invest/form.ts` `TRUE_UP` (edit shows memo only). The
  comparison is tied to the account, security, date, and text it was
  made with; changing any hides it until Compare runs again
  (`TrueUpModal.test.ts`).
- Tests: `tests/integration/true_up.rs` (12), migration 0008 test,
  price test now expects no audit rows.

### Decisions

- Close and recreate: a lot matching on acquisition date, shares, and
  basis (shares only in IRA/Roth) is kept; others closed (`true_up`
  disposal, no gain), broker lots opened. One transaction per holding;
  basis difference against Opening Balance.
- Dated before later sales when needed: later disposals of the holding
  are cleared and planned again in order (FIFO picks from the new
  lots); then a sale can be re-picked Specific. Refused with a later
  share transfer or true-up; fails atomically if a later sale's chosen
  lot is closed.
- Prices not audited at all (option 1), not just non-manual ones.
- CSV: header found below note lines; rows filtered by the security's
  ticker when there is a symbol column; `total cost` accepted.

### Stan's true-up (to do in the app)

Dry run on a scratch copy of `import.db` matched Vanguard's realized
gains reports to the cent:

1. Tools > Securities > True up lots…: Vanguard Brokerage 448, VTI,
   2025-12-31, `ignored/vti-448-2025-12-31.csv`. Then Vanguard Grandma
   140, VTSAX, 2025-12-31, `ignored/vtsax-140-2025-12-31.csv`. (Built
   from Vanguard's current lots with the 2026 sales added back and
   2026 reinvestments left out.)
2. Edit the 2026-01-07 VTI sale: Choose lots, 400 from 2024-04-04 →
   gain $34,406.00 long.
3. Edit the 2026-02-20 VTSAX sale: 89.941 from 2016-09-28 and 1.232
   from 2024-12-23 → gain $10,040.82 long.
4. VTSAX register: reinvestment dated 2026-02-25 → 2026-02-24
   (Vanguard's date); enter the 2026-09-28 reinvestment (0.288 sh,
   $52.71).
5. Check: True up lots with today's date and Vanguard's current
   download (`costbasisdownload_*.csv`) says the lots already match.

### Known gaps

- Pre-2026 realized gains stay as the FIFO replay made them (those
  years are filed). VTI 2024-11-29 and 2025-07-15 DAF gifts show as
  phantom sales (Quicken's workaround); fix with the deferred
  charitable-gift disposal type.
- Preview cannot try the replay (read-only); a failing replay shows
  only when applying (nothing changes).
- True-up UI is a prototype in the Securities window.

### Applied in Stan's book (2026-10-01)

Done in the app on `import.db` (Astro drive): both true-ups dated
2025-12-31, both 2026 sales re-picked, the VTSAX buy moved to
2026-02-24. Checked against a copy: VTI $34,406.00 and VTSAX
$10,040.82 long-term gains, matching Vanguard's realized gains
reports to the cent; open lots match Vanguard's cost basis downloads
except the 2026-09-28 VTSAX reinvestment (Stan enters it with the
month-end reinvestments). Accounts 448 and 140 now default to Minimum
tax, as Vanguard does.

## Gift of shares (INV-060), spec 0.7.1, 2026-10-01

Stan's answers: the DAF (Firefly Hill Fund) gets the cash value, not
the shares; VBS-Cash was 448's external cash account, now closed (cash
now goes straight from 448); redoing the true-up is fine. His Quicken
method: Removed (chosen lots), Added at market, Sold that lot, then
VBS-Cash paid to Charity:Noncash; the DAF balance updated by hand.

Built as Shares removed with a price and a recipient, not a new
action, so no migration and no IPC signature change:

- Engine (`invest/service.rs`): recipient allowed on Shares removed,
  needs the price; holding −basis, recipient +shares × price, Opening
  Balance the difference (posted even at zero, so `to_input` finds the
  recipient as the second of three postings). Disposal kind `removed`:
  no gain, not in realized gains. `trade_amount` gives the value.
- Recipient: a category or a non-investment account
  (`check_counterpart`). Firefly is type Donor Advised Fund, an
  investment account, so it can't be the recipient; use Charity:Noncash
  and keep updating Firefly by hand, as in Quicken.
- Form: Shares removed shows Price per share and "Given to"; a note
  shows the gift value.
- Test: `invest::shares_given_away_leave_with_no_gain_and_the_value_goes_to_the_recipient`
  (postings, edits, zero difference, plain removal, errors);
  `trade_amount_is_computed_in_rust`; `form.test.ts`;
  `reports::shares_given_to_charity_are_a_deduction_at_market_value_not_a_sale`
  (Charity:Noncash on Schedule A, Non-cash charity contributions, at
  market value; Schedule D unchanged).
- Setup: Charity:Noncash tax-related, tax line Schedule A "Non-cash
  charity contributions". No Form 8283 report: Tax Schedule marks
  that line "(Form 8283 needed)" when it is over $500 for the period
  (spec 0.7.2).
- Memo: a gift saved with an empty memo gets "Gift / noncash
  donation" (spec 0.7.2); the Action column stays Shares Removed.
  Stan adds it by hand to gifts entered before.
- Used by Stan in the app for the VTI cleanup below.

Cleanup of the two imported VTI gifts (2024-11-29, 170 sh;
2025-07-15, 179.393 sh), each Quicken's Removed + Added + Sold, whose
sales took FIFO lots on import (phantom gains $33,498.79 and
$36,555.79): re-pick each sale to the lot added that day, so the gain
is zero. Date order blocks editing a sale with later disposals in the
holding, so it starts by deleting the VTI true-up; then re-run it with
`ignored/vti-448-2025-12-31.csv`. Done: dry run on a scratch copy,
then Stan in the app (2026-10-01). Realized gains: 2024 −1.41 (a fee,
kept), 2025 0.00.

## Schedule transaction type (REC-010, REC-300), spec 0.7.3, 2026-10-02

Found while Stan re-entered his Quicken reminders. Payment or deposit
was only the sign of a schedule's lines, so a 0.00 reminder (a bill
known when it comes in) lost it: Reminders showed it as Deposit, and
an amount set with "Set for this occurrence only" on a 0.00 payment
went in as a deposit. Entering through the register was safe (the
user picks the column).

### Files

- `persistence/migrations/0009_schedule_direction.sql`:
  `schedule.direction` (`payment` | `deposit`), filled from the sign
  of each schedule's lines (posting sign: a positive sum is a
  payment); 0.00 ones become payments.
- `schedule/mod.rs`: `Direction` (`of`, `allows`); `ScheduleFields`
  and `OccurrenceView` gain `direction`.
- `schedule/service.rs`: `validate_fields` refuses lines that total
  the other way; `check_direction` on one-time and entered amounts;
  `from_entry` takes the entry's direction.
- `persistence/schedules.rs`: the column.
- UI: `schedule/form.ts` (draft keeps the stored type;
  `byScheduleUse`), `views/Scheduled.svelte` (Method = Payment or
  Deposit, no Transfer), `OccurrenceRow.svelte` (typed amount signed
  by the direction), `ScheduleModal.svelte` (Account list sorted by
  use in existing schedules).
- Tests: `schedule::a_zero_amount_schedule_keeps_its_direction`,
  `lines_must_go_the_schedules_direction`,
  `an_amount_entered_on_a_zero_payment_must_be_a_payment`,
  `a_schedule_from_an_entry_takes_its_direction`;
  `migrations::migration_0009_stores_each_schedules_direction_from_its_sign`;
  `form.test.ts`, `OccurrenceRow.test.ts`.

### Decisions

- Stan: Method shows the transaction type like Quicken; a transfer is
  a Payment or Deposit, not "Transfer".
- The type is stored rather than guessed from the category (a 0.00
  transfer, such as a card payment, has no category to guess from).
- The schedule dialog's Account list puts accounts most used by
  existing schedules first (main account only; ties keep their order).

### Known gaps

- Entering through the register can still go the other way (a full
  edit; allowed on purpose).
- Stan: check 0.00 reminders that should be deposits; the migration
  made them payments.

## Next: tax reports closer to Quicken (designed 2026-10-02)

Design and plan: `devdocs/tax-reports-design.md`. Not built. Closes
gap 13 above (Quicken `R` tax codes) in its step 1.
