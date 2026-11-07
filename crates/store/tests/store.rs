use chrono::NaiveDate;
use ledgerhold_core::dedup::sha256_hex;
use ledgerhold_core::{AccountKind, BalanceCheckpoint, Money, ParsedStatement, ParsedTransaction};
use ledgerhold_store::{ImportMeta, NewAccount, PointKind, Store, StoreError};
use std::{fs, path::PathBuf};

fn d(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 1, day).unwrap()
}

fn tx(day: u32, cents: i64, desc: &str) -> ParsedTransaction {
    ParsedTransaction {
        date: d(day),
        posted_date: None,
        amount: Money(cents),
        description: desc.into(),
        fitid: None,
        source_seq: None,
        balance: None,
    }
}

fn stmt(rows: Vec<ParsedTransaction>) -> ParsedStatement {
    let mut s = ParsedStatement::new(AccountKind::Checking, "CAD");
    s.transactions = rows;
    s
}

fn meta<'a>(name: &'a str, sha: &'a str) -> ImportMeta<'a> {
    ImportMeta { parser: "test", filename: name, file_sha256: sha }
}

struct Fx {
    store: Store,
    inst: String,
    acct: String,
}

fn fx() -> Fx {
    let mut store = Store::open_in_memory().unwrap();
    let inst = store.add_institution("Desjardins", Some("815")).unwrap();
    let acct = store
        .add_account(&NewAccount {
            institution_id: inst.clone(),
            name: "Joint chequing".into(),
            kind: AccountKind::Checking,
            currency: "CAD".into(),
            external_account_id: Some("EOP-12345".into()),
            owners: vec![],
            balance_only: false,
        })
        .unwrap();
    Fx { store, inst, acct }
}

fn fixture(rel: &str) -> Vec<u8> {
    fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(rel)).unwrap()
}

// ---- people & accounts ------------------------------------------------------

#[test]
fn joint_account_has_several_owners_and_a_person_can_own_several_accounts() {
    let mut f = fx();
    let a = f.store.add_person("Alex", None).unwrap();
    let b = f.store.add_person("Sam", None).unwrap();
    f.store.set_owners(&f.acct, &[a.clone(), b.clone()]).unwrap();
    let second = f
        .store
        .add_account(&NewAccount {
            institution_id: f.inst.clone(),
            name: "Alex savings".into(),
            kind: AccountKind::Savings,
            currency: "CAD".into(),
            external_account_id: None,
            owners: vec![a.clone()],
            balance_only: false,
        })
        .unwrap();
    let mut owners = f.store.get_account(&f.acct).unwrap().owners;
    owners.sort();
    let mut expected = vec![a.clone(), b];
    expected.sort();
    assert_eq!(owners, expected);
    assert_eq!(f.store.get_account(&second).unwrap().owners, vec![a]);
    assert_eq!(f.store.list_people().unwrap().len(), 2);
}

#[test]
fn people_and_accounts_can_be_renamed() {
    let f = fx();
    let a = f.store.add_person("Alx", None).unwrap();
    f.store.update_person(&a, "Alex", None).unwrap();
    assert_eq!(f.store.list_people().unwrap()[0].name, "Alex");
    f.store.rename_account(&f.acct, "Joint").unwrap();
    assert_eq!(f.store.get_account(&f.acct).unwrap().name, "Joint");
    assert!(matches!(f.store.update_person("nope", "x", None), Err(StoreError::NotFound(_))));
    assert!(matches!(f.store.rename_account("nope", "x"), Err(StoreError::NotFound(_))));
}

#[test]
fn account_number_can_be_set_changed_and_cleared() {
    let f = fx();
    f.store.update_account(&f.acct, "Joint", Some("12345")).unwrap();
    assert_eq!(f.store.get_account(&f.acct).unwrap().external_account_id.as_deref(), Some("12345"));
    f.store.update_account(&f.acct, "Joint", Some("67890")).unwrap();
    assert_eq!(f.store.get_account(&f.acct).unwrap().external_account_id.as_deref(), Some("67890"));
    f.store.update_account(&f.acct, "Joint", None).unwrap();
    assert_eq!(f.store.get_account(&f.acct).unwrap().external_account_id, None);
    assert!(matches!(f.store.update_account("nope", "x", None), Err(StoreError::NotFound(_))));
}

