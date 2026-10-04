-- Kansha schema, migration 0015: the first insight is named "Status"
-- (INS-010).
--
-- Migration 0014 named the insight it made "Dashboard". That insight
-- is renamed "Status" unless the user has renamed it already or has
-- another insight named "Status" (names are unique, any case). It is
-- known by the creation time 0014 gave it. Nothing else changes.

UPDATE insight SET name = 'Status'
WHERE name = 'Dashboard'
    AND created_at = '1970-01-01T00:00:00Z'
    AND NOT EXISTS (SELECT 1 FROM insight WHERE name = 'Status');
