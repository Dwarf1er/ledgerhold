//! Wealthsimple credit card CSV.
//! Columns: `transaction_date, post_date, type, details, amount, currency`.
//!
//! The CSV's own sign is ignored (as in the legacy tool): the sign comes from
//! the row `type`. Types starting with one of `credit_prefixes` increase the
//! balance (positive); everything else is a charge (negative). One statement is
//! returned per currency found, so USD rows never get mixed into a CAD account.

use crate::common::{Headers, decode_text, line_of, parse_iso_date};
use crate::{ParseError, StatementParser};
use ledgerhold_core::{AccountKind, Money, ParsedStatement, ParsedTransaction, RowError};

pub const DEFAULT_CREDIT_PREFIXES: &[&str] =
    &["payment", "refund", "return", "credit", "reward", "cashback", "reversal"];

pub struct WealthsimpleCredit {
    pub credit_prefixes: Vec<String>,
}

impl Default for WealthsimpleCredit {
    fn default() -> Self {
        Self { credit_prefixes: DEFAULT_CREDIT_PREFIXES.iter().map(|s| s.to_string()).collect() }
    }
}

impl StatementParser for WealthsimpleCredit {
    fn id(&self) -> &'static str {
        "wealthsimple-credit"
    }
    fn description(&self) -> &'static str {
        "Wealthsimple credit card CSV"
    }

    fn parse(&self, input: &[u8]) -> Result<Vec<ParsedStatement>, ParseError> {
        let text = decode_text(input)?;
        let mut rdr = csv::ReaderBuilder::new().flexible(true).from_reader(text.as_bytes());
        let h = Headers::new(rdr.headers()?);
        let i_date = h.require("transaction_date")?;
        let i_type = h.require("type")?;
        let i_details = h.require("details")?;
        let i_amount = h.require("amount")?;
        let i_post = h.find("post_date");
        let i_cur = h.find("currency");

        let mut statements: Vec<ParsedStatement> = Vec::new();
        let mut errors: Vec<RowError> = Vec::new();

        for rec in rdr.records() {
            let rec = rec?;
            let get = |i: usize| rec.get(i).unwrap_or("");
            let row = (|| -> Result<(String, ParsedTransaction), String> {
                let date = parse_iso_date(get(i_date))?;
                let posted_date = match i_post.map(get).map(str::trim) {
                    None | Some("") => None,
                    Some(p) => Some(parse_iso_date(p).map_err(|e| format!("post_date: {e}"))?),
                };
                let magnitude = Money::parse(get(i_amount)).map_err(|e| e.to_string())?.abs();
                let kind = get(i_type).trim().to_lowercase();
                let is_credit = self.credit_prefixes.iter().any(|p| kind.starts_with(p.as_str()));
                let currency = match i_cur.map(get).map(str::trim) {
                    None | Some("") => "CAD".to_string(),
                    Some(c) => c.to_uppercase(),
                };
                Ok((
                    currency,
                    ParsedTransaction {
                        date,
                        posted_date,
                        amount: if is_credit { magnitude } else { -magnitude },
                        description: get(i_details).trim().to_string(),
                        fitid: None,
                        source_seq: None,
                        balance: None,
                    },
                ))
            })();
            match row {
                Ok((currency, t)) => {
                    let idx = match statements.iter().position(|s| s.currency == currency) {
                        Some(i) => i,
                        None => {
                            statements.push(ParsedStatement::new(AccountKind::CreditCard, currency));
                            statements.len() - 1
                        }
                    };
                    statements[idx].transactions.push(t);
                }
                Err(message) => errors.push(RowError { row_ref: line_of(&rec), message }),
            }
        }

        if statements.is_empty() {
            statements.push(ParsedStatement::new(AccountKind::CreditCard, "CAD"));
        }
        statements[0].errors = errors;
        Ok(statements)
    }
}
