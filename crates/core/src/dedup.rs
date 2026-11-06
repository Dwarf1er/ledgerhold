//! Pure deduplication planning. No I/O: the store loads the existing rows that
//! overlap the incoming file's date span and hands them in.
//!
//! Rules:
//! 1. Incoming row WITH a fitid: duplicate iff that fitid already exists for the
//!    account. Distinct fitids are strong evidence of distinct transactions.
//! 2. Incoming row WITHOUT a fitid: multiset match on the content key. If the
//!    file has N rows with key K and the store already has M (not claimed by a
//!    fitid match), the first M are duplicates and the remaining N-M are new.
//!    This is independent of row order and statement window.

use crate::{Money, ParsedTransaction};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

/// Uppercase, collapse whitespace. Digits are preserved on purpose
/// (descriptions differing only by digits are different transactions).
pub fn normalize_description(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_uppercase()
}

/// Stable content key for a transaction within an account.
pub fn dedup_key(account_id: &str, date: NaiveDate, amount: Money, description: &str) -> String {
    let mut h = Sha256::new();
    h.update(account_id.as_bytes());
    h.update([0]);
    h.update(date.format("%Y-%m-%d").to_string().as_bytes());
    h.update([0]);
    h.update(amount.minor().to_le_bytes());
    h.update([0]);
    h.update(normalize_description(description).as_bytes());
    hex::encode(h.finalize())
}

