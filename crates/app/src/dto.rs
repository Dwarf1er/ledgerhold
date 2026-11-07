//! JSON-facing types. Money is `i64` minor units (cents); the UI formats it.

use chrono::NaiveDate;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PersonDto {
    pub id: String,
    pub name: String,
    /// Avatar colour name, one of `ledgerhold_store::PERSON_COLORS`.
    pub color: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstitutionDto {
    pub id: String,
    pub name: String,
    pub bank_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AccountDto {
    pub id: String,
    pub name: String,
    pub institution_id: String,
    pub institution_name: String,
    /// `checking` | `savings` | `credit_card`
    pub kind: String,
    pub currency: String,
    pub number: Option<String>,
    pub owner_ids: Vec<String>,
    /// Balance tracked only by snapshots; the UI shows no reconciliation for it.
    pub balance_only: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AccountSummary {
    pub account: AccountDto,
    /// Balance today; `None` until the account has a starting balance.
    pub balance: Option<i64>,
    pub txn_count: usize,
    pub first_date: Option<NaiveDate>,
    pub last_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub people: Vec<PersonDto>,
    pub institutions: Vec<InstitutionDto>,
    pub accounts: Vec<AccountSummary>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnchorDto {
    pub id: String,
    pub date: NaiveDate,
    pub amount: i64,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BatchDto {
    pub id: String,
    pub parser: String,
    pub filename: String,
    pub imported_at: String,
    pub period_start: Option<NaiveDate>,
    pub period_end: Option<NaiveDate>,
    pub rows_new: usize,
    pub rows_duplicate: usize,
    pub rows_error: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiscrepancyDto {
    pub from: NaiveDate,
    pub from_kind: String,
    pub to: NaiveDate,
    pub to_kind: String,
    pub expected: i64,
    pub actual: i64,
    pub difference: i64,
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AccountDetail {
    pub summary: AccountSummary,
    pub anchors: Vec<AnchorDto>,
    pub batches: Vec<BatchDto>,
    pub reconciliation: Vec<DiscrepancyDto>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TxnDto {
    pub id: String,
    pub date: NaiveDate,
    pub posted_date: Option<NaiveDate>,
    pub amount: i64,
    pub description: String,
    /// Balance after this transaction (end-of-day ordering), when the account
    /// has a starting balance.
    pub balance: Option<i64>,
    pub batch_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BalancePoint {
    pub date: NaiveDate,
    pub balance: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRow {
    /// Position in the statement; identifies the row when excluding it from the import.
    pub index: usize,
    pub date: NaiveDate,
    pub amount: i64,
    pub description: String,
    /// `new` | `duplicate`
    pub status: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RowErrorDto {
    pub row_ref: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreviewDto {
    pub file_name: String,
    pub parser: String,
    pub account_hint: Option<String>,
    pub currency: String,
    pub period: Option<(NaiveDate, NaiveDate)>,
    pub rows: Vec<PreviewRow>,
    pub new_count: usize,
    pub duplicate_count: usize,
    pub errors: Vec<RowErrorDto>,
    pub warnings: Vec<String>,
    pub file_already_imported: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportResultDto {
    pub batch_id: Option<String>,
    pub new_count: usize,
    pub duplicate_count: usize,
    pub error_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportResultDto {
    pub path: String,
    pub account_name: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub txn_count: usize,
    pub balance: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ParserDto {
    pub id: String,
    pub description: String,
}
