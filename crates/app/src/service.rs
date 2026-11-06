use crate::dto::*;
use crate::helpers::{export_file_name, select_statement};
use anyhow::{Context, Result, bail};
use chrono::{NaiveDate, NaiveDateTime};
use ledgerhold_core::dedup::{Disposition, sha256_hex};
use ledgerhold_core::{AccountKind, Money, ParsedStatement};
use ledgerhold_store::{Account, ImportMeta, NewAccount, Store, StoreError};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn account_dto(a: &Account, institutions: &HashMap<String, String>) -> AccountDto {
    AccountDto {
        id: a.id.clone(),
        name: a.name.clone(),
        institution_id: a.institution_id.clone(),
        institution_name: institutions.get(&a.institution_id).cloned().unwrap_or_default(),
        kind: a.kind.as_str().to_string(),
        currency: a.currency.clone(),
        number: a.external_account_id.clone(),
        owner_ids: a.owners.clone(),
    }
}

fn institution_names(store: &Store) -> Result<HashMap<String, String>> {
    Ok(store.list_institutions()?.into_iter().map(|i| (i.id, i.name)).collect())
}

fn summary(store: &Store, a: &Account, names: &HashMap<String, String>, today: NaiveDate) -> Result<AccountSummary> {
    let stats = store.account_stats(&a.id)?;
    let balance = match store.balance_at(&a.id, today) {
        Ok(r) => Some(r.balance.minor()),
        Err(StoreError::NoAnchor) => None,
        Err(e) => return Err(e.into()),
    };
    Ok(AccountSummary {
        account: account_dto(a, names),
        balance,
        txn_count: stats.count,
        first_date: stats.first,
        last_date: stats.last,
    })
}

pub fn overview(store: &Store, today: NaiveDate) -> Result<Overview> {
    let names = institution_names(store)?;
    let accounts = store
        .list_accounts()?
        .iter()
        .map(|a| summary(store, a, &names, today))
        .collect::<Result<Vec<_>>>()?;
    Ok(Overview {
        people: store.list_people()?.into_iter().map(|p| PersonDto { id: p.id, name: p.name }).collect(),
        institutions: store
            .list_institutions()?
            .into_iter()
            .map(|i| InstitutionDto { id: i.id, name: i.name, bank_id: i.ofx_bank_id })
            .collect(),
        accounts,
    })
}

pub fn account_detail(store: &Store, account_id: &str, today: NaiveDate) -> Result<AccountDetail> {
    let a = store.get_account(account_id)?;
    let names = institution_names(store)?;
    Ok(AccountDetail {
        summary: summary(store, &a, &names, today)?,
        anchors: store
            .list_anchors(account_id)?
            .into_iter()
            .map(|x| AnchorDto { id: x.id, date: x.as_of, amount: x.amount.minor(), note: x.note })
            .collect(),
        batches: store
            .list_batches(account_id)?
            .into_iter()
            .map(|b| BatchDto {
                id: b.id,
                parser: b.parser,
                filename: b.filename,
                imported_at: b.imported_at,
                period_start: b.period.map(|p| p.0),
                period_end: b.period.map(|p| p.1),
                rows_new: b.rows_new,
                rows_duplicate: b.rows_duplicate,
                rows_error: b.rows_error,
            })
            .collect(),
        reconciliation: store
            .reconcile(account_id)?
            .into_iter()
            .map(|d| DiscrepancyDto {
                from: d.from,
                from_kind: kind_str(d.from_kind),
                to: d.to,
                to_kind: kind_str(d.to_kind),
                expected: d.expected.minor(),
                actual: d.actual.minor(),
                difference: d.difference.minor(),
                ok: d.is_ok(),
            })
            .collect(),
    })
}

fn kind_str(k: ledgerhold_store::PointKind) -> String {
    match k {
        ledgerhold_store::PointKind::Anchor => "anchor".into(),
        ledgerhold_store::PointKind::Statement => "statement".into(),
    }
}