#[test]
fn people_get_distinct_colours_that_can_be_chosen_and_changed() {
    let f = fx();
    let a = f.store.add_person("Antoine", None).unwrap();
    let b = f.store.add_person("Arabel", None).unwrap();
    let colour = |id: &str| f.store.list_people().unwrap().into_iter().find(|p| p.id == id).unwrap().color;
    assert_eq!((colour(&a).as_str(), colour(&b).as_str()), ("blue", "green"));

    // A chosen colour sticks; the next automatic one skips colours already taken.
    let c = f.store.add_person("Jo", Some("blue")).unwrap();
    assert_eq!(colour(&c), "blue");
    let d = f.store.add_person("Kay", None).unwrap();
    assert_eq!(colour(&d), "orange");

    // Renaming keeps the colour unless a new one is given.
    f.store.update_person(&a, "Antoine P", None).unwrap();
    assert_eq!(colour(&a), "blue");
    f.store.update_person(&a, "Antoine P", Some("pink")).unwrap();
    assert_eq!(colour(&a), "pink");

    assert!(matches!(f.store.add_person("X", Some("chartreuse")), Err(StoreError::Invalid(_))));
    assert!(matches!(f.store.update_person(&a, "Antoine", Some("nope")), Err(StoreError::Invalid(_))));
    assert_eq!(colour(&a), "pink");
}

#[test]
fn deleting_a_person_keeps_their_accounts_and_the_other_owner() {
    let mut f = fx();
    let a = f.store.add_person("Alex", None).unwrap();
    let b = f.store.add_person("Sam", None).unwrap();
    f.store.set_owners(&f.acct, &[a.clone(), b.clone()]).unwrap();
    f.store.delete_person(&a).unwrap();
    assert_eq!(f.store.get_account(&f.acct).unwrap().owners, vec![b]);
    assert_eq!(f.store.list_people().unwrap().len(), 1);
    assert!(matches!(f.store.delete_person(&a), Err(StoreError::NotFound(_))));
}

#[test]
fn deleting_an_account_removes_everything_under_it_and_nothing_else() {
    let mut f = fx();
    let other = f
        .store
        .add_account(&NewAccount {
            institution_id: f.inst.clone(),
            name: "Savings".into(),
            kind: AccountKind::Savings,
            currency: "CAD".into(),
            external_account_id: None,
            owners: vec![],
            balance_only: false,
        })
        .unwrap();
    let s = stmt(vec![tx(1, -500, "COFFEE"), tx(2, -900, "LUNCH")]);
    f.store.import(&f.acct, meta("a.csv", "h1"), &s).unwrap();
    f.store.import(&other, meta("b.csv", "h2"), &s).unwrap();
    f.store.set_anchor(&f.acct, NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), Money(1000), None).unwrap();

    assert_eq!(f.store.delete_account(&f.acct).unwrap(), 2);
    assert!(f.store.get_account(&f.acct).is_err());
    assert_eq!(f.store.list_accounts().unwrap().len(), 1);
    assert_eq!(f.store.list_transactions(&other, None, None).unwrap().len(), 2);
    assert!(f.store.list_batches(&f.acct).unwrap().is_empty());
    assert!(matches!(f.store.delete_account(&f.acct), Err(StoreError::NotFound(_))));
}

#[test]
fn a_bank_can_be_edited_and_only_deleted_once_empty() {
    let f = fx();
    f.store.update_institution(&f.inst, "Desjardins Caisse", None).unwrap();
    let i = &f.store.list_institutions().unwrap()[0];
    assert_eq!((i.name.as_str(), i.ofx_bank_id.as_deref()), ("Desjardins Caisse", None));
    assert!(matches!(f.store.update_institution("nope", "x", None), Err(StoreError::NotFound(_))));

    let err = f.store.delete_institution(&f.inst).unwrap_err();
    assert!(matches!(err, StoreError::InUse(_)) && err.to_string().contains("1 account"), "{err}");
    assert_eq!(f.store.list_institutions().unwrap().len(), 1);

    f.store.delete_account(&f.acct).unwrap();
    f.store.delete_institution(&f.inst).unwrap();
    assert!(f.store.list_institutions().unwrap().is_empty());
    assert!(matches!(f.store.delete_institution(&f.inst), Err(StoreError::NotFound(_))));
}

