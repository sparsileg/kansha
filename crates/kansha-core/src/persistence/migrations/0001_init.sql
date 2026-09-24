-- Kansha schema, migration 0001: initial schema.
--
-- This file IS the schema specification (spec 0.3, §18). Spec prose
-- points here; it does not restate columns.
--
-- Conventions (CONVENTIONS.md §4–5):
--   * Every table is STRICT: SQLite rejects a REAL in an INTEGER column.
--   * Money: INTEGER cents. Quantity and price: INTEGER scaled by 10^6.
--     Interest rates: INTEGER annual percent scaled by 10^6
--     (4.35 % -> 4350000).
--   * Dates: TEXT 'YYYY-MM-DD', validated with `x IS date(x)` (rejects
--     2026-02-30, 2026-2-3, trailing spaces). NULL passes; NOT NULL is
--     stated separately.
--   * Timestamps: TEXT UTC 'YYYY-MM-DDTHH:MM:SSZ', supplied by the
--     injected Clock (never CURRENT_TIMESTAMP).
--   * Booleans: INTEGER 0/1.
--   * Enumerations: TEXT with a CHECK list. Adding a value is a new
--     migration.
--   * IDs: INTEGER PRIMARY KEY AUTOINCREMENT, so an ID is never reused
--     after a delete (audit log entries stay unambiguous).
--   * Balances, positions, open lot quantities, and gains are derived
--     from the ledger (ACCT-230, INT-010). Nothing here stores a balance.
--   * Posting sign: + increases an asset account or records an expense;
--     - increases a liability or records income. A transaction's
--     postings sum to zero (checked by the engine and by the
--     unbalanced_txn view, INT-030).
--
-- The migration runner wraps this file in one transaction and records
-- version 1 in schema_version.

PRAGMA application_id = 1263424328;  -- 0x4B4E5348, 'KNSH'

-- ---------------------------------------------------------------------
-- Schema version (§21)
-- ---------------------------------------------------------------------

CREATE TABLE schema_version (
    version     INTEGER PRIMARY KEY CHECK (version >= 1),
    description TEXT    NOT NULL,
    applied_at  TEXT    NOT NULL
        CHECK (applied_at IS strftime('%Y-%m-%dT%H:%M:%SZ', applied_at))
) STRICT;

-- ---------------------------------------------------------------------
-- Import batches (MIG-040, MIG-080). Defined early: txn references it.
-- ---------------------------------------------------------------------

CREATE TABLE import_batch (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    source_file    TEXT    NOT NULL,
    source_sha256  TEXT    CHECK (source_sha256 IS NULL OR length(source_sha256) = 64),
    archive_path   TEXT,                                   -- MIG-170
    format         TEXT    NOT NULL CHECK (format IN ('qif', 'qxf', 'csv', 'ofx', 'qfx')),
    status         TEXT    NOT NULL DEFAULT 'staged'
        CHECK (status IN ('staged', 'committed', 'rolled_back')),
    created_at     TEXT    NOT NULL
        CHECK (created_at IS strftime('%Y-%m-%dT%H:%M:%SZ', created_at)),
    committed_at   TEXT
        CHECK (committed_at IS strftime('%Y-%m-%dT%H:%M:%SZ', committed_at)),
    rolled_back_at TEXT
        CHECK (rolled_back_at IS strftime('%Y-%m-%dT%H:%M:%SZ', rolled_back_at)),
    CHECK ((status = 'staged') = (committed_at IS NULL)),
    CHECK ((status = 'rolled_back') = (rolled_back_at IS NOT NULL))
) STRICT;

-- ---------------------------------------------------------------------
-- Accounts (ACCT-010 … ACCT-240, D-50, D-100)
-- ---------------------------------------------------------------------

