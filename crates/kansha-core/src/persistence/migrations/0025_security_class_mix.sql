-- Kansha schema, migration 0025: a security may be split across several
-- asset classes (SEC-050), such as a balanced or target-date fund.
--
-- One current mix per security, in percent ×10^6 with at most two
-- decimals; the shares of a security total 100 (checked in Rust). A
-- security with no rows here counts wholly in its asset_class, which
-- for a mixed one is the class of its largest share.

CREATE TABLE security_class_mix (
    security_id INTEGER NOT NULL REFERENCES security (id),
    asset_class TEXT    NOT NULL CHECK (asset_class IN (
        'us_equity', 'us_small_mid', 'intl_equity', 'emerging',
        'bond_short', 'bond_intermediate', 'bond_long', 'bond_intl',
        'bond_tips', 'bond_muni', 'cash', 'real_estate', 'commodity', 'other')),
    percent     INTEGER NOT NULL CHECK (percent > 0 AND percent < 100000000
        AND percent % 10000 = 0),
    PRIMARY KEY (security_id, asset_class)
) STRICT, WITHOUT ROWID;