#[test]
fn external_account_id_is_unique_per_institution_and_findable() {
    let mut f = fx();
    let dup = f.store.add_account(&NewAccount {
        institution_id: f.inst.clone(),
        name: "dup".into(),
        kind: AccountKind::Checking,
        currency: "CAD".into(),
        external_account_id: Some("EOP-12345".into()),
        owners: vec![],
        balance_only: false,
    });
    assert!(dup.is_err());
    assert_eq!(f.store.find_account_by_external(&f.inst, "EOP-12345").unwrap().unwrap().id, f.acct);
    assert!(f.store.find_account_by_external(&f.inst, "nope").unwrap().is_none());
}

// ---- import / dedup / undo ---------------------------------------------------

#[test]
fn reimporting_the_same_statement_adds_nothing_and_records_no_batch() {
    let mut f = fx();
    let s = stmt(vec![tx(1, -500, "COFFEE"), tx(1, -500, "COFFEE"), tx(2, -900, "LUNCH")]);
    let first = f.store.import(&f.acct, meta("a.csv", "h1"), &s).unwrap();
    assert_eq!((first.new, first.duplicates), (3, 0));
    let again = f.store.import(&f.acct, meta("a.csv", "h1"), &s).unwrap();
    assert_eq!((again.new, again.duplicates, again.batch_id), (0, 3, None));
    assert_eq!(f.store.list_batches(&f.acct).unwrap().len(), 1);
    assert_eq!(f.store.list_transactions(&f.acct, None, None).unwrap().len(), 3);
}

#[test]
fn overlapping_statements_only_import_the_new_rows() {
    let mut f = fx();
    let a = stmt(vec![tx(1, -100, "A"), tx(5, -500, "COFFEE"), tx(8, -800, "B")]);
    let b = stmt(vec![tx(5, -500, "COFFEE"), tx(5, -500, "COFFEE"), tx(8, -800, "B"), tx(12, -1200, "C")]);
    f.store.import(&f.acct, meta("a", "h1"), &a).unwrap();
    let out = f.store.import(&f.acct, meta("b", "h2"), &b).unwrap();
    assert_eq!((out.new, out.duplicates), (2, 2)); // 2nd coffee + C new
    let all = f.store.list_transactions(&f.acct, None, None).unwrap();
    assert_eq!(all.len(), 5);
    assert_eq!(all.iter().filter(|t| t.description == "COFFEE").count(), 2);
}

#[test]
fn preview_writes_nothing_and_flags_an_already_imported_file() {
    let mut f = fx();
    let s = stmt(vec![tx(1, -100, "A")]);
    let p = f.store.preview_import(&f.acct, &s, "h1").unwrap();
    assert_eq!((p.new, p.duplicates, p.file_already_imported.is_none()), (1, 0, true));
    assert!(f.store.list_transactions(&f.acct, None, None).unwrap().is_empty());
    f.store.import(&f.acct, meta("a", "h1"), &s).unwrap();
    let p = f.store.preview_import(&f.acct, &s, "h1").unwrap();
    assert!(p.file_already_imported.is_some());
    assert_eq!((p.new, p.duplicates), (0, 1));
}

#[test]
fn undo_removes_the_whole_batch_and_allows_reimport() {
    let mut f = fx();
    let s1 = stmt(vec![tx(1, -100, "A")]);
    let s2 = stmt(vec![tx(2, -200, "B"), tx(3, -300, "C")]);
    f.store.import(&f.acct, meta("a", "h1"), &s1).unwrap();
    let batch = f.store.import(&f.acct, meta("b", "h2"), &s2).unwrap().batch_id.unwrap();
    assert_eq!(f.store.undo_batch(&batch).unwrap(), 2);
    assert_eq!(f.store.list_transactions(&f.acct, None, None).unwrap().len(), 1);
    assert_eq!(f.store.import(&f.acct, meta("b", "h2"), &s2).unwrap().new, 2);
    assert!(matches!(f.store.undo_batch("missing"), Err(StoreError::NotFound(_))));
}

#[test]
fn row_errors_are_stored_with_the_batch() {
    let mut f = fx();
    let mut s = stmt(vec![tx(1, -100, "A")]);
    s.errors.push(ledgerhold_core::RowError { row_ref: "line 9".into(), message: "bad date".into() });
    let out = f.store.import(&f.acct, meta("a", "h1"), &s).unwrap();
    assert_eq!(out.errors, 1);
    let errs = f.store.batch_errors(&out.batch_id.unwrap()).unwrap();
    assert_eq!(errs, vec![("line 9".to_string(), "bad date".to_string())]);
}

