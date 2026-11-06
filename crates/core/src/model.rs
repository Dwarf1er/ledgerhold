//! Canonical, storage-agnostic types produced by parsers and consumed by the store.

use crate::Money;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    Checking,
    Savings,
    CreditCard,
}

impl AccountKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AccountKind::Checking => "checking",
            AccountKind::Savings => "savings",
            AccountKind::CreditCard => "credit_card",
        }
    }
}

/// One transaction as read from a statement, before it is stored.
/// Sign convention: negative = money out of the account (card purchase, debit).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedTransaction {
    /// Transaction date (when it happened).
    pub date: NaiveDate,
    /// Posting date, when the source provides one.
    pub posted_date: Option<NaiveDate>,
    pub amount: Money,
    pub description: String,
    /// Source-provided unique id (OFX FITID), if any.
    pub fitid: Option<String>,
    /// Source-provided row sequence (e.g. Desjardins), informational only.
    pub source_seq: Option<String>,
    /// Running balance after this transaction, if the source provides it.
    pub balance: Option<Money>,
}

/// A row that could not be parsed. Never silently dropped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowError {
    /// Human-readable locator, e.g. "line 14" or "page 2 line 31".
    pub row_ref: String,
    pub message: String,
}

/// A statement balance observed at a given date (reconciliation checkpoint).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BalanceCheckpoint {
    pub as_of: NaiveDate,
    pub balance: Money,
}

/// Everything a parser extracts for ONE account from one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedStatement {
    /// Account number / masked card number as printed in the source, if known.
    pub account_hint: Option<String>,
    pub kind: AccountKind,
    /// ISO 4217 code.
    pub currency: String,
    pub transactions: Vec<ParsedTransaction>,
    pub errors: Vec<RowError>,
    /// Statement period, if the source states it.
    pub period: Option<(NaiveDate, NaiveDate)>,
    pub checkpoints: Vec<BalanceCheckpoint>,
}

impl ParsedStatement {
    pub fn new(kind: AccountKind, currency: impl Into<String>) -> Self {
        Self {
            account_hint: None,
            kind,
            currency: currency.into(),
            transactions: Vec::new(),
            errors: Vec::new(),
            period: None,
            checkpoints: Vec::new(),
        }
    }

    /// Min/max transaction date, if any transactions exist.
    pub fn date_span(&self) -> Option<(NaiveDate, NaiveDate)> {
        let min = self.transactions.iter().map(|t| t.date).min()?;
        let max = self.transactions.iter().map(|t| t.date).max()?;
        Some((min, max))
    }
}
