# Import proposal (Phase 9) — plan, not built

Status: plan agreed in chat 2026-09-30. Part C (performance test) is
built; Part A is in spec 0.6 (§11, settles P-01, P-03, P-05) and built
in Phase 9 (`phase-9.md`) except the A4 true-up (MIG-115).

## Context

Stan has used Quicken for about 20 years, with cutoffs: at each one he
saved the data file and started a new one, dropping most older
transactions. The current file still holds hidden accounts (kept only
for long-term net worth history) and the full investment history (kept
for cost basis). The questions: what to bring into Kansha, how far
back, and what to leave behind.

Facts from sample exports (real data; not copied into the repo):

- A taxable brokerage account's QIF goes back to 2000; a traditional
  IRA's to 2007; a newer IRA's to 2024.
- A credit card's QIF goes back to 2015, so the "2021 cutoff" did not
  cut every account.
- Per-account QIFs hold only the transactions (`!Type:Invst`,
  `!Type:CCard`): no security list, categories, classes, or prices.
  Those come only from a whole-file QIF export.
- Investment actions seen: Buy, BuyX, Sell, SellX, ReinvDiv,
  ReinvLg, ReinvSh, Div, DivX, ShrsIn, ShrsOut, StkSplit, Cash, XIn,
  WithdrwX.
- The samples are not exhaustive; more accounts (including Fidelity)
  come later.

## Part A — Import decisions

### A1. Export shape

| Option | Pros | Cons |
|---|---|---|
| **One whole-file QIF (all accounts)** ★ | Categories, classes, securities, prices, and all accounts in one file; both sides of each transfer present (MIG-070 matching) | Large; includes dead accounts and the memorized list (skipped) |
| Per-account QIFs | Small; pick accounts | No categories or securities; transfers seen from one side; many files |
| QXF | Maybe full fidelity | Undocumented; parser guesswork |

Decided: whole-file QIF for the final import. Per-account samples are
fine while building.

### A2. How far back (banking and credit card)

| Option | Pros | Cons |
|---|---|---|
| Fresh start: opening balances at a date | Clean; small import | Old detail stays in Quicken only |
| **Current file as is** ★ | One export; categories current; balances match Quicken reports, so MIG-100 verification is direct | Uneven start dates per account |
| Stitch all archive files back to 2007 | One continuous history | Category mapping per era; duplicate rows at each seam; seam balances must reconcile; old files must open in Quicken 2013; large verification effort |
| Old eras as separate archive books | Uses named books; each era keeps its own categories | No search or reports across eras |

Decided: every transaction of every active banking and credit card
account, no start date. No start-date option in Phase 9. Archive books
maybe later, separate from this conversion.

### A3. Hidden (dead) accounts

| Option | Pros | Cons |
|---|---|---|
| Import as closed accounts | Computed net worth history continues | Clutter returns; needs their full history |
| **Skip** ★ | Clean book | Net worth history before cutover not in Kansha |

Decided: skip. The preview lists every QIF account with a checkbox;
hidden accounts start unchecked. Stan keeps long-term net worth in
his spreadsheet; Kansha does not record it.

### A4. Investments

| Option | Pros | Cons |
|---|---|---|
| Seed open lots from broker cost-basis CSV at a seed date; import only later rows | Basis matches broker; MIG-120 built | No history before the seed date |
| Replay full history only | Complete record | QIF has no lot IDs, so rebuilt lots can differ from the broker's |
| **Replay full history, then true up lots to broker CSV** ★ | Full history; basis ends matching the broker | True-up step to build |

Decided: Stan wants investment history back to at least 2020, so no
seed date. Import the full QIF investment history (it goes back to
each account's start); Kansha's lot engine rebuilds the lots. At
cutover the preview compares Kansha's open lots per security (shares
and basis) with the broker's cost-basis CSV; each difference is fixed
by a dated, audited true-up. That needs a new lot adjustment kind:
**⚠ Schema change**, riding the next migration with the tithing-column
drop. IRAs: shares must match; basis differences are ignored (no tax
effect).

### A5. Securities, categories, payees, tags

- Import only what the imported rows and open lots use. The preview
  lists unused ones unchecked; Stan can tick any to keep.
- Categories: the MIG-060 mapping step can rename or merge on the way
  in, so categories can be redone without another cutoff.
- Securities: map QIF name to ticker. Prices: only for kept
  securities; optionally thinned by the PRC-060 rule.
- Memorized transactions: never imported (MIG-160).

### A6. Workflow to cutover

1. Build the import against Stan's sample QIFs; expect changes while
   testing and comparing with Quicken.
2. Each trial imports into a throwaway named book (e.g.
   `import-test`); to rerun, delete the book and import again. The
   live book is never touched.
3. Compare each trial with Quicken reports (MIG-100): balance per
   account at the export date, category totals per year, holdings and
   basis per security. Fix; repeat.
4. When clean: Stan freezes Quicken, exports everything (one
   whole-file QIF plus broker lot CSVs), imports into the real book,
   verifies once more, and switches over.

## Part B — Dropped

A hand-entered monthly net worth list (RPT-115) was proposed and
dropped (Stan, 2026-09-30): he keeps it in a spreadsheet. No spec
change, no table.

## Part C — Performance test (built 2026-09-30)

100,000 transactions over 12 accounts (NFR-040, NFR-050).

- `crates/kansha-core/src/sample.rs`: add
  `SampleSpec.extra_accounts` (default 0, output unchanged); each adds
  a checking, savings, or credit card account that shares the daily
  events. The test uses 5 (7 standard + 5 = 12).
- New test `crates/kansha-core/tests/integration/perf.rs`, `#[ignore]`
  so `just check` stays fast: 12 accounts, density set so the book
  holds at least 100,000 transactions over about 20 years. Measures
  and asserts:
  - load time (printed)
  - busiest register, all rows and first page: under 1 s
  - Net Worth over time by month, and Income/Expense by category for
    a year: under 2 s
  - integrity check (printed)
- `justfile`: `perf` recipe running that test in release mode with
  `--ignored --nocapture`.
- Record the times in `devdocs/phase-notes/prototype-review.md`.

Reuse `sample::generate`, `ledger::RegisterQuery`, the report builders
in `crates/kansha-core/src/reports/`, and the pattern of
`register_with_10000_rows_opens_under_a_second` in
`tests/integration/register.rs`.

Result (2026-09-30): 108,662 transactions; Net Worth over 20 years
by month took 11.2 s and the dashboard 2.6 s, both over the 2 s
limit; everything else passed. Times and causes in
`prototype-review.md`. Fix (one pass per account with running
balances) not started.

Known risk: the register's window function scans the whole account
(phase-3 notes); 100,000 rows may expose it.

## Verification

- `just check` green (perf test ignored there).
- `just perf` runs; times reported; any miss becomes a finding.
