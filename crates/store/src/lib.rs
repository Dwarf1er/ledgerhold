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

/// Create `dir` and any missing parents readable only by the current user (mode 0700 on Unix).
pub fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

/// Create an empty file at `path` if there is none, readable only by the current user.
pub fn create_private_file(path: &Path) -> std::io::Result<()> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path).map(drop)
}

/// Make the database and its SQLite side files owner-only (best effort; a no-op off Unix).
fn restrict_to_owner(db: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for suffix in ["", "-wal", "-shm"] {
            let mut name = db.as_os_str().to_owned();
            name.push(suffix);
            let _ = std::fs::set_permissions(Path::new(&name), std::fs::Permissions::from_mode(0o600));
        }
    }
    #[cfg(not(unix))]
    let _ = db;
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{0}")]
    Io(String),
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
    /// Open (creating if needed) the database at `path`. The file is private to the current user: it holds
    /// the household's financial history, so on a shared machine nobody else may read it.
    pub fn open(path: impl AsRef<Path>) -> Result<Store> {
        let path = path.as_ref();
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            create_private_dir(dir).map_err(|e| StoreError::Io(format!("creating {}: {e}", dir.display())))?;
        }
        create_private_file(path).map_err(|e| StoreError::Io(format!("creating {}: {e}", path.display())))?;
        let store = Self::init(Connection::open(path)?)?;
        // SQLite's -wal/-shm files copy the database's mode, but tighten older installs and anything pre-existing.
        restrict_to_owner(path);
        Ok(store)
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