/// Transactions newest first. Running balances are computed over the whole
/// history first, then filters apply, so filtered views still show true balances.
pub fn transactions(
    store: &Store,
    account_id: &str,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    search: Option<&str>,
) -> Result<Vec<TxnDto>> {
    let all = store.list_transactions(account_id, None, None)?;
    let mut running: Option<i64> = all.first().and_then(|first| {
        let before = first.date.pred_opt()?;
        store.balance_at(account_id, before).ok().map(|r| r.balance.minor())
    });
    let needle = search.map(|s| s.trim().to_uppercase()).filter(|s| !s.is_empty());
    let mut out = Vec::new();
    for t in all {
        if let Some(r) = running.as_mut() {
            *r += t.amount.minor();
        }
        let in_range = from.is_none_or(|f| t.date >= f) && to.is_none_or(|e| t.date <= e);
        let matches = needle.as_ref().is_none_or(|n| {
            t.description.to_uppercase().contains(n.as_str()) || t.amount.to_string().contains(n.as_str())
        });
        if in_range && matches {
            out.push(TxnDto {
                id: t.id,
                date: t.date,
                posted_date: t.posted_date,
                amount: t.amount.minor(),
                description: t.description,
                balance: running,
                batch_id: t.import_batch_id,
            });
        }
    }
    out.reverse();
    Ok(out)
}

pub fn balance_series(store: &Store, account_id: &str, start: NaiveDate, end: NaiveDate) -> Result<Vec<BalancePoint>> {
    Ok(store
        .daily_balances(account_id, start, end)?
        .into_iter()
        .map(|(date, m)| BalancePoint { date, balance: m.minor() })
        .collect())
}

// ---- import ---------------------------------------------------------------------

struct Loaded {
    parser_id: String,
    file_name: String,
    sha: String,
    statement: ParsedStatement,
    warnings: Vec<String>,
}

fn parser_for(path: &Path, parser: Option<&str>) -> Result<String> {
    if let Some(p) = parser.filter(|p| !p.is_empty()) {
        return Ok(p.to_string());
    }
    match path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() {
        Some("ofx" | "qfx" | "qbo") => Ok("ofx".into()),
        Some("pdf") => Ok("desjardins-credit".into()),
        _ => bail!("can't tell the file format; choose a parser"),
    }
}

fn load(store: &Store, account_id: &str, path: &Path, parser: Option<&str>) -> Result<Loaded> {
    let account = store.get_account(account_id)?;
    let parser_id = parser_for(path, parser)?;
    let p = ledgerhold_parsers::by_id(&parser_id).with_context(|| format!("unknown parser {parser_id:?}"))?;
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let sha = sha256_hex(&bytes);
    let statements = p.parse(&bytes)?;
    let (st, warning) = select_statement(&account, &statements)?;
    Ok(Loaded {
        parser_id,
        file_name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        sha,
        statement: st.clone(),
        warnings: warning.into_iter().collect(),
    })
}

pub fn parsers() -> Vec<ParserDto> {
    ledgerhold_parsers::all()
        .iter()
        .map(|p| ParserDto { id: p.id().to_string(), description: p.description().to_string() })
        .collect()
}

pub fn preview_import(store: &Store, account_id: &str, path: &Path, parser: Option<&str>) -> Result<ImportPreviewDto> {
    let l = load(store, account_id, path, parser)?;
    let preview = store.preview_import(account_id, &l.statement, &l.sha)?;
    let rows = preview
        .plan
        .items
        .iter()
        .map(|item| {
            let t = &l.statement.transactions[item.index];
            PreviewRow {
                date: t.date,
                amount: t.amount.minor(),
                description: t.description.clone(),
                status: if item.disposition == Disposition::New { "new" } else { "duplicate" }.into(),
            }
        })
        .collect();
    Ok(ImportPreviewDto {
        file_name: l.file_name,
        parser: l.parser_id,
        account_hint: l.statement.account_hint.clone(),
        currency: l.statement.currency.clone(),
        period: l.statement.period.or_else(|| l.statement.date_span()),
        rows,
        new_count: preview.new,
        duplicate_count: preview.duplicates,
        errors: l
            .statement
            .errors
            .iter()
            .map(|e| RowErrorDto { row_ref: e.row_ref.clone(), message: e.message.clone() })
            .collect(),
        warnings: l.warnings,
        file_already_imported: preview.file_already_imported.is_some(),
    })
}