#[test]
fn currency_and_card_vs_bank_mismatches_are_rejected() {
    let mut f = fx();
    let mut usd = stmt(vec![tx(1, -100, "A")]);
    usd.currency = "USD".into();
    assert!(matches!(f.store.import(&f.acct, meta("a", "h"), &usd), Err(StoreError::CurrencyMismatch { .. })));
    let mut card = stmt(vec![tx(1, -100, "A")]);
    card.kind = AccountKind::CreditCard;
    assert!(matches!(f.store.import(&f.acct, meta("a", "h"), &card), Err(StoreError::KindMismatch { .. })));
    assert!(f.store.list_transactions(&f.acct, None, None).unwrap().is_empty());
}

#[test]
fn csv_then_ofx_of_the_same_data_is_recognised_as_duplicates() {
    // Desjardins CSV (no bank ids) first, then the OFX version of the same
    // account (bank ids never seen before): nothing new.
    let mut f = fx();
    let csv = ledgerhold_parsers::by_id("desjardins").unwrap().parse(&fixture("synthetic/desjardins.csv")).unwrap();
    let eop = csv.iter().find(|s| s.account_hint.as_deref() == Some("EOP-12345")).unwrap();
    let first = f.store.import(&f.acct, meta("csv", "h1"), eop).unwrap();
    assert_eq!(first.new, 10);

    let ofx = ledgerhold_ofx::read(&fixture("golden/legacy_desjardins_eop.ofx")).unwrap();
    let second = f.store.import(&f.acct, meta("ofx", "h2"), &ofx[0]).unwrap();
    assert_eq!((second.new, second.duplicates), (0, 10));
}

#[test]
fn real_fixture_roundtrip_through_parser_and_store() {
    let mut f = fx();
    let parsed = ledgerhold_parsers::by_id("wealthsimple").unwrap().parse(&fixture("synthetic/wealthsimple_chequing.csv")).unwrap();
    let sha = sha256_hex(&fixture("synthetic/wealthsimple_chequing.csv"));
    let first = f.store.import(&f.acct, meta("ws.csv", &sha), &parsed[0]).unwrap();
    assert_eq!((first.new, first.errors), (8, 2));
    assert_eq!(f.store.import(&f.acct, meta("ws.csv", &sha), &parsed[0]).unwrap().new, 0);
}

// ---- balances ---------------------------------------------------------------

fn with_history() -> Fx {
    let mut f = fx();
    // Anchor: 1000.00 at END of Jan 1. A Jan-1 transaction is already inside it.
    f.store.set_anchor(&f.acct, d(1), Money(100000), Some("opening")).unwrap();
    let s = stmt(vec![tx(1, -1000, "BEFORE-OR-ON-ANCHOR"), tx(2, -5000, "A"), tx(3, -2500, "B"), tx(10, 20000, "PAY")]);
    f.store.import(&f.acct, meta("h", "h"), &s).unwrap();
    f
}

#[test]
fn balance_forward_backward_and_anchor_day_semantics() {
    let f = with_history();
    let bal = |day| f.store.balance_at(&f.acct, d(day)).unwrap().balance.minor();
    assert_eq!(bal(1), 100000, "anchor day: its own transactions are not double counted");
    assert_eq!(bal(2), 95000);
    assert_eq!(bal(5), 92500);
    assert_eq!(bal(10), 112500);
    let before = f.store.balance_at(&f.acct, NaiveDate::from_ymd_opt(2025, 12, 31).unwrap()).unwrap();
    assert_eq!(before.balance.minor(), 101000, "working backwards adds back the Jan 1 debit");
    assert_eq!(before.anchor.as_of, d(1));
}

#[test]
fn no_anchor_is_a_clear_error() {
    let f = fx();
    assert!(matches!(f.store.balance_at(&f.acct, d(5)), Err(StoreError::NoAnchor)));
}

#[test]
fn daily_balances_cover_every_day() {
    let f = with_history();
    let v: Vec<i64> = f.store.daily_balances(&f.acct, d(1), d(4)).unwrap().into_iter().map(|(_, m)| m.minor()).collect();
    assert_eq!(v, [100000, 95000, 92500, 92500]);
}

