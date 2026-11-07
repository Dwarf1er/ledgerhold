use chrono::NaiveDateTime;
use clap::Parser;
use ledgerhold_cli::{Cli, run};
use ledgerhold_store::Store;
use std::{fs, path::PathBuf};

fn now() -> NaiveDateTime {
    chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap().and_hms_opt(9, 0, 0).unwrap()
}

fn fixture(rel: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(rel).display().to_string()
}

fn tmp(name: &str) -> PathBuf {
    let p = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("cli-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    p
}

struct T {
    store: Store,
}

impl T {
    fn new() -> T {
        T { store: Store::open_in_memory().unwrap() }
    }
    /// Run a command line; returns (result, output).
    fn try_run(&mut self, line: &str) -> (anyhow::Result<()>, String) {
        let mut args = vec!["ledgerhold"];
        args.extend(line.split_whitespace());
        let cli = Cli::try_parse_from(args).unwrap_or_else(|e| panic!("bad args {line:?}: {e}"));
        let mut out = Vec::new();
        let r = run(&cli, &mut self.store, &mut out, now());
        (r, String::from_utf8(out).unwrap())
    }
    fn ok(&mut self, line: &str) -> String {
        let (r, out) = self.try_run(line);
        r.unwrap_or_else(|e| panic!("{line:?} failed: {e:#}\n{out}"));
        out
    }
}

fn household() -> T {
    let mut t = T::new();
    t.ok("person add Alex");
    t.ok("person add Sam");
    t.ok("institution add Desjardins --bank-id 815");
    t.ok("account add --institution Desjardins --name Joint --kind checking --number EOP-12345 --owner Alex --owner Sam");
    t
}

#[test]
fn rename_and_delete_people_and_accounts() {
    let mut t = household();
    assert!(t.ok("person list").contains("Alex  color=blue"));
    t.ok("person color Sam teal");
    assert!(t.ok("person list").contains("Sam  color=teal"));
    assert!(t.try_run("person color Sam mauve").0.is_err());
    t.ok("person rename Alex Alexandre");
    assert!(t.ok("person list").contains("Alexandre"));
    t.ok("account rename Joint Household");
    assert!(t.ok("account list").contains("Household"));

    t.ok("person delete Sam");
    let out = t.ok("account list");
    assert!(out.contains("Alexandre") && !out.contains("Sam"), "{out}");

    // Deleting an account needs --yes, and says what it would take with it.
    let (r, _) = t.try_run("account delete Household");
    assert!(format!("{:#}", r.unwrap_err()).contains("--yes"));
    assert!(t.ok("account list").contains("Household"));
    assert!(t.ok("account delete Household --yes").contains("deleted account Household"));
    assert!(!t.ok("account list").contains("Household"));
}

#[test]
fn edit_and_delete_banks() {
    let mut t = household();
    t.ok("institution edit Desjardins --name Caisse");
    assert!(t.ok("institution list").contains("Caisse  bank-id=815"));
    t.ok("institution edit Caisse --bank-id 900");
    assert!(t.ok("institution list").contains("Caisse  bank-id=900"));
    // `ok` splits on whitespace, so an empty value has to be passed as a real argument.
    let cli = Cli::try_parse_from(["ledgerhold", "institution", "edit", "Caisse", "--bank-id", ""]).unwrap();
    run(&cli, &mut t.store, &mut Vec::new(), now()).unwrap();
    assert!(t.ok("institution list").contains("Caisse  bank-id=-"));

    let (r, _) = t.try_run("institution delete Caisse");
    assert!(format!("{:#}", r.unwrap_err()).contains("still has 1 account"));
    t.ok("account delete Joint --yes");
    t.ok("institution delete Caisse");
    assert!(!t.ok("institution list").contains("Caisse"));
}

#[test]
fn household_setup_lists_joint_owners() {
    let mut t = household();
    let out = t.ok("account list");
    assert!(out.contains("Joint") && out.contains("number=EOP-12345"), "{out}");
    assert!(out.contains("Alex") && out.contains("Sam"), "{out}");
    assert!(t.ok("institution list").contains("bank-id=815"));
}

#[test]
fn import_picks_the_right_account_from_a_multi_account_file_and_dedups() {
    let mut t = household();
    let f = fixture("synthetic/desjardins.csv");
    let out = t.ok(&format!("import Joint {f} --parser desjardins"));
    assert!(out.contains("imported 10 new, 0 duplicate, 2 row error(s)"), "{out}");
    assert!(out.contains("! line"), "row errors are shown: {out}");

    let again = t.ok(&format!("import Joint {f} --parser desjardins"));
    assert!(again.contains("imported 0 new, 10 duplicate"), "{again}");
    assert!(again.contains("this exact file was imported before"), "{again}");
    assert_eq!(t.ok("batches Joint").lines().count(), 2, "second import changed nothing so no batch");
}

#[test]
fn dry_run_changes_nothing() {
    let mut t = household();
    let f = fixture("synthetic/desjardins.csv");
    let out = t.ok(&format!("import Joint {f} --parser desjardins --dry-run"));
    assert!(out.contains("would import 10 new"), "{out}");
    assert!(t.ok("transactions Joint").is_empty());
}

#[test]
fn multi_account_file_without_account_number_asks_for_it() {
    let mut t = T::new();
    t.ok("institution add Desjardins");
    t.ok("account add --institution Desjardins --name NoNumber --kind checking");
    let f = fixture("synthetic/desjardins.csv");
    let (r, out) = t.try_run(&format!("import NoNumber {f} --parser desjardins"));
    assert!(r.is_err());
    assert!(out.contains("several accounts") && out.contains("EOP-12345") && out.contains("PR1-67890"), "{out}");
}

#[test]
fn currency_selection_for_multi_currency_card_file() {
    let mut t = T::new();
    t.ok("institution add Wealthsimple");
    t.ok("account add --institution Wealthsimple --name CAD-card --kind credit-card");
    t.ok("account add --institution Wealthsimple --name USD-card --kind credit-card --currency USD");
    let f = fixture("synthetic/wealthsimple_credit.csv");
    assert!(t.ok(&format!("import CAD-card {f} --parser wealthsimple-credit")).contains("imported 8 new"));
    assert!(t.ok(&format!("import USD-card {f} --parser wealthsimple-credit")).contains("imported 1 new"));
}

#[test]
fn format_is_inferred_for_ofx_and_required_otherwise() {
    let mut t = household();
    let ofx = fixture("golden/legacy_desjardins_eop.ofx");
    assert!(t.ok(&format!("import Joint {ofx}")).contains("imported 10 new"));
    let csv = fixture("synthetic/desjardins.csv");
    let (r, out) = t.try_run(&format!("import Joint {csv}"));
    assert!(r.is_err() && out.contains("pass --parser"), "{out}");
}

#[test]
fn csv_then_ofx_of_same_account_is_all_duplicates_via_cli() {
    let mut t = household();
    t.ok(&format!("import Joint {} --parser desjardins", fixture("synthetic/desjardins.csv")));
    let out = t.ok(&format!("import Joint {}", fixture("golden/legacy_desjardins_eop.ofx")));
    assert!(out.contains("imported 0 new, 10 duplicate"), "{out}");
}

#[test]
fn balance_only_account_is_not_reconciled() {
    let mut t = household();
    t.ok("anchor set Joint 2026-01-31 10000.00");
    t.ok("anchor set Joint 2026-02-28 10412.37");
    assert!(t.ok("reconcile Joint").contains("MISMATCH"));

    let out = t.ok("account balance-only Joint true");
    assert!(out.contains("tracked by snapshots only"), "{out}");
    let out = t.ok("reconcile Joint");
    assert!(out.contains("nothing to reconcile") && !out.contains("MISMATCH"), "{out}");
    // Balances still come from the snapshots.
    assert!(t.ok("balance Joint --on 2026-03-15").contains("10412.37"));

    t.ok("account balance-only Joint false");
    assert!(t.ok("reconcile Joint").contains("MISMATCH"));

    // And it can be chosen when the account is created.
    t.ok("account add --institution Desjardins --name TFSA --kind savings --balance-only");
    t.ok("anchor set TFSA 2026-01-31 1000.00");
    t.ok("anchor set TFSA 2026-02-28 1100.00");
    assert!(t.ok("reconcile TFSA").contains("nothing to reconcile"));
}

#[test]
fn balance_reconcile_and_undo_flow() {
    let mut t = household();
    // Opening balance is the one printed on the first CSV row's balance minus its amount: 3200.00 - 2500.00 = 700.00
    t.ok("anchor set Joint 2026-01-01 700.00 --note opening");
    t.ok(&format!("import Joint {} --parser desjardins", fixture("synthetic/desjardins.csv")));
    let b = t.ok("balance Joint --on 2026-01-05");
    assert!(b.contains("3156.83"), "matches the bank's own running balance on 01-05: {b}");

    // The CSV's own balances give a second known point: anchor the bank's figure at 01-09.
    t.ok("anchor set Joint 2026-01-09 1964.45");
    let r = t.ok("reconcile Joint");
    assert!(r.contains("0 mismatch"), "{r}");
    // A wrong statement balance is flagged for the exact period.
    t.ok("anchor set Joint 2026-01-31 1900.00");
    let r = t.ok("reconcile Joint");
    // Between 01-09 and 01-31 there is one 3.00 debit (01-11): expected 1961.45 vs statement 1900.00.
    assert!(r.contains("expected 1961.45") && r.contains("MISMATCH -61.45") && r.contains("1 mismatch"), "{r}");

    let batch = t.ok("batches Joint").split_whitespace().next().unwrap().to_string();
    assert!(t.ok(&format!("undo {batch}")).contains("removed 10"));
    assert!(t.ok("transactions Joint").is_empty());
}

#[test]
fn negative_anchor_for_a_credit_card_is_accepted() {
    let mut t = T::new();
    t.ok("institution add Desjardins");
    t.ok("account add --institution Desjardins --name Visa --kind credit-card");
    t.ok("anchor set Visa 2025-12-31 -250.00");
    assert!(t.ok("balance Visa --on 2025-12-31").contains("-250.00"));
}

#[test]
fn export_month_writes_a_file_with_range_and_balance_and_reimports_cleanly() {
    let mut t = household();
    t.ok("anchor set Joint 2026-01-01 700.00");
    t.ok(&format!("import Joint {} --parser desjardins", fixture("synthetic/desjardins.csv")));
    let dir = tmp("export");
    let out = t.ok(&format!("export Joint --month 2026-01 -o {}", dir.display()));
    assert!(out.contains("joint-20260101-20260131.ofx"), "{out}");
    assert!(out.contains("10 transaction(s), balance 1961.45"), "{out}");
    let file = dir.join("joint-20260101-20260131.ofx");
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("<BANKID>815") && text.contains("<DTSTART>20260101") && text.contains("<DTEND>20260131"));
    assert!(text.contains("<LEDGERBAL>"));
    // Importing our own export adds nothing.
    let again = t.ok(&format!("import Joint {}", file.display()));
    assert!(again.contains("imported 0 new, 10 duplicate"), "{again}");
    // Sub-range export only has that range.
    let out = t.ok(&format!("export Joint --from 2026-01-03 --to 2026-01-03 -o {}", dir.display()));
    assert!(out.contains("3 transaction(s)"), "{out}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn export_all_months_splits_like_the_legacy_monthly_flag() {
    let mut t = household();
    let dir = tmp("months");
    // Statement spanning two months.
    let csv = dir.join("two.csv");
    fs::create_dir_all(&dir).unwrap();
    let row = |date: &str, memo: &str| format!("1,EOP,12345,{date},001,{memo},,,10.00,,,,,10.00\n");
    fs::write(&csv, [row("2026/01/30", "A"), row("2026/02/02", "B"), row("2026/03/05", "C")].concat()).unwrap();
    t.ok(&format!("import Joint {} --parser desjardins", csv.display()));
    let out = t.ok(&format!("export Joint --all-months -o {}", dir.display()));
    assert_eq!(out.matches("wrote").count(), 3, "{out}");
    assert!(dir.join("joint-20260201-20260228.ofx").exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unknown_names_give_helpful_errors() {
    let mut t = household();
    let (r, _) = t.try_run("balance Nope");
    assert!(format!("{:#}", r.unwrap_err()).contains("no account matches"));
    let (r, _) = t.try_run("account owners Joint Nobody");
    assert!(format!("{:#}", r.unwrap_err()).contains("no person matches"));
    let (r, _) = t.try_run("balance Joint --on 2026-01-05");
    assert!(format!("{:#}", r.unwrap_err()).contains("starting balance"));
}

#[test]
fn parsers_are_listed() {
    let mut t = T::new();
    let out = t.ok("parsers");
    for id in ["desjardins", "desjardins-credit", "wealthsimple", "wealthsimple-credit", "ofx"] {
        assert!(out.contains(id), "{id} missing from {out}");
    }
}
