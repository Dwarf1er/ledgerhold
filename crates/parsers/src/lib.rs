//! Bank statement parsers. Each turns raw file bytes into canonical
//! [`ParsedStatement`]s (one per account/currency found in the file).

mod common;
pub mod wealthsimple;
pub mod wealthsimple_credit;

use ledgerhold_core::ParsedStatement;

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("file is not valid text: {0}")]
    Encoding(String),
    #[error("missing required column {0:?}")]
    MissingColumn(&'static str),
    #[error("csv error: {0}")]
    Csv(#[from] csv::Error),
}

pub trait StatementParser {
    /// Stable id, matches the legacy CLI subcommand names.
    fn id(&self) -> &'static str;
    fn description(&self) -> &'static str;
    /// Parse a whole file. Row-level problems go into `ParsedStatement::errors`;
    /// `Err` is reserved for files that cannot be understood at all.
    fn parse(&self, input: &[u8]) -> Result<Vec<ParsedStatement>, ParseError>;
}

/// All built-in parsers.
pub fn all() -> Vec<Box<dyn StatementParser>> {
    vec![
        Box::new(wealthsimple::Wealthsimple),
        Box::new(wealthsimple_credit::WealthsimpleCredit::default()),
    ]
}

pub fn by_id(id: &str) -> Option<Box<dyn StatementParser>> {
    all().into_iter().find(|p| p.id() == id)
}
