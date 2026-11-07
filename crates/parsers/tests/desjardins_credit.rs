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
fn rows_before_the_header_line_are_kept() {
    // pdf-extract can emit table rows ahead of the header line (content-stream order).
    let text = "DATE DU RELEVÉ 21 01 2026\n\
        01 01 01 01 ROW BEFORE HEADER 9,99\n\
        Transactions effectuées avec la carte de X\n\
        02 01 03 01 ROW AFTER 1,00\n";
    let st = parse_text(text, NaiveDate::from_ymd_opt(2000, 1, 1).unwrap());
    assert_eq!(st.transactions.len(), 2);
}

#[test]
fn no_table_header_means_no_transactions() {
    let text = "DATE DU RELEVÉ 21 01 2026\n01 01 01 01 SOMETHING ELSE 9,99\n";
    let st = parse_text(text, NaiveDate::from_ymd_opt(2000, 1, 1).unwrap());
    assert!(st.transactions.is_empty());
}

#[test]
fn rewards_column_is_not_part_of_the_description() {
    let text = "DATE DU RELEVÉ 08 06 2026\n\
        Transactions effectuées avec la carte de X\n\
        05 06 06 06 EPICERIE      1,00 %        25,00\n\
        05 06 06 06 PAIEMENT 100,00CR\n";
    let st = parse_text(text, NaiveDate::from_ymd_opt(2000, 1, 1).unwrap());
    assert_eq!(st.transactions[0].description, "EPICERIE");
    assert_eq!(st.transactions[0].amount.minor(), -2500);
    assert_eq!(st.transactions[1].amount.minor(), 10000);
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

/// Real statements carry fonts whose `Widths` is longer than `LastChar - FirstChar + 1`; the
/// extractor used to panic on that ("not a PDF this reader understands").
#[test]
fn font_widths_longer_than_last_char_is_tolerated() {
    use lopdf::{Document, Object};
    let mut doc = Document::load_mem(&fs::read(fixtures().join("synthetic/desjardins_visa.pdf")).unwrap()).unwrap();
    let mut patched = 0;
    for obj in doc.objects.values_mut() {
        if let Object::Dictionary(d) = obj
            && d.get(b"Type").and_then(Object::as_name).ok() == Some(b"Font".as_slice())
        {
            d.set("FirstChar", 0);
            d.set("LastChar", 10);
            d.set("Widths", Object::Array(vec![Object::Integer(500); 20]));
            patched += 1;
        }
    }
    assert!(patched > 0);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    let sts = by_id("desjardins-credit").unwrap().parse(&bytes).unwrap();
    check(&sts[0]);
}

#[test]
fn statement_date_as_plain_dd_mm_yyyy() {
    let text = "Date du Jour Mois Année\nrelevé\nDATE DU RELEVÉ 08 06 2026\n\
        Transactions effectuées avec la carte de X\n\
        05 06 06 06 PHARMACIE 12,50\n\
        28 12 29 12 VIEUX ACHAT 3,00\n";
    let st = parse_text(text, NaiveDate::from_ymd_opt(2000, 1, 1).unwrap());
    assert!(st.errors.is_empty(), "{:?}", st.errors);
    assert_eq!(st.transactions[0].date.to_string(), "2026-06-05");
    assert_eq!(st.transactions[1].date.to_string(), "2025-12-28");
}

/// Realistic synthetic statement (see tools/make_synthetic_visa_pdf_realistic.py): rows on both
/// sides of the header line, plain DD MM YYYY statement date, rewards column, glued CR, a font
/// with an over-long Widths array, and noise lines that must not parse.
fn realistic() -> ledgerhold_core::ParsedStatement {
    let bytes = fs::read(fixtures().join("synthetic/desjardins_visa_realistic.pdf")).unwrap();
    let mut sts = by_id("desjardins-credit").unwrap().parse(&bytes).unwrap();
    assert_eq!(sts.len(), 1);
    sts.remove(0)
}

#[test]
fn realistic_statement_yields_every_transaction() {
    let st = realistic();
    assert!(st.errors.is_empty(), "{:?}", st.errors); // includes: statement date was found
    assert_eq!(st.transactions.len(), 22);
    // Net of everything: purchases negative, the three CR rows positive.
    let net: i64 = st.transactions.iter().map(|t| t.amount.minor()).sum();
    assert_eq!(net, -(8412 + 2345 + 6180 + 5230 + 1499 + 1200 + 3725 + 12999 + 1860 + 4815
        + 975 + 3200 + 7640 + 120450 + 2735 + 5510 + 2107 + 645 + 8890) + 45000 + 1000 + 10000);
}

#[test]
fn realistic_statement_details() {
    let st = realistic();
    let by_desc = |d: &str| st.transactions.iter().find(|t| t.description == d).unwrap_or_else(|| panic!("no {d:?}"));

    // Row that sits BEFORE the header line in the extracted text.
    let pharmacy = by_desc("PHARMACIE CENTRALE QUEBEC QC");
    assert_eq!(pharmacy.date.to_string(), "2026-05-13");
    assert_eq!(pharmacy.posted_date.unwrap().to_string(), "2026-05-14");
    assert_eq!(pharmacy.amount.minor(), -2345);

    // Rewards column is not part of the description; the amount is the last number.
    assert_eq!(by_desc("STATION ESSO #4821 LEVIS QC").amount.minor(), -5230);
    // Thousands separator.
    assert_eq!(by_desc("VOYAGES ETE INC QUEBEC QC").amount.minor(), -120450);
    // Punctuation that appears in real merchant names survives.
    by_desc("PAYPAL *JEUXVIDEO 4029357733 ON");
    by_desc("LIBRAIRIE A.B.C. QUEBEC QC");
    by_desc("RESTO-BAR LE PORT (QC) QUEBEC QC");

    // Payments/refunds are credits, with CR glued to the amount or in its own cell.
    let paid: Vec<_> = st.transactions.iter().filter(|t| t.description.starts_with("PAIEMENT")).collect();
    assert_eq!(paid.len(), 2);
    assert!(paid.iter().all(|t| t.amount.minor() > 0));
    assert_eq!(by_desc("REMBOURSEMENT QUINCAILLERIE").amount.minor(), 1000);

    // Noise (rate table, subtotals, due date, totals) never becomes a transaction.
    assert!(st.transactions.iter().all(|t| !t.description.contains("Taux") && !t.description.contains("Total")));
}

#[test]
fn realistic_statement_card_number() {
    // Real statements mask the card with asterisks ("4510 12** **** 7777"), not X's.
    assert_eq!(realistic().account_hint.as_deref(), Some("451012******7777"));
}
