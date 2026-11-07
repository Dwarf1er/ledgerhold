-- Accounts whose balance is tracked only by periodic snapshots (anchors), e.g. an investment
-- account valued once a month. Reconciliation is skipped for them: with no transactions between
-- two snapshots, the change is market movement, not a data error.
ALTER TABLE account ADD COLUMN balance_only INTEGER NOT NULL DEFAULT 0;
