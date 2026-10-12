-- Kansha schema, migration 0019: pending entries (CLI-040, D-180).
--
-- 1. entry_file: each file of entries received (the inbox, File >
--    Import entries…), with its sender and where its encrypted copy is.
-- 2. pending_entry, pending_split: the entries, outside the ledger
--    until approved. Names are kept as text and resolved at review, as
--    categories and accounts may change in between. uid is the entry's
--    id: approved and rejected rows stay, so a file sent twice adds
--    nothing. An approved entry points to its transaction; deleting
--    that transaction later leaves the entry approved, pointing nowhere.
-- 3. audit_log accepts the entity 'pending_entry'. The table is rebuilt
--    for the CHECK, with its indexes and append-only triggers.
--
-- No table references audit_log, so foreign keys stay on.

CREATE TABLE entry_file (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    file_name    TEXT    NOT NULL,
    sha256       TEXT    NOT NULL CHECK (length(sha256) = 64),
    sender       TEXT    NOT NULL DEFAULT '',
    source       TEXT    NOT NULL CHECK (source IN ('cli', 'file')),
    received_at  TEXT    NOT NULL
        CHECK (received_at IS strftime('%Y-%m-%dT%H:%M:%SZ', received_at)),
    archive_path TEXT,
    entry_count  INTEGER NOT NULL CHECK (entry_count >= 0)
) STRICT;

CREATE TABLE pending_entry (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id       INTEGER NOT NULL REFERENCES entry_file (id),
    line_no       INTEGER NOT NULL,
    uid           TEXT    NOT NULL UNIQUE CHECK (length(uid) = 36),
    txn_date      TEXT    NOT NULL CHECK (txn_date IS date(txn_date)),
    payee         TEXT    NOT NULL CHECK (length(trim(payee)) > 0),
    amount        INTEGER NOT NULL CHECK (amount <> 0),
    account_name  TEXT    NOT NULL,
    category_name TEXT,                                    -- NULL: a split
    memo          TEXT    NOT NULL DEFAULT '',
    status        TEXT    NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'approved', 'rejected')),
    txn_id        INTEGER REFERENCES txn (id) ON DELETE SET NULL,
    decided_at    TEXT
        CHECK (decided_at IS strftime('%Y-%m-%dT%H:%M:%SZ', decided_at)),
    CHECK ((status = 'pending') = (decided_at IS NULL)),
    CHECK (txn_id IS NULL OR status = 'approved')
) STRICT;

CREATE INDEX pending_entry_status ON pending_entry (status, id);
CREATE INDEX pending_entry_file ON pending_entry (file_id);
CREATE INDEX pending_entry_txn ON pending_entry (txn_id) WHERE txn_id IS NOT NULL;

CREATE TABLE pending_split (
    entry_id      INTEGER NOT NULL REFERENCES pending_entry (id) ON DELETE CASCADE,
    line_no       INTEGER NOT NULL,
    amount        INTEGER NOT NULL CHECK (amount <> 0),
    category_name TEXT    NOT NULL,
    memo          TEXT    NOT NULL DEFAULT '',
    PRIMARY KEY (entry_id, line_no)
) STRICT;

CREATE TABLE audit_log_new (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    at              TEXT    NOT NULL CHECK (at IS strftime('%Y-%m-%dT%H:%M:%SZ', at)),
    entity          TEXT    NOT NULL CHECK (entity IN (
        'account', 'category', 'payee', 'tag', 'txn', 'security', 'price',
        'lot', 'schedule', 'reconciliation', 'import_batch', 'saved_report',
        'report_folder', 'insight', 'spending_card', 'pending_entry')),
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