pub fn commit_import(store: &mut Store, account_id: &str, path: &Path, parser: Option<&str>) -> Result<ImportResultDto> {
    let l = load(store, account_id, path, parser)?;
    let o = store.import(
        account_id,
        ImportMeta { parser: &l.parser_id, filename: &l.file_name, file_sha256: &l.sha },
        &l.statement,
    )?;
    Ok(ImportResultDto { batch_id: o.batch_id, new_count: o.new, duplicate_count: o.duplicates, error_count: o.errors })
}

pub fn undo_batch(store: &Store, batch_id: &str) -> Result<usize> {
    Ok(store.undo_batch(batch_id)?)
}

// ---- balances -------------------------------------------------------------------

pub fn set_anchor(store: &Store, account_id: &str, date: NaiveDate, amount_minor: i64, note: Option<&str>) -> Result<()> {
    store.set_anchor(account_id, date, Money(amount_minor), note.filter(|n| !n.trim().is_empty()))?;
    Ok(())
}

pub fn delete_anchor(store: &Store, anchor_id: &str) -> Result<()> {
    Ok(store.delete_anchor(anchor_id)?)
}

// ---- export ---------------------------------------------------------------------

/// Write one OFX file per account into `dir` for `[start, end]`.
pub fn export_ofx(
    store: &Store,
    account_ids: &[String],
    start: NaiveDate,
    end: NaiveDate,
    dir: &Path,
    now: NaiveDateTime,
) -> Result<Vec<ExportResultDto>> {
    if end < start {
        bail!("end date is before start date");
    }
    std::fs::create_dir_all(dir)?;
    let mut out = Vec::new();
    for id in account_ids {
        let account = store.get_account(id)?;
        let exports = store.build_ofx_exports(std::slice::from_ref(id), start, end)?;
        let text = ledgerhold_ofx::write(&exports, now);
        let path: PathBuf = dir.join(export_file_name(&account.name, start, end));
        std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
        out.push(ExportResultDto {
            path: path.display().to_string(),
            account_name: account.name,
            start,
            end,
            txn_count: exports[0].transactions.len(),
            balance: exports[0].ledger_balance.map(|(_, b)| b.minor()),
        });
    }
    Ok(out)
}

// ---- setup ----------------------------------------------------------------------

pub fn add_person(store: &Store, name: &str) -> Result<PersonDto> {
    let name = name.trim();
    if name.is_empty() {
        bail!("name is required");
    }
    Ok(PersonDto { id: store.add_person(name)?, name: name.into() })
}

pub fn add_institution(store: &Store, name: &str, bank_id: Option<&str>) -> Result<InstitutionDto> {
    let name = name.trim();
    if name.is_empty() {
        bail!("name is required");
    }
    let bank_id = bank_id.map(str::trim).filter(|b| !b.is_empty());
    Ok(InstitutionDto { id: store.add_institution(name, bank_id)?, name: name.into(), bank_id: bank_id.map(Into::into) })
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAccountInput {
    pub institution_id: String,
    pub name: String,
    pub kind: String,
    pub currency: String,
    pub number: Option<String>,
    pub owner_ids: Vec<String>,
}

pub fn add_account(store: &mut Store, input: NewAccountInput) -> Result<String> {
    let kind = match input.kind.as_str() {
        "checking" => AccountKind::Checking,
        "savings" => AccountKind::Savings,
        "credit_card" => AccountKind::CreditCard,
        other => bail!("unknown account kind {other:?}"),
    };
    if input.name.trim().is_empty() {
        bail!("name is required");
    }
    Ok(store.add_account(&NewAccount {
        institution_id: input.institution_id,
        name: input.name.trim().into(),
        kind,
        currency: input.currency.trim().to_uppercase(),
        external_account_id: input.number.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()),
        owners: input.owner_ids,
    })?)
}

pub fn set_owners(store: &mut Store, account_id: &str, owner_ids: &[String]) -> Result<()> {
    Ok(store.set_owners(account_id, owner_ids)?)
}
