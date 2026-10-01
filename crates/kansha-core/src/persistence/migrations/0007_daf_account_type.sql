-- Kansha schema, migration 0007: Donor Advised Fund is an account type
-- (ACCT-010), not a security type (SEC-010).
--
-- A DAF account is an investment account: it holds securities, or only
-- cash. A security of type donor_advised_fund (migration 0006) becomes
-- Other.
--
-- SQLite cannot change a CHECK constraint in place: both tables are
-- rebuilt with the same columns, rows, and indexes. Other tables
-- reference them, so this migration runs with foreign keys off; the
-- runner checks every reference before it commits.

CREATE TABLE account_new (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT    NOT NULL COLLATE NOCASE UNIQUE
        CHECK (length(trim(name)) > 0),
    type          TEXT    NOT NULL CHECK (type IN (
        'checking', 'savings', 'credit_card', 'cash', 'money_market',
        'brokerage', 'traditional_ira', 'roth_ira', 'hsa', 'retirement_401k',
        'donor_advised_fund', 'other_asset', 'other_liability', 'loan')),
    account_group TEXT    NOT NULL CHECK (account_group IN (
        'banking', 'credit', 'investments', 'retirement', 'assets', 'liabilities', 'other')),
    tax_treatment TEXT    NOT NULL
        CHECK (tax_treatment IN ('taxable', 'tax_deferred', 'tax_exempt')),
    description   TEXT    NOT NULL DEFAULT '',
    institution   TEXT    NOT NULL DEFAULT '',
    account_number TEXT   NOT NULL DEFAULT '',            -- shown masked (ACCT-150)
    contact_phone TEXT    NOT NULL DEFAULT '',
    home_url      TEXT    NOT NULL DEFAULT '',
    notes         TEXT    NOT NULL DEFAULT '',            -- ACCT-160
    opening_date  TEXT    CHECK (opening_date IS date(opening_date)),
    show_in_bar   INTEGER NOT NULL DEFAULT 1 CHECK (show_in_bar IN (0, 1)),
    show_in_list  INTEGER NOT NULL DEFAULT 1 CHECK (show_in_list IN (0, 1)),
    status        TEXT    NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'closed')),
    closed_date   TEXT    CHECK (closed_date IS date(closed_date)),
    sort_order    INTEGER NOT NULL DEFAULT 0,

    -- Banking (ACCT-110)
    interest_rate INTEGER CHECK (interest_rate IS NULL OR interest_rate >= 0),
    -- Credit card (ACCT-120)
    credit_limit  INTEGER CHECK (credit_limit IS NULL OR credit_limit >= 0),
    -- Investment (ACCT-130, INV-300, INV-050/D-50, LOT-100)
    account_subtype    TEXT,
    cash_mode          TEXT CHECK (cash_mode IN ('internal', 'linked')),
    linked_cash_account_id INTEGER REFERENCES account (id),
    mmf_mode           TEXT CHECK (mmf_mode IN ('security', 'cash')),
    default_lot_method TEXT CHECK (default_lot_method IN (
        'fifo', 'specific', 'average', 'hifo', 'min_tax')),
    -- Other asset (ACCT-140)
    asset_subtype      TEXT CHECK (asset_subtype IN ('house', 'vehicle', 'other')),
    linked_liability_account_id INTEGER REFERENCES account (id),

    created_at    TEXT    NOT NULL
        CHECK (created_at IS strftime('%Y-%m-%dT%H:%M:%SZ', created_at)),

    -- Tax lines for transfers out and in (migration 0003, CAT-050)
    tax_line_out_id INTEGER REFERENCES tax_line (id),
    tax_line_in_id  INTEGER REFERENCES tax_line (id),

    CHECK ((status = 'closed') = (closed_date IS NOT NULL)),
    CHECK (interest_rate IS NULL OR type IN ('checking', 'savings', 'money_market')),
    CHECK (credit_limit IS NULL OR type = 'credit_card'),
    -- Investment accounts must say how cash and money market funds are
    -- held; other accounts must not.
    CHECK ((type IN ('brokerage', 'traditional_ira', 'roth_ira', 'hsa', 'retirement_401k',
                          'donor_advised_fund'))
           = (cash_mode IS NOT NULL AND mmf_mode IS NOT NULL AND default_lot_method IS NOT NULL)),
    CHECK (account_subtype IS NULL
           OR type IN ('brokerage', 'traditional_ira', 'roth_ira', 'hsa', 'retirement_401k',
                          'donor_advised_fund')),
    CHECK ((cash_mode = 'linked') = (linked_cash_account_id IS NOT NULL)),
    CHECK (linked_cash_account_id IS NULL OR linked_cash_account_id <> id),
    CHECK ((type = 'other_asset') = (asset_subtype IS NOT NULL)),
    CHECK (linked_liability_account_id IS NULL OR type = 'other_asset')
) STRICT;

INSERT INTO account_new (id, name, type, account_group, tax_treatment, description,
        institution, account_number, contact_phone, home_url, notes, opening_date,
        show_in_bar, show_in_list, status, closed_date, sort_order, interest_rate,
        credit_limit, account_subtype, cash_mode, linked_cash_account_id, mmf_mode,
        default_lot_method, asset_subtype, linked_liability_account_id, created_at,
        tax_line_out_id, tax_line_in_id)
    SELECT id, name, type,
        account_group,
        tax_treatment, description, institution, account_number, contact_phone, home_url,
        notes, opening_date, show_in_bar, show_in_list, status, closed_date, sort_order,
        interest_rate, credit_limit, account_subtype, cash_mode, linked_cash_account_id,
        mmf_mode, default_lot_method, asset_subtype, linked_liability_account_id, created_at,
        tax_line_out_id, tax_line_in_id
    FROM account;

DROP TABLE account;
ALTER TABLE account_new RENAME TO account;

CREATE INDEX account_linked_cash ON account (linked_cash_account_id)
    WHERE linked_cash_account_id IS NOT NULL;
CREATE INDEX account_linked_liability ON account (linked_liability_account_id)
    WHERE linked_liability_account_id IS NOT NULL;

UPDATE security SET type = 'other' WHERE type = 'donor_advised_fund';

CREATE TABLE security_new (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    name               TEXT    NOT NULL CHECK (length(trim(name)) > 0),
    ticker             TEXT    COLLATE NOCASE UNIQUE,   -- NULL allowed (CDs, some bonds)
    type               TEXT    NOT NULL CHECK (type IN (
        'stock', 'etf', 'mutual_fund', 'bond', 'money_market', 'cd',
        'other')),
    asset_class        TEXT    NOT NULL CHECK (asset_class IN (
        'us_equity', 'intl_equity', 'bond', 'cash', 'real_estate', 'commodity', 'other')),
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
    SELECT id, name, ticker, type, asset_class, cusip, default_lot_method, hidden, notes,
        created_at
    FROM security;

DROP TABLE security;
ALTER TABLE security_new RENAME TO security;