CREATE TABLE account (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT    NOT NULL COLLATE NOCASE UNIQUE
        CHECK (length(trim(name)) > 0),
    type          TEXT    NOT NULL CHECK (type IN (
        'checking', 'savings', 'credit_card', 'cash', 'money_market',
        'brokerage', 'traditional_ira', 'roth_ira', 'hsa', 'retirement_401k',
        'other_asset', 'other_liability', 'loan')),
    account_group TEXT    NOT NULL CHECK (account_group IN (
        'banking', 'credit', 'investments', 'retirement', 'assets', 'liabilities')),
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

    CHECK ((status = 'closed') = (closed_date IS NOT NULL)),
    CHECK (interest_rate IS NULL OR type IN ('checking', 'savings', 'money_market')),
    CHECK (credit_limit IS NULL OR type = 'credit_card'),
    -- Investment accounts must say how cash and money market funds are
    -- held; other accounts must not.
    CHECK ((type IN ('brokerage', 'traditional_ira', 'roth_ira', 'hsa', 'retirement_401k'))
           = (cash_mode IS NOT NULL AND mmf_mode IS NOT NULL AND default_lot_method IS NOT NULL)),
    CHECK (account_subtype IS NULL
           OR type IN ('brokerage', 'traditional_ira', 'roth_ira', 'hsa', 'retirement_401k')),
    CHECK ((cash_mode = 'linked') = (linked_cash_account_id IS NOT NULL)),
    CHECK (linked_cash_account_id IS NULL OR linked_cash_account_id <> id),
    CHECK ((type = 'other_asset') = (asset_subtype IS NOT NULL)),
    CHECK (linked_liability_account_id IS NULL OR type = 'other_asset')
) STRICT;

CREATE INDEX account_linked_cash ON account (linked_cash_account_id)
    WHERE linked_cash_account_id IS NOT NULL;
CREATE INDEX account_linked_liability ON account (linked_liability_account_id)
    WHERE linked_liability_account_id IS NOT NULL;

-- ---------------------------------------------------------------------
-- Categories (CAT-010 … CAT-060)
-- ---------------------------------------------------------------------
-- kind 'equity' is system-only (opening balances). system_key marks the
-- built-in categories of CAT-060; they cannot be deleted (FKs from
-- postings) and the engine refuses to rename or move them.

CREATE TABLE category (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_id   INTEGER REFERENCES category (id),
    kind        TEXT    NOT NULL CHECK (kind IN ('income', 'expense', 'equity')),
    name        TEXT    NOT NULL COLLATE NOCASE CHECK (length(trim(name)) > 0),
    system_key  TEXT    UNIQUE,
    tax_related INTEGER NOT NULL DEFAULT 0 CHECK (tax_related IN (0, 1)),
    tithable    INTEGER NOT NULL DEFAULT 0 CHECK (tithable IN (0, 1)),
    giving      INTEGER NOT NULL DEFAULT 0 CHECK (giving IN (0, 1)),
    hidden      INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0, 1)),
    created_at  TEXT    NOT NULL
        CHECK (created_at IS strftime('%Y-%m-%dT%H:%M:%SZ', created_at)),
    CHECK (parent_id IS NULL OR parent_id <> id),
    CHECK (kind <> 'equity' OR system_key IS NOT NULL),
    CHECK (tithable = 0 OR kind = 'income'),
    CHECK (giving = 0 OR kind = 'expense')
) STRICT;

-- Sibling names are unique; top-level names too (NULL parent -> 0).
CREATE UNIQUE INDEX category_sibling_name ON category (ifnull(parent_id, 0), name COLLATE NOCASE);
CREATE INDEX category_parent ON category (parent_id);

-- ---------------------------------------------------------------------
-- Tags (TAG-010 … TAG-030) and payees (PAY-010 … PAY-030)
-- ---------------------------------------------------------------------

CREATE TABLE tag (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL COLLATE NOCASE UNIQUE CHECK (length(trim(name)) > 0),
    hidden     INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0, 1)),
    created_at TEXT    NOT NULL
        CHECK (created_at IS strftime('%Y-%m-%dT%H:%M:%SZ', created_at))
) STRICT;

CREATE TABLE payee (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    name                TEXT    NOT NULL COLLATE NOCASE UNIQUE CHECK (length(trim(name)) > 0),
    -- Memorized defaults (PAY-020)
    default_category_id INTEGER REFERENCES category (id),
    default_tag_id      INTEGER REFERENCES tag (id),
    default_memo        TEXT    NOT NULL DEFAULT '',
    default_amount      INTEGER,                          -- cents, signed
    hidden              INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0, 1)),
    created_at          TEXT    NOT NULL
        CHECK (created_at IS strftime('%Y-%m-%dT%H:%M:%SZ', created_at))
) STRICT;

