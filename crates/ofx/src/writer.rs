//! OFX 1.02 SGML writer.
//!
//! Improvements over the legacy tool: values are escaped (`& < >`), the bank id
//! and account type come from the account (not hardcoded), the statement range
//! is the requested range (not min/max of the data), a `LEDGERBAL` is emitted
//! when known, `STATUS` precedes `DTSERVER` as the spec requires, and dates carry
//! a noon time + GMT so importers in other timezones don't shift the day.

use chrono::{NaiveDate, NaiveDateTime};
use ledgerhold_core::{AccountKind, Money};
use std::fmt::Write as _;

#[derive(Debug, Clone)]
pub struct OfxAccount {
    /// Routing/institution id. Required by `BANKACCTFROM` (ignored for cards).
    pub bank_id: String,
    pub account_id: String,
    pub kind: AccountKind,
    pub currency: String,
}

#[derive(Debug, Clone)]
pub struct OfxTxn {
    pub date: NaiveDate,
    pub amount: Money,
    pub fitid: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct OfxExport {
    pub account: OfxAccount,
    /// Requested statement range, written as `DTSTART`/`DTEND`.
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub transactions: Vec<OfxTxn>,
    /// Balance at end of day on the given date -> `LEDGERBAL`.
    pub ledger_balance: Option<(NaiveDate, Money)>,
}

fn date(d: NaiveDate) -> String {
    format!("{}120000[0:GMT]", d.format("%Y%m%d"))
}

/// Escape and flatten text for an SGML leaf value. Truncates to `max` chars
/// *before* escaping, so entities are never cut in half.
fn text(s: &str, max: usize) -> String {
    let mut out = String::new();
    for c in s.chars().take(max) {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out.trim().to_string()
}

/// Write one OFX document containing every export (bank statements first,
/// then credit cards, as the spec groups them by message set).
pub fn write(exports: &[OfxExport], server_time: NaiveDateTime) -> String {
    let mut o = String::new();
    for l in [
        "OFXHEADER:100",
        "DATA:OFXSGML",
        "VERSION:102",
        "SECURITY:NONE",
        "ENCODING:UTF-8",
        "CHARSET:UTF-8",
        "COMPRESSION:NONE",
        "OLDFILEUID:NONE",
        "NEWFILEUID:NONE",
        "",
        "<OFX>",
        "<SIGNONMSGSRSV1>",
        "<SONRS>",
        "<STATUS><CODE>0<SEVERITY>INFO</STATUS>",
    ] {
        o.push_str(l);
        o.push('\n');
    }
    let _ = writeln!(o, "<DTSERVER>{}[0:GMT]", server_time.format("%Y%m%d%H%M%S"));
    o.push_str("<LANGUAGE>ENG\n</SONRS>\n</SIGNONMSGSRSV1>\n");

    let is_card = |e: &&OfxExport| e.account.kind == AccountKind::CreditCard;
    let banks: Vec<_> = exports.iter().filter(|e| !is_card(e)).collect();
    let cards: Vec<_> = exports.iter().filter(is_card).collect();
    let mut uid = 0;

    if !banks.is_empty() {
        o.push_str("<BANKMSGSRSV1>\n");
        for e in banks {
            uid += 1;
            statement(&mut o, e, uid, false);
        }
        o.push_str("</BANKMSGSRSV1>\n");
    }
    if !cards.is_empty() {
        o.push_str("<CREDITCARDMSGSRSV1>\n");
        for e in cards {
            uid += 1;
            statement(&mut o, e, uid, true);
        }
        o.push_str("</CREDITCARDMSGSRSV1>\n");
    }
    o.push_str("</OFX>\n");
    o
}

fn statement(o: &mut String, e: &OfxExport, uid: u32, card: bool) {
    let (rs, stmtrs, from) =
        if card { ("CCSTMTTRNRS", "CCSTMTRS", "CCACCTFROM") } else { ("STMTTRNRS", "STMTRS", "BANKACCTFROM") };
    let _ = writeln!(o, "<{rs}>\n<TRNUID>{uid}\n<STATUS><CODE>0<SEVERITY>INFO</STATUS>");
    let _ = writeln!(o, "<{stmtrs}>\n<CURDEF>{}", text(&e.account.currency, 3));
    let _ = writeln!(o, "<{from}>");
    if !card {
        let _ = writeln!(o, "<BANKID>{}", text(&e.account.bank_id, 9));
    }
    let _ = writeln!(o, "<ACCTID>{}", text(&e.account.account_id, 22));
    if !card {
        let ty = if e.account.kind == AccountKind::Savings { "SAVINGS" } else { "CHECKING" };
        let _ = writeln!(o, "<ACCTTYPE>{ty}");
    }
    let _ = writeln!(o, "</{from}>\n<BANKTRANLIST>\n<DTSTART>{}\n<DTEND>{}", date(e.start), date(e.end));
    for t in &e.transactions {
        let kind = if t.amount > Money::ZERO { "CREDIT" } else { "DEBIT" };
        let _ = writeln!(
            o,
            "<STMTTRN>\n<TRNTYPE>{kind}\n<DTPOSTED>{}\n<TRNAMT>{}\n<FITID>{}\n<NAME>{}\n<MEMO>{}\n</STMTTRN>",
            date(t.date),
            t.amount,
            text(&t.fitid, 255),
            text(&t.description, 32),
            text(&t.description, 255),
        );
    }
    o.push_str("</BANKTRANLIST>\n");
    if let Some((asof, bal)) = e.ledger_balance {
        let _ = writeln!(o, "<LEDGERBAL>\n<BALAMT>{bal}\n<DTASOF>{}\n</LEDGERBAL>", date(asof));
    }
    let _ = writeln!(o, "</{stmtrs}>\n</{rs}>");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 1, day).unwrap()
    }
    fn now() -> NaiveDateTime {
        d(31).and_hms_opt(8, 0, 0).unwrap()
    }
    fn export(kind: AccountKind) -> OfxExport {
        OfxExport {
            account: OfxAccount { bank_id: "815".into(), account_id: "12345".into(), kind, currency: "CAD".into() },
            start: d(1),
            end: d(31),
            transactions: vec![OfxTxn {
                date: d(6),
                amount: Money(-120000),
                fitid: "F1".into(),
                description: "Rent & utilities <Jan>".into(),
            }],
            ledger_balance: Some((d(31), Money(160683))),
        }
    }

