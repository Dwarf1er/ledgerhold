//! `ledgerhold` command line. All logic lives in `run`, which takes the store,
//! an output sink and "now" so whole command sequences can be tested in-process.

use anyhow::{Context, Result, bail};
use chrono::{Datelike, NaiveDate, NaiveDateTime};
use clap::{Args, Parser, Subcommand, ValueEnum};
use ledgerhold_app::{default_db_path as app_default_db_path, kebab, month_bounds, parse_month, select_statement};
use ledgerhold_core::dedup::sha256_hex;
use ledgerhold_core::{AccountKind, Money};
use ledgerhold_store::{Account, ImportMeta, NewAccount, Store};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(name = "ledgerhold", version, about = "Household bank ledger: import, dedupe, reconcile, export OFX")]
pub struct Cli {
    /// Database file (default: ledgerhold/ledgerhold.sqlite in your platform's data directory)
    #[arg(long, env = "LEDGERHOLD_DB", global = true)]
    pub db: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Manage household members
    #[command(subcommand)]
    Person(PersonCmd),
    /// Manage banks
    #[command(subcommand)]
    Institution(InstitutionCmd),
    /// Manage accounts and cards
    #[command(subcommand)]
    Account(AccountCmd),
    /// Set or list starting balances ("anchors")
    #[command(subcommand)]
    Anchor(AnchorCmd),
    /// Import statement files into an account
    Import(ImportArgs),
    /// List imports of an account
    Batches { account: String },
    /// Undo one import (removes all of its transactions)
    Undo { batch_id: String },
    /// List transactions
    Transactions(TransactionsArgs),
    /// Balance of an account at a date
    Balance {
        account: String,
        /// Date (YYYY-MM-DD); default today
        #[arg(long)]
        on: Option<NaiveDate>,
    },
    /// Compare transactions against known balances (anchors + statement balances)
    Reconcile { account: String },
    /// Export OFX for a date range
    Export(ExportArgs),
    /// List supported statement parsers
    Parsers,
}

#[derive(Subcommand, Debug)]
pub enum PersonCmd {
    Add { name: String },
    List,
}