CREATE INDEX payee_default_category ON payee (default_category_id);
CREATE INDEX payee_default_tag ON payee (default_tag_id);

-- ---------------------------------------------------------------------
-- Securities and prices (SEC-010 … SEC-040, PRC-010 … PRC-050)
-- ---------------------------------------------------------------------

CREATE TABLE security (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    name               TEXT    NOT NULL CHECK (length(trim(name)) > 0),
    ticker             TEXT    COLLATE NOCASE UNIQUE,   -- NULL allowed (CDs, some bonds)
    type               TEXT    NOT NULL CHECK (type IN (
        'stock', 'etf', 'mutual_fund', 'bond', 'money_market', 'cd', 'other')),
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

CREATE TABLE price (
    security_id INTEGER NOT NULL REFERENCES security (id),
    price_date  TEXT    NOT NULL CHECK (price_date IS date(price_date)),
    price       INTEGER NOT NULL CHECK (price >= 0),        -- x 10^6
    source      TEXT    NOT NULL CHECK (source IN ('manual', 'csv', 'qif', 'download')),
    PRIMARY KEY (security_id, price_date)                   -- serves PRC-050 lookups
) STRICT, WITHOUT ROWID;

-- ---------------------------------------------------------------------
-- Schedules (REC-010 … REC-160). Defined before txn: txn references it.
-- ---------------------------------------------------------------------
-- Recurrence encoding:
--   once                 start_date only
--   daily                every `interval` days
--   weekly               every `interval` weeks on start_date's weekday
--   twice_monthly        day1 and day2 of every month (day1 < day2)
--   monthly              day1 of every `interval` months
--                        (quarterly = interval 3, twice a year = 6)
--   monthly_last_day     last day of every `interval` months
--   monthly_nth_weekday  week_of_month (1-4, or -1 = last) weekday
--                        (1 = Monday … 7 = Sunday, ISO) every `interval` months
--   yearly               start_date's month/day every `interval` years
-- Days that don't exist in a month clamp to the month's last day (REC-040).
-- The template's main side is account_id; schedule_line rows are the
-- other side (categories and/or transfer accounts). The scheduled amount
-- for account_id is minus the sum of the lines.

CREATE TABLE schedule (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id    INTEGER NOT NULL REFERENCES account (id),
    payee_id      INTEGER REFERENCES payee (id),
    memo          TEXT    NOT NULL DEFAULT '',
    amount_type   TEXT    NOT NULL DEFAULT 'fixed' CHECK (amount_type IN ('fixed', 'estimated')),
    frequency     TEXT    NOT NULL CHECK (frequency IN (
        'once', 'daily', 'weekly', 'twice_monthly', 'monthly',
        'monthly_last_day', 'monthly_nth_weekday', 'yearly')),
    interval      INTEGER NOT NULL DEFAULT 1 CHECK (interval >= 1),
    day1          INTEGER CHECK (day1 BETWEEN 1 AND 31),
    day2          INTEGER CHECK (day2 BETWEEN 1 AND 31),
    weekday       INTEGER CHECK (weekday BETWEEN 1 AND 7),
    week_of_month INTEGER CHECK (week_of_month IN (1, 2, 3, 4, -1)),
    start_date    TEXT    NOT NULL CHECK (start_date IS date(start_date)),
    next_due      TEXT    CHECK (next_due IS date(next_due)),     -- NULL once ended
    end_kind      TEXT    NOT NULL DEFAULT 'never'
        CHECK (end_kind IN ('never', 'on_date', 'after_count')),
    end_date      TEXT    CHECK (end_date IS date(end_date)),
    remaining     INTEGER CHECK (remaining >= 0),               -- "# left"
    remind_days   INTEGER NOT NULL DEFAULT 0 CHECK (remind_days >= 0),
    mode          TEXT    NOT NULL DEFAULT 'remind' CHECK (mode IN ('remind', 'auto')),
    weekend_rule  TEXT    NOT NULL DEFAULT 'none' CHECK (weekend_rule IN ('none', 'previous', 'next')),
    status        TEXT    NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'ended', 'deleted')),
    created_at    TEXT    NOT NULL
        CHECK (created_at IS strftime('%Y-%m-%dT%H:%M:%SZ', created_at)),

    CHECK ((end_kind = 'on_date') = (end_date IS NOT NULL)),
    CHECK ((end_kind = 'after_count') = (remaining IS NOT NULL)),
    CHECK (end_date IS NULL OR end_date >= start_date),
    CHECK (status = 'active' OR next_due IS NULL),
    CHECK ((frequency IN ('twice_monthly', 'monthly')) = (day1 IS NOT NULL)),
    CHECK ((frequency = 'twice_monthly') = (day2 IS NOT NULL)),
    CHECK (day2 IS NULL OR day1 < day2),
    CHECK ((frequency = 'monthly_nth_weekday') = (weekday IS NOT NULL AND week_of_month IS NOT NULL)),
    CHECK (frequency NOT IN ('once', 'twice_monthly') OR interval = 1)
) STRICT;

