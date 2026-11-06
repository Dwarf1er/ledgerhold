use chrono::NaiveDate;
use ledgerhold_core::{AccountKind, Money};
use ledgerhold_ofx::{OfxAccount, OfxExport, OfxTxn, read, write};
use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::PathBuf};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}
fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

#[derive(Deserialize)]
struct G {
    date: String,
    amount_cents: i64,
    memo: String,
}
fn golden(name: &str, key: &str) -> Vec<G> {
    let mut m: BTreeMap<String, Vec<G>> =
        serde_json::from_slice(&fs::read(fixtures().join("golden").join(name)).unwrap()).unwrap();
    m.remove(key).unwrap()
}

// ---- reading files produced by the legacy Python writer -------------------

#[test]
fn reads_legacy_bank_output_equal_to_python_golden() {
    let sts = read(&fs::read(fixtures().join("golden/legacy_desjardins_eop.ofx")).unwrap()).unwrap();
    assert_eq!(sts.len(), 1);
    let st = &sts[0];
    assert_eq!(st.account_hint.as_deref(), Some("EOP-12345"));
    assert_eq!(st.kind, AccountKind::Checking);
    let exp = golden("desjardins.json", "EOP-12345");
    assert_eq!(st.transactions.len(), exp.len());
    for (t, e) in st.transactions.iter().zip(&exp) {
        assert_eq!(t.date.to_string(), e.date);
        assert_eq!(t.amount.minor(), e.amount_cents);
        assert_eq!(t.description, e.memo);
        assert!(t.fitid.as_deref().unwrap().starts_with("2026"), "legacy FITIDs are YYYYMMDD-NNN");
    }
    assert!(st.errors.is_empty(), "{:?}", st.errors);
}

#[test]
fn reads_legacy_credit_card_output() {
    let sts = read(&fs::read(fixtures().join("golden/legacy_desjardins_visa.ofx")).unwrap()).unwrap();
    let st = &sts[0];
    assert_eq!(st.kind, AccountKind::CreditCard);
    assert_eq!(st.account_hint.as_deref(), Some("4540XXXXXXXX1234"));
    let exp = golden("desjardins_visa.json", "4540XXXXXXXX1234");
    assert_eq!(st.transactions.len(), exp.len());
    for (t, e) in st.transactions.iter().zip(&exp) {
        assert_eq!((t.date.to_string(), t.amount.minor(), t.description.as_str()), (e.date.clone(), e.amount_cents, e.memo.as_str()));
    }
}

// ---- write -> read round trip ---------------------------------------------

fn sample(kind: AccountKind, id: &str) -> OfxExport {
    let tx = |day, cents, fitid: &str, desc: &str| OfxTxn {
        date: d(2026, 1, day),
        amount: Money(cents),
        fitid: fitid.into(),
        description: desc.into(),
    };
    OfxExport {
        account: OfxAccount { bank_id: "815".into(), account_id: id.into(), kind, currency: "USD".into() },
        start: d(2026, 1, 1),
        end: d(2026, 1, 31),
        transactions: vec![
            tx(2, 250000, "A1", "Payroll, ACME & Sons"),
            tx(3, -450, "A2", "Café <déjà vu> \"quoted\""),
            tx(3, -450, "A3", "Café <déjà vu> \"quoted\""),
            tx(9, -123456, "A4", &"X".repeat(100)),
        ],
        ledger_balance: Some((d(2026, 1, 31), Money(-98765))),
    }
}

