-- Kansha schema, migration 0013: saved report folders (RPT-020).
--
-- 1. report_folder: named folders for saved reports, one level, one
--    folder per report. Folder 1, "Unfiled", is permanent (never renamed
--    or deleted); every existing and new saved report starts there.
-- 2. saved_report gains folder_id. ALTER TABLE cannot add a NOT NULL
--    reference with a default, so the table is rebuilt.
-- 3. audit_log accepts the entity 'report_folder'. The table is rebuilt
--    for the CHECK, with its indexes and append-only triggers.
--
-- No table references saved_report or audit_log, so foreign keys stay on.

CREATE TABLE report_folder (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL COLLATE NOCASE UNIQUE CHECK (length(trim(name)) > 0),
    created_at TEXT    NOT NULL
        CHECK (created_at IS strftime('%Y-%m-%dT%H:%M:%SZ', created_at))
) STRICT;

INSERT INTO report_folder (id, name, created_at)
    VALUES (1, 'Unfiled', '1970-01-01T00:00:00Z');

CREATE TABLE saved_report_new (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT    NOT NULL COLLATE NOCASE UNIQUE CHECK (length(trim(name)) > 0),
    report_type   TEXT    NOT NULL,
    settings_json TEXT    NOT NULL CHECK (json_valid(settings_json)),
    created_at    TEXT    NOT NULL
        CHECK (created_at IS strftime('%Y-%m-%dT%H:%M:%SZ', created_at)),
    updated_at    TEXT    NOT NULL
        CHECK (updated_at IS strftime('%Y-%m-%dT%H:%M:%SZ', updated_at)),
    folder_id     INTEGER NOT NULL DEFAULT 1 REFERENCES report_folder (id)
) STRICT;

INSERT INTO saved_report_new (id, name, report_type, settings_json, created_at, updated_at)
    SELECT id, name, report_type, settings_json, created_at, updated_at FROM saved_report;

DROP TABLE saved_report;
ALTER TABLE saved_report_new RENAME TO saved_report;

CREATE INDEX saved_report_folder ON saved_report (folder_id);

CREATE TABLE audit_log_new (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    at              TEXT    NOT NULL CHECK (at IS strftime('%Y-%m-%dT%H:%M:%SZ', at)),
    entity          TEXT    NOT NULL CHECK (entity IN (
        'account', 'category', 'payee', 'tag', 'txn', 'security', 'price',
        'lot', 'schedule', 'reconciliation', 'import_batch', 'saved_report',
        'report_folder')),
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
