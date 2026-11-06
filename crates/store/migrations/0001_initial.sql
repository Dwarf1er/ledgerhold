-- Money is INTEGER minor units (cents). Dates are ISO 'YYYY-MM-DD' TEXT so that
-- ordering and BETWEEN work. Ids are UUIDv7 TEXT so devices could sync later.

CREATE TABLE person (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE institution (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    ofx_bank_id TEXT,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE account (
    id                  TEXT PRIMARY KEY,
    institution_id      TEXT NOT NULL REFERENCES institution(id),
    name                TEXT NOT NULL,
    kind                TEXT NOT NULL CHECK (kind IN ('checking','savings','credit_card')),
    currency            TEXT NOT NULL,
    external_account_id TEXT,
    archived            INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE UNIQUE INDEX account_external
    ON account(institution_id, external_account_id) WHERE external_account_id IS NOT NULL;

-- Many-to-many: a joint account has several owners.
CREATE TABLE account_owner (
    account_id TEXT NOT NULL REFERENCES account(id) ON DELETE CASCADE,
    person_id  TEXT NOT NULL REFERENCES person(id)  ON DELETE CASCADE,
    PRIMARY KEY (account_id, person_id)
);

-- "Balance at END of day as_of_date is amount." Several per account allowed.
CREATE TABLE balance_anchor (
    id         TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES account(id) ON DELETE CASCADE,
    as_of_date TEXT NOT NULL,
    amount     INTEGER NOT NULL,
    note       TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE (account_id, as_of_date)
);

CREATE TABLE import_batch (
    id             TEXT PRIMARY KEY,
    account_id     TEXT NOT NULL REFERENCES account(id) ON DELETE CASCADE,
    parser         TEXT NOT NULL,
    filename       TEXT NOT NULL,
    file_sha256    TEXT NOT NULL,
    imported_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    period_start   TEXT,
    period_end     TEXT,
    rows_new       INTEGER NOT NULL,
    rows_duplicate INTEGER NOT NULL,
    rows_error     INTEGER NOT NULL
);
CREATE INDEX import_batch_account ON import_batch(account_id, file_sha256);

CREATE TABLE transactions (
    id             TEXT PRIMARY KEY,
    account_id     TEXT NOT NULL REFERENCES account(id) ON DELETE CASCADE,
    date           TEXT NOT NULL,
    posted_date    TEXT,
    amount         INTEGER NOT NULL,
    description    TEXT NOT NULL,
    description_norm TEXT NOT NULL,
    fitid          TEXT,              -- bank-provided id, if the source had one
    export_fitid   TEXT NOT NULL,     -- fitid, or a stable derived id for OFX export
    source_seq     TEXT,
    balance_after  INTEGER,           -- running balance from the source, if given
    dedup_key      TEXT NOT NULL,
    occurrence     INTEGER NOT NULL,  -- unique per (account, dedup_key)
    import_batch_id TEXT NOT NULL REFERENCES import_batch(id) ON DELETE CASCADE,
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE (account_id, export_fitid),
    UNIQUE (account_id, dedup_key, occurrence)
);
CREATE INDEX transactions_account_date ON transactions(account_id, date);
CREATE UNIQUE INDEX transactions_source_fitid
    ON transactions(account_id, fitid) WHERE fitid IS NOT NULL;
CREATE INDEX transactions_batch ON transactions(import_batch_id);

CREATE TABLE import_error (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    batch_id  TEXT NOT NULL REFERENCES import_batch(id) ON DELETE CASCADE,
    row_ref   TEXT NOT NULL,
    message   TEXT NOT NULL
);

-- Statement/OFX balances used as reconciliation checkpoints.
CREATE TABLE balance_checkpoint (
    id              TEXT PRIMARY KEY,
    account_id      TEXT NOT NULL REFERENCES account(id) ON DELETE CASCADE,
    as_of_date      TEXT NOT NULL,
    amount          INTEGER NOT NULL,
    import_batch_id TEXT REFERENCES import_batch(id) ON DELETE CASCADE,
    UNIQUE (account_id, as_of_date)
);