#[test]
fn a_later_anchor_replaces_the_earlier_one_for_dates_after_it() {
    let f = with_history();
    f.store.set_anchor(&f.acct, d(10), Money(115000), Some("re-anchored from statement")).unwrap();
    let r = f.store.balance_at(&f.acct, d(11)).unwrap();
    assert_eq!((r.balance.minor(), r.anchor.as_of), (115000, d(10)));
    // Replacing an anchor on the same day updates it.
    f.store.set_anchor(&f.acct, d(10), Money(116000), None).unwrap();
    assert_eq!(f.store.list_anchors(&f.acct).unwrap().len(), 2);
    assert_eq!(f.store.balance_at(&f.acct, d(10)).unwrap().balance.minor(), 116000);
}

#[test]
fn reconcile_pinpoints_the_period_with_a_missing_transaction() {
    let mut f = with_history();
    // Statement says 1125.00 at Jan 10 (matches), 1100.00 at Jan 31 although we
    // have no transactions after Jan 10: a 25.00 debit is missing.
    let mut s = stmt(vec![]);
    s.checkpoints = vec![
        BalanceCheckpoint { as_of: d(10), balance: Money(112500) },
        BalanceCheckpoint { as_of: d(31), balance: Money(110000) },
    ];
    f.store.import(&f.acct, meta("ofx", "h3"), &s).unwrap();
    let r = f.store.reconcile(&f.acct).unwrap();
    assert_eq!(r.len(), 2);
    assert!(r[0].is_ok());
    assert_eq!((r[0].from_kind, r[0].to_kind), (PointKind::Anchor, PointKind::Statement));
    assert_eq!((r[1].from, r[1].to), (d(10), d(31)));
    assert_eq!(r[1].difference, Money(-2500));
    assert!(!r[1].is_ok());
}

// ---- export -----------------------------------------------------------------

#[test]
fn export_range_has_balance_at_range_end_and_stable_ids() {
    let f = with_history();
    let ex = f.store.build_ofx_exports(std::slice::from_ref(&f.acct), d(3), d(5)).unwrap();
    let e = &ex[0];
    assert_eq!(e.account.bank_id, "815");
    assert_eq!(e.account.account_id, "EOP-12345");
    assert_eq!(e.transactions.len(), 1);
    assert_eq!(e.transactions[0].description, "B");
    assert_eq!(e.ledger_balance, Some((d(5), Money(92500))));
    let again = f.store.build_ofx_exports(std::slice::from_ref(&f.acct), d(3), d(5)).unwrap();
    assert_eq!(e.transactions[0].fitid, again[0].transactions[0].fitid);
    assert!(e.transactions[0].fitid.starts_with("LH-"));
    assert!(f.store.build_ofx_exports(std::slice::from_ref(&f.acct), d(5), d(3)).is_err());
}

#[test]
fn account_without_anchor_exports_without_ledger_balance() {
    let mut f = fx();
    f.store.import(&f.acct, meta("h", "h"), &stmt(vec![tx(2, -100, "A")])).unwrap();
    let ex = f.store.build_ofx_exports(std::slice::from_ref(&f.acct), d(1), d(31)).unwrap();
    assert!(ex[0].ledger_balance.is_none());
}

#[test]
fn exporting_then_reimporting_our_own_ofx_adds_nothing() {
    let mut f = with_history();
    let ex = f.store.build_ofx_exports(std::slice::from_ref(&f.acct), d(1), d(31)).unwrap();
    let text = ledgerhold_ofx::write(&ex, d(31).and_hms_opt(0, 0, 0).unwrap());
    let back = ledgerhold_ofx::read(text.as_bytes()).unwrap();
    assert_eq!(back[0].transactions.len(), 4);
    let out = f.store.import(&f.acct, meta("own.ofx", "h9"), &back[0]).unwrap();
    assert_eq!((out.new, out.duplicates), (0, 4));
}

// ---- persistence ---------------------------------------------------------------

#[test]
fn file_database_persists_and_migrations_are_idempotent() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let path = dir.join(format!("ledgerhold-test-{}.sqlite", std::process::id()));
    let _ = fs::remove_file(&path);
    let id;
    {
        let s = Store::open(&path).unwrap();
        assert_eq!(s.schema_version().unwrap(), 3);
        id = s.add_person("Alex", None).unwrap();
    }
    let s = Store::open(&path).unwrap();
    assert_eq!(s.schema_version().unwrap(), 3);
    assert_eq!(s.list_people().unwrap()[0].id, id);
    drop(s);
    for ext in ["", "-wal", "-shm"] {
        let _ = fs::remove_file(format!("{}{ext}", path.display()));
    }
}