    #[test]
    fn escapes_special_characters() {
        let s = write(&[export(AccountKind::Checking)], now());
        assert!(s.contains("<MEMO>Rent &amp; utilities &lt;Jan&gt;\n"));
        assert!(!s.contains("<Jan>"));
    }

    #[test]
    fn bank_statement_has_real_ids_range_and_balance() {
        let s = write(&[export(AccountKind::Savings)], now());
        assert!(s.contains("<BANKID>815\n"));
        assert!(s.contains("<ACCTTYPE>SAVINGS\n"));
        assert!(s.contains("<DTSTART>20260101120000[0:GMT]\n<DTEND>20260131120000[0:GMT]\n"));
        assert!(s.contains("<LEDGERBAL>\n<BALAMT>1606.83\n<DTASOF>20260131120000[0:GMT]\n</LEDGERBAL>"));
        assert!(s.contains("<TRNTYPE>DEBIT\n"));
        assert!(s.contains("<TRNAMT>-1200.00\n"));
    }

    #[test]
    fn status_precedes_dtserver() {
        let s = write(&[export(AccountKind::Checking)], now());
        assert!(s.find("<STATUS>").unwrap() < s.find("<DTSERVER>").unwrap());
    }

    #[test]
    fn credit_card_uses_cc_aggregates_without_bankid() {
        let s = write(&[export(AccountKind::CreditCard)], now());
        assert!(s.contains("<CREDITCARDMSGSRSV1>") && s.contains("<CCSTMTRS>") && s.contains("<CCACCTFROM>"));
        assert!(!s.contains("BANKID") && !s.contains("<BANKMSGSRSV1>"));
    }

    #[test]
    fn mixed_accounts_get_distinct_trnuids_and_banks_come_first() {
        let s = write(&[export(AccountKind::CreditCard), export(AccountKind::Checking)], now());
        assert!(s.find("<BANKMSGSRSV1>").unwrap() < s.find("<CREDITCARDMSGSRSV1>").unwrap());
        assert!(s.contains("<TRNUID>1\n") && s.contains("<TRNUID>2\n"));
    }

    #[test]
    fn name_is_truncated_to_32_chars_without_cutting_entities() {
        let mut e = export(AccountKind::Checking);
        e.transactions[0].description = "A".repeat(31) + "&B";
        let s = write(&[e], now());
        let name = s.lines().find(|l| l.starts_with("<NAME>")).unwrap();
        assert_eq!(name, format!("<NAME>{}&amp;", "A".repeat(31)));
    }

    #[test]
    fn newlines_in_descriptions_are_flattened() {
        let mut e = export(AccountKind::Checking);
        e.transactions[0].description = "line1\nline2".into();
        let s = write(&[e], now());
        assert!(s.contains("<MEMO>line1 line2\n"));
    }

    #[test]
    fn empty_range_still_valid() {
        let mut e = export(AccountKind::Checking);
        e.transactions.clear();
        e.ledger_balance = None;
        let s = write(&[e], now());
        assert!(s.contains("<BANKTRANLIST>\n<DTSTART>") && !s.contains("<STMTTRN>") && !s.contains("LEDGERBAL"));
    }
}
