-- Kansha schema, migration 0022: spending card tags and columns
-- (CARD-060, CARD-070).
--
-- spending_card gains:
-- 1. tags: a JSON list of tag IDs. Empty counts every line; otherwise a
--    line counts when it has any listed tag.
-- 2. columns: a JSON list of the columns shown, in display order. Every
--    existing card keeps the three it has always shown.

ALTER TABLE spending_card ADD COLUMN tags TEXT NOT NULL DEFAULT '[]'
    CHECK (json_valid(tags) AND json_type(tags) = 'array');

ALTER TABLE spending_card ADD COLUMN columns TEXT NOT NULL
    DEFAULT '["year","month","monthly_avg"]'
    CHECK (json_valid(columns) AND json_type(columns) = 'array'
        AND json_array_length(columns) > 0);
