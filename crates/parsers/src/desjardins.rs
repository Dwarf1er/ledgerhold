//! Desjardins CSV export (chequing / savings). Headerless, positional columns:
//!
//! | idx   | meaning                                              |
//! |-------|------------------------------------------------------|
//! | 1, 2  | folio + account number -> account id `"folio-account"` |
//! | 3     | date `YYYY/MM/DD`                                    |
//! | 4     | sequence number                                      |
//! | 5     | description                                          |
//! | 6..=12| the amount, in ONE of these columns                  |
//! | 13    | running balance                                      |
//!
//! Which amount column is used depends on the row's folio type (EOP, PR1, ...).
//! Rule (from legacy fix d6caf1a): the first non-empty column in 6..=12 is the
//! amount; an **odd** column index means debit (negative), an **even** index
//! means credit (positive).
//!
//! A file may contain several accounts; one statement is returned per account.
//! A leading header row (first non-empty row whose date doesn't parse) is
//! skipped silently, as in the legacy tool. Every other unusable row becomes a
//! [`RowError`] rather than vanishing.

use crate::common::{decode_text, line_of};
use crate::{ParseError, StatementParser};
use chrono::NaiveDate;
use ledgerhold_core::{AccountKind, Money, ParsedStatement, ParsedTransaction, RowError};

pub struct Desjardins;

const COL_FOLIO: usize = 1;
const COL_ACCOUNT: usize = 2;
const COL_DATE: usize = 3;
const COL_SEQ: usize = 4;
const COL_MEMO: usize = 5;
const AMOUNT_COLS: std::ops::RangeInclusive<usize> = 6..=12;
const COL_BALANCE: usize = 13;

/// Signed amount from the first non-empty amount column, or `None` if all empty.
fn extract_amount(rec: &csv::StringRecord) -> Result<Option<Money>, String> {
    for i in AMOUNT_COLS {
        let v = rec.get(i).unwrap_or("").trim();
        if v.is_empty() {
            continue;
        }
        let m = Money::parse(v).map_err(|e| format!("column {i}: {e}"))?;
        return Ok(Some(if i % 2 == 1 { -m } else { m }));
    }
    Ok(None)
}

impl StatementParser for Desjardins {
    fn id(&self) -> &'static str {
        "desjardins"
    }
    fn description(&self) -> &'static str {
        "Desjardins chequing/savings CSV"
    }

    fn parse(&self, input: &[u8]) -> Result<Vec<ParsedStatement>, ParseError> {
        let text = decode_text(input)?;
        let mut rdr = csv::ReaderBuilder::new().has_headers(false).flexible(true).from_reader(text.as_bytes());

        let mut statements: Vec<ParsedStatement> = Vec::new();
        let mut orphan_errors: Vec<RowError> = Vec::new();
        let mut first_row = true;

        for rec in rdr.records() {
            let rec = rec?;
            if rec.iter().all(|f| f.trim().is_empty()) {
                continue;
            }
            let is_first = std::mem::replace(&mut first_row, false);

            let account_id = (rec.len() > COL_ACCOUNT)
                .then(|| format!("{}-{}", rec[COL_FOLIO].trim(), rec[COL_ACCOUNT].trim()));

            let date = NaiveDate::parse_from_str(rec.get(COL_DATE).unwrap_or("").trim(), "%Y/%m/%d");
            let result: Result<Option<ParsedTransaction>, String> = match date {
                Err(_) if is_first => continue, // header row
                Err(_) => Err(format!("invalid date {:?} (expected YYYY/MM/DD)", rec.get(COL_DATE).unwrap_or(""))),
                Ok(date) => (|| {
                    let Some(amount) = extract_amount(&rec)? else {
                        return Ok(None);
                    };
                    let balance = match rec.get(COL_BALANCE).map(str::trim) {
                        None | Some("") => None,
                        Some(b) => Some(Money::parse(b).map_err(|e| format!("balance: {e}"))?),
                    };
                    let seq = rec.get(COL_SEQ).unwrap_or("").trim();
                    Ok(Some(ParsedTransaction {
                        date,
                        posted_date: None,
                        amount,
                        description: rec.get(COL_MEMO).unwrap_or("").trim().to_string(),
                        fitid: None,
                        source_seq: (!seq.is_empty()).then(|| seq.to_string()),
                        balance,
                    }))
                })(),
            };

            let outcome = match result {
                Ok(Some(t)) => Ok(t),
                Ok(None) => Err("no amount in columns 6-12 (informational row?)".to_string()),
                Err(m) => Err(m),
            };

            let Some(id) = account_id else {
                if let Err(message) = outcome {
                    orphan_errors.push(RowError { row_ref: line_of(&rec), message });
                }
                continue;
            };
            let idx = match statements.iter().position(|s| s.account_hint.as_deref() == Some(&id)) {
                Some(i) => i,
                None => {
                    let mut st = ParsedStatement::new(AccountKind::Checking, "CAD");
                    st.account_hint = Some(id);
                    statements.push(st);
                    statements.len() - 1
                }
            };
            match outcome {
                Ok(t) => statements[idx].transactions.push(t),
                Err(message) => statements[idx].errors.push(RowError { row_ref: line_of(&rec), message }),
            }
        }

        if statements.is_empty() {
            statements.push(ParsedStatement::new(AccountKind::Checking, "CAD"));
        }
        statements[0].errors.extend(orphan_errors);
        Ok(statements)
    }
}