#[derive(Subcommand, Debug)]
pub enum InstitutionCmd {
    Add {
        name: String,
        /// OFX BANKID written to exports (e.g. a transit/routing number)
        #[arg(long)]
        bank_id: Option<String>,
    },
    List,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum KindArg {
    Checking,
    Savings,
    CreditCard,
}

impl From<KindArg> for AccountKind {
    fn from(k: KindArg) -> Self {
        match k {
            KindArg::Checking => AccountKind::Checking,
            KindArg::Savings => AccountKind::Savings,
            KindArg::CreditCard => AccountKind::CreditCard,
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum AccountCmd {
    Add {
        /// Institution name or id
        #[arg(long)]
        institution: String,
        #[arg(long)]
        name: String,
        #[arg(long, value_enum)]
        kind: KindArg,
        #[arg(long, default_value = "CAD")]
        currency: String,
        /// Account/card number as the bank prints it (used to match statements)
        #[arg(long)]
        number: Option<String>,
        /// Owner (person name or id); repeat for joint accounts
        #[arg(long = "owner")]
        owners: Vec<String>,
    },
    List,
    /// Replace the owners of an account
    Owners { account: String, people: Vec<String> },
}

#[derive(Subcommand, Debug)]
pub enum AnchorCmd {
    /// Declare the balance at the END of a day
    Set {
        account: String,
        date: NaiveDate,
        #[arg(allow_hyphen_values = true)]
        amount: String,
        #[arg(long)]
        note: Option<String>,
    },
    List { account: String },
}

#[derive(Args, Debug)]
pub struct ImportArgs {
    /// Target account (name or id)
    pub account: String,
    pub files: Vec<PathBuf>,
    /// Parser id (see `parsers`); inferred for .ofx/.qfx/.qbo files
    #[arg(long)]
    pub parser: Option<String>,
    /// Show what would be imported without changing anything
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args, Debug)]
pub struct TransactionsArgs {
    pub account: String,
    #[arg(long)]
    pub from: Option<NaiveDate>,
    #[arg(long)]
    pub to: Option<NaiveDate>,
}

#[derive(Args, Debug)]
pub struct ExportArgs {
    /// Accounts (name or id); one file per account
    #[arg(required = true)]
    pub accounts: Vec<String>,
    /// Calendar month, YYYY-MM
    #[arg(long, conflicts_with_all = ["from", "to", "all_months"])]
    pub month: Option<String>,
    #[arg(long, requires = "to")]
    pub from: Option<NaiveDate>,
    #[arg(long, requires = "from")]
    pub to: Option<NaiveDate>,
    /// One file per calendar month covering all data
    #[arg(long)]
    pub all_months: bool,
    /// Output directory
    #[arg(short, long, default_value = ".")]
    pub out: PathBuf,
}

pub fn default_db_path() -> PathBuf {
    app_default_db_path()
}

// ---- helpers -----------------------------------------------------------------

fn resolve_account(store: &Store, key: &str) -> Result<Account> {
    let all = store.list_accounts()?;
    if let Some(a) = all.iter().find(|a| a.id == key) {
        return Ok(a.clone());
    }
    let by_name: Vec<_> = all.iter().filter(|a| a.name.eq_ignore_ascii_case(key)).collect();
    match by_name.as_slice() {
        [a] => return Ok((*a).clone()),
        [_, ..] => bail!("account name {key:?} is ambiguous; use its id"),
        [] => {}
    }
    let by_prefix: Vec<_> = all.iter().filter(|a| a.id.starts_with(key)).collect();
    match by_prefix.as_slice() {
        [a] => Ok((*a).clone()),
        [] => bail!("no account matches {key:?} (see `ledgerhold account list`)"),
        _ => bail!("id prefix {key:?} is ambiguous"),
    }
}

fn resolve_person(store: &Store, key: &str) -> Result<String> {
    let people = store.list_people()?;
    if let Some(p) = people.iter().find(|p| p.id == key || p.name.eq_ignore_ascii_case(key)) {
        return Ok(p.id.clone());
    }
    bail!("no person matches {key:?} (see `ledgerhold person list`)")
}

fn resolve_institution(store: &Store, key: &str) -> Result<String> {
    let all = store.list_institutions()?;
    if let Some(i) = all.iter().find(|i| i.id == key || i.name.eq_ignore_ascii_case(key)) {
        return Ok(i.id.clone());
    }
    bail!("no institution matches {key:?}; add it with `ledgerhold institution add`")
}

fn money_col(m: Money) -> String {
    format!("{:>12}", m.to_string())
}

// ---- dispatch ----------------------------------------------------------------

pub fn run(cli: &Cli, store: &mut Store, out: &mut impl Write, now: NaiveDateTime) -> Result<()> {
    match &cli.command {
        Command::Person(PersonCmd::Add { name }) => {
            let id = store.add_person(name)?;
            writeln!(out, "added person {name} ({id})")?;
        }
        Command::Person(PersonCmd::List) => {
            for p in store.list_people()? {
                writeln!(out, "{}  {}", p.id, p.name)?;
            }
        }
        Command::Institution(InstitutionCmd::Add { name, bank_id }) => {
            let id = store.add_institution(name, bank_id.as_deref())?;
            writeln!(out, "added institution {name} ({id})")?;
        }
        Command::Institution(InstitutionCmd::List) => {
            for i in store.list_institutions()? {
                writeln!(out, "{}  {}  bank-id={}", i.id, i.name, i.ofx_bank_id.as_deref().unwrap_or("-"))?;
            }
        }
        Command::Account(AccountCmd::Add { institution, name, kind, currency, number, owners }) => {
            let owners = owners.iter().map(|o| resolve_person(store, o)).collect::<Result<Vec<_>>>()?;
            let id = store.add_account(&NewAccount {
                institution_id: resolve_institution(store, institution)?,
                name: name.clone(),
                kind: (*kind).into(),
                currency: currency.to_uppercase(),
                external_account_id: number.clone(),
                owners,
            })?;
            writeln!(out, "added account {name} ({id})")?;
        }
        Command::Account(AccountCmd::List) => {
            let people = store.list_people()?;
            for a in store.list_accounts()? {
                let owners: Vec<_> = a
                    .owners
                    .iter()
                    .map(|id| people.iter().find(|p| &p.id == id).map_or(id.as_str(), |p| p.name.as_str()))
                    .collect();
                writeln!(
                    out,
                    "{}  {}  [{} {}]  number={}  owners={}",
                    a.id,
                    a.name,
                    a.kind.as_str(),
                    a.currency,
                    a.external_account_id.as_deref().unwrap_or("-"),
                    if owners.is_empty() { "-".into() } else { owners.join(", ") }
                )?;
            }
        }
        Command::Account(AccountCmd::Owners { account, people }) => {
            let a = resolve_account(store, account)?;
            let ids = people.iter().map(|p| resolve_person(store, p)).collect::<Result<Vec<_>>>()?;
            store.set_owners(&a.id, &ids)?;
            writeln!(out, "{} now has {} owner(s)", a.name, ids.len())?;
        }
        Command::Anchor(AnchorCmd::Set { account, date, amount, note }) => {
            let a = resolve_account(store, account)?;
            let m = Money::parse(amount).with_context(|| format!("bad amount {amount:?}"))?;
            store.set_anchor(&a.id, *date, m, note.as_deref())?;
            writeln!(out, "{}: balance at end of {date} is {m}", a.name)?;
        }
        Command::Anchor(AnchorCmd::List { account }) => {
            let a = resolve_account(store, account)?;
            for an in store.list_anchors(&a.id)? {
                writeln!(out, "{}  {}  {}", an.as_of, money_col(an.amount), an.note.unwrap_or_default())?;
            }
        }
        Command::Import(args) => import(store, args, out)?,
        Command::Batches { account } => {
            let a = resolve_account(store, account)?;
            for b in store.list_batches(&a.id)? {
                let period = b.period.map(|(s, e)| format!("{s}..{e}")).unwrap_or_else(|| "-".into());
                writeln!(
                    out,
                    "{}  {}  {}  {}  new={} dup={} err={}",
                    b.id, b.imported_at, b.filename, period, b.rows_new, b.rows_duplicate, b.rows_error
                )?;
            }
        }
        Command::Undo { batch_id } => {
            let n = store.undo_batch(batch_id)?;
            writeln!(out, "removed {n} transaction(s)")?;
        }
        Command::Transactions(args) => {
            let a = resolve_account(store, &args.account)?;
            for t in store.list_transactions(&a.id, args.from, args.to)? {
                writeln!(out, "{}  {}  {}", t.date, money_col(t.amount), t.description)?;
            }
        }
        Command::Balance { account, on } => {
            let a = resolve_account(store, account)?;
            let on = on.unwrap_or_else(|| now.date());
            let r = store.balance_at(&a.id, on)?;
            writeln!(out, "{}: {} at end of {on}", a.name, r.balance)?;
            writeln!(out, "  (from the {} balance of {})", r.anchor.as_of, r.anchor.amount)?;
        }
        Command::Reconcile { account } => {
            let a = resolve_account(store, account)?;
            let rows = store.reconcile(&a.id)?;
            if rows.is_empty() {
                writeln!(out, "need at least two known balances (anchor or statement) to reconcile")?;
            }
            let mut bad = 0;
            for r in &rows {
                let status = if r.is_ok() {
                    "ok".to_string()
                } else {
                    bad += 1;
                    format!("MISMATCH {}", r.difference)
                };
                writeln!(out, "{} -> {}  expected {}  statement {}  {status}", r.from, r.to, r.expected, r.actual)?;
            }
            if !rows.is_empty() {
                writeln!(out, "{} period(s) checked, {bad} mismatch(es)", rows.len())?;
            }
        }
        Command::Export(args) => export(store, args, out, now)?,
        Command::Parsers => {
            for p in ledgerhold_parsers::all() {
                writeln!(out, "{:<20} {}", p.id(), p.description())?;
            }
        }
    }
    Ok(())
}

fn import(store: &mut Store, args: &ImportArgs, out: &mut impl Write) -> Result<()> {
    if args.files.is_empty() {
        bail!("no files given");
    }
    let account = resolve_account(store, &args.account)?;
    let mut failed = 0;
    for file in &args.files {
        let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        writeln!(out, "{name} -> {}", account.name)?;
        if let Err(e) = import_one(store, &account, file, &name, args, out) {
            failed += 1;
            writeln!(out, "  error: {e:#}")?;
        }
    }
    if failed > 0 {
        bail!("{failed} file(s) failed");
    }
    Ok(())
}

fn parser_for(args: &ImportArgs, file: &Path) -> Result<String> {
    if let Some(p) = &args.parser {
        return Ok(p.clone());
    }
    match file.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() {
        Some("ofx" | "qfx" | "qbo") => Ok("ofx".into()),
        _ => bail!("cannot tell the format; pass --parser (see `ledgerhold parsers`)"),
    }
}

fn import_one(
    store: &mut Store,
    account: &Account,
    file: &Path,
    name: &str,
    args: &ImportArgs,
    out: &mut impl Write,
) -> Result<()> {
    let parser_id = parser_for(args, file)?;
    let parser = ledgerhold_parsers::by_id(&parser_id).with_context(|| format!("unknown parser {parser_id:?}"))?;
    let bytes = std::fs::read(file).with_context(|| format!("reading {}", file.display()))?;
    let sha = sha256_hex(&bytes);
    let statements = parser.parse(&bytes)?;
    let (st, warning) = select_statement(account, &statements)?;
    if let Some(w) = warning {
        writeln!(out, "  warning: {w}")?;
    }

    let preview = store.preview_import(&account.id, st, &sha)?;
    if preview.file_already_imported.is_some() {
        writeln!(out, "  note: this exact file was imported before")?;
    }
    let (new, dup, errors) = if args.dry_run {
        (preview.new, preview.duplicates, preview.errors)
    } else {
        let o = store.import(&account.id, ImportMeta { parser: &parser_id, filename: name, file_sha256: &sha }, st)?;
        (o.new, o.duplicates, o.errors)
    };
    let verb = if args.dry_run { "would import" } else { "imported" };
    writeln!(out, "  {verb} {new} new, {dup} duplicate, {errors} row error(s)")?;
    for e in st.errors.iter().take(10) {
        writeln!(out, "  ! {}: {}", e.row_ref, e.message)?;
    }
    if st.errors.len() > 10 {
        writeln!(out, "  ! ... and {} more", st.errors.len() - 10)?;
    }
    if !args.dry_run && new > 0 {
        let bal_rows = st.transactions.iter().filter(|t| t.balance.is_some()).count();
        if bal_rows > 0 {
            writeln!(out, "  (file has running balances; use `anchor set` + `reconcile` to verify)")?;
        }
    }
    Ok(())
}

fn export(store: &mut Store, args: &ExportArgs, out: &mut impl Write, now: NaiveDateTime) -> Result<()> {
    let accounts = args.accounts.iter().map(|a| resolve_account(store, a)).collect::<Result<Vec<_>>>()?;
    std::fs::create_dir_all(&args.out)?;

    let mut jobs: Vec<(Account, NaiveDate, NaiveDate)> = Vec::new();
    for a in &accounts {
        if args.all_months {
            let txns = store.list_transactions(&a.id, None, None)?;
            let (Some(first), Some(last)) = (txns.first(), txns.last()) else {
                writeln!(out, "{}: no transactions", a.name)?;
                continue;
            };
            let (mut y, mut m) = (first.date.year(), first.date.month());
            loop {
                let (s, e) = month_bounds(y, m).context("calendar overflow")?;
                jobs.push((a.clone(), s, e));
                if e >= last.date {
                    break;
                }
                (y, m) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
            }
        } else {
            let (s, e) = match (&args.month, args.from, args.to) {
                (Some(m), ..) => parse_month(m)?,
                (None, Some(f), Some(t)) => (f, t),
                _ => bail!("give --month YYYY-MM, or --from and --to, or --all-months"),
            };
            if e < s {
                bail!("--to is before --from");
            }
            jobs.push((a.clone(), s, e));
        }
    }

    for (a, start, end) in jobs {
        let exports = store.build_ofx_exports(std::slice::from_ref(&a.id), start, end)?;
        let text = ledgerhold_ofx::write(&exports, now);
        let file = args.out.join(format!("{}-{}-{}.ofx", kebab(&a.name), start.format("%Y%m%d"), end.format("%Y%m%d")));
        std::fs::write(&file, text)?;
        let e = &exports[0];
        let bal = e.ledger_balance.map(|(_, b)| format!(", balance {b}")).unwrap_or_default();
        writeln!(out, "wrote {} ({} transaction(s){bal})", file.display(), e.transactions.len())?;
    }
    Ok(())
}