#[test]
fn upgrading_an_existing_database_gives_each_person_a_distinct_colour() {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("upgrade-{}.sqlite", std::process::id()));
    for ext in ["", "-wal", "-shm"] {
        let _ = fs::remove_file(format!("{}{ext}", path.display()));
    }
    {
        // A database as version 1 left it: no colour column.
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(include_str!("../migrations/0001_initial.sql")).unwrap();
        for (id, name) in [("p1", "Antoine"), ("p2", "Arabel"), ("p3", "Sam")] {
            conn.execute("INSERT INTO person(id, name) VALUES (?1, ?2)", [id, name]).unwrap();
        }
        conn.pragma_update(None, "user_version", 1).unwrap();
    }
    let s = Store::open(&path).unwrap();
    assert_eq!(s.schema_version().unwrap(), 3);
    let by_name: Vec<_> = s.list_people().unwrap().into_iter().map(|p| (p.name, p.color)).collect();
    assert_eq!(
        by_name,
        vec![("Antoine".into(), "blue".into()), ("Arabel".into(), "green".into()), ("Sam".into(), "orange".into())]
    );
    // And the next person added gets a colour nobody has.
    let id = s.add_person("Jo", None).unwrap();
    assert_eq!(s.list_people().unwrap().into_iter().find(|p| p.id == id).unwrap().color, "red");
    drop(s);
    for ext in ["", "-wal", "-shm"] {
        let _ = fs::remove_file(format!("{}{ext}", path.display()));
    }
}

#[cfg(unix)]
mod file_permissions {
    use ledgerhold_store::Store;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("perm-{name}-{}", std::process::id()))
    }
    fn mode(p: &std::path::Path) -> u32 {
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn a_new_database_and_its_folders_are_private() {
        let root = scratch("new");
        let _ = std::fs::remove_dir_all(&root);
        let db = root.join("a/b/ledgerhold.sqlite");
        let store = Store::open(&db).unwrap();
        // Writing makes SQLite create its -wal and -shm files; they must be private too.
        store.add_person("Alex", None).unwrap();
        assert_eq!(mode(&db), 0o600);
        for side in ["-wal", "-shm"] {
            let p = PathBuf::from(format!("{}{side}", db.display()));
            if p.exists() {
                assert_eq!(mode(&p), 0o600, "{}", p.display());
            }
        }
        assert_eq!(mode(db.parent().unwrap()), 0o700);
        assert_eq!(mode(&root), 0o700);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_existing_world_readable_database_is_tightened() {
        let root = scratch("old");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("ledgerhold.sqlite");
        drop(Store::open(&db).unwrap());
        std::fs::set_permissions(&db, std::fs::Permissions::from_mode(0o644)).unwrap();
        drop(Store::open(&db).unwrap());
        assert_eq!(mode(&db), 0o600, "installs made before this was enforced get fixed on next start");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_existing_world_readable_folder_is_tightened() {
        let root = scratch("dir");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        ledgerhold_store::restrict_dir_to_owner(&root);
        assert_eq!(mode(&root), 0o700);
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[test]
fn upgrading_leaves_existing_accounts_reconciling_as_before() {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("upgrade-bo-{}.sqlite", std::process::id()));
    for ext in ["", "-wal", "-shm"] {
        let _ = fs::remove_file(format!("{}{ext}", path.display()));
    }
    {
        // A database as version 2 left it: no balance_only column.
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(include_str!("../migrations/0001_initial.sql")).unwrap();
        conn.execute_batch(include_str!("../migrations/0002_person_color.sql")).unwrap();
        conn.execute("INSERT INTO institution(id, name) VALUES ('i1', 'Desjardins')", []).unwrap();
        conn.execute(
            "INSERT INTO account(id, institution_id, name, kind, currency) VALUES ('a1', 'i1', 'Chequing', 'checking', 'CAD')",
            [],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 2).unwrap();
    }
    let s = Store::open(&path).unwrap();
    assert_eq!(s.schema_version().unwrap(), 3);
    let a = s.get_account("a1").unwrap();
    assert!(!a.balance_only);
    s.set_balance_only("a1", true).unwrap();
    assert!(s.get_account("a1").unwrap().balance_only);
    s.set_balance_only("a1", false).unwrap();
    assert!(!s.get_account("a1").unwrap().balance_only);
    assert!(s.set_balance_only("missing", true).is_err());
    drop(s);
    for ext in ["", "-wal", "-shm"] {
        let _ = fs::remove_file(format!("{}{ext}", path.display()));
    }
}
