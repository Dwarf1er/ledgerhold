//! OFX / QFX / QBO files, via the `ledgerhold-ofx` reader.

use crate::{ParseError, StatementParser};
use ledgerhold_core::ParsedStatement;

pub struct Ofx;

impl StatementParser for Ofx {
    fn id(&self) -> &'static str {
        "ofx"
    }
    fn description(&self) -> &'static str {
        "OFX/QFX/QBO statement (any bank)"
    }
    fn parse(&self, input: &[u8]) -> Result<Vec<ParsedStatement>, ParseError> {
        ledgerhold_ofx::read(input).map_err(|e| ParseError::Ofx(e.to_string()))
    }
}