#[test]
fn roundtrip_preserves_everything_that_matters() {
    let exports = [sample(AccountKind::Savings, "111"), sample(AccountKind::CreditCard, "4540XXXXXXXX9999")];
    let text = write(&exports, d(2026, 2, 1).and_hms_opt(0, 0, 0).unwrap());
    let sts = read(text.as_bytes()).unwrap();
    assert_eq!(sts.len(), 2);
    for (st, e) in sts.iter().zip(&exports) {
        assert_eq!(st.account_hint.as_deref(), Some(e.account.account_id.as_str()));
        assert_eq!(st.kind, e.account.kind);
        assert_eq!(st.currency, "USD");
        assert_eq!(st.period, Some((e.start, e.end)));
        assert_eq!(st.checkpoints.len(), 1);
        assert_eq!(st.checkpoints[0].balance, Money(-98765));
        assert_eq!(st.checkpoints[0].as_of, d(2026, 1, 31));
        assert_eq!(st.transactions.len(), e.transactions.len());
        for (t, x) in st.transactions.iter().zip(&e.transactions) {
            assert_eq!((t.date, t.amount, t.fitid.as_deref(), t.description.as_str()),
                       (x.date, x.amount, Some(x.fitid.as_str()), x.description.as_str()));
        }
        assert!(st.errors.is_empty());
    }
}

// ---- reading third-party styles --------------------------------------------

#[test]
fn reads_xml_ofx2_on_a_single_line_with_entities() {
    let xml = r#"<?xml version="1.0"?><?OFX OFXHEADER="200" VERSION="211"?><OFX><SIGNONMSGSRSV1><SONRS><STATUS><CODE>0</CODE><SEVERITY>INFO</SEVERITY></STATUS><DTSERVER>20260101</DTSERVER></SONRS></SIGNONMSGSRSV1><BANKMSGSRSV1><STMTTRNRS><TRNUID>1</TRNUID><STMTRS><CURDEF>CAD</CURDEF><BANKACCTFROM><BANKID>1</BANKID><ACCTID>777</ACCTID><ACCTTYPE>SAVINGS</ACCTTYPE></BANKACCTFROM><BANKTRANLIST><DTSTART>20260101</DTSTART><DTEND>20260131</DTEND><STMTTRN><TRNTYPE>DEBIT</TRNTYPE><DTPOSTED>20260105120000[-5:EST]</DTPOSTED><TRNAMT>-12,34</TRNAMT><FITID>X1</FITID><NAME>Shop &amp; Go</NAME></STMTTRN></BANKTRANLIST><LEDGERBAL><BALAMT>100.00</BALAMT><DTASOF>20260131</DTASOF></LEDGERBAL><AVAILBAL><BALAMT>999.00</BALAMT><DTASOF>20260131</DTASOF></AVAILBAL></STMTRS></STMTTRNRS></BANKMSGSRSV1></OFX>"#;
    let sts = read(xml.as_bytes()).unwrap();
    let st = &sts[0];
    assert_eq!(st.kind, AccountKind::Savings);
    assert_eq!(st.transactions[0].description, "Shop & Go");
    assert_eq!(st.transactions[0].amount, Money(-1234));
    assert_eq!(st.transactions[0].date, d(2026, 1, 5));
    assert_eq!(st.checkpoints.len(), 1, "AVAILBAL must not be used as the ledger balance");
    assert_eq!(st.checkpoints[0].balance, Money(10000));
}

#[test]
fn reads_cp1252_sgml_with_crlf() {
    let mut bytes = b"OFXHEADER:100\r\nDATA:OFXSGML\r\nVERSION:102\r\nCHARSET:1252\r\n\r\n<OFX>\r\n<BANKMSGSRSV1><STMTTRNRS><STMTRS><CURDEF>CAD\r\n<BANKACCTFROM><BANKID>1<ACCTID>5<ACCTTYPE>CHECKING</BANKACCTFROM>\r\n<BANKTRANLIST><STMTTRN><TRNTYPE>DEBIT<DTPOSTED>20260105<TRNAMT>-1.00<FITID>1<NAME>Caf".to_vec();
    bytes.push(0xE9);
    bytes.extend_from_slice(b"\r\n</STMTTRN></BANKTRANLIST></STMTRS></STMTTRNRS></BANKMSGSRSV1></OFX>");
    let sts = read(&bytes).unwrap();
    assert_eq!(sts[0].transactions[0].description, "Café");
}

