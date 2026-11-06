use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use ledgerhold_server::{AppState, Config, router};
use serde_json::{Value, json};
use std::path::PathBuf;
use tower::ServiceExt;

const PASSWORD: &str = "household-pass";

fn fixture(rel: &str) -> Vec<u8> {
    std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(rel)).unwrap()
}

struct Env {
    app: Router,
    dir: PathBuf,
    cookie: Option<String>,
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn config(dir: &std::path::Path, password: Option<&str>) -> Config {
    Config {
        db_path: dir.join("data/ledgerhold.sqlite"),
        listen: "127.0.0.1:0".parse().unwrap(),
        password: password.map(String::from),
        secret: None,
        static_dir: dir.join("ui"),
        allow_no_auth: false,
        allowed_origins: vec![],
    }
}

fn env_with(password: Option<&str>, tweak: impl FnOnce(&mut Config)) -> Env {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("srv-{}", uuid_like()));
    std::fs::create_dir_all(dir.join("ui")).unwrap();
    std::fs::write(dir.join("ui/index.html"), "<!doctype html><title>Ledgerhold</title>").unwrap();
    let mut cfg = config(&dir, password);
    tweak(&mut cfg);
    let app = router(AppState::open(&cfg).unwrap());
    Env { app, dir, cookie: None }
}

fn uuid_like() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    format!("{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst))
}

fn env() -> Env {
    env_with(Some(PASSWORD), |_| {})
}

struct Resp {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl Resp {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|_| panic!("not JSON: {}", String::from_utf8_lossy(&self.body)))
    }
    fn error(&self) -> String {
        self.json()["error"].as_str().unwrap_or("").to_string()
    }
}

impl Env {
    async fn send(&self, method: Method, path: &str, content_type: Option<&str>, body: Vec<u8>, extra: &[(&str, &str)]) -> Resp {
        let mut b = Request::builder().method(method).uri(path).header(header::HOST, "ledger.local");
        if let Some(c) = &self.cookie {
            b = b.header(header::COOKIE, c);
        }
        if let Some(ct) = content_type {
            b = b.header(header::CONTENT_TYPE, ct);
        }
        for (k, v) in extra {
            b = b.header(*k, *v);
        }
        let res = self.app.clone().oneshot(b.body(Body::from(body)).unwrap()).await.unwrap();
        let (parts, body) = res.into_parts();
        Resp { status: parts.status, headers: parts.headers, body: body.collect().await.unwrap().to_bytes().to_vec() }
    }

    async fn get(&self, path: &str) -> Resp {
        self.send(Method::GET, path, None, vec![], &[]).await
    }

    async fn rpc(&self, command: &str, args: Value) -> Resp {
        self.send(Method::POST, &format!("/api/rpc/{command}"), Some("application/json"), args.to_string().into_bytes(), &[]).await
    }

    async fn ok(&self, command: &str, args: Value) -> Value {
        let r = self.rpc(command, args).await;
        assert_eq!(r.status, StatusCode::OK, "{command}: {}", r.error());
        r.json()
    }

    async fn login(&mut self) {
        let r = self
            .send(Method::POST, "/api/login", Some("application/json"), json!({ "password": PASSWORD }).to_string().into_bytes(), &[])
            .await;
        assert_eq!(r.status, StatusCode::NO_CONTENT);
        let set = r.headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
        self.cookie = Some(set.split(';').next().unwrap().to_string());
    }

    async fn upload(&self, name: &str, bytes: Vec<u8>) -> Resp {
        // What encodeURIComponent does in the browser, for the characters used in these tests.
        let name = name.replace(' ', "%20");
        self.send(Method::POST, &format!("/api/upload?name={name}"), Some("application/octet-stream"), bytes, &[]).await
    }

