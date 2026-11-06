//! Desjardins CSV: differential test against the legacy Python goldens.

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

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn parse() -> Vec<ledgerhold_core::ParsedStatement> {
    let bytes = fs::read(fixtures().join("synthetic/desjardins.csv")).unwrap();
    by_id("desjardins").unwrap().parse(&bytes).unwrap()
}

#[test]
fn matches_python_for_every_account() {
    let golden: BTreeMap<String, Vec<GoldenTxn>> =
        serde_json::from_slice(&fs::read(fixtures().join("golden/desjardins.json")).unwrap()).unwrap();
    let sts = parse();
    assert_eq!(sts.len(), golden.len());
    for st in &sts {
        let id = st.account_hint.as_deref().unwrap();
        let exp = &golden[id];
        assert_eq!(st.transactions.len(), exp.len(), "{id}");
        for (t, e) in st.transactions.iter().zip(exp) {
            assert_eq!(t.date.to_string(), e.date, "{id}");
            assert_eq!(t.amount.minor(), e.amount_cents, "{id} {}", e.memo);
            assert_eq!(t.description, e.memo);
            assert_eq!(t.balance.map(|b| b.minor()), e.balance_cents, "{id} {}", e.memo);
        }
    }
}

/// The odd=debit / even=credit rule, one case per amount column (6..=12).
#[test]
fn amount_column_parity_rule() {
    let sts = parse();
    let eop = sts.iter().find(|s| s.account_hint.as_deref() == Some("EOP-12345")).unwrap();
    let amt = |memo: &str| eop.transactions.iter().find(|t| t.description == memo).unwrap().amount.minor();
    assert_eq!(amt("VIREMENT INTERAC RECU"), 5000); // col 6 even -> credit
    assert_eq!(amt("ACHAT EPICERIE #123"), -8417); // col 7 odd  -> debit
    assert_eq!(amt("PAIE ACME INC"), 250000); // col 8 even -> credit
    assert_eq!(amt("PRELEVEMENT LOYER, JANVIER"), -120000); // col 9 odd
    assert_eq!(amt("INTERETS"), 12); // col 10 even
    assert_eq!(amt("FRAIS DE SERVICE"), -250); // col 11 odd
    assert_eq!(amt("DEPOT GUICHET"), 1000); // col 12 even
}

#[test]
fn header_and_blank_rows_are_skipped_silently_but_bad_rows_are_reported() {
    let sts = parse();
    let eop = sts.iter().find(|s| s.account_hint.as_deref() == Some("EOP-12345")).unwrap();
    let msgs: Vec<_> = eop.errors.iter().map(|e| (e.row_ref.as_str(), e.message.as_str())).collect();
    assert_eq!(msgs.len(), 2, "{msgs:?}");
    assert!(msgs[0].1.contains("no amount")); // SOLDE REPORTE
    assert!(msgs[1].1.contains("invalid date")); // 2026/13/45
    assert!(sts.iter().all(|s| s.kind == ledgerhold_core::AccountKind::Checking));
}

#[test]
fn keeps_sequence_numbers_and_multiple_accounts() {
    let sts = parse();
    let ids: Vec<_> = sts.iter().map(|s| s.account_hint.clone().unwrap()).collect();
    assert_eq!(ids, ["EOP-12345", "PR1-67890"]);
    assert_eq!(sts[0].transactions[0].source_seq.as_deref(), Some("001"));
}
