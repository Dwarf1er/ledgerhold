//! Tauri shell: thin command wrappers over `ledgerhold-app`. All behaviour
//! lives (and is tested) in the service layer; this file only adapts types and
//! keeps blocking work off the UI thread.

use chrono::{Local, NaiveDate, Utc};
use ledgerhold_app::dto::*;
use ledgerhold_app::{self as svc, NewAccountInput};
use ledgerhold_store::Store;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use tauri::{Manager, State};

pub struct AppState {
    store: Mutex<Store>,
    db_path: PathBuf,
}

type R<T> = Result<T, String>;

fn msg(e: anyhow::Error) -> String {
    format!("{e:#}")
}

fn lock<'a>(state: &'a State<'_, AppState>) -> R<MutexGuard<'a, Store>> {
    state.store.lock().map_err(|_| "the database lock was poisoned by an earlier crash; restart the app".to_string())
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

// `async` here means "run on the async runtime's thread pool", not the main thread,
// so a large PDF import can't freeze the window.

#[tauri::command(async)]
fn overview(state: State<'_, AppState>) -> R<Overview> {
    let store = lock(&state)?;
    svc::overview(&store, today()).map_err(msg)
}

#[tauri::command(async)]
fn account_detail(state: State<'_, AppState>, account_id: String) -> R<AccountDetail> {
    let store = lock(&state)?;
    svc::account_detail(&store, &account_id, today()).map_err(msg)
}

#[tauri::command(async)]
fn transactions(
    state: State<'_, AppState>,
    account_id: String,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    search: Option<String>,
) -> R<Vec<TxnDto>> {
    let store = lock(&state)?;
    svc::transactions(&store, &account_id, from, to, search.as_deref()).map_err(msg)
}

#[tauri::command(async)]
fn balance_series(state: State<'_, AppState>, account_id: String, start: NaiveDate, end: NaiveDate) -> R<Vec<BalancePoint>> {
    let store = lock(&state)?;
    svc::balance_series(&store, &account_id, start, end).map_err(msg)
}

#[tauri::command]
fn parsers() -> Vec<ParserDto> {
    svc::parsers()
}

#[tauri::command(async)]
fn preview_import(state: State<'_, AppState>, account_id: String, path: String, parser: Option<String>) -> R<ImportPreviewDto> {
    let store = lock(&state)?;
    svc::preview_import(&store, &account_id, Path::new(&path), parser.as_deref()).map_err(msg)
}

#[tauri::command(async)]
fn commit_import(state: State<'_, AppState>, account_id: String, path: String, parser: Option<String>) -> R<ImportResultDto> {
    let mut store = lock(&state)?;
    svc::commit_import(&mut store, &account_id, Path::new(&path), parser.as_deref()).map_err(msg)
}

#[tauri::command(async)]
fn undo_batch(state: State<'_, AppState>, batch_id: String) -> R<usize> {
    let store = lock(&state)?;
    svc::undo_batch(&store, &batch_id).map_err(msg)
}

#[tauri::command(async)]
fn set_anchor(state: State<'_, AppState>, account_id: String, date: NaiveDate, amount: i64, note: Option<String>) -> R<()> {
    let store = lock(&state)?;
    svc::set_anchor(&store, &account_id, date, amount, note.as_deref()).map_err(msg)
}

#[tauri::command(async)]
fn delete_anchor(state: State<'_, AppState>, anchor_id: String) -> R<()> {
    let store = lock(&state)?;
    svc::delete_anchor(&store, &anchor_id).map_err(msg)
}

#[tauri::command(async)]
fn export_ofx(
    state: State<'_, AppState>,
    account_ids: Vec<String>,
    start: NaiveDate,
    end: NaiveDate,
    dir: String,
) -> R<Vec<ExportResultDto>> {
    let store = lock(&state)?;
    svc::export_ofx(&store, &account_ids, start, end, Path::new(&dir), Utc::now().naive_utc()).map_err(msg)
}

#[tauri::command(async)]
fn add_person(state: State<'_, AppState>, name: String) -> R<PersonDto> {
    let store = lock(&state)?;
    svc::add_person(&store, &name).map_err(msg)
}

#[tauri::command(async)]
fn add_institution(state: State<'_, AppState>, name: String, bank_id: Option<String>) -> R<InstitutionDto> {
    let store = lock(&state)?;
    svc::add_institution(&store, &name, bank_id.as_deref()).map_err(msg)
}

#[tauri::command(async)]
fn add_account(state: State<'_, AppState>, input: NewAccountInput) -> R<String> {
    let mut store = lock(&state)?;
    svc::add_account(&mut store, input).map_err(msg)
}

#[tauri::command(async)]
fn set_owners(state: State<'_, AppState>, account_id: String, owner_ids: Vec<String>) -> R<()> {
    let mut store = lock(&state)?;
    svc::set_owners(&mut store, &account_id, &owner_ids).map_err(msg)
}

#[tauri::command]
fn db_path(state: State<'_, AppState>) -> String {
    state.db_path.display().to_string()
}

/// `LEDGERHOLD_DB` overrides the location; otherwise the same file the CLI uses.
fn open_store() -> anyhow::Result<(Store, PathBuf)> {
    let path = std::env::var_os("LEDGERHOLD_DB").map(PathBuf::from).unwrap_or_else(svc::default_db_path);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let store = Store::open(&path).map_err(|e| anyhow::anyhow!("opening {}: {e}", path.display()))?;
    Ok((store, path))
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let (store, db_path) = open_store()?;
            app.manage(AppState { store: Mutex::new(store), db_path });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            overview,
            account_detail,
            transactions,
            balance_series,
            parsers,
            preview_import,
            commit_import,
            undo_batch,
            set_anchor,
            delete_anchor,
            export_ofx,
            add_person,
            add_institution,
            add_account,
            set_owners,
            db_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Ledgerhold");
}
