-- Kansha schema, migration 0004: average cost (LOT-110).
--
-- An average-cost sale first evens out the basis of every open lot of
-- the holding to its share of the total (by shares), so each share
-- carries the average; shares then leave oldest first. Those changes are
-- lot adjustments of kind 'average': basis only, and they sum to zero
-- within a transaction, so the holding's basis does not change.
--
-- SQLite cannot change a CHECK constraint in place: the table is rebuilt
-- with the same columns, rows, and indexes. Nothing references it.

CREATE TABLE lot_adjustment_new (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    lot_id         INTEGER NOT NULL REFERENCES lot (id),
    txn_id         INTEGER NOT NULL REFERENCES txn (id),
    kind           TEXT    NOT NULL CHECK (kind IN ('split', 'return_of_capital', 'average')),
    quantity_delta INTEGER NOT NULL DEFAULT 0,
    basis_delta    INTEGER NOT NULL DEFAULT 0,
    CHECK (kind <> 'split' OR (basis_delta = 0 AND quantity_delta <> 0)),
    CHECK (kind <> 'return_of_capital' OR (quantity_delta = 0 AND basis_delta < 0)),
    CHECK (kind <> 'average' OR (quantity_delta = 0 AND basis_delta <> 0))
) STRICT;

INSERT INTO lot_adjustment_new (id, lot_id, txn_id, kind, quantity_delta, basis_delta)
    SELECT id, lot_id, txn_id, kind, quantity_delta, basis_delta FROM lot_adjustment;

DROP TABLE lot_adjustment;
ALTER TABLE lot_adjustment_new RENAME TO lot_adjustment;

CREATE INDEX lot_adjustment_lot ON lot_adjustment (lot_id);
CREATE INDEX lot_adjustment_txn ON lot_adjustment (txn_id);
