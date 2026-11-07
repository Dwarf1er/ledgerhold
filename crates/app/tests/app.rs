use chrono::NaiveDate;
use ledgerhold_app::*;
use ledgerhold_store::Store;
use serde_json::json;
use std::{fs, path::PathBuf};

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}
fn today() -> NaiveDate {
    d(2026, 2, 1)
}
fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(rel)
}

struct Fx {
    store: Store,
    acct: String,
}

fn fx() -> Fx {
    let mut store = Store::open_in_memory().unwrap();
    let alex = add_person(&store, "Alex", None).unwrap();
    let sam = add_person(&store, "Sam", None).unwrap();
    let inst = add_institution(&store, "Desjardins", Some("815")).unwrap();
    let acct = add_account(
        &mut store,
        NewAccountInput {
            institution_id: inst.id,
            name: "Joint chequing".into(),
            kind: "checking".into(),
            currency: "cad".into(),
            number: Some("EOP-12345".into()),
            owner_ids: vec![alex.id, sam.id],
        },
    )
    .unwrap();
    Fx { store, acct }
}

fn import_csv(f: &mut Fx) {
    let r = commit_import(&mut f.store, &f.acct, &fixture("synthetic/desjardins.csv"), Some("desjardins"), &[]).unwrap();
    assert_eq!(r.new_count, 10);
}

fn anchored() -> Fx {
    let mut f = fx();
    import_csv(&mut f);
    set_anchor(&f.store, &f.acct, d(2026, 1, 1), 70000, Some("opening")).unwrap();
    f
}

#[test]
fn overview_has_no_balance_until_an_anchor_exists() {
    let mut f = fx();
    let o = overview(&f.store, today()).unwrap();
    assert_eq!((o.people.len(), o.institutions.len(), o.accounts.len()), (2, 1, 1));
    let s = &o.accounts[0];
    assert_eq!(s.account.institution_name, "Desjardins");
    assert_eq!(s.account.currency, "CAD", "currency is normalised");
    assert_eq!(s.account.owner_ids.len(), 2);
    assert_eq!((s.balance, s.txn_count), (None, 0));

    import_csv(&mut f);
    set_anchor(&f.store, &f.acct, d(2026, 1, 1), 70000, Some("opening")).unwrap();
    let s = &overview(&f.store, today()).unwrap().accounts[0];
    assert_eq!(s.balance, Some(196145));
    assert_eq!((s.txn_count, s.first_date, s.last_date), (10, Some(d(2026, 1, 2)), Some(d(2026, 1, 11))));
}

#[test]
fn dtos_serialize_camel_case_with_iso_dates_and_minor_units() {
    let f = anchored();
    let v = serde_json::to_value(overview(&f.store, today()).unwrap()).unwrap();
    let a = &v["accounts"][0];
    assert_eq!(a["txnCount"], 10);
    assert_eq!(a["firstDate"], "2026-01-02");
    assert_eq!(a["balance"], 196145);
    assert_eq!(a["account"]["institutionName"], "Desjardins");
    assert_eq!(a["account"]["kind"], "checking");
    assert!(a["account"]["ownerIds"].is_array());
    let t = serde_json::to_value(transactions(&f.store, &f.acct, None, None, None).unwrap()).unwrap();
    assert_eq!(t[0], json!({
        "id": t[0]["id"], "date": "2026-01-11", "postedDate": null, "amount": -300,
        "description": "SANS SOLDE", "balance": 196145, "batchId": t[0]["batchId"]
    }));
}

#[test]
fn transactions_are_newest_first_with_true_running_balances() {
    let f = anchored();
    let t = transactions(&f.store, &f.acct, None, None, None).unwrap();
    assert_eq!(t.len(), 10);
    assert_eq!(t[0].date, d(2026, 1, 11));
    assert_eq!(t.last().unwrap().balance, Some(320000), "first row: 700.00 + 2500.00");
    // The bank's own running balance column is reproduced by ours (the file has them, except the last row).
    for x in &t[1..] {
        let bank = match x.description.as_str() {
            "PAIE ACME INC" => 320000,
            "ACHAT EPICERIE #123" => 311583,
            "PRELEVEMENT LOYER, JANVIER" => 195683,
            "DEPOT GUICHET" => 196445,
            _ => continue,
        };
        assert_eq!(x.balance, Some(bank), "{}", x.description);
    }
}

#[test]
fn filters_do_not_change_running_balances() {
    let f = anchored();
    let all = transactions(&f.store, &f.acct, None, None, None).unwrap();
    let ranged = transactions(&f.store, &f.acct, Some(d(2026, 1, 6)), Some(d(2026, 1, 8)), None).unwrap();
    assert_eq!(ranged.len(), 3);
    for r in &ranged {
        assert_eq!(all.iter().find(|a| a.id == r.id).unwrap().balance, r.balance);
    }
    let found = transactions(&f.store, &f.acct, None, None, Some("  cafe ")).unwrap();
    assert_eq!(found.len(), 2);
    assert!(found.iter().all(|t| t.description == "CAFE"));
    assert_eq!(transactions(&f.store, &f.acct, None, None, Some("1200.00")).unwrap().len(), 1, "search by amount");
    assert!(transactions(&f.store, &f.acct, None, None, Some("zzz")).unwrap().is_empty());
}