CREATE INDEX schedule_account ON schedule (account_id);
CREATE INDEX schedule_payee ON schedule (payee_id);
CREATE INDEX schedule_next_due ON schedule (next_due) WHERE status = 'active';

CREATE TABLE schedule_line (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    schedule_id INTEGER NOT NULL REFERENCES schedule (id) ON DELETE CASCADE,
    line_no     INTEGER NOT NULL CHECK (line_no >= 1),
    account_id  INTEGER REFERENCES account (id),       -- transfer line
    category_id INTEGER REFERENCES category (id),
    tag_id      INTEGER REFERENCES tag (id),
    amount      INTEGER NOT NULL,                      -- cents, posting sign
    memo        TEXT    NOT NULL DEFAULT '',
    UNIQUE (schedule_id, line_no),
    CHECK ((account_id IS NULL) <> (category_id IS NULL))
) STRICT;

CREATE INDEX schedule_line_account ON schedule_line (account_id) WHERE account_id IS NOT NULL;
CREATE INDEX schedule_line_category ON schedule_line (category_id) WHERE category_id IS NOT NULL;
CREATE INDEX schedule_line_tag ON schedule_line (tag_id) WHERE tag_id IS NOT NULL;

-- ---------------------------------------------------------------------
-- Transactions and postings (TXN-010 … TXN-070, INT-010, TAG-010)
-- ---------------------------------------------------------------------
-- `txn` rather than `transaction`: TRANSACTION is an SQL keyword.

CREATE TABLE txn (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,   -- immutable (TXN-070)
    txn_date        TEXT    NOT NULL CHECK (txn_date IS date(txn_date)),  -- trade date for investments
    payee_id        INTEGER REFERENCES payee (id),
    check_num       TEXT    NOT NULL DEFAULT '',
    memo            TEXT    NOT NULL DEFAULT '',
    notes           TEXT    NOT NULL DEFAULT '',
    status          TEXT    NOT NULL DEFAULT 'normal' CHECK (status IN ('normal', 'void')),
    origin          TEXT    NOT NULL
        CHECK (origin IN ('manual', 'import', 'schedule', 'reconcile', 'system')),
    import_batch_id INTEGER REFERENCES import_batch (id),
    schedule_id     INTEGER REFERENCES schedule (id),
    created_at      TEXT    NOT NULL
        CHECK (created_at IS strftime('%Y-%m-%dT%H:%M:%SZ', created_at)),
    CHECK ((origin = 'import') = (import_batch_id IS NOT NULL)),
    CHECK ((origin = 'schedule') = (schedule_id IS NOT NULL))
) STRICT;

CREATE INDEX txn_date ON txn (txn_date, id);                 -- REG-020 order
CREATE INDEX txn_payee ON txn (payee_id);
CREATE INDEX txn_import_batch ON txn (import_batch_id) WHERE import_batch_id IS NOT NULL;
CREATE INDEX txn_schedule ON txn (schedule_id) WHERE schedule_id IS NOT NULL;

