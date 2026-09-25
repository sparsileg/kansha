# Phase 6 — Investments

Spec: 0.3.11. Engine, IPC, and UI in one pass. `just check` green.

**Exit criteria (spec §24):** lot scenario suite and basis-conservation
properties pass (Kubuntu). **Open:** Stan reviews the lot scenarios
(`tests/scenarios/invest/INV-001…013.toml`); `just test` on Windows not
yet run.

**⚠ API change:** 25 new commands (below); `Posting` and
`PostingInput` gain `security`; `just bindings` run.
**No schema change.** Migration 0001 already had `security`, `price`,
`investment_txn`, `lot`, `lot_disposal`, `lot_adjustment`,
`import_batch`.

## Files

- `money.rs`: `mul_div` (half-even, i128), `allocate` (exact
  proportional split, largest remainder), `Price::per_share`.
- `csv.rs`: small RFC 4180 reader; value helpers for `$1,234.56`,
  `M/D/YYYY`.
- `securities/`: `SecurityId`, `SecurityType`, `AssetClass`,
  `PriceSource`, `SecurityFields`, `Security`, `PricePoint`; price CSV
  import (`preview_prices`, `commit_prices`).
- `persistence/securities.rs`: security CRUD (ticker unique, CUSIP 9
  chars, delete only when unused, with its prices), prices (set/replace,
  delete, latest on or before), all audited.
- `invest/mod.rs`: `InvAction` and its rules, `InvInput`, `InvTxn`
  (flattens `Txn`; lots, disposals, adjustments; `to_input` for edits),
  `Lot`, `Disposal`, `Adjustment`, `LotPick`, `SplitRatio`, `Term`.
- `invest/lots.rs`: pure lot math: FIFO, specific picks, proportional
  basis, proceeds division, holding period, split, return of capital.
- `invest/service.rs`: `create`, `update`, `delete`, `trade_amount`;
  `plan` (validation → postings and lot records); the date-order rule.
- `invest/reads.rs`: `register`, `holdings`, `account_value`,
  `open_lots`, `realized_gains`, `income`, `performance`, `allocation`.
- `invest/seed.rs`: lot seeding CSV (`preview_seed`, `commit_seed`).
- `persistence/invest.rs`: all investment SQL; one audit entry per
  transaction holding the whole `InvTxn`.
- `persistence/imports.rs`: `stage`/`commit` import batches (MIG-080).
- `persistence/ledger.rs`: `insert_header`, `insert_postings` shared;
  postings read and write `security_id`; `set_cleared` touches cash
  postings only.
- `integrity`: `LotOverdrawn`, `ShareBalanceMismatch`,
  `LotBasisMismatch`, `LotQuantityMismatch`.
- `ledger`: `close_account` checks investment cash and positions;
  `account_balances` shows investment accounts at market value;
  `posting_for` prefers the cash posting.
- `reconcile`: internal-cash investment accounts reconcile their cash
  (RCN-010 carry-over); statement interest/fees become investment
  transactions.
- `persistence/accounts.rs`: cash handling fixed once used; default lot
  method fifo or specific only.
- `sample.rs`: Brokerage (internal cash) and Roth IRA, five securities,
  monthly prices (BND left stale), monthly buys, quarterly dividends,
  Roth reinvestments and year-end LT gains, yearly sales (FIFO and
  specific), an AAPL 4:1 split, opening lots (shares added). Banking
  data unchanged (separate random stream).
- `testkit.rs`: `security`, `security_with`, `price`, `invest`,
  `seed_lots`.
- `src-tauri/src/commands/invest.rs`: `security_list`,
  `security_defaults`, `security_create`, `security_update`,
  `security_delete`, `price_list`, `price_set`, `price_delete`,
  `price_import_preview`, `price_import`, `inv_register`, `inv_get`,
  `inv_input`, `inv_create`, `inv_update`, `inv_delete`,
  `inv_trade_amount`, `inv_holdings`, `inv_lots`, `inv_gains`,
  `inv_income`, `inv_performance`, `inv_allocation`,
  `lot_seed_preview`, `lot_seed`.
- UI: `components/invest/InvestmentAccount.svelte` (tabs Overview,
  Transactions, Holdings, Lots, Income, Performance with realized
  gains), `InvEntryModal.svelte` (fields by action, amount from Rust,
  lot picker, Delete, History), `SecurityManager.svelte` (Manage >
  Securities: securities and prices), `CsvImportModal.svelte` (price
  import, lot seeding: file or paste, preview, all or nothing),
  `views/Investments.svelte` (all investment accounts, allocation);
  `state/invest.svelte.ts`; `invest/form.ts`; `format/quantity.ts`.
  Tools > Securities; the navigation bar's Investments button works.
  Account dialog offers fifo and specific only.
