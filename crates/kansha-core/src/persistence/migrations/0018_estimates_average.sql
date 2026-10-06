-- Kansha schema, migration 0018: estimated amounts become averages
-- (REC-060, REC-065).
--
-- The schedule dialog no longer offers a typed estimate: an amount is
-- fixed, or the average of the last N payments. Each estimated,
-- one-line, Remind schedule (not deleted) now averages its last 3
-- payments, and its amount is set to that average as the engine works
-- it out: the schedule's entered, non-void payments in its account,
-- latest occurrence first; the sum over their count, rounded half to
-- even; 0.00 with no payments, or when the average goes the other way
-- from the schedule's direction. schedule_line.amount is in posting
-- sign, the negative of the register amount. Estimated splits and
-- auto-entry schedules are left as they are.

UPDATE schedule SET average_of = 3
WHERE amount_type = 'estimated' AND mode = 'remind' AND status <> 'deleted'
  AND average_of IS NULL
  AND (SELECT count(*) FROM schedule_line l WHERE l.schedule_id = schedule.id) = 1;

WITH pay AS (
    SELECT o.schedule_id AS sid, sum(p.amount) AS amt,
           row_number() OVER (PARTITION BY o.schedule_id ORDER BY o.due_date DESC) AS rn
    FROM schedule_occurrence o
    JOIN schedule s ON s.id = o.schedule_id
    JOIN txn t ON t.id = o.txn_id AND t.status = 'normal'
    JOIN posting p ON p.txn_id = t.id AND p.account_id = s.account_id AND p.security_id IS NULL
    WHERE o.status = 'entered' AND s.average_of = 3 AND s.amount_type = 'estimated'
    GROUP BY o.id
),
tot AS (
    SELECT sid, sum(amt) AS total, count(*) AS n FROM pay WHERE rn <= 3 GROUP BY sid
),
avg AS (
    -- Integer division truncates toward zero; the remainder has the
    -- sum's sign. Round half to even.
    SELECT sid,
           CASE
               WHEN 2 * abs(total % n) > n OR (2 * abs(total % n) = n AND (total / n) % 2 <> 0)
                   THEN total / n + (CASE WHEN total < 0 THEN -1 ELSE 1 END)
               ELSE total / n
           END AS value
    FROM tot
)
UPDATE schedule_line
SET amount = coalesce(-(SELECT value FROM avg WHERE avg.sid = schedule_line.schedule_id), 0)
WHERE schedule_id IN (SELECT id FROM schedule WHERE average_of = 3 AND amount_type = 'estimated');

-- An average going the other way is 0.00 (a payment's line is >= 0).
UPDATE schedule_line SET amount = 0
WHERE schedule_id IN (
    SELECT id FROM schedule
    WHERE average_of IS NOT NULL
      AND ((direction = 'payment' AND schedule_line.amount < 0)
        OR (direction = 'deposit' AND schedule_line.amount > 0))
);