#[test]
fn name_and_memo_combine_unless_one_extends_the_other() {
    let mk = |name: &str, memo: &str| {
        format!("<OFX><STMTRS><BANKACCTFROM><ACCTID>1</BANKACCTFROM><BANKTRANLIST><STMTTRN><DTPOSTED>20260105<TRNAMT>-1.00<NAME>{name}<MEMO>{memo}</STMTTRN></BANKTRANLIST></STMTRS></OFX>")
    };
    let desc = |s: String| read(s.as_bytes()).unwrap()[0].transactions[0].description.clone();
    assert_eq!(desc(mk("ACME", "ACME PAYROLL")), "ACME PAYROLL");
    assert_eq!(desc(mk("ACME", "January pay")), "ACME January pay");
}

#[test]
fn dtuser_wins_over_dtposted_for_the_transaction_date() {
    let s = "<OFX><STMTRS><BANKACCTFROM><ACCTID>1</BANKACCTFROM><BANKTRANLIST><STMTTRN><DTPOSTED>20260110<DTUSER>20260108<TRNAMT>-1.00<NAME>X</STMTTRN></BANKTRANLIST></STMTRS></OFX>";
    let t = &read(s.as_bytes()).unwrap()[0].transactions[0];
    assert_eq!((t.date, t.posted_date), (d(2026, 1, 8), Some(d(2026, 1, 10))));
}

#[test]
fn broken_transactions_are_reported_not_dropped_silently() {
    let s = "<OFX><STMTRS><BANKACCTFROM><ACCTID>1</BANKACCTFROM><BANKTRANLIST>\
        <STMTTRN><DTPOSTED>20260110<NAME>no amount</STMTTRN>\
        <STMTTRN><DTPOSTED>20260111<TRNAMT>-2.00<NAME>ok</STMTTRN></BANKTRANLIST></STMTRS></OFX>";
    let st = &read(s.as_bytes()).unwrap()[0];
    assert_eq!(st.transactions.len(), 1);
    assert_eq!(st.errors.len(), 1);
    assert_eq!(st.errors[0].row_ref, "transaction #1");
}

#[test]
fn multiple_statements_in_one_file() {
    let s = "<OFX><BANKMSGSRSV1><STMTRS><BANKACCTFROM><ACCTID>A</BANKACCTFROM><BANKTRANLIST><STMTTRN><DTPOSTED>20260110<TRNAMT>-1.00<NAME>x</STMTTRN></BANKTRANLIST></STMTRS>\
        <STMTRS><BANKACCTFROM><ACCTID>B</BANKACCTFROM><BANKTRANLIST><STMTTRN><DTPOSTED>20260111<TRNAMT>-2.00<NAME>y</STMTTRN></BANKTRANLIST></STMTRS></BANKMSGSRSV1></OFX>";
    let sts = read(s.as_bytes()).unwrap();
    let ids: Vec<_> = sts.iter().map(|s| s.account_hint.clone().unwrap()).collect();
    assert_eq!(ids, ["A", "B"]);
    assert_eq!(sts[1].transactions[0].amount, Money(-200));
}

#[test]
fn tolerates_legacy_unescaped_angle_brackets_without_crashing() {
    // The legacy tool wrote "Rent <Jan>" raw; we can't recover the text but must not panic or lose the row.
    let s = "<OFX><STMTRS><BANKACCTFROM><ACCTID>1</BANKACCTFROM><BANKTRANLIST><STMTTRN><DTPOSTED>20260110<TRNAMT>-1.00<NAME>Rent <Jan><MEMO>Rent <Jan></STMTTRN></BANKTRANLIST></STMTRS></OFX>";
    assert_eq!(read(s.as_bytes()).unwrap()[0].transactions.len(), 1);
}

#[test]
fn non_ofx_is_an_error() {
    assert!(read(b"hello world").is_err());
}
