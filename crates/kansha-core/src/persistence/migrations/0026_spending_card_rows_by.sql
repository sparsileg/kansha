-- Kansha schema, migration 0026: spending card rows by tag (CARD-080,
-- #68).
--
-- spending_card gains rows_by: 'category' (one row per spending
-- category, as before; every existing card) or 'tag' (one row per tag
-- group, TAG-030).

ALTER TABLE spending_card ADD COLUMN rows_by TEXT NOT NULL DEFAULT 'category'
    CHECK (rows_by IN ('category', 'tag'));
