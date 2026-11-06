//! Import = parse (elsewhere) -> plan (pure, core::dedup) -> preview -> commit as
//! one batch. The plan is recomputed inside the commit transaction so a stale
//! preview can never insert duplicates. Undo removes a whole batch.

use crate::accounts::Account;
use crate::{Result, Store, StoreError, d2s, money, new_id, s2d};
use chrono::NaiveDate;
use ledgerhold_core::dedup::{Disposition, ExistingTxn, ImportPlan, normalize_description, plan_import};
use ledgerhold_core::{AccountKind, Money, ParsedStatement};
use rusqlite::{Connection, params, params_from_iter};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy)]
pub struct ImportMeta<'a> {
    pub parser: &'a str,
    pub filename: &'a str,
    pub file_sha256: &'a str,
}

#[derive(Debug, Clone)]
pub struct ImportPreview {
    pub plan: ImportPlan,
    pub new: usize,
    pub duplicates: usize,
    pub errors: usize,
    /// Set if a file with the same SHA-256 was already imported into this account.
    pub file_already_imported: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOutcome {
    /// `None` when the import changed nothing (everything was a duplicate), in
    /// which case no batch is recorded.
    pub batch_id: Option<String>,
    pub new: usize,
    pub duplicates: usize,
    pub errors: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchInfo {
    pub id: String,
    pub parser: String,
    pub filename: String,
    pub imported_at: String,
    pub period: Option<(NaiveDate, NaiveDate)>,
    pub rows_new: usize,
    pub rows_duplicate: usize,
    pub rows_error: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredTxn {
    pub id: String,
    pub date: NaiveDate,
    pub posted_date: Option<NaiveDate>,
    pub amount: Money,
    pub description: String,
    pub fitid: Option<String>,
    pub export_fitid: String,
    pub balance_after: Option<Money>,
    pub dedup_key: String,
    pub import_batch_id: String,
}

fn check_compat(account: &Account, st: &ParsedStatement) -> Result<()> {
    if account.currency != st.currency {
        return Err(StoreError::CurrencyMismatch { account: account.currency.clone(), statement: st.currency.clone() });
    }
    // Checking vs savings can't be told apart from a file; a card vs a bank account can.
    if (account.kind == AccountKind::CreditCard) != (st.kind == AccountKind::CreditCard) {
        return Err(StoreError::KindMismatch {
            account: account.kind.as_str().into(),
            statement: st.kind.as_str().into(),
        });
    }
    Ok(())
}

/// Stored rows that could be duplicates of the statement: same date span, or
/// sharing a bank id with an incoming row.
fn load_existing(conn: &Connection, account_id: &str, st: &ParsedStatement) -> Result<Vec<ExistingTxn>> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    let mut push = |key: String, fitid: Option<String>, export: String| {
        if seen.insert(export.clone()) {
            let mut ids = Vec::new();
            if let Some(f) = &fitid {
                ids.push(f.clone());
            }
            if fitid.as_deref() != Some(export.as_str()) {
                ids.push(export);
            }
            out.push(ExistingTxn { dedup_key: key, ids, has_source_id: fitid.is_some() });
        }
    };

    if let Some((min, max)) = st.date_span() {
        let mut q = conn.prepare(
            "SELECT dedup_key, fitid, export_fitid FROM transactions
             WHERE account_id = ?1 AND date BETWEEN ?2 AND ?3",
        )?;
        let rows = q.query_map(params![account_id, d2s(min), d2s(max)], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        for r in rows {
            let (k, f, e) = r?;
            push(k, f, e);
        }
    }

    let ids: Vec<&str> = st.transactions.iter().filter_map(|t| t.fitid.as_deref()).collect();
    for chunk in ids.chunks(400) {
        let marks = vec!["?"; chunk.len()].join(",");
        let sql = format!(
            "SELECT dedup_key, fitid, export_fitid FROM transactions
             WHERE account_id = ? AND (fitid IN ({marks}) OR export_fitid IN ({marks}))"
        );
        let mut q = conn.prepare(&sql)?;
        let params: Vec<&str> = std::iter::once(account_id).chain(chunk.iter().copied()).chain(chunk.iter().copied()).collect();
        let rows = q.query_map(params_from_iter(params), |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        for r in rows {
            let (k, f, e) = r?;
            push(k, f, e);
        }
    }
    Ok(out)
}

impl Store {
    /// Dry run: what would happen if this statement were imported now.
    pub fn preview_import(&self, account_id: &str, st: &ParsedStatement, file_sha256: &str) -> Result<ImportPreview> {
        let account = self.get_account(account_id)?;
        check_compat(&account, st)?;
        let existing = load_existing(&self.conn, account_id, st)?;
        let plan = plan_import(account_id, &st.transactions, &existing);
        let file_already_imported = self
            .conn
            .query_row(
                "SELECT id FROM import_batch WHERE account_id = ?1 AND file_sha256 = ?2 LIMIT 1",
                params![account_id, file_sha256],
                |r| r.get(0),
            )
            .ok();
        Ok(ImportPreview {
            new: plan.new_count(),
            duplicates: plan.duplicate_count(),
            errors: st.errors.len(),
            plan,
            file_already_imported,
        })
    }

    pub fn import(&mut self, account_id: &str, meta: ImportMeta<'_>, st: &ParsedStatement) -> Result<ImportOutcome> {
        let account = self.get_account(account_id)?;
        check_compat(&account, st)?;

        let tx = self.conn.transaction()?;
        let existing = load_existing(&tx, account_id, st)?;
        let plan = plan_import(account_id, &st.transactions, &existing);
        let batch_id = new_id();
        let period = st.period.or_else(|| st.date_span());

        tx.execute(
            "INSERT INTO import_batch(id, account_id, parser, filename, file_sha256, period_start, period_end,
                                      rows_new, rows_duplicate, rows_error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 0, 0)",
            params![
                batch_id,
                account_id,
                meta.parser,
                meta.filename,
                meta.file_sha256,
                period.map(|p| d2s(p.0)),
                period.map(|p| d2s(p.1))
            ],
        )?;

        // occurrence is unique per (account, key) across ALL imports, so the
        // derived export id never collides.
        let mut next_occ: HashMap<String, i64> = HashMap::new();
        let mut inserted = 0usize;
        for item in &plan.items {
            if item.disposition != Disposition::New {
                continue;
            }
            let t = &st.transactions[item.index];
            let occ = match next_occ.get(&item.dedup_key) {
                Some(n) => *n,
                None => tx.query_row(
                    "SELECT COALESCE(MAX(occurrence) + 1, 0) FROM transactions WHERE account_id = ?1 AND dedup_key = ?2",
                    params![account_id, item.dedup_key],
                    |r| r.get(0),
                )?,
            };
            next_occ.insert(item.dedup_key.clone(), occ + 1);
            let export_fitid = t.fitid.clone().unwrap_or_else(|| derived_fitid(&item.dedup_key, occ));
            tx.execute(
                "INSERT INTO transactions(id, account_id, date, posted_date, amount, description, description_norm,
                                          fitid, export_fitid, source_seq, balance_after, dedup_key, occurrence,
                                          import_batch_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    new_id(),
                    account_id,
                    d2s(t.date),
                    t.posted_date.map(d2s),
                    t.amount.minor(),
                    t.description,
                    normalize_description(&t.description),
                    t.fitid,
                    export_fitid,
                    t.source_seq,
                    t.balance.map(|b| b.minor()),
                    item.dedup_key,
                    occ,
                    batch_id
                ],
            )?;
            inserted += 1;
        }

        for e in &st.errors {
            tx.execute(
                "INSERT INTO import_error(batch_id, row_ref, message) VALUES (?1, ?2, ?3)",
                params![batch_id, e.row_ref, e.message],
            )?;
        }

        let mut new_checkpoints = 0usize;
        for c in &st.checkpoints {
            new_checkpoints += tx.execute(
                "INSERT OR IGNORE INTO balance_checkpoint(id, account_id, as_of_date, amount, import_batch_id)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![new_id(), account_id, d2s(c.as_of), c.balance.minor(), batch_id],
            )?;
        }

        let duplicates = plan.duplicate_count();
        let errors = st.errors.len();
        if inserted == 0 && errors == 0 && new_checkpoints == 0 {
            // Nothing changed: don't record a pointless batch.
            drop(tx);
            return Ok(ImportOutcome { batch_id: None, new: 0, duplicates, errors: 0 });
        }
        tx.execute(
            "UPDATE import_batch SET rows_new = ?2, rows_duplicate = ?3, rows_error = ?4 WHERE id = ?1",
            params![batch_id, inserted as i64, duplicates as i64, errors as i64],
        )?;
        tx.commit()?;
        Ok(ImportOutcome { batch_id: Some(batch_id), new: inserted, duplicates, errors })
    }

    /// Remove a whole import (its transactions, errors and checkpoints).
    /// Returns the number of transactions removed.
    pub fn undo_batch(&self, batch_id: &str) -> Result<usize> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM transactions WHERE import_batch_id = ?1",
            params![batch_id],
            |r| r.get(0),
        )?;
        let gone = self.conn.execute("DELETE FROM import_batch WHERE id = ?1", params![batch_id])?;
        if gone == 0 {
            return Err(StoreError::NotFound(format!("import batch {batch_id}")));
        }
        Ok(n as usize)
    }

