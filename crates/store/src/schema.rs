//! Versioned migrations tracked in `PRAGMA user_version`. Append only: never
//! edit a shipped migration, add the next numbered file.

use crate::Result;
use rusqlite::Connection;

const MIGRATIONS: &[&str] = &[include_str!("../migrations/0001_initial.sql")];

pub fn migrate(conn: &mut Connection) -> Result<()> {
    let current: usize = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}