- Tests: scenarios `tests/scenarios/invest/INV-001…013`,
  `reconcile/RCN-012`; runner: securities, prices, investment accounts,
  `invest`/`invest_edit`/`invest_delete`/`price`/`import_prices`/
  `seed_lots`, `expect.cash_balances`, `account_values`, `holdings`,
  `lots`, `gains`, `income`, `inv_register`, `allocation`,
  `performance` (README updated). Integration `tests/integration/
  invest.rs` (repositories, audit, edit, clearing, cash handling,
  seeding as import, specific lots through an edit, integrity finds
  damage, every enum in the schema). Property
  `lots_conserve_shares_and_basis`. Frontend: `quantity.test.ts`,
  `invest/form.test.ts`, `InvestmentAccount.test.ts`,
  `InvEntryModal.test.ts`.

## Decisions

- Postings per action, rounding, lot selection, holding period, splits,
  return of capital, linked cash, money market funds, stale prices: spec
  §18 "Investment rules (Phase 6)".
- **Date order (the main design choice).** Lots are immutable and
  disposals point at them, so history is not replayed. A holding's
  sales, transfers, removals, splits, and returns of capital must be
  entered in (date, entry) order; a trade changes or goes only while
  nothing follows it in its holding; memo and settlement date always
  change. Backdating a buy before a later sale is refused (FIFO would
  have picked differently). The message says to change or delete the
  later one first. Replay-on-edit (Quicken style) is deferred.
- Investment transactions are deleted, never voided.
- Amounts are entered positive; the action gives the sign. Buy/sell/
  reinvest amount defaults to shares × price (± commission), computed
  in Rust (`inv_trade_amount` for the form).
- Splits round the position once, then allocate to lots, so shares
  never drift; a reverse split that would empty a tiny lot is refused.
- Return of capital beyond basis is a Realized Gain/Loss posting with
  no lot record; the gains list shows it with no term.
- Linked-cash income posts only to the linked account and the category
  (no zero posting in the investment account), so the checking register
  shows "Dividends", not "--Split--".
- Cash handling (internal/linked, linked account) is fixed once the
  account has investment transactions.
- Default lot method (account, security) is fifo or specific; average,
  HIFO, min-tax are refused everywhere until built.
- Account list value of an investment account = cash + market value
  today, a holding with no price at cost.
- Stale price threshold fixed at 7 days (`DEFAULT_STALE_DAYS`) until
  the SET-040 setting exists.
- Performance (POS-030, recommendation followed): total gain =
  unrealized + realized + income; return = total gain ÷ (basis held +
  basis sold). Reinvested income counts as income and as basis.
- Investment cash reconciliation (RCN-010): internal-cash accounts
  only; holdings never listed; statement interest in the Interest
  category → Interest, service charge in Investment Fees → Fee, other
  categories → misc income/expense. Share reconciliation (RCN-070)
  stays Later.
- Lot seeding (MIG-120) stages an import batch, then commits the Shares
  Added transactions and the batch in one transaction (origin import).
- CSV dates accept `YYYY-MM-DD` and `M/D/YYYY` (US brokerage order).

## Known gaps

- Stan's review of the lot scenarios (exit criterion) and hands-on UI
  test not done; `just test` on Windows not run.
- No replay: fixing an old trade after later sales means deleting and
  re-entering the later ones.
- Average cost (LOT-110), HIFO and minimum tax (LOT-115) not built.
- Return-of-capital excess gain has no holding period.
- Stale threshold not a setting (SET-040).
- Performance is simple only (POS-030); no IRR or time-weighted.
- Price download (PRC-040) and QIF prices (PRC-030 QIF half, MIG-140)
  not built.
- Investment register loads each transaction separately; fine for the
  sample (hundreds), not measured at thousands (NFR-040).
- The investment register is read-only in place: edits go through the
  entry dialog (no keyboard grid like the banking register); the Clr
  column is set by reconciling.
- Transfer-in rows cannot be opened from the receiving account; edit
  from the sending account.
- Closing an investment account ignores linked-cash income dates
  (those transactions post only to the linked account).
- Search results in an investment account open the account's tabs, not
  the transaction.
- Allocation by one asset class per security (SEC-050 multi-class
  later).