    /// A household with one joint account ready to import into.
    async fn household(&self) -> String {
        let inst = self.ok("add_institution", json!({"name": "Desjardins", "bankId": "815"})).await;
        let acct = self
            .ok("add_account", json!({"input": {"institutionId": inst["id"], "name": "Joint chequing", "kind": "checking",
                    "currency": "CAD", "number": "EOP-12345", "ownerIds": []}}))
            .await;
        acct.as_str().unwrap().to_string()
    }
}

// ---- basics -------------------------------------------------------------------------

#[tokio::test]
async fn health_is_public_and_responses_carry_security_headers() {
    let e = env();
    let r = e.get("/healthz").await;
    assert_eq!((r.status, r.body.as_slice()), (StatusCode::OK, b"ok".as_slice()));
    let csp = r.headers.get("content-security-policy").unwrap().to_str().unwrap();
    assert!(csp.contains("default-src 'self'") && csp.contains("frame-ancestors 'none'"));
    assert_eq!(r.headers.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(r.headers.get("x-frame-options").unwrap(), "DENY");
    assert_eq!(r.headers.get("referrer-policy").unwrap(), "no-referrer");
}

#[tokio::test]
async fn serves_the_ui_and_404s_unknown_files() {
    let e = env();
    let r = e.get("/").await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(String::from_utf8_lossy(&r.body).contains("<title>Ledgerhold</title>"));
    assert_eq!(e.get("/missing.js").await.status, StatusCode::NOT_FOUND);
    assert_eq!(e.get("/../Cargo.toml").await.status, StatusCode::NOT_FOUND);
}

// ---- authentication -----------------------------------------------------------------

#[tokio::test]
async fn protected_endpoints_require_a_session() {
    let e = env();
    assert_eq!(e.rpc("overview", json!({})).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(e.upload("a.csv", b"x".to_vec()).await.status, StatusCode::UNAUTHORIZED);
    let export = e.send(Method::POST, "/api/export", Some("application/json"), b"{}".to_vec(), &[]).await;
    assert_eq!(export.status, StatusCode::UNAUTHORIZED);
    assert_eq!(e.get("/api/download/x/y.ofx").await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_sets_a_strict_http_only_cookie_that_unlocks_the_api() {
    let mut e = env();
    e.login().await;
    let r = e.send(Method::POST, "/api/login", Some("application/json"), json!({"password": PASSWORD}).to_string().into_bytes(), &[]).await;
    let cookie = r.headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict") && cookie.contains("Path=/"));
    assert!(!cookie.contains("Secure"), "plain http: no Secure flag");
    assert_eq!(e.rpc("overview", json!({})).await.status, StatusCode::OK);
}

#[tokio::test]
async fn cookie_is_marked_secure_behind_an_https_proxy() {
    let e = env();
    let r = e
        .send(Method::POST, "/api/login", Some("application/json"), json!({"password": PASSWORD}).to_string().into_bytes(), &[("x-forwarded-proto", "https")])
        .await;
    assert!(r.headers.get(header::SET_COOKIE).unwrap().to_str().unwrap().contains("; Secure"));
}

#[tokio::test]
async fn wrong_password_and_forged_cookies_are_refused() {
    let mut e = env();
    let bad = e.send(Method::POST, "/api/login", Some("application/json"), json!({"password": "nope"}).to_string().into_bytes(), &[]).await;
    assert_eq!(bad.status, StatusCode::UNAUTHORIZED);
    assert!(bad.headers.get(header::SET_COOKIE).is_none());
    e.cookie = Some("lh_session=9999999999.deadbeef".into());
    assert_eq!(e.rpc("overview", json!({})).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test(start_paused = true)]
async fn repeated_bad_logins_are_throttled() {
    let e = env();
    let attempt = |pw: &'static str| {
        e.send(Method::POST, "/api/login", Some("application/json"), json!({ "password": pw }).to_string().into_bytes(), &[])
    };
    for _ in 0..10 {
        assert_eq!(attempt("wrong").await.status, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(attempt("wrong").await.status, StatusCode::TOO_MANY_REQUESTS);
    // Even the right password waits out the window.
    assert_eq!(attempt(PASSWORD).await.status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn session_endpoint_reports_state_and_logout_clears_the_cookie() {
    let mut e = env();
    let before = e.get("/api/session").await.json();
    assert_eq!((before["authRequired"].clone(), before["authenticated"].clone()), (json!(true), json!(false)));
    e.login().await;
    assert_eq!(e.get("/api/session").await.json()["authenticated"], json!(true));
    let out = e.send(Method::POST, "/api/logout", Some("application/json"), vec![], &[]).await;
    assert!(out.headers.get(header::SET_COOKIE).unwrap().to_str().unwrap().contains("Max-Age=0"));
}

#[tokio::test]
async fn without_a_password_the_api_is_open() {
    let e = env_with(None, |_| {});
    let s = e.get("/api/session").await.json();
    assert_eq!((s["authRequired"].clone(), s["authenticated"].clone()), (json!(false), json!(true)));
    assert_eq!(e.rpc("overview", json!({})).await.status, StatusCode::OK);
}

#[test]
fn config_refuses_to_expose_an_unprotected_server() {
    let dir = PathBuf::from("/tmp/x");
    let mut c = config(&dir, None);
    assert!(c.validate().is_ok(), "loopback without a password is fine");
    c.listen = "0.0.0.0:3000".parse().unwrap();
    let msg = c.validate().unwrap_err().to_string();
    assert!(msg.contains("LEDGERHOLD_PASSWORD"), "{msg}");
    c.allow_no_auth = true;
    assert!(c.validate().is_ok());
    let mut c = config(&dir, Some("short"));
    assert!(c.validate().unwrap_err().to_string().contains("at least 8"));
    c.password = Some("long enough".into());
    c.listen = "0.0.0.0:3000".parse().unwrap();
    assert!(c.validate().is_ok());
}

// ---- commands -----------------------------------------------------------------------

#[tokio::test]
async fn command_errors_map_to_http_statuses() {
    let mut e = env();
    e.login().await;
    let unknown = e.rpc("nope", json!({})).await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    assert!(unknown.error().contains("unknown command"));
    assert_eq!(e.rpc("account_detail", json!({})).await.status, StatusCode::BAD_REQUEST);
    let bad_json = e.send(Method::POST, "/api/rpc/overview", Some("application/json"), b"{not json".to_vec(), &[]).await;
    assert_eq!(bad_json.status, StatusCode::BAD_REQUEST);
    let failed = e.rpc("account_detail", json!({"accountId": "missing"})).await;
    assert_eq!(failed.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(failed.error().contains("not found"), "{}", failed.error());
    assert_eq!(e.get("/api/rpc/overview").await.status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn an_empty_body_means_no_arguments() {
    let mut e = env();
    e.login().await;
    let r = e.send(Method::POST, "/api/rpc/overview", None, vec![], &[]).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(r.json()["accounts"].is_array());
}

// ---- upload -> import -----------------------------------------------------------------

#[tokio::test]
async fn upload_preview_import_roundtrip_and_the_upload_is_consumed() {
    let mut e = env();
    e.login().await;
    let acct = e.household().await;

    let up = e.upload("releve desjardins.csv", fixture("synthetic/desjardins.csv")).await;
    assert_eq!(up.status, StatusCode::OK);
    let token = up.json()["path"].as_str().unwrap().to_string();
    assert!(token.starts_with("upload:") && token.ends_with("releve desjardins.csv"));

    let p = e.ok("preview_import", json!({"accountId": acct, "path": token, "parser": "desjardins"})).await;
    assert_eq!((p["newCount"].clone(), p["duplicateCount"].clone()), (json!(10), json!(0)));
    assert_eq!(p["fileName"], "releve desjardins.csv");
    let done = e.ok("commit_import", json!({"accountId": acct, "path": token, "parser": "desjardins"})).await;
    assert_eq!(done["newCount"], 10);

    // The temporary copy is gone after commit.
    let again = e.rpc("preview_import", json!({"accountId": acct, "path": token, "parser": "desjardins"})).await;
    assert_eq!(again.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(again.error().contains("upload it again"), "{}", again.error());

    // Uploading the same content again is recognised as already imported.
    let up2 = e.upload("again.csv", fixture("synthetic/desjardins.csv")).await.json();
    let p2 = e.ok("preview_import", json!({"accountId": acct, "path": up2["path"], "parser": "desjardins"})).await;
    assert_eq!((p2["newCount"].clone(), p2["duplicateCount"].clone(), p2["fileAlreadyImported"].clone()), (json!(0), json!(10), json!(true)));
}

#[tokio::test]
async fn clients_cannot_make_the_server_read_arbitrary_files() {
    let mut e = env();
    e.login().await;
    let acct = e.household().await;
    for evil in ["/etc/passwd", "../../../etc/passwd", "upload:../../etc/passwd", "file:///etc/passwd", "~/secret.csv"] {
        let r = e.rpc("preview_import", json!({"accountId": acct, "path": evil, "parser": "ofx"})).await;
        assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY, "{evil}");
    }
}

#[tokio::test]
async fn upload_rejects_empty_bodies_and_oversized_files() {
    let mut e = env();
    e.login().await;
    assert_eq!(e.upload("a.csv", vec![]).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(e.upload("a.csv", vec![b'x'; 26 * 1024 * 1024]).await.status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn upload_names_are_sanitised() {
    let mut e = env();
    e.login().await;
    let r = e.upload("..%2F..%2Fetc%2Fevil.csv", b"x".to_vec()).await.json();
    // The server decodes %2F to '/', so this arrives as "../../etc/evil.csv": only the base name survives.
    assert_eq!(r["fileName"], "evil.csv");
    assert!(r["path"].as_str().unwrap().ends_with("/evil.csv"));
}

// ---- export -> download -----------------------------------------------------------------

async fn exported(e: &Env) -> (String, String) {
    let acct = e.household().await;
    let up = e.upload("d.csv", fixture("synthetic/desjardins.csv")).await.json();
    e.ok("commit_import", json!({"accountId": acct, "path": up["path"], "parser": "desjardins"})).await;
    e.ok("set_anchor", json!({"accountId": acct, "date": "2026-01-01", "amount": 70000})).await;
    (acct, "6f1c1f3e-0b0a-4c63-9d1e-1a2b3c4d5e6f".to_string())
}

#[tokio::test]
async fn export_then_download_a_single_file_and_a_zip() {
    let mut e = env();
    e.login().await;
    let (acct, session) = exported(&e).await;
    let body = json!({"accountIds": [acct], "start": "2026-01-01", "end": "2026-01-31", "session": session});
    let r = e.send(Method::POST, "/api/export", Some("application/json"), body.to_string().into_bytes(), &[]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.error());
    let results = r.json();
    assert_eq!(results[0]["txnCount"], 10);
    assert_eq!(results[0]["balance"], 196145);
    let path = results[0]["path"].as_str().unwrap();
    assert!(path.starts_with(&session) && path.ends_with("joint-chequing-20260101-20260131.ofx"), "{path}");
    assert!(!path.contains("/tmp") && !path.contains("srv-"), "server paths are never revealed: {path}");

    let file = e.get(&format!("/api/download/{path}")).await;
    assert_eq!(file.status, StatusCode::OK);
    assert!(file.headers.get(header::CONTENT_DISPOSITION).unwrap().to_str().unwrap().contains("attachment; filename=\"joint-chequing-"));
    let text = String::from_utf8(file.body).unwrap();
    assert!(text.contains("<BANKID>815") && text.contains("<LEDGERBAL>") && text.contains("<STMTTRN>"));

    // A second export into the same session accumulates into the zip.
    let body2 = json!({"accountIds": [acct], "start": "2026-02-01", "end": "2026-02-28", "session": session});
    e.send(Method::POST, "/api/export", Some("application/json"), body2.to_string().into_bytes(), &[]).await;
    let zip = e.get(&format!("/api/download/{session}/all.zip")).await;
    assert_eq!(zip.status, StatusCode::OK);
    assert_eq!(&zip.body[..2], b"PK");
    assert_eq!(zip.headers.get(header::CONTENT_TYPE).unwrap(), "application/zip");
}

#[tokio::test]
async fn export_validates_input_and_downloads_cannot_traverse() {
    let mut e = env();
    e.login().await;
    let (acct, session) = exported(&e).await;
    let post = |body: Value| {
        let e = &e;
        async move { e.send(Method::POST, "/api/export", Some("application/json"), body.to_string().into_bytes(), &[]).await }
    };
    assert_eq!(post(json!({"accountIds": [], "start": "2026-01-01", "end": "2026-01-31", "session": session})).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(post(json!({"accountIds": [acct], "start": "2026-02-01", "end": "2026-01-01", "session": session})).await.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(post(json!({"accountIds": [acct], "start": "2026-01-01", "end": "2026-01-31", "session": "../../etc"})).await.status, StatusCode::UNPROCESSABLE_ENTITY);

    post(json!({"accountIds": [acct], "start": "2026-01-01", "end": "2026-01-31", "session": session})).await;
    for evil in [format!("/api/download/{session}/..%2F..%2Fsession.key"), "/api/download/..%2F..%2F/x.ofx".to_string(), "/api/download/not-a-uuid/x.ofx".to_string(), format!("/api/download/{session}/missing.ofx")] {
        assert_eq!(e.get(&evil).await.status, StatusCode::NOT_FOUND, "{evil}");
    }
}

// ---- cross-site protection ------------------------------------------------------------

#[tokio::test]
async fn cross_site_writes_are_refused_even_with_a_valid_cookie() {
    let mut e = env();
    e.login().await;
    let from = |origin: &'static str| {
        let e = &e;
        async move { e.send(Method::POST, "/api/rpc/overview", Some("application/json"), b"{}".to_vec(), &[("origin", origin)]).await.status }
    };
    assert_eq!(from("http://evil.example").await, StatusCode::FORBIDDEN);
    assert_eq!(from("https://ledger.local.evil.example").await, StatusCode::FORBIDDEN);
    assert_eq!(from("http://ledger.local").await, StatusCode::OK, "same origin");
    assert_eq!(from("https://ledger.local").await, StatusCode::OK, "same host behind an https proxy");
    // Reads without Origin (curl, health probes) still work.
    assert_eq!(e.get("/healthz").await.status, StatusCode::OK);
}

#[tokio::test]
async fn configured_origins_are_allowed() {
    let mut e = env_with(Some(PASSWORD), |c| c.allowed_origins = vec!["https://ledger.example.com".into()]);
    e.login().await;
    let r = e.send(Method::POST, "/api/rpc/overview", Some("application/json"), b"{}".to_vec(), &[("origin", "https://ledger.example.com")]).await;
    assert_eq!(r.status, StatusCode::OK);
}

#[tokio::test]
async fn the_session_key_persists_so_restarts_keep_you_signed_in() {
    let mut e = env();
    e.login().await;
    let cfg = config(&e.dir, Some(PASSWORD));
    assert!(e.dir.join("data/session.key").is_file());
    let restarted = router(AppState::open(&cfg).unwrap());
    let req = Request::builder().method(Method::POST).uri("/api/rpc/overview").header(header::HOST, "ledger.local")
        .header(header::COOKIE, e.cookie.clone().unwrap()).body(Body::empty()).unwrap();
    assert_eq!(restarted.oneshot(req).await.unwrap().status(), StatusCode::OK);
}