-- Reconciliation (RCN-010 … RCN-060). Defined before posting.
CREATE TABLE reconciliation (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id        INTEGER NOT NULL REFERENCES account (id),
    statement_date    TEXT    NOT NULL CHECK (statement_date IS date(statement_date)),
    opening_balance   INTEGER NOT NULL,       -- prior statement's ending (RCN-030)
    statement_balance INTEGER NOT NULL,
    status            TEXT    NOT NULL DEFAULT 'in_progress'
        CHECK (status IN ('in_progress', 'finished', 'abandoned')),
    started_at        TEXT    NOT NULL
        CHECK (started_at IS strftime('%Y-%m-%dT%H:%M:%SZ', started_at)),
    finished_at       TEXT
        CHECK (finished_at IS strftime('%Y-%m-%dT%H:%M:%SZ', finished_at)),
    CHECK ((status = 'finished') = (finished_at IS NOT NULL))
) STRICT;

CREATE INDEX reconciliation_account ON reconciliation (account_id, statement_date);
-- At most one reconciliation in progress per account (RCN-050).
CREATE UNIQUE INDEX reconciliation_one_open ON reconciliation (account_id)
    WHERE status = 'in_progress';

CREATE TABLE posting (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    txn_id            INTEGER NOT NULL REFERENCES txn (id) ON DELETE CASCADE,
    line_no           INTEGER NOT NULL CHECK (line_no >= 1),
    account_id        INTEGER REFERENCES account (id),
    category_id       INTEGER REFERENCES category (id),
    -- Investment accounts: a posting with security_id carries that
    -- holding's cost basis; one without is the account's cash.
    security_id       INTEGER REFERENCES security (id),
    amount            INTEGER NOT NULL,                  -- cents, signed
    memo              TEXT    NOT NULL DEFAULT '',
    cleared           TEXT    NOT NULL DEFAULT 'unmarked'
        CHECK (cleared IN ('unmarked', 'cleared', 'reconciled')),
    reconciliation_id INTEGER REFERENCES reconciliation (id),
    UNIQUE (txn_id, line_no),
    CHECK ((account_id IS NULL) <> (category_id IS NULL)),
    CHECK (security_id IS NULL OR account_id IS NOT NULL),
    CHECK (account_id IS NOT NULL OR cleared = 'unmarked'),
    -- Imported 'reconciled' status has no reconciliation row (MIG-090).
    CHECK (reconciliation_id IS NULL OR cleared = 'reconciled')
) STRICT;

CREATE INDEX posting_account ON posting (account_id, txn_id) WHERE account_id IS NOT NULL;
CREATE INDEX posting_category ON posting (category_id, txn_id) WHERE category_id IS NOT NULL;
CREATE INDEX posting_security ON posting (security_id) WHERE security_id IS NOT NULL;
CREATE INDEX posting_reconciliation ON posting (reconciliation_id) WHERE reconciliation_id IS NOT NULL;

