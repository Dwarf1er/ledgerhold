//! Balance = a known anchor balance +/- the transactions between the anchor date
//! and the requested date. Anchors are "end of day" balances, so transactions
//! dated ON the anchor day are already included in it.

use crate::accounts::Anchor;
use crate::{Result, Store, StoreError, d2s, money, s2d};
use chrono::NaiveDate;
use ledgerhold_core::Money;
use rusqlite::params;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BalanceReport {
    pub balance: Money,
    /// The anchor the figure was derived from.
    pub anchor: Anchor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointKind {
    Anchor,
    Statement,
}

/// Comparison of two consecutive known balances (anchors / statement
/// balances): what the transactions in between imply vs what was reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discrepancy {
    pub from: NaiveDate,
    pub from_kind: PointKind,
    pub to: NaiveDate,
    pub to_kind: PointKind,
    /// `from` balance plus the transactions in `(from, to]`.
    pub expected: Money,
    pub actual: Money,
    /// `actual - expected`. Negative: the bank shows less than our transactions
    /// imply (a debit is missing, or a credit was recorded twice). Positive: a
    /// credit is missing (or a debit was recorded twice).
    pub difference: Money,
}

impl Discrepancy {
    pub fn is_ok(&self) -> bool {
        self.difference == Money::ZERO
    }
}

impl Store {
    /// Sum of amounts with `after < date <= upto`.
    fn sum_between(&self, account_id: &str, after: NaiveDate, upto: NaiveDate) -> Result<Money> {
        let v: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(amount), 0) FROM transactions WHERE account_id = ?1 AND date > ?2 AND date <= ?3",
            params![account_id, d2s(after), d2s(upto)],
            |r| r.get(0),
        )?;
        Ok(money(v))
    }

    /// Balance at the end of day `on`. Uses the latest anchor on or before
    /// `on`; if there is none, works backwards from the earliest later anchor.
    pub fn balance_at(&self, account_id: &str, on: NaiveDate) -> Result<BalanceReport> {
        let anchors = self.list_anchors(account_id)?;
        if let Some(a) = anchors.iter().rev().find(|a| a.as_of <= on) {
            let delta = self.sum_between(account_id, a.as_of, on)?;
            return Ok(BalanceReport { balance: a.amount + delta, anchor: a.clone() });
        }
        if let Some(a) = anchors.into_iter().find(|a| a.as_of > on) {
            let delta = self.sum_between(account_id, on, a.as_of)?;
            return Ok(BalanceReport { balance: a.amount - delta, anchor: a });
        }
        Err(StoreError::NoAnchor)
    }

    /// End-of-day balance for every date in `[start, end]`.
    pub fn daily_balances(&self, account_id: &str, start: NaiveDate, end: NaiveDate) -> Result<Vec<(NaiveDate, Money)>> {
        let before = start.pred_opt().ok_or_else(|| StoreError::Corrupt("date underflow".into()))?;
        let mut running = self.balance_at(account_id, before)?.balance;
        let mut q = self.conn.prepare(
            "SELECT date, SUM(amount) FROM transactions WHERE account_id = ?1 AND date BETWEEN ?2 AND ?3 GROUP BY date",
        )?;
        let mut per_day: HashMap<String, i64> = HashMap::new();
        for row in q.query_map(params![account_id, d2s(start), d2s(end)], |r| Ok((r.get(0)?, r.get(1)?)))? {
            let (d, s): (String, i64) = row?;
            per_day.insert(d, s);
        }
        let mut out = Vec::new();
        let mut day = start;
        while day <= end {
            running = running + money(per_day.get(&d2s(day)).copied().unwrap_or(0));
            out.push((day, running));
            day = day.succ_opt().ok_or_else(|| StoreError::Corrupt("date overflow".into()))?;
        }
        Ok(out)
    }

    /// Statement balances recorded during imports (OFX `LEDGERBAL`, ...).
    pub fn list_checkpoints(&self, account_id: &str) -> Result<Vec<(NaiveDate, Money)>> {
        let mut q = self
            .conn
            .prepare("SELECT as_of_date, amount FROM balance_checkpoint WHERE account_id = ?1 ORDER BY as_of_date")?;
        let mut out = Vec::new();
        for row in q.query_map(params![account_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
            let (d, a) = row?;
            out.push((s2d(&d)?, money(a)));
        }
        Ok(out)
    }

    /// Check every consecutive pair of known balances (anchors + statement
    /// balances) against the transactions between them. A non-zero difference
    /// pinpoints the period with missing or duplicated transactions.
    pub fn reconcile(&self, account_id: &str) -> Result<Vec<Discrepancy>> {
        let mut points: Vec<(NaiveDate, PointKind, Money)> = self
            .list_anchors(account_id)?
            .into_iter()
            .map(|a| (a.as_of, PointKind::Anchor, a.amount))
            .chain(self.list_checkpoints(account_id)?.into_iter().map(|(d, m)| (d, PointKind::Statement, m)))
            .collect();
        // At equal dates, anchors come first.
        points.sort_by_key(|(d, k, _)| (*d, matches!(k, PointKind::Statement)));
        let mut out = Vec::new();
        for w in points.windows(2) {
            let (from, from_kind, from_bal) = w[0];
            let (to, to_kind, actual) = w[1];
            let expected = from_bal + self.sum_between(account_id, from, to)?;
            out.push(Discrepancy { from, from_kind, to, to_kind, expected, actual, difference: actual - expected });
        }
        Ok(out)
    }
}
