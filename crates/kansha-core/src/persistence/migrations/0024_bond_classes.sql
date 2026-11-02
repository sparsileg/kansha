-- Kansha schema, migration 0024: the Bonds asset class splits into
-- Short-term, Intermediate, Long-term, and International bonds, and four
-- classes are new: US small/mid cap, Emerging markets, TIPS, and
-- Municipal bonds (SEC-010, POS-020).
--
-- A Bonds security becomes Intermediate bonds, the class of most
-- total-market bond funds; the owner re-classes the others.
--
-- SQLite cannot change a CHECK constraint in place: the table is rebuilt
-- with the same columns and rows. Other tables reference it, so this
-- migration runs with foreign keys off; the runner checks every
-- reference before it commits.

CREATE TABLE security_new (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    name               TEXT    NOT NULL CHECK (length(trim(name)) > 0),
    ticker             TEXT    COLLATE NOCASE UNIQUE,   -- NULL allowed (CDs, some bonds)
    type               TEXT    NOT NULL CHECK (type IN (
        'stock', 'etf', 'mutual_fund', 'bond', 'money_market', 'cd',
        'other')),
    asset_class        TEXT    NOT NULL CHECK (asset_class IN (
        'us_equity', 'us_small_mid', 'intl_equity', 'emerging',
        'bond_short', 'bond_intermediate', 'bond_long', 'bond_intl',
        'bond_tips', 'bond_muni', 'cash', 'real_estate', 'commodity', 'other')),
    cusip              TEXT    CHECK (cusip IS NULL OR length(cusip) = 9),   -- SEC-020
    default_lot_method TEXT    CHECK (default_lot_method IN (
        'fifo', 'specific', 'average', 'hifo', 'min_tax')),                  -- SEC-030
    hidden             INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0, 1)),
    notes              TEXT    NOT NULL DEFAULT '',
    created_at         TEXT    NOT NULL
        CHECK (created_at IS strftime('%Y-%m-%dT%H:%M:%SZ', created_at))
) STRICT;

INSERT INTO security_new (id, name, ticker, type, asset_class, cusip, default_lot_method,
        hidden, notes, created_at)
    SELECT id, name, ticker, type,
        CASE asset_class WHEN 'bond' THEN 'bond_intermediate' ELSE asset_class END,
        cusip, default_lot_method, hidden, notes, created_at
    FROM security;

DROP TABLE security;
ALTER TABLE security_new RENAME TO security;
