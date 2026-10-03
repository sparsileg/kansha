-- Kansha schema, migration 0011: a dividend without a security (INV-010).
--
-- A dividend paid by the account's cash (a brokerage's settlement fund)
-- names no security, as interest may not. investment_txn is rebuilt for
-- the CHECK; nothing else changes. Quicken's Div with no security now
-- imports as a dividend (MIG-020).
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
        'dividend', 'interest', 'cash_in', 'cash_out', 'fee', 'tax_withholding', 'misc_income', 'misc_expense',
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
        commission, split_new, split_old, to_account_id, lot_method, settle_date, nontaxable,
        withheld_federal, withheld_state)
    SELECT txn_id, account_id, security_id, action, quantity, price, commission, split_new,
        split_old, to_account_id, lot_method, settle_date, nontaxable, withheld_federal,
        withheld_state
    FROM investment_txn;

DROP TABLE investment_txn;
ALTER TABLE investment_txn_new RENAME TO investment_txn;

CREATE INDEX investment_txn_account ON investment_txn (account_id, security_id);
CREATE INDEX investment_txn_security ON investment_txn (security_id);
CREATE INDEX investment_txn_to_account ON investment_txn (to_account_id) WHERE to_account_id IS NOT NULL;
