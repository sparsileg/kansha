-- Kansha schema, migration 0017: a schedule's amount estimated from its
-- last payments (REC-065).
--
-- average_of is how many of the schedule's latest entered payments its
-- amount is the average of; NULL (every existing schedule) keeps the
-- amount as typed.

ALTER TABLE schedule ADD COLUMN average_of INTEGER
    CHECK (average_of IS NULL OR average_of BETWEEN 1 AND 99);
