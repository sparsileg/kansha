-- Kansha schema, migration 0009: a schedule's transaction type (REC-010).
--
-- Payment or deposit was only the sign of the lines, so a schedule whose
-- amount is 0.00 (a bill estimated when it comes in) lost it: the list
-- showed it as a deposit, and an amount typed on entry went in as one.
-- The type is now stored. Existing schedules take it from the sign of
-- their amount; 0.00 ones become payments.
--
-- schedule_line.amount is in posting sign, so a payment's lines sum to
-- more than zero and a deposit's to less.

ALTER TABLE schedule ADD COLUMN direction TEXT NOT NULL DEFAULT 'payment'
    CHECK (direction IN ('payment', 'deposit'));

UPDATE schedule SET direction = 'deposit'
WHERE (SELECT sum(amount) FROM schedule_line WHERE schedule_id = schedule.id) < 0;