    pub fn list_batches(&self, account_id: &str) -> Result<Vec<BatchInfo>> {
        let mut st = self.conn.prepare(
            "SELECT id, parser, filename, imported_at, period_start, period_end, rows_new, rows_duplicate, rows_error
             FROM import_batch WHERE account_id = ?1 ORDER BY imported_at, id",
        )?;
        let rows = st.query_map(params![account_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, i64>(8)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, parser, filename, imported_at, ps, pe, n, dup, err) = row?;
            let period = match (ps, pe) {
                (Some(a), Some(b)) => Some((s2d(&a)?, s2d(&b)?)),
                _ => None,
            };
            out.push(BatchInfo {
                id,
                parser,
                filename,
                imported_at,
                period,
                rows_new: n as usize,
                rows_duplicate: dup as usize,
                rows_error: err as usize,
            });
        }
        Ok(out)
    }

    pub fn batch_errors(&self, batch_id: &str) -> Result<Vec<(String, String)>> {
        let mut st = self.conn.prepare("SELECT row_ref, message FROM import_error WHERE batch_id = ?1 ORDER BY id")?;
        let rows = st.query_map(params![batch_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Transactions in `[start, end]` (either bound optional), oldest first;
    /// ties keep import order.
    pub fn list_transactions(
        &self,
        account_id: &str,
        start: Option<NaiveDate>,
        end: Option<NaiveDate>,
    ) -> Result<Vec<StoredTxn>> {
        let mut st = self.conn.prepare(
            "SELECT id, date, posted_date, amount, description, fitid, export_fitid, balance_after, dedup_key,
                    import_batch_id
             FROM transactions
             WHERE account_id = ?1 AND date >= ?2 AND date <= ?3
             ORDER BY date, rowid",
        )?;
        let lo = start.map(d2s).unwrap_or_else(|| "0000-01-01".into());
        let hi = end.map(d2s).unwrap_or_else(|| "9999-12-31".into());
        let rows = st.query_map(params![account_id, lo, hi], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, Option<i64>>(7)?,
                r.get::<_, String>(8)?,
                r.get::<_, String>(9)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, date, posted, amount, description, fitid, export_fitid, bal, dedup_key, import_batch_id) = row?;
            out.push(StoredTxn {
                id,
                date: s2d(&date)?,
                posted_date: posted.as_deref().map(s2d).transpose()?,
                amount: money(amount),
                description,
                fitid,
                export_fitid,
                balance_after: bal.map(money),
                dedup_key,
                import_batch_id,
            });
        }
        Ok(out)
    }
}

/// Stable OFX FITID for rows whose source had none.
pub(crate) fn derived_fitid(dedup_key: &str, occurrence: i64) -> String {
    format!("LH-{}-{}", &dedup_key[..16], occurrence)
}
