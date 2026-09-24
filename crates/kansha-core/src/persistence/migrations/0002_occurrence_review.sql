-- Auto-entered occurrences are flagged for the user's review (REC-070).
-- The flag clears when the user dismisses it.

ALTER TABLE schedule_occurrence
    ADD COLUMN needs_review INTEGER NOT NULL DEFAULT 0 CHECK (needs_review IN (0, 1));

CREATE INDEX schedule_occurrence_review ON schedule_occurrence (id) WHERE needs_review = 1;
