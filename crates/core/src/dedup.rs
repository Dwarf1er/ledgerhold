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
    /// Every id that identifies this row: its source FITID and its export FITID.
    pub ids: Vec<String>,
    /// True if the row came with a real source FITID (so it is a distinct,
    /// bank-identified transaction), false if its ids are only derived.
    pub has_source_id: bool,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
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

fn take(map: &mut HashMap<String, usize>, key: &str) -> bool {
    match map.get_mut(key) {
        Some(n) if *n > 0 => {
            *n -= 1;
            true
        }
        _ => false,
    }
}

pub fn plan_import(account_id: &str, incoming: &[ParsedTransaction], existing: &[ExistingTxn]) -> ImportPlan {
    let mut by_id: HashMap<&str, usize> = HashMap::new();
    let mut cap_all: HashMap<String, usize> = HashMap::new();
    let mut cap_unsourced: HashMap<String, usize> = HashMap::new();
    for (i, e) in existing.iter().enumerate() {
        for id in &e.ids {
            by_id.insert(id.as_str(), i);
        }
        *cap_all.entry(e.dedup_key.clone()).or_default() += 1;
        if !e.has_source_id {
            *cap_unsourced.entry(e.dedup_key.clone()).or_default() += 1;
        }
    }

    // Pass 1: an incoming id that already exists claims that stored row (also
    // marks a fitid repeated within the file as a duplicate).
    let mut claimed = vec![false; existing.len()];
    let mut seen: HashSet<&str> = HashSet::new();
    let mut matched = vec![false; incoming.len()];
    for (i, t) in incoming.iter().enumerate() {
        let Some(f) = t.fitid.as_deref() else { continue };
        let first_in_file = seen.insert(f);
        if let Some(&ei) = by_id.get(f) {
            matched[i] = true;
            if !std::mem::replace(&mut claimed[ei], true) {
                let e = &existing[ei];
                take(&mut cap_all, &e.dedup_key);
                if !e.has_source_id {
                    take(&mut cap_unsourced, &e.dedup_key);
                }
            }
        } else if !first_in_file {
            matched[i] = true;
        }
    }

    // Pass 2: decide the rest and assign in-file occurrence indices.
    let mut occurrences: HashMap<String, u32> = HashMap::new();
    let mut items = Vec::with_capacity(incoming.len());
    for (i, t) in incoming.iter().enumerate() {
        let key = dedup_key(account_id, t.date, t.amount, &t.description);
        let o = occurrences.entry(key.clone()).or_default();
        let occurrence = *o;
        *o += 1;
        let duplicate = if matched[i] {
            true
        } else if t.fitid.is_some() {
            // New bank id: may still be the same transaction as a row imported
            // earlier WITHOUT a bank id (e.g. from CSV). Never steal a row that
            // carries a different source id.
            let hit = take(&mut cap_unsourced, &key);
            if hit {
                take(&mut cap_all, &key);
            }
            hit
        } else {
            let hit = take(&mut cap_all, &key);
            if hit {
                take(&mut cap_unsourced, &key);
            }
            hit
        };
        items.push(PlanItem {
            index: i,
            disposition: if duplicate { Disposition::Duplicate } else { Disposition::New },
            dedup_key: key,
            occurrence,
        });
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
                ids: t.fitid.iter().cloned().collect(),
                has_source_id: t.fitid.is_some(),
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

    #[test]
    fn new_bank_id_matches_row_previously_imported_without_one() {
        // CSV first (no fitid), OFX later (fitid never seen): same transaction.
        let csv = tx(1, -500, "COFFEE");
        let mut ofx = tx(1, -500, "COFFEE");
        ofx.fitid = Some("BANK-1".into());
        assert_eq!(plan_import(ACCT, &[ofx], &stored(&[csv])).duplicate_count(), 1);
    }

    #[test]
    fn new_bank_id_does_not_steal_a_row_with_a_different_bank_id() {
        let mut old = tx(1, -500, "COFFEE");
        old.fitid = Some("BANK-1".into());
        let mut other = tx(1, -500, "COFFEE");
        other.fitid = Some("BANK-2".into());
        assert_eq!(plan_import(ACCT, &[other], &stored(&[old])).new_count(), 1);
    }

    #[test]
    fn export_ids_count_as_ids() {
        // A row stored from CSV carries only a derived export id; re-importing our
        // own OFX export (which uses that id) must be recognised.
        let csv = tx(1, -500, "COFFEE");
        let mut ex = stored(&[csv]);
        ex[0].ids.push("LH-abc-0".into());
        let mut back = tx(1, -500, "COFFEE");
        back.fitid = Some("LH-abc-0".into());
        assert_eq!(plan_import(ACCT, &[back], &ex).duplicate_count(), 1);
    }

    #[test]
    fn one_unsourced_row_absorbs_only_one_incoming_bank_id() {
        let csv = tx(1, -500, "COFFEE");
        let mk = |id: &str| {
            let mut t = tx(1, -500, "COFFEE");
            t.fitid = Some(id.into());
            t
        };
        let plan = plan_import(ACCT, &[mk("B1"), mk("B2")], &stored(&[csv]));
        assert_eq!((plan.duplicate_count(), plan.new_count()), (1, 1));
    }
}
