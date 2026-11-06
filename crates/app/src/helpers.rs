//! Small pure helpers shared by the CLI and the desktop app.

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use ledgerhold_core::ParsedStatement;
use ledgerhold_store::Account;
use std::path::PathBuf;

/// Default database location, shared by CLI and GUI so both see the same data.
pub fn default_db_path() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("ledgerhold").join("ledgerhold.sqlite")
}

pub fn norm_number(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace() && *c != '-').collect::<String>().to_uppercase()
}

/// `"Joint chequing"` -> `"joint-chequing"`.
pub fn kebab(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

pub fn month_bounds(y: i32, m: u32) -> Option<(NaiveDate, NaiveDate)> {
    let first = NaiveDate::from_ymd_opt(y, m, 1)?;
    let next = if m == 12 { NaiveDate::from_ymd_opt(y + 1, 1, 1)? } else { NaiveDate::from_ymd_opt(y, m + 1, 1)? };
    Some((first, next.pred_opt()?))
}

/// `"2026-01"` -> first and last day of that month.
pub fn parse_month(s: &str) -> Result<(NaiveDate, NaiveDate)> {
    let (y, m) = s.split_once('-').context("month must look like 2026-01")?;
    let (y, m): (i32, u32) = (y.parse().context("bad year")?, m.parse().context("bad month")?);
    month_bounds(y, m).with_context(|| format!("invalid month {s:?}"))
}

pub fn export_file_name(account_name: &str, start: NaiveDate, end: NaiveDate) -> String {
    format!("{}-{}-{}.ofx", kebab(account_name), start.format("%Y%m%d"), end.format("%Y%m%d"))
}

/// Pick the statement in a parsed file that belongs to `account`: by account
/// number when the account has one and the file mentions it, then by currency.
/// Returns the statement plus an optional warning for the user.
pub fn select_statement<'a>(
    account: &Account,
    statements: &'a [ParsedStatement],
) -> Result<(&'a ParsedStatement, Option<String>)> {
    if statements.is_empty() {
        bail!("no statements found in file");
    }
    let mut cands: Vec<&ParsedStatement> = statements.iter().collect();
    if let Some(ext) = &account.external_account_id {
        let matching: Vec<_> = cands
            .iter()
            .copied()
            .filter(|s| s.account_hint.as_deref().is_some_and(|h| norm_number(h) == norm_number(ext)))
            .collect();
        if !matching.is_empty() {
            cands = matching;
        }
    }
    let by_currency: Vec<_> = cands.iter().copied().filter(|s| s.currency == account.currency).collect();
    let describe = |ss: &[&ParsedStatement]| {
        ss.iter()
            .map(|s| format!("{} ({})", s.account_hint.as_deref().unwrap_or("unknown account"), s.currency))
            .collect::<Vec<_>>()
            .join(", ")
    };
    match by_currency.as_slice() {
        [s] => {
            let warning = match (&s.account_hint, &account.external_account_id) {
                (Some(h), Some(ext)) if norm_number(h) != norm_number(ext) => {
                    Some(format!("file is for {h}, but this account's number is {ext}"))
                }
                _ => None,
            };
            Ok((s, warning))
        }
        [] => bail!(
            "no statement in {} currency (file has: {}); import into an account with the right currency",
            account.currency,
            describe(&cands)
        ),
        many => bail!(
            "file has several accounts: {}; set the account number on the account so the right one is picked",
            describe(many)
        ),
    }
}
