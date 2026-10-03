-- Kansha schema, migration 0010: Roth conversion (INV-070); tax forms in
-- Quicken's order (CAT-050, RPT-145).
--
-- 1. A Roth conversion is an investment transaction of its own kind
--    ('roth_conversion'), entered in a traditional IRA or 401(k): cash,
--    or shares in kind (security and shares), into a Roth IRA
--    (to_account_id). It keeps the part that is not taxable (basis) and
--    the federal and state tax withheld from it; these three are set on
--    a conversion and on nothing else. investment_txn is rebuilt for the
--    new CHECK values.
-- 2. Built-in tax lines get new sort orders so the forms list as
--    Quicken lists them: Form 1040, Schedule A, Schedule B, (Schedule D,
--    computed, 350), 1099-DIV, W-2, SSA-1099, 1099-R, 1099-G, 1099-SA,
--    Form 8889. Lines keep their order within a form. Rows are matched
--    by form and line; other rows are untouched.
--
-- Other tables reference investment_txn, so this migration runs with
-- foreign keys off; the runner checks every reference before it commits.

CREATE TABLE investment_txn_new (
    txn_id        INTEGER PRIMARY KEY REFERENCES txn (id) ON DELETE CASCADE,
    account_id    INTEGER NOT NULL REFERENCES account (id),
    security_id   INTEGER REFERENCES security (id),
    action        TEXT    NOT NULL CHECK (action IN (
        'buy', 'sell',
        'dividend', 'interest',
        'reinvest_dividend', 'reinvest_cg_short', 'reinvest_cg_long',
        'cg_dist_short', 'cg_dist_long',
        'return_of_capital', 'split',
        'transfer_shares', 'shares_added', 'shares_removed',
        'cash_in', 'cash_out',
        'fee', 'tax_withholding', 'misc_income', 'misc_expense',
        'true_up', 'roth_conversion')),
    quantity      INTEGER CHECK (quantity > 0),
    price         INTEGER CHECK (price >= 0),
    commission    INTEGER NOT NULL DEFAULT 0 CHECK (commission >= 0),
    split_new     INTEGER CHECK (split_new > 0),     -- split ratio new:old, e.g. 2:1
    split_old     INTEGER CHECK (split_old > 0),
    to_account_id INTEGER REFERENCES account (id),   -- transfer_shares, roth_conversion
    lot_method    TEXT    CHECK (lot_method IN ('fifo', 'specific', 'average', 'hifo', 'min_tax')),
    settle_date   TEXT    CHECK (settle_date IS date(settle_date)),   -- INV-020
    -- Roth conversion (INV-070), cents.
    nontaxable       INTEGER CHECK (nontaxable >= 0),
    withheld_federal INTEGER CHECK (withheld_federal >= 0),
    withheld_state   INTEGER CHECK (withheld_state >= 0),

    CHECK (security_id IS NOT NULL OR action IN (
        'interest', 'cash_in', 'cash_out', 'fee', 'tax_withholding', 'misc_income', 'misc_expense',
        'roth_conversion')),
    CHECK (security_id IS NULL OR action NOT IN ('cash_in', 'cash_out')),
    -- A conversion has shares when it is in kind (has a security).
    CHECK (CASE WHEN action = 'roth_conversion'
                THEN (quantity IS NOT NULL) = (security_id IS NOT NULL)
                ELSE (action IN ('buy', 'sell', 'reinvest_dividend', 'reinvest_cg_short',
                                 'reinvest_cg_long', 'transfer_shares', 'shares_added',
                                 'shares_removed'))
                     = (quantity IS NOT NULL) END),
    CHECK ((action = 'split') = (split_new IS NOT NULL AND split_old IS NOT NULL)),
    CHECK (split_new IS NULL OR split_new <> split_old),
    CHECK ((action IN ('transfer_shares', 'roth_conversion')) = (to_account_id IS NOT NULL)),
    CHECK (to_account_id IS NULL OR to_account_id <> account_id),
    CHECK (lot_method IS NULL
           OR action IN ('sell', 'transfer_shares', 'shares_removed', 'roth_conversion')),
    CHECK (CASE WHEN action = 'roth_conversion'
                THEN nontaxable IS NOT NULL AND withheld_federal IS NOT NULL
                     AND withheld_state IS NOT NULL
                ELSE nontaxable IS NULL AND withheld_federal IS NULL
                     AND withheld_state IS NULL END)
) STRICT;

INSERT INTO investment_txn_new (txn_id, account_id, security_id, action, quantity, price,
        commission, split_new, split_old, to_account_id, lot_method, settle_date)
    SELECT txn_id, account_id, security_id, action, quantity, price, commission, split_new,
        split_old, to_account_id, lot_method, settle_date
    FROM investment_txn;

DROP TABLE investment_txn;
ALTER TABLE investment_txn_new RENAME TO investment_txn;

CREATE INDEX investment_txn_account ON investment_txn (account_id, security_id);
CREATE INDEX investment_txn_security ON investment_txn (security_id);
CREATE INDEX investment_txn_to_account ON investment_txn (to_account_id) WHERE to_account_id IS NOT NULL;

-- Form order: each built-in line moves by its form's offset.
UPDATE tax_line SET sort_order = sort_order + CASE form
        WHEN 'Form 1040'  THEN -100
        WHEN 'Schedule A' THEN -400
        WHEN 'Schedule B' THEN -400
        WHEN '1099-DIV'   THEN -400
        WHEN 'W-2'        THEN 400
        WHEN 'SSA-1099'   THEN 300
        WHEN '1099-R'     THEN 300
        WHEN '1099-G'     THEN 300
        ELSE 0 END
WHERE (form, line) IN (VALUES
    ('W-2',         'Salary or wages'),
    ('W-2',         'Federal tax withheld'),
    ('W-2',         'State tax withheld'),
    ('W-2',         'Social Security tax withheld'),
    ('W-2',         'Medicare tax withheld'),
    ('Form 1040',   'Other income, misc.'),
    ('Form 1040',   'Fed. estimated tax, quarterly'),
    ('Form 1040',   'IRA contribution, self'),
    ('SSA-1099',    'Net social security benefits'),
    ('SSA-1099',    'Federal tax withheld'),
    ('1099-R',      'Total IRA taxable distrib.'),
    ('1099-R',      'IRA federal tax withheld'),
    ('1099-R',      'IRA state tax withheld'),
    ('1099-R',      'Total pension taxable distrib.'),
    ('1099-R',      'Pension federal tax withheld'),
    ('1099-R',      'Pension state tax withheld'),
    ('1099-G',      'State and local tax refunds'),
    ('1099-G',      'Unemployment compensation'),
    ('Schedule A',  'Medicine and drugs'),
    ('Schedule A',  'Doctors, dentists, hospitals'),
    ('Schedule A',  'Medical insurance premiums'),
    ('Schedule A',  'State income taxes'),
    ('Schedule A',  'State estimated tax, quarterly'),
    ('Schedule A',  'Real estate taxes'),
    ('Schedule A',  'Personal property taxes'),
    ('Schedule A',  'Home mortgage interest (1098)'),
    ('Schedule A',  'Cash charity contributions'),
    ('Schedule A',  'Non-cash charity contributions'),
    ('Schedule B',  'Interest income'),
    ('Schedule B',  'Dividend income'),
    ('1099-DIV',    'Qualified dividends'),
    ('1099-DIV',    'Total capital gain distr.'),
    ('1099-DIV',    'Foreign tax paid'));
