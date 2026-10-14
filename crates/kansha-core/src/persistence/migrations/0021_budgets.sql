-- Kansha schema, migration 0021: budgets (BUD-010).
--
-- 1. budget: a named budget; names are unique in any case.
-- 2. budget_year: the whole years a budget holds.
-- 3. budget_category: the income and expense categories it budgets,
--    at any level.
-- 4. budget_amount: one amount per category and month of a held year,
--    in cents, signed (a refund category may have a negative budget).
--    A zero amount is not stored; missing = 0.
-- 5. audit_log accepts the entity 'budget'. The table is rebuilt for
--    the CHECK, with its indexes and append-only triggers.
--
-- No table references audit_log, so foreign keys stay on.

CREATE TABLE budget (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL UNIQUE COLLATE NOCASE CHECK (length(trim(name)) > 0),
    created_at TEXT    NOT NULL
        CHECK (created_at IS strftime('%Y-%m-%dT%H:%M:%SZ', created_at))
) STRICT;

CREATE TABLE budget_year (
    budget_id INTEGER NOT NULL REFERENCES budget (id) ON DELETE CASCADE,
    year      INTEGER NOT NULL CHECK (year BETWEEN 1900 AND 9999),
    PRIMARY KEY (budget_id, year)
) STRICT;

CREATE TABLE budget_category (
    budget_id   INTEGER NOT NULL REFERENCES budget (id) ON DELETE CASCADE,
    category_id INTEGER NOT NULL REFERENCES category (id),
    PRIMARY KEY (budget_id, category_id)
) STRICT;

CREATE INDEX budget_category_category ON budget_category (category_id);

CREATE TABLE budget_amount (
    budget_id   INTEGER NOT NULL,
    year        INTEGER NOT NULL,
    category_id INTEGER NOT NULL,
    month       INTEGER NOT NULL CHECK (month BETWEEN 1 AND 12),
    amount      INTEGER NOT NULL CHECK (amount <> 0),
    PRIMARY KEY (budget_id, year, category_id, month),
    FOREIGN KEY (budget_id, year)
        REFERENCES budget_year (budget_id, year) ON DELETE CASCADE,
    FOREIGN KEY (budget_id, category_id)
        REFERENCES budget_category (budget_id, category_id) ON DELETE CASCADE
) STRICT;

CREATE INDEX budget_amount_category ON budget_amount (budget_id, category_id);

CREATE TABLE audit_log_new (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    at              TEXT    NOT NULL CHECK (at IS strftime('%Y-%m-%dT%H:%M:%SZ', at)),
    entity          TEXT    NOT NULL CHECK (entity IN (
        'account', 'category', 'payee', 'tag', 'txn', 'security', 'price',
        'lot', 'schedule', 'reconciliation', 'import_batch', 'saved_report',
        'report_folder', 'insight', 'spending_card', 'pending_entry', 'loan', 'budget')),
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

INSERT INTO audit_log_new (id, at, entity, entity_id, action, before_json, after_json, origin,
        import_batch_id)
    SELECT id, at, entity, entity_id, action, before_json, after_json, origin, import_batch_id
    FROM audit_log;

DROP TABLE audit_log;
ALTER TABLE audit_log_new RENAME TO audit_log;

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
