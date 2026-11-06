//! Differential tests: Rust parsers vs goldens produced by the legacy Python
//! tool (see tools/py_golden.py) on synthetic fixtures.

use ledgerhold_parsers::by_id;
use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::PathBuf};

#[derive(Deserialize)]
struct GoldenTxn {
    date: String,
    amount_cents: i64,
    memo: String,
    balance_cents: Option<i64>,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn golden(name: &str) -> BTreeMap<String, Vec<GoldenTxn>> {
    serde_json::from_slice(&fs::read(root().join("golden").join(name)).unwrap()).unwrap()
}

fn parse(id: &str, fixture: &str) -> Vec<ledgerhold_core::ParsedStatement> {
    let bytes = fs::read(root().join("synthetic").join(fixture)).unwrap();
    by_id(id).unwrap().parse(&bytes).unwrap()
}

#[test]
fn chequing_matches_python() {
    let g = &golden("wealthsimple_chequing.json")["Wealthsimple"];
    let st = &parse("wealthsimple", "wealthsimple_chequing.csv")[0];
    assert_eq!(st.transactions.len(), g.len());
    for (t, e) in st.transactions.iter().zip(g) {
        assert_eq!(t.date.to_string(), e.date);
        assert_eq!(t.amount.minor(), e.amount_cents);
        assert_eq!(t.description, e.memo);
        assert_eq!(t.balance.map(|b| b.minor()), e.balance_cents);
    }
}

#[test]
fn chequing_reports_bad_rows_instead_of_dropping_silently() {
    let st = &parse("wealthsimple", "wealthsimple_chequing.csv")[0];
    let refs: Vec<_> = st.errors.iter().map(|e| e.row_ref.as_str()).collect();
    assert_eq!(refs, ["line 8", "line 10"]);
    assert!(st.errors[0].message.contains("invalid date"));
}

#[test]
fn credit_matches_python_across_currencies() {
    let g = &golden("wealthsimple_credit.json")["Wealthsimple-Credit"];
    let sts = parse("wealthsimple-credit", "wealthsimple_credit.csv");
    // Python merged everything; we split per currency, so compare in file order.
    let mut got: Vec<_> = sts.iter().flat_map(|s| s.transactions.iter()).collect();
    got.sort_by_key(|t| (t.date, t.description.clone()));
    let mut exp: Vec<_> = g.iter().collect();
    exp.sort_by_key(|t| (t.date.clone(), t.memo.clone()));
    assert_eq!(got.len(), exp.len());
    for (t, e) in got.iter().zip(exp) {
        assert_eq!(t.date.to_string(), e.date);
        assert_eq!(t.amount.minor(), e.amount_cents, "{}", e.memo);
        assert_eq!(t.description, e.memo);
    }
}

#[test]
fn credit_splits_currencies_keeps_post_date_and_reports_errors() {
    let sts = parse("wealthsimple-credit", "wealthsimple_credit.csv");
    let cur: Vec<_> = sts.iter().map(|s| (s.currency.as_str(), s.transactions.len())).collect();
    assert_eq!(cur, [("CAD", 8), ("USD", 1)]);
    assert_eq!(sts[0].transactions[0].posted_date.unwrap().to_string(), "2026-01-03");
    assert_eq!(sts[0].errors.len(), 1);
    assert!(sts[0].transactions.last().unwrap().posted_date.is_none());
}

#[test]
fn credit_sign_comes_from_type_not_csv_sign() {
    let sts = parse("wealthsimple-credit", "wealthsimple_credit.csv");
    let by = |d: &str| sts[0].transactions.iter().find(|t| t.description == d).unwrap().amount.minor();
    assert_eq!(by("Gas station"), -4510);
    assert_eq!(by("Restaurant, downtown"), -6230);
    assert_eq!(by("Payment from chequing"), 20000);
    assert_eq!(by("Online store refund"), 1500);
    assert_eq!(by("Monthly cashback"), 321);
    assert_eq!(by("Interest charge"), -875);
}

#[test]
fn missing_required_column_is_an_error() {
    let r = by_id("wealthsimple").unwrap().parse(b"foo,bar\n1,2\n");
    assert!(r.is_err());
}