#[test]
fn transactions_without_an_anchor_have_no_balance() {
    let mut f = fx();
    import_csv(&mut f);
    assert!(transactions(&f.store, &f.acct, None, None, None).unwrap().iter().all(|t| t.balance.is_none()));
}

#[test]
fn balance_series_covers_each_day() {
    let f = anchored();
    let s = balance_series(&f.store, &f.acct, d(2026, 1, 1), d(2026, 1, 4)).unwrap();
    // End-of-day: Jan 3 has three transactions (-84.17, -4.50, -4.50); Jan 4 has none, so it carries forward.
    assert_eq!(s.iter().map(|p| p.balance).collect::<Vec<_>>(), [70000, 320000, 310683, 310683]);
}

#[test]
fn account_detail_includes_anchors_batches_and_reconciliation() {
    let f = anchored();
    set_anchor(&f.store, &f.acct, d(2026, 1, 31), 190000, Some("statement")).unwrap();
    let det = account_detail(&f.store, &f.acct, today()).unwrap();
    assert_eq!(det.anchors.len(), 2);
    assert_eq!(det.batches.len(), 1);
    assert_eq!((det.batches[0].rows_new, det.batches[0].rows_error), (10, 2));
    assert_eq!(det.reconciliation.len(), 1);
    let r = &det.reconciliation[0];
    assert!(!r.ok);
    assert_eq!((r.expected, r.actual, r.difference), (196145, 190000, -6145));
    assert_eq!((r.from_kind.as_str(), r.to_kind.as_str()), ("anchor", "anchor"));
    delete_anchor(&f.store, &det.anchors[1].id).unwrap();
    assert!(account_detail(&f.store, &f.acct, today()).unwrap().reconciliation.is_empty());
}

#[test]
fn preview_shows_rows_with_status_errors_and_writes_nothing() {
    let f = fx();
    let p = preview_import(&f.store, &f.acct, &fixture("synthetic/desjardins.csv"), Some("desjardins")).unwrap();
    assert_eq!((p.new_count, p.duplicate_count, p.errors.len()), (10, 0, 2));
    assert_eq!(p.rows.len(), 10);
    assert!(p.rows.iter().all(|r| r.status == "new"));
    assert_eq!(p.account_hint.as_deref(), Some("EOP-12345"));
    assert_eq!(p.period, Some((d(2026, 1, 2), d(2026, 1, 11))));
    assert_eq!(p.file_name, "desjardins.csv");
    assert!(!p.file_already_imported);
    assert!(transactions(&f.store, &f.acct, None, None, None).unwrap().is_empty());
}

#[test]
fn preview_after_import_marks_everything_duplicate_and_flags_the_file() {
    let mut f = fx();
    import_csv(&mut f);
    let p = preview_import(&f.store, &f.acct, &fixture("synthetic/desjardins.csv"), Some("desjardins")).unwrap();
    assert_eq!((p.new_count, p.duplicate_count), (0, 10));
    assert!(p.rows.iter().all(|r| r.status == "duplicate"));
    assert!(p.file_already_imported);
}

#[test]
fn commit_then_undo_roundtrip() {
    let mut f = fx();
    import_csv(&mut f);
    let det = account_detail(&f.store, &f.acct, today()).unwrap();
    assert_eq!(undo_batch(&f.store, &det.batches[0].id).unwrap(), 10);
    assert!(transactions(&f.store, &f.acct, None, None, None).unwrap().is_empty());
    assert!(undo_batch(&f.store, "nope").is_err());
}

#[test]
fn format_is_inferred_from_extension_and_unknown_formats_are_rejected() {
    let f = fx();
    let ofx = preview_import(&f.store, &f.acct, &fixture("golden/legacy_desjardins_eop.ofx"), None).unwrap();
    assert_eq!((ofx.parser.as_str(), ofx.new_count), ("ofx", 10));
    let err = preview_import(&f.store, &f.acct, &fixture("synthetic/desjardins.csv"), None).unwrap_err();
    assert!(format!("{err:#}").contains("choose a parser"));
    assert!(preview_import(&f.store, &f.acct, &fixture("nope.ofx"), None).is_err());
}

#[test]
fn wrong_currency_account_is_rejected_with_an_explanation() {
    let mut f = fx();
    let inst = overview(&f.store, today()).unwrap().institutions[0].id.clone();
    let usd = add_account(
        &mut f.store,
        NewAccountInput { institution_id: inst, name: "USD".into(), kind: "credit_card".into(), currency: "usd".into(), number: None, owner_ids: vec![] },
    )
    .unwrap();
    let ok = preview_import(&f.store, &usd, &fixture("synthetic/wealthsimple_credit.csv"), Some("wealthsimple-credit")).unwrap();
    assert_eq!((ok.currency.as_str(), ok.new_count), ("USD", 1));
    let err = preview_import(&f.store, &f.acct, &fixture("synthetic/wealthsimple_credit.csv"), Some("wealthsimple-credit"));
    assert!(err.is_err(), "CAD rows exist but the account number doesn't match, and kind differs");
}

