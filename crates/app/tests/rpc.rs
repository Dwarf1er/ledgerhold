use chrono::NaiveDate;
use ledgerhold_app::rpc::{COMMANDS, RpcContext, RpcError, call};
use ledgerhold_store::Store;
use serde_json::{Value, json};
use std::path::PathBuf;

fn now() -> chrono::NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 2, 1).unwrap().and_hms_opt(9, 0, 0).unwrap()
}

fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(rel)
}

/// Resolver that maps "fixture:<rel>" to a fixture file and refuses everything else.
fn resolver(p: &str) -> anyhow::Result<PathBuf> {
    match p.strip_prefix("fixture:") {
        Some(rel) => Ok(fixture(rel)),
        None => anyhow::bail!("unknown file {p:?}"),
    }
}

fn run(store: &mut Store, cmd: &str, args: Value) -> Result<Value, RpcError> {
    let ctx = RpcContext { now: now(), data_location: "server data".into(), resolve_path: &resolver };
    call(store, cmd, args, &ctx)
}

fn ok(store: &mut Store, cmd: &str, args: Value) -> Value {
    run(store, cmd, args).unwrap_or_else(|e| panic!("{cmd} failed: {}", e.message()))
}

#[test]
fn full_household_flow_over_json() {
    let mut s = Store::open_in_memory().unwrap();
    let alex = ok(&mut s, "add_person", json!({"name": "Alex"}));
    let inst = ok(&mut s, "add_institution", json!({"name": "Desjardins", "bankId": "815"}));
    assert_eq!(inst["bankId"], "815");
    let acct = ok(
        &mut s,
        "add_account",
        json!({"input": {"institutionId": inst["id"], "name": "Joint", "kind": "checking", "currency": "CAD",
                          "number": "EOP-12345", "ownerIds": [alex["id"]]}}),
    );
    let acct = acct.as_str().unwrap().to_string();

    let p = ok(&mut s, "preview_import", json!({"accountId": acct, "path": "fixture:synthetic/desjardins.csv", "parser": "desjardins"}));
    assert_eq!((p["newCount"].as_u64(), p["duplicateCount"].as_u64()), (Some(10), Some(0)));
    let c = ok(&mut s, "commit_import", json!({"accountId": acct, "path": "fixture:synthetic/desjardins.csv", "parser": "desjardins"}));
    assert_eq!(c["newCount"], 10);
    ok(&mut s, "set_anchor", json!({"accountId": acct, "date": "2026-01-01", "amount": 70000, "note": "opening"}));

    let ov = ok(&mut s, "overview", json!({}));
    assert_eq!(ov["accounts"][0]["balance"], 196145);
    let t = ok(&mut s, "transactions", json!({"accountId": acct, "search": "cafe"}));
    assert_eq!(t.as_array().unwrap().len(), 2);
    let series = ok(&mut s, "balance_series", json!({"accountId": acct, "start": "2026-01-01", "end": "2026-01-03"}));
    assert_eq!(series.as_array().unwrap().len(), 3);
    let det = ok(&mut s, "account_detail", json!({"accountId": acct}));
    let batch = det["batches"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(ok(&mut s, "undo_batch", json!({"batchId": batch})), 10);
    assert_eq!(ok(&mut s, "db_path", json!({})), "server data");
    assert_eq!(ok(&mut s, "parsers", json!({})).as_array().unwrap().len(), 5);
}

#[test]
fn errors_are_classified() {
    let mut s = Store::open_in_memory().unwrap();
    assert!(matches!(run(&mut s, "nope", json!({})), Err(RpcError::UnknownCommand(_))));
    assert!(matches!(run(&mut s, "account_detail", json!({})), Err(RpcError::BadArgs(_))));
    assert!(matches!(run(&mut s, "set_anchor", json!({"accountId": "x", "date": "not-a-date", "amount": 1})), Err(RpcError::BadArgs(_))));
    // A well-formed request for something that doesn't exist is a failure, not bad args.
    let e = run(&mut s, "account_detail", json!({"accountId": "missing"})).unwrap_err();
    assert!(matches!(e, RpcError::Failed(_)) && e.message().contains("not found"), "{}", e.message());
}

#[test]
fn file_paths_go_through_the_resolver_only() {
    let mut s = Store::open_in_memory().unwrap();
    let inst = ok(&mut s, "add_institution", json!({"name": "B"}));
    let acct = ok(&mut s, "add_account", json!({"input": {"institutionId": inst["id"], "name": "A", "kind": "checking", "currency": "CAD", "number": null, "ownerIds": []}}));
    for evil in ["/etc/passwd", "../../secret.csv", "fixture-less"] {
        let e = run(&mut s, "preview_import", json!({"accountId": acct, "path": evil, "parser": "ofx"})).unwrap_err();
        assert!(matches!(e, RpcError::Failed(_)), "{evil}");
    }
}

#[test]
fn every_advertised_command_is_handled() {
    let mut s = Store::open_in_memory().unwrap();
    for c in COMMANDS {
        let r = run(&mut s, c, json!({}));
        assert!(!matches!(r, Err(RpcError::UnknownCommand(_))), "{c} is listed but not handled");
    }
}
