-- Kansha schema, migration 0020: loans (LOAN-090).
--
-- 1. loan: the terms of a Loan/Mortgage account, one per account. A
--    NULL payment is worked out from the terms (LOAN-025); a number is
--    the lender's figure, as typed. day1 and day2 are twice a month's
--    two days. schedule is the payment reminder the loan owns
--    (LOAN-060). A closed account keeps its terms.
-- 2. loan_rate: the rate history (LOAN-050), percent × 10^6; the first
--    takes effect on the opening date.
-- 3. loan_other: fees, escrow, and insurance paid with each payment.
-- 4. audit_log accepts the entity 'loan'. The table is rebuilt for the
--    CHECK, with its indexes and append-only triggers.
--
-- No table references audit_log, so foreign keys stay on.

CREATE TABLE loan (
    account_id           INTEGER PRIMARY KEY REFERENCES account (id),
    loan_type            TEXT    NOT NULL CHECK (loan_type IN (
        'mortgage', 'auto', 'consumer', 'student', 'commercial', 'business',
        'construction', 'home_equity')),
    opening_date         TEXT    NOT NULL CHECK (opening_date IS date(opening_date)),
    original_balance     INTEGER NOT NULL CHECK (original_balance > 0),
    length_months        INTEGER NOT NULL CHECK (length_months BETWEEN 1 AND 600),
    compounding          TEXT    NOT NULL CHECK (compounding IN (
        'daily', 'monthly', 'semi_annually')),
    frequency            TEXT    NOT NULL CHECK (frequency IN (
        'twice_per_year', 'quarterly', 'every_other_month', 'monthly',
        'twice_per_month', 'every_other_week', 'weekly')),
    day1                 INTEGER CHECK (day1 BETWEEN 1 AND 31),
    day2                 INTEGER CHECK (day2 BETWEEN 1 AND 31),
    payment              INTEGER CHECK (payment > 0),
    extra_principal      INTEGER NOT NULL DEFAULT 0 CHECK (extra_principal >= 0),
    lender_id            INTEGER REFERENCES payee (id),
    paying_account_id    INTEGER NOT NULL REFERENCES account (id),
    interest_category_id INTEGER NOT NULL REFERENCES category (id),
    schedule_id          INTEGER UNIQUE REFERENCES schedule (id),
    CHECK ((frequency = 'twice_per_month') = (day1 IS NOT NULL AND day2 IS NOT NULL)),
    CHECK (frequency = 'twice_per_month' OR (day1 IS NULL AND day2 IS NULL)),
    CHECK (day2 IS NULL OR day1 < day2),
    CHECK (paying_account_id <> account_id)
) STRICT;

CREATE INDEX loan_paying_account ON loan (paying_account_id);
CREATE INDEX loan_interest_category ON loan (interest_category_id);
CREATE INDEX loan_lender ON loan (lender_id) WHERE lender_id IS NOT NULL;

CREATE TABLE loan_rate (
    loan_id        INTEGER NOT NULL REFERENCES loan (account_id) ON DELETE CASCADE,
    effective_date TEXT    NOT NULL CHECK (effective_date IS date(effective_date)),
    rate           INTEGER NOT NULL CHECK (rate BETWEEN 0 AND 100000000),
    PRIMARY KEY (loan_id, effective_date)
) STRICT;

CREATE TABLE loan_other (
    loan_id     INTEGER NOT NULL REFERENCES loan (account_id) ON DELETE CASCADE,
    line_no     INTEGER NOT NULL CHECK (line_no >= 1),
    category_id INTEGER NOT NULL REFERENCES category (id),
    amount      INTEGER NOT NULL CHECK (amount > 0),
    PRIMARY KEY (loan_id, line_no)
) STRICT;

CREATE INDEX loan_other_category ON loan_other (category_id);

CREATE TABLE audit_log_new (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    at              TEXT    NOT NULL CHECK (at IS strftime('%Y-%m-%dT%H:%M:%SZ', at)),
    entity          TEXT    NOT NULL CHECK (entity IN (
        'account', 'category', 'payee', 'tag', 'txn', 'security', 'price',
        'lot', 'schedule', 'reconciliation', 'import_batch', 'saved_report',
        'report_folder', 'insight', 'spending_card', 'pending_entry', 'loan')),
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
