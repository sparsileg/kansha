-- Kansha schema, migration 0008: lot true-up (MIG-115), tithing columns
-- dropped, price audit rows purged.
--
-- 1. A true-up (MIG-115) is an investment transaction of its own kind
--    ('true_up'): it closes the holding's open lots that differ from the
--    broker's list (disposal kind 'true_up', no gain) and opens the
--    broker's lots in their place. investment_txn and lot_disposal are
--    rebuilt for the new CHECK values.
-- 2. category.tithable and category.giving go (tithing withdrawn in spec
--    0.3.22). Table CHECKs name them, so category is rebuilt.
-- 3. Price writes are no longer audited (CONVENTIONS §5); the old price
--    entries are deleted. The append-only trigger is dropped for the
--    delete and created again.
--
-- Other tables reference these, so this migration runs with foreign keys
-- off; the runner checks every reference before it commits.

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
        'true_up')),
    quantity      INTEGER CHECK (quantity > 0),
    price         INTEGER CHECK (price >= 0),
    commission    INTEGER NOT NULL DEFAULT 0 CHECK (commission >= 0),
    split_new     INTEGER CHECK (split_new > 0),     -- split ratio new:old, e.g. 2:1
    split_old     INTEGER CHECK (split_old > 0),
    to_account_id INTEGER REFERENCES account (id),   -- transfer_shares destination
    lot_method    TEXT    CHECK (lot_method IN ('fifo', 'specific', 'average', 'hifo', 'min_tax')),
    settle_date   TEXT    CHECK (settle_date IS date(settle_date)),   -- INV-020

    CHECK (security_id IS NOT NULL OR action IN (
        'interest', 'cash_in', 'cash_out', 'fee', 'tax_withholding', 'misc_income', 'misc_expense')),
    CHECK (security_id IS NULL OR action NOT IN ('cash_in', 'cash_out')),
    CHECK ((action IN ('buy', 'sell', 'reinvest_dividend', 'reinvest_cg_short', 'reinvest_cg_long',
                       'transfer_shares', 'shares_added', 'shares_removed'))
           = (quantity IS NOT NULL)),
    CHECK ((action = 'split') = (split_new IS NOT NULL AND split_old IS NOT NULL)),
    CHECK (split_new IS NULL OR split_new <> split_old),
    CHECK ((action = 'transfer_shares') = (to_account_id IS NOT NULL)),
    CHECK (to_account_id IS NULL OR to_account_id <> account_id),
    CHECK (lot_method IS NULL OR action IN ('sell', 'transfer_shares', 'shares_removed'))
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

-- Shares leaving a lot. kind 'sale' is a realized gain record (LOT-040);
-- 'true_up' closes a lot replaced by a true-up (MIG-115).
CREATE TABLE lot_disposal_new (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    lot_id   INTEGER NOT NULL REFERENCES lot (id),
    txn_id   INTEGER NOT NULL REFERENCES txn (id),
    kind     TEXT    NOT NULL CHECK (kind IN ('sale', 'transfer_out', 'removed', 'true_up')),
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    basis    INTEGER NOT NULL CHECK (basis >= 0),
    proceeds INTEGER CHECK (proceeds >= 0),
    gain     INTEGER,
    term     TEXT    CHECK (term IN ('short', 'long')),
    CHECK ((kind = 'sale') = (proceeds IS NOT NULL AND gain IS NOT NULL AND term IS NOT NULL)),
    CHECK (gain IS NULL OR gain = proceeds - basis)
) STRICT;

INSERT INTO lot_disposal_new (id, lot_id, txn_id, kind, quantity, basis, proceeds, gain, term)
    SELECT id, lot_id, txn_id, kind, quantity, basis, proceeds, gain, term FROM lot_disposal;

DROP TABLE lot_disposal;
ALTER TABLE lot_disposal_new RENAME TO lot_disposal;

CREATE INDEX lot_disposal_lot ON lot_disposal (lot_id);
CREATE INDEX lot_disposal_txn ON lot_disposal (txn_id);

CREATE TABLE category_new (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_id   INTEGER REFERENCES category (id),
    kind        TEXT    NOT NULL CHECK (kind IN ('income', 'expense', 'equity')),
    name        TEXT    NOT NULL COLLATE NOCASE CHECK (length(trim(name)) > 0),
    system_key  TEXT    UNIQUE,
    tax_related INTEGER NOT NULL DEFAULT 0 CHECK (tax_related IN (0, 1)),
    hidden      INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0, 1)),
    created_at  TEXT    NOT NULL
        CHECK (created_at IS strftime('%Y-%m-%dT%H:%M:%SZ', created_at)),
    -- Tax line (migration 0003, CAT-050)
    tax_line_id INTEGER REFERENCES tax_line (id),
    CHECK (parent_id IS NULL OR parent_id <> id),
    CHECK (kind <> 'equity' OR system_key IS NOT NULL)
) STRICT;

INSERT INTO category_new (id, parent_id, kind, name, system_key, tax_related, hidden,
        created_at, tax_line_id)
    SELECT id, parent_id, kind, name, system_key, tax_related, hidden, created_at, tax_line_id
    FROM category;

DROP TABLE category;
ALTER TABLE category_new RENAME TO category;

-- Sibling names are unique; top-level names too (NULL parent -> 0).
CREATE UNIQUE INDEX category_sibling_name ON category (ifnull(parent_id, 0), name COLLATE NOCASE);
CREATE INDEX category_parent ON category (parent_id);
CREATE INDEX category_tax_line ON category (tax_line_id) WHERE tax_line_id IS NOT NULL;

DROP TRIGGER audit_log_no_delete;

DELETE FROM audit_log WHERE entity = 'price';

CREATE TRIGGER audit_log_no_delete BEFORE DELETE ON audit_log
BEGIN
    SELECT RAISE(ABORT, 'audit_log is append-only');
END;
