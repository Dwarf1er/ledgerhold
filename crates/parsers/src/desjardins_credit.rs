//! Desjardins Visa PDF statements (Desjardins offers no CSV for credit cards).
//!
//! Two stages so the fragile part is isolated:
//! 1. PDF -> text (`pdf-extract`, pure Rust). Layout differences vs the legacy
//!    pdfplumber are the main porting risk; see `fixtures/` PDF test.
//! 2. [`parse_text`]: pure line scanning, a port of the legacy state machine.
//!
//! Scanning starts after the line containing "Transactions effectuées avec la
//! carte de" and matches transaction-shaped lines:
//! `DD MM DD MM [NNN] description amount[ CR]` (transaction date, posting date,
//! optional 3-digit sequence on statements before 2021-05).
//!
//! Sign: purchases are negative, `CR` (payments/refunds) positive. Dates carry
//! no year; it is taken from the statement date, one year earlier if that would
//! land after the statement date (Dec transactions on a Jan statement).

use crate::{ParseError, StatementParser};
use chrono::{Datelike, Local, NaiveDate};
use ledgerhold_core::{AccountKind, Money, ParsedStatement, ParsedTransaction, RowError};
use regex::Regex;
use std::sync::LazyLock;

const TABLE_HEADER: &str = "Transactions effectuées avec la carte de";

static STATEMENT_DATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)DATE DU RELEV[ÉE].*?Jour\D*(\d{2}).*?Mois\D*(\d{2}).*?Ann[ée]e\D*(\d{4})").unwrap()
});

/// Masked card number, e.g. "4540 XXXX XXXX 1234".
static ACCOUNT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(\d{4}(?:[ -]?X{4}){2}[ -]?\d{4})\b").unwrap());

static TX_LINE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?P<tx_day>\d{2})\s+(?P<tx_month>\d{2})\s+(?P<post_day>\d{2})\s+(?P<post_month>\d{2})\s+(?:(?P<seq>\d{3})\s+)?(?P<desc>.+?)\s+(?P<amount>-?[\d\s]+,\d{2})\s*(?P<credit>CR)?$",
    )
    .unwrap()
});

pub struct DesjardinsCredit;

/// Resolve a day/month (no year) against the statement date: the latest valid
/// date that is not after the statement date, looking at the statement year and
/// the year before.
fn resolve_date(day: u32, month: u32, statement: NaiveDate) -> Option<NaiveDate> {
    [statement.year(), statement.year() - 1]
        .into_iter()
        .filter_map(|y| NaiveDate::from_ymd_opt(y, month, day))
        .find(|d| *d <= statement)
}

fn find_statement_date(text: &str) -> Option<NaiveDate> {
    let c = STATEMENT_DATE_RE.captures(text)?;
    NaiveDate::from_ymd_opt(c[3].parse().ok()?, c[2].parse().ok()?, c[1].parse().ok()?)
}

/// Parse already-extracted statement text. `fallback_date` is used (with a
/// warning in `errors`) when the statement date cannot be found.
pub fn parse_text(text: &str, fallback_date: NaiveDate) -> ParsedStatement {
    let mut st = ParsedStatement::new(AccountKind::CreditCard, "CAD");

    let statement_date = match find_statement_date(text) {
        Some(d) => d,
        None => {
            st.errors.push(RowError {
                row_ref: "statement".into(),
                message: format!("statement date not found; years inferred from {fallback_date}"),
            });
            fallback_date
        }
    };
    st.account_hint = ACCOUNT_RE.captures(text).map(|c| c[1].replace([' ', '-'], ""));

    let mut in_table = false;
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.contains(TABLE_HEADER) {
            in_table = true;
            continue;
        }
        if !in_table {
            continue;
        }
        let Some(m) = TX_LINE_RE.captures(line) else { continue };
        let row_ref = format!("text line {}", n + 1);
        let num = |name: &str| m[name].parse::<u32>().unwrap();

        let Some(date) = resolve_date(num("tx_day"), num("tx_month"), statement_date) else {
            st.errors.push(RowError { row_ref, message: format!("invalid date {:02}/{:02}", num("tx_day"), num("tx_month")) });
            continue;
        };
        let posted_date = resolve_date(num("post_day"), num("post_month"), statement_date);
        let value = match Money::parse(&m["amount"]) {
            Ok(v) => v,
            Err(e) => {
                st.errors.push(RowError { row_ref, message: e.to_string() });
                continue;
            }
        };
        st.transactions.push(ParsedTransaction {
            date,
            posted_date,
            // Legacy quirk kept on purpose: an explicit "-" on a non-CR line flips to positive.
            amount: if m.name("credit").is_some() { value } else { -value },
            description: m["desc"].trim().to_string(),
            fitid: None,
            source_seq: m.name("seq").map(|s| s.as_str().to_string()),
            balance: None,
        });
    }
    st
}

impl StatementParser for DesjardinsCredit {
    fn id(&self) -> &'static str {
        "desjardins-credit"
    }
    fn description(&self) -> &'static str {
        "Desjardins Visa PDF statement"
    }

    fn parse(&self, input: &[u8]) -> Result<Vec<ParsedStatement>, ParseError> {
        let text = pdf_extract::extract_text_from_mem(input).map_err(|e| ParseError::Pdf(e.to_string()))?;
        Ok(vec![parse_text(&text, Local::now().date_naive())])
    }
}
