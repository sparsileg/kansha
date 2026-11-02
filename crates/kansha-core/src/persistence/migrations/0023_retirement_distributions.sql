-- Kansha schema, migration 0023: the Retirement Distributions category
-- (CAT-060, BUD-030).
--
-- Money leaving a tax-deferred or tax-exempt account for one of your own
-- accounts is income. It is a transfer in the ledger; reports list it
-- under this built-in category, which no posting needs to use.
--
-- A book may already have an income category of this name at the top
-- level: it becomes the built-in one. Any other category of this name
-- keeps it, and the built-in one takes a name of its own.

UPDATE category SET system_key = 'retirement_distributions'
WHERE parent_id IS NULL AND name = 'Retirement Distributions'
    AND kind = 'income' AND system_key IS NULL;

INSERT INTO category (kind, name, system_key, tax_related, created_at)
SELECT 'income',
    CASE WHEN EXISTS (SELECT 1 FROM category
                      WHERE parent_id IS NULL AND name = 'Retirement Distributions')
        THEN 'Retirement Distributions (built-in)'
        ELSE 'Retirement Distributions' END,
    'retirement_distributions', 0, '1970-01-01T00:00:00Z'
WHERE NOT EXISTS (SELECT 1 FROM category WHERE system_key = 'retirement_distributions');
