//! Desjardins Visa: logic tested on pdfplumber's text (isolates the regex
//! state machine), and end-to-end on the PDF via pdf-extract (checks that the
//! pure-Rust extractor yields the same lines). Both vs legacy Python goldens.

use chrono::NaiveDate;
use ledgerhold_parsers::{by_id, desjardins_credit::parse_text};
use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::PathBuf};

#[derive(Deserialize)]
struct GoldenTxn {
    date: String,
    amount_cents: i64,
    memo: String,
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn golden() -> Vec<GoldenTxn> {
    let g: BTreeMap<String, Vec<GoldenTxn>> =
        serde_json::from_slice(&fs::read(fixtures().join("golden/desjardins_visa.json")).unwrap()).unwrap();
    assert_eq!(g.keys().collect::<Vec<_>>(), ["4540XXXXXXXX1234"]);
    g.into_values().next().unwrap()
}

fn check(st: &ledgerhold_core::ParsedStatement) {
    assert_eq!(st.account_hint.as_deref(), Some("4540XXXXXXXX1234"));
    let exp = golden();
    assert_eq!(st.transactions.len(), exp.len());
    for (t, e) in st.transactions.iter().zip(&exp) {
        assert_eq!(t.date.to_string(), e.date);
        assert_eq!(t.amount.minor(), e.amount_cents, "{}", e.memo);
        assert_eq!(t.description, e.memo);
    }
    assert!(st.errors.is_empty(), "{:?}", st.errors);
}

#[test]
fn text_logic_matches_python_on_pdfplumber_text() {
    let text = fs::read_to_string(fixtures().join("golden/desjardins_visa.pdfplumber.txt")).unwrap();
    check(&parse_text(&text, NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()));
}

#[test]
fn pdf_end_to_end_matches_python() {
    let bytes = fs::read(fixtures().join("synthetic/desjardins_visa.pdf")).unwrap();
    let sts = by_id("desjardins-credit").unwrap().parse(&bytes).unwrap();
    assert_eq!(sts.len(), 1);
    check(&sts[0]);
}

#[test]
fn posting_dates_and_year_rollover() {
    let text = fs::read_to_string(fixtures().join("golden/desjardins_visa.pdfplumber.txt")).unwrap();
    let st = parse_text(&text, NaiveDate::from_ymd_opt(2000, 1, 1).unwrap());
    let first = &st.transactions[0];
    assert_eq!(first.date.to_string(), "2025-12-30");
    assert_eq!(first.posted_date.unwrap().to_string(), "2025-12-31");
    // Posting date of the last row resolves into the statement year.
    let last = st.transactions.last().unwrap();
    assert_eq!(last.posted_date.unwrap().to_string(), "2026-01-21");
}

#[test]
fn legacy_statement_with_sequence_numbers() {
    let text = "DATE DU RELEVÉ Jour 21 Mois 04 Année 2021\n\
        Transactions effectuées avec la carte de X\n\
        02 04 03 04 123 PHARMACIE 12,50\n\
        05 04 06 04 456 PAIEMENT 100,00 CR\n";
    let st = parse_text(text, NaiveDate::from_ymd_opt(2000, 1, 1).unwrap());
    assert_eq!(st.transactions.len(), 2);
    assert_eq!(st.transactions[0].source_seq.as_deref(), Some("123"));
    assert_eq!(st.transactions[0].description, "PHARMACIE");
    assert_eq!(st.transactions[1].amount.minor(), 10000);
}

#[test]
fn lines_before_the_table_header_are_ignored() {
    let text = "DATE DU RELEVÉ Jour 21 Mois 01 Année 2026\n\
        01 01 01 01 NOT YET IN TABLE 9,99\n\
        Transactions effectuées avec la carte de X\n\
        02 01 03 01 REAL ONE 1,00\n";
    let st = parse_text(text, NaiveDate::from_ymd_opt(2000, 1, 1).unwrap());
    assert_eq!(st.transactions.len(), 1);
}

#[test]
fn missing_statement_date_uses_fallback_and_warns() {
    let text = "Transactions effectuées avec la carte de X\n02 01 03 01 SHOP 1,00\n";
    let st = parse_text(text, NaiveDate::from_ymd_opt(2026, 1, 21).unwrap());
    assert_eq!(st.transactions[0].date.to_string(), "2026-01-02");
    assert_eq!(st.errors.len(), 1);
    assert!(st.errors[0].message.contains("statement date not found"));
}

#[test]
fn feb_29_resolves_to_leap_year_instead_of_being_dropped() {
    // Legacy tool raised on date(2025, 2, 29) and skipped the row.
    let text = "DATE DU RELEVÉ Jour 05 Mois 03 Année 2025\n\
        Transactions effectuées avec la carte de X\n\
        29 02 29 02 LEAP DAY 5,00\n";
    let st = parse_text(text, NaiveDate::from_ymd_opt(2000, 1, 1).unwrap());
    assert_eq!(st.transactions[0].date.to_string(), "2024-02-29");
}

#[test]
fn non_pdf_input_is_an_error_not_a_panic() {
    assert!(by_id("desjardins-credit").unwrap().parse(b"definitely not a pdf").is_err());
}