/// A row already in the store, as needed for planning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingTxn {
    pub dedup_key: String,
    pub fitid: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    New,
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanItem {
    /// Index into the incoming slice.
    pub index: usize,
    pub disposition: Disposition,
    pub dedup_key: String,
    /// 0-based occurrence of this key within the incoming file.
    pub occurrence: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ImportPlan {
    pub items: Vec<PlanItem>,
}

impl ImportPlan {
    pub fn new_count(&self) -> usize {
        self.items.iter().filter(|i| i.disposition == Disposition::New).count()
    }
    pub fn duplicate_count(&self) -> usize {
        self.items.len() - self.new_count()
    }
}

pub fn plan_import(account_id: &str, incoming: &[ParsedTransaction], existing: &[ExistingTxn]) -> ImportPlan {
    let existing_fitids: HashMap<&str, &str> = existing
        .iter()
        .filter_map(|e| e.fitid.as_deref().map(|f| (f, e.dedup_key.as_str())))
        .collect();

    // Remaining existing capacity per key (rows not claimed by a fitid match).
    let mut capacity: HashMap<&str, usize> = HashMap::new();
    for e in existing {
        *capacity.entry(e.dedup_key.as_str()).or_default() += 1;
    }

    // Pass 1: fitid matches claim an existing row (also guards against the same
    // fitid appearing twice in one file).
    let mut seen_fitids: HashSet<&str> = HashSet::new();
    let mut fitid_dup: Vec<bool> = vec![false; incoming.len()];
    for (i, t) in incoming.iter().enumerate() {
        if let Some(f) = t.fitid.as_deref() {
            let in_store = existing_fitids.get(f);
            let repeated = !seen_fitids.insert(f);
            if let Some(key) = in_store {
                fitid_dup[i] = true;
                if let Some(c) = capacity.get_mut(key) {
                    *c = c.saturating_sub(1);
                }
            } else if repeated {
                fitid_dup[i] = true;
            }
        }
    }

    // Pass 2: assign occurrence indices and decide.
    let mut occurrences: HashMap<String, u32> = HashMap::new();
    let mut consumed: HashMap<String, usize> = HashMap::new();
    let mut items = Vec::with_capacity(incoming.len());
    for (i, t) in incoming.iter().enumerate() {
        let key = dedup_key(account_id, t.date, t.amount, &t.description);
        let occ = {
            let o = occurrences.entry(key.clone()).or_default();
            let cur = *o;
            *o += 1;
            cur
        };
        let disposition = if t.fitid.is_some() {
            if fitid_dup[i] { Disposition::Duplicate } else { Disposition::New }
        } else {
            let cap = capacity.get(key.as_str()).copied().unwrap_or(0);
            let used = consumed.entry(key.clone()).or_default();
            if *used < cap {
                *used += 1;
                Disposition::Duplicate
            } else {
                Disposition::New
            }
        };
        items.push(PlanItem { index: i, disposition, dedup_key: key, occurrence: occ });
    }
    ImportPlan { items }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCT: &str = "acct-1";

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 1, day).unwrap()
    }

    fn tx(day: u32, cents: i64, desc: &str) -> ParsedTransaction {
        ParsedTransaction {
            date: d(day),
            posted_date: None,
            amount: Money(cents),
            description: desc.into(),
            fitid: None,
            source_seq: None,
            balance: None,
        }
    }

    fn stored(rows: &[ParsedTransaction]) -> Vec<ExistingTxn> {
        rows.iter()
            .map(|t| ExistingTxn {
                dedup_key: dedup_key(ACCT, t.date, t.amount, &t.description),
                fitid: t.fitid.clone(),
            })
            .collect()
    }

    #[test]
    fn empty_store_everything_new() {
        let rows = vec![tx(1, -500, "COFFEE"), tx(2, -900, "LUNCH")];
        let plan = plan_import(ACCT, &rows, &[]);
        assert_eq!(plan.new_count(), 2);
    }

    #[test]
    fn reimport_same_file_is_all_duplicates() {
        let rows = vec![tx(1, -500, "COFFEE"), tx(1, -500, "COFFEE"), tx(2, -900, "LUNCH")];
        let plan = plan_import(ACCT, &rows, &stored(&rows));
        assert_eq!(plan.duplicate_count(), 3);
    }

    #[test]
    fn same_day_identical_rows_stay_distinct() {
        let rows = vec![tx(1, -500, "COFFEE"), tx(1, -500, "COFFEE")];
        let plan = plan_import(ACCT, &rows, &[]);
        assert_eq!(plan.new_count(), 2);
        assert_eq!(plan.items[0].occurrence, 0);
        assert_eq!(plan.items[1].occurrence, 1);
    }

    #[test]
    fn overlap_with_extra_identical_row_imports_only_the_extra() {
        let first = vec![tx(1, -500, "COFFEE")];
        let second = vec![tx(1, -500, "COFFEE"), tx(1, -500, "COFFEE")];
        let plan = plan_import(ACCT, &second, &stored(&first));
        assert_eq!(plan.duplicate_count(), 1);
        assert_eq!(plan.new_count(), 1);
    }

    #[test]
    fn row_order_does_not_matter() {
        let a = vec![tx(1, -500, "A"), tx(2, -700, "B"), tx(3, -100, "C")];
        let b = vec![tx(3, -100, "C"), tx(1, -500, "A"), tx(2, -700, "B"), tx(4, -1, "D")];
        let plan = plan_import(ACCT, &b, &stored(&a));
        assert_eq!(plan.duplicate_count(), 3);
        assert_eq!(plan.new_count(), 1);
        assert_eq!(plan.items[3].disposition, Disposition::New);
    }

    #[test]
    fn whitespace_and_case_normalized() {
        let a = vec![tx(1, -500, "Coffee  Shop")];
        let b = vec![tx(1, -500, "COFFEE SHOP")];
        assert_eq!(plan_import(ACCT, &b, &stored(&a)).duplicate_count(), 1);
    }

    #[test]
    fn digits_in_description_distinguish() {
        let a = vec![tx(1, -500, "STORE 123")];
        let b = vec![tx(1, -500, "STORE 124")];
        assert_eq!(plan_import(ACCT, &b, &stored(&a)).new_count(), 1);
    }

    #[test]
    fn fitid_decides_when_present() {
        let mut old = tx(1, -500, "COFFEE");
        old.fitid = Some("F1".into());
        let mut same_id = tx(1, -500, "COFFEE (renamed by bank)");
        same_id.fitid = Some("F1".into());
        let mut other_id = tx(1, -500, "COFFEE");
        other_id.fitid = Some("F2".into());
        let plan = plan_import(ACCT, &[same_id, other_id], &stored(&[old]));
        assert_eq!(plan.items[0].disposition, Disposition::Duplicate);
        assert_eq!(plan.items[1].disposition, Disposition::New);
    }

    #[test]
    fn cross_format_no_fitid_matches_stored_fitid_row() {
        // Stored from OFX (has fitid); re-imported from CSV (no fitid).
        let mut ofx = tx(1, -500, "COFFEE");
        ofx.fitid = Some("F1".into());
        let csv = tx(1, -500, "COFFEE");
        assert_eq!(plan_import(ACCT, &[csv], &stored(&[ofx])).duplicate_count(), 1);
    }

    #[test]
    fn repeated_fitid_within_file_is_duplicate() {
        let mut a = tx(1, -500, "X");
        a.fitid = Some("F1".into());
        let plan = plan_import(ACCT, &[a.clone(), a], &[]);
        assert_eq!(plan.new_count(), 1);
        assert_eq!(plan.duplicate_count(), 1);
    }
}
