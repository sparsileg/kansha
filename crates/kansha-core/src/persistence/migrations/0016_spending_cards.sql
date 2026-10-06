-- Kansha schema, migration 0016: spending cards (CARD-060).
--
-- 1. spending_card: named cards, each listing its chosen spending
--    categories over its chosen accounts. accounts is a JSON list of
--    account IDs, or NULL for every open account (never customized);
--    categories is a JSON list of category IDs. An insight shows one by
--    the card ID 'spending:<id>'.
-- 2. The Auto Expenses card (the settings auto_accounts and
--    auto_categories, card ID 'auto_expenses') becomes the spending card
--    "Auto Expenses", if it was customized or is on an insight; each
--    insight showing it shows the new card in its place. The settings
--    are removed.
-- 3. audit_log accepts the entity 'spending_card'. The table is rebuilt
--    for the CHECK, with its indexes and append-only triggers.
--
-- No table references audit_log, so foreign keys stay on.

CREATE TABLE spending_card (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL COLLATE NOCASE UNIQUE CHECK (length(trim(name)) > 0),
    accounts   TEXT    CHECK (accounts IS NULL
        OR (json_valid(accounts) AND json_type(accounts) = 'array')),
    categories TEXT    NOT NULL
        CHECK (json_valid(categories) AND json_type(categories) = 'array'),
    created_at TEXT    NOT NULL
        CHECK (created_at IS strftime('%Y-%m-%dT%H:%M:%SZ', created_at))
) STRICT;

INSERT INTO spending_card (name, accounts, categories, created_at)
    SELECT 'Auto Expenses',
        (SELECT value FROM setting WHERE key = 'auto_accounts'
            AND json_valid(value) AND json_type(value) = 'array'),
        COALESCE((SELECT value FROM setting WHERE key = 'auto_categories'
            AND json_valid(value) AND json_type(value) = 'array'), '[]'),
        '1970-01-01T00:00:00Z'
    WHERE EXISTS (SELECT 1 FROM setting WHERE key IN ('auto_accounts', 'auto_categories'))
        OR EXISTS (SELECT 1 FROM insight, json_each(insight.cards) j
            WHERE j.value = 'auto_expenses');

UPDATE insight SET cards = (
        SELECT json_group_array(CASE WHEN j.value = 'auto_expenses'
                THEN 'spending:' || (SELECT max(id) FROM spending_card)
                ELSE j.value END)
        FROM (SELECT value FROM json_each(insight.cards) ORDER BY key) j)
    WHERE EXISTS (SELECT 1 FROM json_each(insight.cards) WHERE value = 'auto_expenses');

DELETE FROM setting WHERE key IN ('auto_accounts', 'auto_categories');

CREATE TABLE audit_log_new (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    at              TEXT    NOT NULL CHECK (at IS strftime('%Y-%m-%dT%H:%M:%SZ', at)),
    entity          TEXT    NOT NULL CHECK (entity IN (
        'account', 'category', 'payee', 'tag', 'txn', 'security', 'price',
        'lot', 'schedule', 'reconciliation', 'import_batch', 'saved_report',
        'report_folder', 'insight', 'spending_card')),
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
