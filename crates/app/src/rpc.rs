//! Transport-agnostic command dispatcher: a command name plus JSON arguments in, JSON out.
//! The web server uses it so it never re-implements a command; the Tauri shell has typed
//! wrappers instead, and a contract test keeps both command lists in step with the UI.
//!
//! File arguments (`path`) are resolved through a caller-supplied function so a server can
//! map an opaque upload token to a file it controls instead of trusting a client path.

use crate::{NewAccountInput, service as svc};
use chrono::{NaiveDate, NaiveDateTime};
use ledgerhold_store::Store;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug)]
pub enum RpcError {
    /// No such command.
    UnknownCommand(String),
    /// The arguments did not match what the command expects.
    BadArgs(String),
    /// The command ran and reported a problem (missing balance, bad file, ...).
    Failed(String),
}

impl RpcError {
    pub fn message(&self) -> String {
        match self {
            RpcError::UnknownCommand(c) => format!("unknown command {c:?}"),
            RpcError::BadArgs(m) | RpcError::Failed(m) => m.clone(),
        }
    }
}

pub struct RpcContext<'a> {
    pub now: NaiveDateTime,
    /// Shown by `db_path`; a server shouldn't reveal its filesystem layout.
    pub data_location: String,
    /// Turns the `path` argument of an import into a readable file.
    pub resolve_path: &'a dyn Fn(&str) -> anyhow::Result<PathBuf>,
}

/// The commands `call` understands, in the same snake_case the UI uses.
pub const COMMANDS: &[&str] = &[
    "overview",
    "account_detail",
    "transactions",
    "balance_series",
    "parsers",
    "preview_import",
    "commit_import",
    "undo_batch",
    "set_anchor",
    "delete_anchor",
    "add_person",
    "update_person",
    "delete_person",
    "add_institution",
    "update_institution",
    "delete_institution",
    "add_account",
    "rename_account",
    "update_account",
    "delete_account",
    "set_owners",
    "db_path",
];

fn args<T: DeserializeOwned>(v: Value) -> Result<T, RpcError> {
    serde_json::from_value(v).map_err(|e| RpcError::BadArgs(e.to_string()))
}

fn out<T: serde::Serialize>(r: anyhow::Result<T>) -> Result<Value, RpcError> {
    let v = r.map_err(|e| RpcError::Failed(format!("{e:#}")))?;
    serde_json::to_value(v).map_err(|e| RpcError::Failed(e.to_string()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountId {
    account_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Txns {
    account_id: String,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    search: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Series {
    account_id: String,
    start: NaiveDate,
    end: NaiveDate,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportArgs {
    account_id: String,
    path: String,
    parser: Option<String>,
    /// Preview row indexes to leave out of a commit.
    #[serde(default)]
    excluded: Vec<usize>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchId {
    batch_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetAnchor {
    account_id: String,
    date: NaiveDate,
    amount: i64,
    note: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnchorId {
    anchor_id: String,
}
#[derive(Deserialize)]
struct NewPerson {
    name: String,
    color: Option<String>,
}
#[derive(Deserialize)]
struct UpdatePerson {
    id: String,
    name: String,
    color: Option<String>,
}
#[derive(Deserialize)]
struct PersonId {
    id: String,
}
#[derive(Deserialize)]
struct Rename {
    id: String,
    name: String,
}
#[derive(Deserialize)]
struct UpdateAccount {
    id: String,
    name: String,
    number: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Institution {
    name: String,
    bank_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInstitution {
    id: String,
    name: String,
    bank_id: Option<String>,
}
#[derive(Deserialize)]
struct AddAccount {
    input: NewAccountInput,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Owners {
    account_id: String,
    owner_ids: Vec<String>,
}

pub fn call(store: &mut Store, command: &str, a: Value, ctx: &RpcContext<'_>) -> Result<Value, RpcError> {
    let today = ctx.now.date();
    match command {
        "overview" => out(svc::overview(store, today)),
        "account_detail" => {
            let a: AccountId = args(a)?;
            out(svc::account_detail(store, &a.account_id, today))
        }
        "transactions" => {
            let a: Txns = args(a)?;
            out(svc::transactions(store, &a.account_id, a.from, a.to, a.search.as_deref()))
        }
        "balance_series" => {
            let a: Series = args(a)?;
            out(svc::balance_series(store, &a.account_id, a.start, a.end))
        }
        "parsers" => out(Ok(svc::parsers())),
        "preview_import" => {
            let a: ImportArgs = args(a)?;
            let path = (ctx.resolve_path)(&a.path).map_err(|e| RpcError::Failed(format!("{e:#}")))?;
            out(svc::preview_import(store, &a.account_id, &path, a.parser.as_deref()))
        }
        "commit_import" => {
            let a: ImportArgs = args(a)?;
            let path = (ctx.resolve_path)(&a.path).map_err(|e| RpcError::Failed(format!("{e:#}")))?;
            out(svc::commit_import(store, &a.account_id, &path, a.parser.as_deref(), &a.excluded))
        }
        "undo_batch" => {
            let a: BatchId = args(a)?;
            out(svc::undo_batch(store, &a.batch_id))
        }
        "set_anchor" => {
            let a: SetAnchor = args(a)?;
            out(svc::set_anchor(store, &a.account_id, a.date, a.amount, a.note.as_deref()))
        }
        "delete_anchor" => {
            let a: AnchorId = args(a)?;
            out(svc::delete_anchor(store, &a.anchor_id))
        }
        "add_person" => {
            let a: NewPerson = args(a)?;
            out(svc::add_person(store, &a.name, a.color.as_deref()))
        }
        "update_person" => {
            let a: UpdatePerson = args(a)?;
            out(svc::update_person(store, &a.id, &a.name, a.color.as_deref()))
        }
        "delete_person" => {
            let a: PersonId = args(a)?;
            out(svc::delete_person(store, &a.id))
        }
        "rename_account" => {
            let a: Rename = args(a)?;
            out(svc::rename_account(store, &a.id, &a.name))
        }
        "update_account" => {
            let a: UpdateAccount = args(a)?;
            out(svc::update_account(store, &a.id, &a.name, a.number.as_deref()))
        }
        "delete_account" => {
            let a: PersonId = args(a)?;
            out(svc::delete_account(store, &a.id))
        }
        "add_institution" => {
            let a: Institution = args(a)?;
            out(svc::add_institution(store, &a.name, a.bank_id.as_deref()))
        }
        "update_institution" => {
            let a: UpdateInstitution = args(a)?;
            out(svc::update_institution(store, &a.id, &a.name, a.bank_id.as_deref()))
        }
        "delete_institution" => {
            let a: PersonId = args(a)?;
            out(svc::delete_institution(store, &a.id))
        }
        "add_account" => {
            let a: AddAccount = args(a)?;
            out(svc::add_account(store, a.input))
        }
        "set_owners" => {
            let a: Owners = args(a)?;
            out(svc::set_owners(store, &a.account_id, &a.owner_ids))
        }
        "db_path" => Ok(Value::String(ctx.data_location.clone())),
        other => Err(RpcError::UnknownCommand(other.to_string())),
    }
}
