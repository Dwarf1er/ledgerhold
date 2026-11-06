//! SQLite persistence: people, accounts, imports with dedup + undo, balances,
//! reconciliation, and OFX export data.

mod accounts;
mod balances;
mod export;
mod import;
mod schema;

pub use accounts::{Account, Anchor, Institution, NewAccount, Person};
pub use balances::{BalanceReport, Discrepancy, PointKind};
pub use import::{AccountStats, BatchInfo, ImportMeta, ImportOutcome, ImportPreview, StoredTxn};

use chrono::NaiveDate;
use ledgerhold_core::Money;
use rusqlite::Connection;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("{0} not found")]
    NotFound(String),
    #[error("currency mismatch: account is {account}, statement is {statement}")]
    CurrencyMismatch { account: String, statement: String },
    #[error("kind mismatch: account is {account}, statement looks like {statement}")]
    KindMismatch { account: String, statement: String },
    #[error("account has no balance anchor; set a starting balance first")]
    NoAnchor,
    #[error("invalid data in database: {0}")]
    Corrupt(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

pub struct Store {
    pub(crate) conn: Connection,
}

pub(crate) fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

pub(crate) fn d2s(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub(crate) fn s2d(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| StoreError::Corrupt(format!("bad date {s:?}")))
}

pub(crate) fn money(v: i64) -> Money {
    Money(v)
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Store> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Store> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Store> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let _: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        let mut store = Store { conn };
        schema::migrate(&mut store.conn)?;
        Ok(store)
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self.conn.pragma_query_value(None, "user_version", |r| r.get(0))?)
    }
}
