# Tax planning — design note

Post-1.0 module (spec §3 lists tax planning as a 1.0.0 non-goal; §15.6
leaves room for it). Written 2026-11-02 after the 2026 tax planning
workbook (`~/pCloudSync/Finances/Taxes/2026/2026_Tax_Planning_v3.ods`)
was audited and extended. No spec text yet: requirement IDs (proposed
prefix **TAX-**) are written once §12 questions are answered.

## 1. Goal

Predict what the user will owe for a tax year, from the income, gains,
losses, deductions and payments in the book plus a few entered facts:

- federal income tax, net investment income tax (NIIT), additional
  Medicare tax, additional tax on early distributions;
- penalties and surcharges: estimated-tax underpayment penalty,
  Medicare IRMAA (Part B and D surcharges two years later);
- what has been paid (withholding, estimates) and what is still due,
  by estimated-tax installment.

Uses:

1. **Estimated taxes**: how much to pay each quarter to meet the safe
   harbor, and the penalty if not.
2. **Limited planning with scenarios**: "convert $X to Roth in
   December", "realize $Y of gains", "give shares to the DAF". Show
   the tax, IRMAA, and threshold headroom for each, side by side.

Not a tax-preparation program: no forms, no e-file, no carry of
results into next year's return. Every screen says "estimate".

## 2. Scope

- All five filing statuses: single, married filing jointly (MFJ),
  married filing separately (MFS), head of household (HoH),
  qualifying surviving spouse (QSS).
- Federal first. State second, one state (Virginia) to prove the
  state file format; other states can be added by anyone who writes
  the file.
- Individuals with wages, retirement income, Social Security,
  investments. Out: Schedule C business income and self-employment
  tax, rental (Schedule E), K-1s, farm income, foreign earned income
  exclusion (spec §3: no business features). See Q7.

## 3. Shape: engine plus year files

A tax year's law has two kinds of content:

- **Numbers** that change every year: brackets, standard deduction,
  thresholds, caps, rates, due dates, IRMAA tables.
- **Rules** that change only when Congress acts: how a phase-out
  works, what a floor applies to, how gains stack on ordinary income.
  The One Big Beautiful Bill Act (OBBBA, 2025) added five new rules at
  once (senior bonus, 0.5% charitable floor, 2/37 itemized limit, SALT
  phase-down, non-itemizer charitable deduction).

So the "plugin" for a year is a **data file**, and the rules live in a
**Rust engine** in `kansha-core` as a library of rule types:

- The engine knows rule types (`ordinary_brackets`, `ltcg_stacking`,
  `senior_bonus`, `salt_cap_phasedown`, …). Each has a version.
- A year file says which rule types apply and gives their numbers.
- A year that only changes numbers needs only a new file.
- A year that adds a new kind of rule needs engine code (with tests)
  and the file. Real tax software works the same way.