#[test]
fn export_writes_one_named_file_per_account_with_balance() {
    let f = anchored();
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("app-export-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let now = d(2026, 2, 1).and_hms_opt(9, 0, 0).unwrap();
    let r = export_ofx(&f.store, std::slice::from_ref(&f.acct), d(2026, 1, 1), d(2026, 1, 31), &dir, now).unwrap();
    assert_eq!(r.len(), 1);
    assert!(r[0].path.ends_with("joint-chequing-20260101-20260131.ofx"));
    assert_eq!((r[0].txn_count, r[0].balance), (10, Some(196145)));
    let text = fs::read_to_string(&r[0].path).unwrap();
    assert!(text.contains("<BANKID>815") && text.contains("<LEDGERBAL>"));
    assert!(export_ofx(&f.store, std::slice::from_ref(&f.acct), d(2026, 2, 1), d(2026, 1, 1), &dir, now).is_err());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn setup_validation_and_owner_changes() {
    let mut f = fx();
    assert!(add_person(&f.store, "   ", None).is_err());
    assert!(add_institution(&f.store, "", None).is_err());
    let inst = overview(&f.store, today()).unwrap().institutions[0].id.clone();
    let bad = add_account(
        &mut f.store,
        NewAccountInput { institution_id: inst, name: "x".into(), kind: "brokerage".into(), currency: "CAD".into(), number: None, owner_ids: vec![] },
    );
    assert!(bad.is_err());
    let o = overview(&f.store, today()).unwrap();
    set_owners(&mut f.store, &f.acct, &[o.people[0].id.clone()]).unwrap();
    assert_eq!(overview(&f.store, today()).unwrap().accounts[0].account.owner_ids.len(), 1);
}

#[test]
fn parsers_lists_all_formats() {
    let ids: Vec<_> = parsers().into_iter().map(|p| p.id).collect();
    assert_eq!(ids, ["desjardins", "desjardins-credit", "ofx", "wealthsimple", "wealthsimple-credit"]);
}

#[test]
fn helpers() {
    assert_eq!(kebab("Joint  chequing (CAD)"), "joint-chequing-cad");
    assert_eq!(parse_month("2026-02").unwrap(), (d(2026, 2, 1), d(2026, 2, 28)));
    assert_eq!(parse_month("2026-12").unwrap().1, d(2026, 12, 31));
    assert!(parse_month("2026-13").is_err() && parse_month("nope").is_err());
}

#[test]
fn database_location_follows_each_platforms_conventions() {
    use std::collections::HashMap;
    let env = |pairs: &[(&str, &str)]| {
        let m: HashMap<String, std::ffi::OsString> = pairs.iter().map(|(k, v)| (k.to_string(), (*v).into())).collect();
        move |k: &str| m.get(k).cloned()
    };
    let p = |os, e: &dyn Fn(&str) -> Option<std::ffi::OsString>| db_path_for(os, e).to_string_lossy().replace('\\', "/");

    assert_eq!(p("linux", &env(&[("HOME", "/home/a")])), "/home/a/.local/share/ledgerhold/ledgerhold.sqlite");
    assert_eq!(p("linux", &env(&[("HOME", "/home/a"), ("XDG_DATA_HOME", "/data")])), "/data/ledgerhold/ledgerhold.sqlite");
    assert_eq!(p("macos", &env(&[("HOME", "/Users/a")])), "/Users/a/Library/Application Support/ledgerhold/ledgerhold.sqlite");
    // Windows has no HOME: it must use LOCALAPPDATA, never the working directory.
    assert_eq!(p("windows", &env(&[("LOCALAPPDATA", "C:/Users/a/AppData/Local")])), "C:/Users/a/AppData/Local/ledgerhold/ledgerhold.sqlite");
    assert_eq!(p("windows", &env(&[("APPDATA", "C:/Users/a/AppData/Roaming")])), "C:/Users/a/AppData/Roaming/ledgerhold/ledgerhold.sqlite");
    // Nothing known: still produces a usable path.
    assert_eq!(p("linux", &env(&[])), "./ledgerhold/ledgerhold.sqlite");
    assert_eq!(p("windows", &env(&[("HOME", "/ignored")])), "./ledgerhold/ledgerhold.sqlite");
}

#[test]
fn commit_leaves_out_excluded_rows_and_offers_them_again_later() {
    let mut f = fx();
    let path = fixture("synthetic/desjardins.csv");
    let p = preview_import(&f.store, &f.acct, &path, Some("desjardins")).unwrap();
    let skip: Vec<usize> = p.rows.iter().filter(|r| r.status == "new").take(2).map(|r| r.index).collect();
    assert_eq!(skip.len(), 2);

    let r = commit_import(&mut f.store, &f.acct, &path, Some("desjardins"), &skip).unwrap();
    assert_eq!(r.new_count, p.new_count - 2);

    let again = preview_import(&f.store, &f.acct, &path, Some("desjardins")).unwrap();
    assert_eq!(again.new_count, 2);
}
