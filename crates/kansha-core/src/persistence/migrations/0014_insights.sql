-- Kansha schema, migration 0014: insights (INS-010 … INS-040).
--
-- 1. insight: named views of dashboard cards, shown as tabs in the order
--    of position. cards is the UI's JSON list of card IDs, in order.
-- 2. The dashboard's card choice (the setting dashboard_cards, DSH-040)
--    becomes the first insight, "Dashboard": the cards it showed, in its
--    order; cards it did not list show after them, as they did. The
--    setting is removed. The IDs below are the cards of this release.
-- 3. audit_log accepts the entity 'insight'. The table is rebuilt for the
--    CHECK, with its indexes and append-only triggers.
--
-- No table references audit_log, so foreign keys stay on.

CREATE TABLE insight (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL COLLATE NOCASE UNIQUE CHECK (length(trim(name)) > 0),
    position   INTEGER NOT NULL,
    cards      TEXT    NOT NULL CHECK (json_valid(cards) AND json_type(cards) = 'array'),
    created_at TEXT    NOT NULL
        CHECK (created_at IS strftime('%Y-%m-%dT%H:%M:%SZ', created_at))
) STRICT;

WITH
    card (k, id) AS (VALUES
        (0, 'net_worth'), (1, 'this_month'), (2, 'net_worth_trend'),
        (3, 'upcoming'), (4, 'attention')),
    stored (v) AS (
        SELECT value FROM setting
        WHERE key = 'dashboard_cards' AND json_valid(value) AND json_type(value) = 'object'),
    listed (k, id) AS (
        SELECT min(j.key), j.value FROM stored, json_each(stored.v, '$.order') j
        WHERE j.type = 'text' AND j.value IN (SELECT id FROM card)
        GROUP BY j.value),
    hidden (id) AS (
        SELECT j.value FROM stored, json_each(stored.v, '$.hidden') j WHERE j.type = 'text'),
    shown (k, id) AS (
        SELECT k, id FROM listed
        UNION ALL
        SELECT 1000 + k, id FROM card WHERE id NOT IN (SELECT id FROM listed))
INSERT INTO insight (name, position, cards, created_at)
    SELECT 'Dashboard', 1,
        (SELECT json_group_array(id) FROM
            (SELECT id FROM shown WHERE id NOT IN (SELECT id FROM hidden) ORDER BY k)),
        '1970-01-01T00:00:00Z';

DELETE FROM setting WHERE key = 'dashboard_cards';

CREATE TABLE audit_log_new (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    at              TEXT    NOT NULL CHECK (at IS strftime('%Y-%m-%dT%H:%M:%SZ', at)),
    entity          TEXT    NOT NULL CHECK (entity IN (
        'account', 'category', 'payee', 'tag', 'txn', 'security', 'price',
        'lot', 'schedule', 'reconciliation', 'import_batch', 'saved_report',
        'report_folder', 'insight')),
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