- A year file that names a rule type or version the engine does not
  know is refused with a clear message ("This 2027 file needs a newer
  Kansha").
- No scripting language in year files (Lua, WASM, formulas): they
  would be hard to test and a way in for a bad file.

### 3.1 Year file format

TOML, under the scenario TOML rules (CONVENTIONS): amounts and rates
as strings, inline tables on one line only. One file per jurisdiction
per year: `tax/us/2026.toml`, `tax/va/2026.toml`. Example (federal,
abridged):

```toml
[meta]
jurisdiction = "US"
year = 2026
engine = 1                       # rule-library version needed
sources = ["Rev. Proc. 2025-32", "P.L. 119-21 (OBBBA)", "CMS 2026 IRMAA"]
status = "final"                 # or "draft" (numbers projected)

[standard_deduction]
base = { single = "16100", mfj = "32200", mfs = "16100", hoh = "24150", qss = "32200" }
additional_65_or_blind = { married = "1650", unmarried = "2050" }

[ordinary_brackets.mfj]
rates = ["0.10", "0.12", "0.22", "0.24", "0.32", "0.35", "0.37"]
tops  = ["24800", "100800", "211400", "403550", "512450", "768700"]

[ltcg_stacking.mfj]
zero_top = "98900"
fifteen_top = "613700"

[senior_bonus]                   # OBBBA, 2025-2028
per_person = "6000"
phaseout_start = { mfj = "150000", other = "75000" }
phaseout_rate = "0.06"
mfs_allowed = false

[salt_cap_phasedown]
cap = { mfs = "20200", other = "40400" }
threshold = { mfs = "252500", other = "505000" }
rate = "0.30"
floor = { mfs = "5000", other = "10000" }
```

The numbers above are the ones used in the 2026 workbook; they are
checked against the sources when the file is written (spec §15.7).

### 3.2 Federal rule types (first engine version)

Income:

- `ordinary_income`: wages, interest, ordinary dividends, IRA and
  pension distributions, Roth conversions, short-term gains, taxable
  state refund, other income.
- `capital_gains`: net short and long term from lot disposals, plus
  capital gain distributions; $3,000 ($1,500 MFS) loss limit;
  carryover in (entered) and out (shown).
- `social_security_taxation`: 0/50/85% worksheet (Pub 915), base
  amounts by status, MFS living together.
- `state_refund_taxability`: tax benefit rule (prior-year itemized,
  standard deduction, SALT deducted and cap), as the workbook's
  Other Income G3:H9.

Deductions:

- `standard_deduction` with 65+/blind additions.
- `senior_bonus` (2025–2028, phase-out, not MFS).
- `itemized`: medical over 7.5% of AGI; SALT with
  `salt_cap_phasedown`; mortgage interest; charitable with AGI limits
  (60% cash, 30% capital-gain property, 50% combined) and the 0.5%
  floor; `itemized_limit_37` (2/37 reduction).
- `nonitemizer_charitable` (2026+, cash to public charities, not DAFs).
- `qbi_deduction`: entered amount only (no business module).

Tax:

- `ordinary_brackets`.
- `ltcg_stacking` (0/15/20% on qualified dividends and long-term gains
  stacked on ordinary income; 25% unrecaptured §1250 and 28%
  collectibles not modeled at first, see Q8).
- `niit` (3.8%, thresholds not indexed).
- `additional_medicare` (0.9% on wages over threshold).
- `early_distribution` (10% additional tax under 59½, exceptions as
  an entered amount).
- `foreign_tax_credit_simple` (1099-DIV foreign tax paid, up to the
  no-Form-1116 limit).
- `amt`: not at first, see Q9.

Payments and penalties:

- `safe_harbor`: 90% of current tax or 100%/110% of prior-year tax
  (AGI over $150,000, $75,000 MFS); $1,000 de minimis; four due
  dates; withholding treated as paid evenly; estimates on their date.
- `underpayment_penalty`: Form 2210 regular method, penalty rate per
  quarter from the year file. Annualized method (Schedule AI) later,
  see Q10.
- `irmaa`: MAGI (AGI plus tax-exempt interest) against the IRMAA
  table, by status, per person on Medicare. Income of year N sets
  premiums of year N+2; the table for N+2 is not known in year N
  (Q11).

### 3.3 State rule types (second step)

Virginia first: starts from federal AGI; additions and subtractions
(state refund, US-obligation interest, …); VA standard deduction or
itemized (federal itemized minus state income tax, prorated when the
SALT cap binds); exemptions; age deduction with AGI phase-out;
graduated 2/3/5/5.75% brackets; spouse tax adjustment; estimated
tax (90% current or 100% prior, $150 threshold, VA due dates). Each
state needs its own rule types; the file format and engine interface
are shared. Whether a state conforms to new federal rules (OBBBA) is
a per-state, per-year flag in the state file.

## 4. Inputs

### 4.1 From the book

| Need | Where Kansha has it |
|---|---|
| Wages, withholding | Tax lines W-2 (wages, federal, state, SS, Medicare withheld) |
| Interest | Schedule B Interest income (built-in Interest category) |
| Dividends | Schedule B Dividend income; 1099-DIV Qualified dividends; Total capital gain distr. |
| Foreign tax paid | 1099-DIV Foreign tax paid |
| Capital gains | Lot disposals (short/long term), §18 lots |
| IRA/pension distributions, withholding | 1099-R lines; retirement distributions (migration 0023) |
| Roth conversions | INV-070 (taxable part, nontaxable part, federal and state withheld) |
| Social Security | SSA-1099 Net benefits, Federal tax withheld |
| State refund received | 1099-G State and local tax refunds |
| Estimated payments | Form 1040 Fed. estimated tax, quarterly; Schedule A State estimated tax (with dates) |
| Itemized | Schedule A lines (medical, state income tax, real estate, personal property, mortgage interest, cash and non-cash charity) |
| Gifts to a DAF, grants from it | DAF account type (migrations 0006–0007): deposits deductible, grants not |
| Gift of shares | INV-060 (market value; gain not taxed) |
| HSA | 1099-SA, Form 8889 lines |
| Future items | Schedules (REC), for projection (§5) |

### 4.2 Entered (per book, per tax year)

- Filing status.
- People: taxpayer and spouse birth dates (65+, 59½, Medicare age),
  blind, on Medicare. Dependents (count, ages) if credits are in
  scope (Q6).
- Prior year: total tax, AGI (safe harbor); itemized total, SALT
  before and after cap, standard deduction that applied (state refund
  taxability); IRMAA tier already applying (display only).
- Carryovers in: capital loss (short, long), charitable (by limit
  type).
- QBI deduction (amount).
- Tax-exempt interest, if no tax line holds it (Q4).
- Qualified share of dividends before 1099-DIVs arrive (Q3).
- State: items the state file asks for (VA: exemptions count, spouse
  tax adjustment, US-obligation interest).

## 5. Base year, projection, scenarios

- **Actual**: everything dated in the tax year up to today (injected
  `Clock`).
- **Projection** for the rest of the year: scheduled transactions
  with tax lines; for unscheduled income (dividends, interest), a
  method per line: none, annualize year to date, same period last
  year, or an entered amount (Q2).
- **Base** = actual + projection.
- **Scenario** = base + adjustments. Adjustment kinds (first set):
  - Roth conversion: amount, date, federal and state withheld;
  - realize gains or losses: amount, short or long;
  - IRA distribution: amount, date, withheld;
  - charitable gift: cash, DAF, or shares (market value);
  - estimated payment: amount, date;
  - other income or deduction: amount, tax line.
  A scenario never writes transactions.
- Compare base and up to three scenarios side by side: tax by kind,
  total due, refund or balance, installments, penalty, IRMAA tier and
  surcharge two years out, marginal rate.
- **Headroom**: for base or a scenario, how much more ordinary income
  fits before the next threshold: top of the current bracket, 0% and
  15% gains ceilings, NIIT threshold, senior bonus phase-out, IRMAA
  tiers, SALT phase-down, state age-deduction phase-out. This is
  the Roth-conversion question ("fill the 24% bracket", "stay under
  IRMAA tier 1").
- **Marginal tax** on the next $1,000 (or a typed amount) of ordinary
  income and of long-term gain, which shows the stacking effects (a
  conversion that pushes gains out of 0%).

## 6. Outputs

- Tax Planner screen: year, scenario columns, sections Income,
  Deductions, Tax, Payments, Penalties/IRMAA, Headroom. Each figure
  opens its sources (the transactions, or the entered input), as
  reports do.
- Installment table per jurisdiction: due date, required
  (cumulative), paid, shortfall, penalty.
- An Insights card: "Q4 estimate due Jan 15: $X" (Q12).
- Printing as reports print.

## 7. Money and dates

- Money `i64` cents; intermediate math `rust_decimal`; round half
  even at the end of each line, as CONVENTIONS.
- IRS whole-dollar rounding is optional on a return; the engine works
  in cents and shows whole dollars.
- Below $100,000 taxable income the IRS uses the Tax Table ($50
  bands), which differs from the formula by a few dollars (Q13).
- Dates date-only; "today" from the injected `Clock`; due dates from
  the year file (weekend and holiday shifts are in the file, not
  computed).

## 8. Persistence (sketch)

New STRICT tables, one migration:

- `tax_profile` (per tax year: filing status, state).
- `tax_person` (taxpayer, spouse: birth date, blind, on Medicare).
- `tax_input` (tax year, key, amount cents or text): the entered
  facts of §4.2, keys defined by the engine.
- `tax_scenario`, `tax_scenario_item`.

Year files are not in the database (Q1). Writes via `Db::write` with
audit entries, as everything else.

## 9. Verification

- **Public tests**: IRS worked examples (Qualified Dividends and
  Capital Gain Tax Worksheet, Pub 915 Social Security worksheets,
  Pub 526 limit examples, Form 2210 examples), one test per rule
  type, written before the rule (money logic: tests first).
- **Stan's returns**: the filed 2024 and 2025 federal and VA returns
  as fixtures; a 2025 year file must reproduce them line by line.
  These hold personal data and stay out of the public repo (`ignored/`
  or a local-only test path; Q14).
- **The 2026 workbook** as the check for the 2026 file and the
  OBBBA rules, including the scenario cases it was tested with
  (conversions $0–$1.5M, charitable mixes).
- A year file test: every rule type it names loads, every status has
  every number, brackets ascend.

## 10. Yearly upkeep

- Each fall the IRS publishes the next year's numbers (Rev. Proc.,
  usually October–November); CMS publishes IRMAA in the fall.
- A new year file starts as `status = "draft"` (numbers copied and
  indexed by an estimate), becomes `final` when checked against the
  sources. The screen marks a draft file.
- Claude can draft the file from the sources; Stan checks it.

## 11. Steps

1. **Engine and federal files**: rule types of §3.2 except penalties
   and IRMAA; 2025 and 2026 files; inputs; actual year to date.
   Must reproduce the 2025 return and the 2026 workbook.
2. **Planning**: projection, scenarios, headroom, marginal tax, safe
   harbor and installments, underpayment penalty, IRMAA.
3. **State**: file format and interface; Virginia 2025 and 2026;
   reproduce the 2025 VA return.
4. **Later**: AMT, annualized penalty method, more states, credits
   beyond those in step 1.

## 12. Questions

1. Year files: built into the app only, or also loadable from a
   folder (a user writes or updates a file, e.g. another state)? If
   loadable, signed or just validated?
2. Projection of unscheduled income: which default (annualize year
   to date, or last year's same period), and per tax line or one for
   all?
3. Qualified dividends during the year: Kansha learns the qualified
   share only from the 1099-DIV. Estimate by a per-security qualified
   percentage (a security field), a per-year percentage, or treat all
   as qualified until entered?
4. Tax-exempt interest (needed for IRMAA MAGI and Social Security):
   add a built-in tax line, or an entered amount?
5. Charitable type for the AGI limits: the tax lines have cash and
   non-cash; DAF deposits are known from the account type. Is that
   enough, or do private foundations (30%/20%) need handling?
6. Credits: which are in scope for "all users"? Child tax credit,
   credit for other dependents, education credits, saver's credit,
   premium tax credit (ACA, matters for early retirees), elderly
   credit (Schedule R)?
7. Users with some self-employment or rental income: refuse, accept
   an entered net amount (taxed as ordinary, no SE tax), or model SE
   tax?
8. Unrecaptured §1250 gain (25%) and collectibles (28%): needed for
   rental property sales and gold funds. Model now or later?
9. AMT: rare for retirees after 2017, possible with large ISOs or
   private-activity bond interest. Skip with a warning, or model in
   step 1?
10. Underpayment penalty: is the regular method enough, given a
    December Roth conversion is exactly when the annualized method
    (Schedule AI) saves money? Move Schedule AI into step 2?
11. IRMAA two years out: use the latest known table, or the latest
    table indexed by an assumed inflation rate? Show both?
12. Insights card for the next estimated payment: wanted?
13. Below $100,000 taxable income: compute by formula (cents
    accurate to the law's rates) or match the IRS Tax Table ($50
    bands) as the return will?
14. Fixtures from Stan's returns: keep in `ignored/` (local only), or
    scale/anonymize the figures so they can be committed?
15. Where does the screen live: Reports menu, Tools menu, or its own
    menu (Planning)? UI is placeholder either way.
16. One profile per tax year, or a profile that carries forward with
    changes (filing status rarely changes; birth dates never)?
17. MFS: model one spouse's return only (the book's owner), or both
    returns from one book (needs income split by person)?
18. Income by person: some rules need it (Social Security for each
    spouse, early-distribution age, IRMAA per person on Medicare,
    VA age deduction per person). Kansha has no owner on accounts.
    Add an owner field to accounts (taxpayer, spouse, joint), or
    enter the split?
19. Roth conversion scenarios across several years (convert $X a
    year for five years): in scope later, or never (that is
    multi-year projection, closer to retirement planning)?
20. Should a scenario be turned into real scheduled transactions
    (e.g. the estimated payments it calls for), or stay read-only?
