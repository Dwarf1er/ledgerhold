//! Wealthsimple chequing CSV. Header row: `date,description,amount,balance`.
//! Amounts are already signed (negative = money out). The file does not say
//! which account it belongs to; the caller picks the account at import time.

use crate::common::{Headers, decode_text, line_of, parse_iso_date};
use crate::{ParseError, StatementParser};
use ledgerhold_core::{AccountKind, Money, ParsedStatement, ParsedTransaction, RowError};

pub struct Wealthsimple;

impl StatementParser for Wealthsimple {
    fn id(&self) -> &'static str {
        "wealthsimple"
    }
    fn description(&self) -> &'static str {
        "Wealthsimple chequing CSV"
    }

    fn parse(&self, input: &[u8]) -> Result<Vec<ParsedStatement>, ParseError> {
        let text = decode_text(input)?;
        let mut rdr = csv::ReaderBuilder::new().flexible(true).from_reader(text.as_bytes());
        let h = Headers::new(rdr.headers()?);
        let (i_date, i_desc, i_amount) = (h.require("date")?, h.require("description")?, h.require("amount")?);
        let i_balance = h.find("balance");

        let mut st = ParsedStatement::new(AccountKind::Checking, "CAD");
        for rec in rdr.records() {
            let rec = rec?;
            let get = |i: usize| rec.get(i).unwrap_or("");
            let row = (|| -> Result<ParsedTransaction, String> {
                let date = parse_iso_date(get(i_date))?;
                let amount = Money::parse(get(i_amount)).map_err(|e| e.to_string())?;
                let balance = match i_balance.map(get).map(str::trim) {
                    None | Some("") => None,
                    Some(b) => Some(Money::parse(b).map_err(|e| format!("balance: {e}"))?),
                };
                Ok(ParsedTransaction {
                    date,
                    posted_date: None,
                    amount,
                    description: get(i_desc).to_string(),
                    fitid: None,
                    source_seq: None,
                    balance,
                })
            })();
            match row {
                Ok(t) => st.transactions.push(t),
                Err(message) => st.errors.push(RowError { row_ref: line_of(&rec), message }),
            }
        }
        Ok(vec![st])
    }
}