CREATE TABLE posting_tag (
    posting_id INTEGER NOT NULL REFERENCES posting (id) ON DELETE CASCADE,
    tag_id     INTEGER NOT NULL REFERENCES tag (id),
    PRIMARY KEY (posting_id, tag_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX posting_tag_tag ON posting_tag (tag_id);

-- Occurrences are stored only once acted on: entered, skipped, or
-- edited individually (REC-110). Pending future dates are generated.
CREATE TABLE schedule_occurrence (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    schedule_id    INTEGER NOT NULL REFERENCES schedule (id),
    due_date       TEXT    NOT NULL CHECK (due_date IS date(due_date)),   -- nominal date
    status         TEXT    NOT NULL CHECK (status IN ('pending', 'entered', 'skipped')),
    override_date  TEXT    CHECK (override_date IS date(override_date)),  -- "this occurrence only"
    override_amount INTEGER,
    txn_id         INTEGER UNIQUE REFERENCES txn (id),
    UNIQUE (schedule_id, due_date),
    CHECK ((status = 'entered') = (txn_id IS NOT NULL))
) STRICT;

-- ---------------------------------------------------------------------
-- Investments (INV-010 … INV-310)
-- ---------------------------------------------------------------------
-- One row per investment transaction; the ledger postings for its cash
-- and basis effects live in posting under the same txn_id (INV-040).
-- quantity and price x 10^6; commission cents.

CREATE TABLE investment_txn (
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
        'fee', 'tax_withholding', 'misc_income', 'misc_expense')),
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

CREATE INDEX investment_txn_account ON investment_txn (account_id, security_id);
CREATE INDEX investment_txn_security ON investment_txn (security_id);
CREATE INDEX investment_txn_to_account ON investment_txn (to_account_id) WHERE to_account_id IS NOT NULL;

-- Lots (LOT-010 … LOT-160). Acquisition facts are immutable; the open
-- quantity and basis are derived:
--   open quantity = quantity + Σ lot_adjustment.quantity_delta − Σ lot_disposal.quantity
--   open basis    = cost_basis + Σ lot_adjustment.basis_delta − Σ lot_disposal.basis
-- A partial sale is a disposal, not a physical split of the row (LOT-020).
CREATE TABLE lot (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id     INTEGER NOT NULL REFERENCES account (id),
    security_id    INTEGER NOT NULL REFERENCES security (id),
    acquired_date  TEXT    NOT NULL CHECK (acquired_date IS date(acquired_date)),
    quantity       INTEGER NOT NULL CHECK (quantity > 0),     -- at acquisition, x 10^6
    cost_basis     INTEGER NOT NULL CHECK (cost_basis >= 0),  -- cents, incl. fees
    origin_txn_id  INTEGER NOT NULL REFERENCES txn (id),
    source_lot_id  INTEGER REFERENCES lot (id),               -- share transfer (LOT-140)
    CHECK (source_lot_id IS NULL OR source_lot_id <> id)
) STRICT;

CREATE INDEX lot_position ON lot (account_id, security_id, acquired_date);
CREATE INDEX lot_security ON lot (security_id);
CREATE INDEX lot_origin_txn ON lot (origin_txn_id);
CREATE INDEX lot_source ON lot (source_lot_id) WHERE source_lot_id IS NOT NULL;

-- Shares leaving a lot. kind 'sale' is a realized gain record (LOT-040);
-- sale date comes from txn, acquisition date from lot.
CREATE TABLE lot_disposal (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    lot_id   INTEGER NOT NULL REFERENCES lot (id),
    txn_id   INTEGER NOT NULL REFERENCES txn (id),
    kind     TEXT    NOT NULL CHECK (kind IN ('sale', 'transfer_out', 'removed')),
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    basis    INTEGER NOT NULL CHECK (basis >= 0),
    proceeds INTEGER CHECK (proceeds >= 0),
    gain     INTEGER,
    term     TEXT    CHECK (term IN ('short', 'long')),
    CHECK ((kind = 'sale') = (proceeds IS NOT NULL AND gain IS NOT NULL AND term IS NOT NULL)),
    CHECK (gain IS NULL OR gain = proceeds - basis)
) STRICT;

CREATE INDEX lot_disposal_lot ON lot_disposal (lot_id);
CREATE INDEX lot_disposal_txn ON lot_disposal (txn_id);

-- Changes to an open lot that are not disposals (LOT-120, LOT-130).
CREATE TABLE lot_adjustment (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    lot_id         INTEGER NOT NULL REFERENCES lot (id),
    txn_id         INTEGER NOT NULL REFERENCES txn (id),
    kind           TEXT    NOT NULL CHECK (kind IN ('split', 'return_of_capital')),
    quantity_delta INTEGER NOT NULL DEFAULT 0,
    basis_delta    INTEGER NOT NULL DEFAULT 0,
    CHECK (kind <> 'split' OR (basis_delta = 0 AND quantity_delta <> 0)),
    CHECK (kind <> 'return_of_capital' OR (quantity_delta = 0 AND basis_delta < 0))
) STRICT;

CREATE INDEX lot_adjustment_lot ON lot_adjustment (lot_id);
CREATE INDEX lot_adjustment_txn ON lot_adjustment (txn_id);

-- ---------------------------------------------------------------------
-- Audit log (AUD-010 … AUD-030). Append-only, enforced by triggers.
-- ---------------------------------------------------------------------

CREATE TABLE audit_log (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    at              TEXT    NOT NULL CHECK (at IS strftime('%Y-%m-%dT%H:%M:%SZ', at)),
    entity          TEXT    NOT NULL CHECK (entity IN (
        'account', 'category', 'payee', 'tag', 'txn', 'security', 'price',
        'lot', 'schedule', 'reconciliation', 'import_batch', 'saved_report')),
    entity_id       INTEGER NOT NULL,
    action          TEXT    NOT NULL CHECK (action IN (
        'create', 'update', 'void', 'delete', 'merge', 'close', 'reopen', 'rollback')),
    before_json     TEXT    CHECK (before_json IS NULL OR json_valid(before_json)),
    after_json      TEXT    CHECK (after_json IS NULL OR json_valid(after_json)),
    origin          TEXT    NOT NULL CHECK (origin IN ('ui', 'import', 'scheduler', 'system')),
    import_batch_id INTEGER REFERENCES import_batch (id),
    CHECK (action <> 'create' OR (before_json IS NULL AND after_json IS NOT NULL)),
    CHECK (action <> 'delete' OR (before_json IS NOT NULL AND after_json IS NULL)),
    CHECK (action IN ('create', 'delete') OR (before_json IS NOT NULL AND after_json IS NOT NULL)),
    CHECK ((origin = 'import') = (import_batch_id IS NOT NULL))
) STRICT;

CREATE INDEX audit_log_entity ON audit_log (entity, entity_id, id);
CREATE INDEX audit_log_import_batch ON audit_log (import_batch_id) WHERE import_batch_id IS NOT NULL;

CREATE TRIGGER audit_log_no_update BEFORE UPDATE ON audit_log
BEGIN
    SELECT RAISE(ABORT, 'audit_log is append-only');
END;

CREATE TRIGGER audit_log_no_delete BEFORE DELETE ON audit_log
BEGIN
    SELECT RAISE(ABORT, 'audit_log is append-only');
END;

-- ---------------------------------------------------------------------
-- Saved reports (RPT-020) and settings (SET-070)
-- ---------------------------------------------------------------------

CREATE TABLE saved_report (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT    NOT NULL COLLATE NOCASE UNIQUE CHECK (length(trim(name)) > 0),
    report_type   TEXT    NOT NULL,
    settings_json TEXT    NOT NULL CHECK (json_valid(settings_json)),
    created_at    TEXT    NOT NULL
        CHECK (created_at IS strftime('%Y-%m-%dT%H:%M:%SZ', created_at)),
    updated_at    TEXT    NOT NULL
        CHECK (updated_at IS strftime('%Y-%m-%dT%H:%M:%SZ', updated_at))
) STRICT;

CREATE TABLE setting (
    key   TEXT NOT NULL PRIMARY KEY CHECK (length(key) > 0),
    value TEXT NOT NULL
) STRICT, WITHOUT ROWID;

-- ---------------------------------------------------------------------
-- Integrity views (INT-030). Read-only helpers; also useful from
-- DB Browser for SQLite.
-- ---------------------------------------------------------------------

-- Transactions whose postings do not sum to zero. Must always be empty.
CREATE VIEW unbalanced_txn AS
    SELECT txn_id, sum(amount) AS total
    FROM posting
    GROUP BY txn_id
    HAVING sum(amount) <> 0;

-- ---------------------------------------------------------------------
-- Seed data: built-in categories (CAT-060, RCN-040)
-- ---------------------------------------------------------------------

INSERT INTO category (kind, name, system_key, tax_related, created_at) VALUES
    ('income',  'Dividends',                      'dividends',            1, '1970-01-01T00:00:00Z'),
    ('income',  'Interest',                       'interest',             1, '1970-01-01T00:00:00Z'),
    ('income',  'Capital Gains Distribution ST',  'cg_dist_short',        1, '1970-01-01T00:00:00Z'),
    ('income',  'Capital Gains Distribution LT',  'cg_dist_long',         1, '1970-01-01T00:00:00Z'),
    ('income',  'Realized Gain/Loss',             'realized_gain',        1, '1970-01-01T00:00:00Z'),
    ('income',  'Investment Misc Income',         'investment_income',    0, '1970-01-01T00:00:00Z'),
    ('expense', 'Investment Fees',                'investment_fees',      1, '1970-01-01T00:00:00Z'),
    ('expense', 'Investment Misc Expense',        'investment_expense',   0, '1970-01-01T00:00:00Z'),
    ('expense', 'Tax Withheld',                   'tax_withheld',         1, '1970-01-01T00:00:00Z'),
    ('expense', 'Balance Adjustment',             'balance_adjustment',   0, '1970-01-01T00:00:00Z'),
    ('equity',  'Opening Balance',                'opening_balance',      0, '1970-01-01T00:00:00Z');
