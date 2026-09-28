-- Kansha schema, migration 0003: tax lines (CAT-050, RPT-140, RPT-145).
--
-- A tax line is one line of a tax form ("Schedule A" / "Real estate
-- taxes"). Categories map to one line; an account maps transfers out of
-- it (money leaving, e.g. an IRA distribution) and transfers into it
-- (e.g. an HSA contribution) to one line each. The Tax Schedule report
-- groups by form and line; Tax Summary shows the line beside each item.
--
-- Realized gains are not mapped through a category: the Tax Schedule
-- report builds Schedule D from lot disposals, split by holding period.

CREATE TABLE tax_line (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    form       TEXT    NOT NULL CHECK (length(trim(form)) > 0),
    line       TEXT    NOT NULL CHECK (length(trim(line)) > 0),
    sort_order INTEGER NOT NULL,
    UNIQUE (form, line)
) STRICT;

ALTER TABLE category ADD COLUMN tax_line_id INTEGER REFERENCES tax_line (id);
ALTER TABLE account ADD COLUMN tax_line_out_id INTEGER REFERENCES tax_line (id);
ALTER TABLE account ADD COLUMN tax_line_in_id INTEGER REFERENCES tax_line (id);

CREATE INDEX category_tax_line ON category (tax_line_id) WHERE tax_line_id IS NOT NULL;

-- Forms in return order; lines in form order. Names follow Quicken's
-- tax line names so imported mappings read the same.
INSERT INTO tax_line (form, line, sort_order) VALUES
    ('W-2',         'Salary or wages',                    100),
    ('W-2',         'Federal tax withheld',               110),
    ('W-2',         'State tax withheld',                 120),
    ('W-2',         'Social Security tax withheld',       130),
    ('W-2',         'Medicare tax withheld',              140),
    ('Form 1040',   'Other income, misc.',                200),
    ('Form 1040',   'Fed. estimated tax, quarterly',      210),
    ('Form 1040',   'IRA contribution, self',             220),
    ('SSA-1099',    'Net social security benefits',       300),
    ('SSA-1099',    'Federal tax withheld',               310),
    ('1099-R',      'Total IRA taxable distrib.',         400),
    ('1099-R',      'IRA federal tax withheld',           410),
    ('1099-R',      'IRA state tax withheld',             420),
    ('1099-R',      'Total pension taxable distrib.',     430),
    ('1099-R',      'Pension federal tax withheld',       440),
    ('1099-R',      'Pension state tax withheld',         450),
    ('1099-G',      'State and local tax refunds',        500),
    ('1099-G',      'Unemployment compensation',          510),
    ('Schedule A',  'Medicine and drugs',                 600),
    ('Schedule A',  'Doctors, dentists, hospitals',       610),
    ('Schedule A',  'Medical insurance premiums',         620),
    ('Schedule A',  'State income taxes',                 630),
    ('Schedule A',  'State estimated tax, quarterly',     640),
    ('Schedule A',  'Real estate taxes',                  650),
    ('Schedule A',  'Personal property taxes',            660),
    ('Schedule A',  'Home mortgage interest (1098)',      670),
    ('Schedule A',  'Cash charity contributions',         680),
    ('Schedule A',  'Non-cash charity contributions',     690),
    ('Schedule B',  'Interest income',                    700),
    ('Schedule B',  'Dividend income',                    710),
    ('1099-DIV',    'Qualified dividends',                800),
    ('1099-DIV',    'Total capital gain distr.',          810),
    ('1099-DIV',    'Foreign tax paid',                   820),
    ('1099-SA',     'HSA distributions',                  900),
    ('Form 8889',   'HSA contributions',                  910);

-- Built-in investment income categories map by default. Short-term
-- capital gain distributions are taxed as ordinary dividends.
UPDATE category SET tax_line_id =
    (SELECT id FROM tax_line WHERE form = 'Schedule B' AND line = 'Interest income')
    WHERE system_key = 'interest';
UPDATE category SET tax_line_id =
    (SELECT id FROM tax_line WHERE form = 'Schedule B' AND line = 'Dividend income')
    WHERE system_key IN ('dividends', 'cg_dist_short');
UPDATE category SET tax_line_id =
    (SELECT id FROM tax_line WHERE form = '1099-DIV' AND line = 'Total capital gain distr.')
    WHERE system_key = 'cg_dist_long';
