//! Ledgerhold web server: serves the UI, a JSON command API, uploads and downloads.
//! All behaviour lives in `ledgerhold-app`; this crate adds HTTP, auth and temp files.

pub mod auth;
pub mod files;

use anyhow::{Context, Result};
use auth::Auth;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{Local, NaiveDate};
use files::{Files, ZIP_NAME};
use ledgerhold_app::rpc::{self, RpcContext, RpcError};
use ledgerhold_store::Store;
use serde::Deserialize;
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::path::{Path as FsPath, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tower_http::services::ServeDir;

const MAX_UPLOAD: usize = 25 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Config {
    pub db_path: PathBuf,
    pub listen: SocketAddr,
    pub password: Option<String>,
    pub secret: Option<String>,
    pub static_dir: PathBuf,
    pub allow_no_auth: bool,
    /// Extra origins allowed to call the API (e.g. a reverse proxy that rewrites Host).
    pub allowed_origins: Vec<String>,
}

impl Config {
    /// Read `LEDGERHOLD_*` environment variables.
    pub fn from_env() -> Result<Config> {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let listen = var("LEDGERHOLD_LISTEN").unwrap_or_else(|| "127.0.0.1:3000".into());
        Ok(Config {
            db_path: var("LEDGERHOLD_DB").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("data/ledgerhold.sqlite")),
            listen: listen.parse().with_context(|| format!("LEDGERHOLD_LISTEN={listen:?} is not host:port, e.g. 0.0.0.0:3000"))?,
            password: var("LEDGERHOLD_PASSWORD"),
            secret: var("LEDGERHOLD_SECRET"),
            static_dir: var("LEDGERHOLD_STATIC").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("ui/dist")),
            allow_no_auth: var("LEDGERHOLD_ALLOW_NO_AUTH").is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true")),
            allowed_origins: var("LEDGERHOLD_ALLOWED_ORIGINS")
                .map(|v| v.split(',').map(|s| s.trim().trim_end_matches('/').to_string()).filter(|s| !s.is_empty()).collect())
                .unwrap_or_default(),
        })
    }

    pub fn validate(&self) -> Result<()> {
        if let Some(p) = &self.password {
            auth::validate_password(p).map_err(|m| anyhow::anyhow!("LEDGERHOLD_PASSWORD: {m}"))?;
        }
        Ok(())
    }

    /// The directory holding the database, the session key and the password hash.
    pub fn data_dir(&self) -> PathBuf {
        self.db_path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(FsPath::new(".")).to_path_buf()
    }
}

/// Forget the stored password so the next start offers first-run setup again.
/// Returns whether there was one to forget.
pub fn reset_password(cfg: &Config) -> Result<bool> {
    let path = auth::hash_path(&cfg.data_dir());
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e).with_context(|| format!("removing {}", path.display())),
    }
}

pub struct AppState {
    store: Mutex<Store>,
    auth: Auth,
    files: Files,
    static_dir: PathBuf,
    allowed_origins: Vec<String>,
    /// Open on loopback: only answer requests addressed to a local name (defeats DNS rebinding).
    local_hosts_only: bool,
    /// Argon2 is memory-hungry; never run more than a couple of hashes at once.
    hash_slots: tokio::sync::Semaphore,
}

/// Is `host` (a `Host` header, with optional port) a name that means "this machine"?
fn is_local_host(host: &str) -> bool {
    let name = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        host.rsplit_once(':').map_or(host, |(h, _)| h)
    };
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    name == "localhost" || name.ends_with(".localhost") || name == "::1" || name.parse::<std::net::Ipv4Addr>().is_ok_and(|ip| ip.is_loopback())
}

fn host_of(origin: &str) -> &str {
    origin.strip_prefix("http://").or_else(|| origin.strip_prefix("https://")).unwrap_or(origin).trim_end_matches('/')
}

impl AppState {
    pub fn open(cfg: &Config) -> Result<Arc<AppState>> {
        cfg.validate()?;
        let data_dir = cfg.data_dir();
        ledgerhold_store::create_private_dir(&data_dir).with_context(|| format!("creating {}", data_dir.display()))?;
        let store = Store::open(&cfg.db_path).map_err(|e| anyhow::anyhow!("opening {}: {e}", cfg.db_path.display()))?;
        let key = match &cfg.secret {
            Some(s) => s.as_bytes().to_vec(),
            None => load_or_create_key(&data_dir.join("session.key"))?,
        };
        let auth = choose_auth(cfg, &data_dir, key)?;
        Ok(Arc::new(AppState {
            store: Mutex::new(store),
            local_hosts_only: auth.mode() == auth::Mode::Open && !cfg.allow_no_auth,
            hash_slots: tokio::sync::Semaphore::new(2),
            auth,
            files: Files::new(&data_dir)?,
            static_dir: cfg.static_dir.clone(),
            allowed_origins: cfg.allowed_origins.clone(),
        }))
    }

    /// The one-time first-run code, while the server is waiting for its password to be chosen.
    pub fn setup_code(&self) -> Option<String> {
        self.auth.setup_code()
    }

    pub fn auth_summary(&self) -> &'static str {
        match self.auth.mode() {
            auth::Mode::Open => "none",
            auth::Mode::Setup => "not set yet (first-run setup)",
            auth::Mode::Password => "required",
        }
    }

    /// Delete stale temporary uploads/exports; call periodically.
    pub fn sweep(&self) {
        self.files.sweep(Duration::from_secs(3600));
    }
}

/// Decide how the server is protected:
/// an environment password wins, then explicit "no auth", then a stored password; with none of those
/// a loopback server is open (it's your own machine) and a networked one starts first-run setup.
fn choose_auth(cfg: &Config, data_dir: &FsPath, key: Vec<u8>) -> Result<Auth> {
    let hash_file = auth::hash_path(data_dir);
    if let Some(password) = &cfg.password {
        return Auth::with_env_password(password, key);
    }
    if cfg.allow_no_auth {
        return Ok(Auth::open(key));
    }
    match auth::load_hash(&hash_file)? {
        Some(phc) => Auth::with_stored_hash(phc, key, hash_file),
        None if cfg.listen.ip().is_loopback() => Ok(Auth::open(key)),
        None => Ok(Auth::setup(key, hash_file)),
    }
}

fn load_or_create_key(path: &FsPath) -> Result<Vec<u8>> {
    if let Ok(s) = std::fs::read_to_string(path)
        && let Ok(k) = hex::decode(s.trim())
        && k.len() >= 32
    {
        return Ok(k);
    }
    let mut key = Vec::new();
    key.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    key.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    auth::write_private(path, hex::encode(&key).as_bytes())?;
    Ok(key)
}

// ---- router ------------------------------------------------------------------------

pub fn router(state: Arc<AppState>) -> Router {
    let protected = Router::new()
        .route("/api/rpc/{command}", post(rpc_handler))
        .route("/api/upload", post(upload).layer(DefaultBodyLimit::max(MAX_UPLOAD)))
        .route("/api/password", post(change_password))
        .route("/api/export", post(export))
        .route("/api/download/{session}/{name}", get(download))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/api/session", get(session))
        .route("/api/setup", post(setup))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .merge(protected)
        .fallback_service(ServeDir::new(&state.static_dir))
        .layer(middleware::from_fn_with_state(state.clone(), check_origin))
        .layer(middleware::map_response(security_headers))
        .with_state(state)
}

async fn security_headers(mut res: Response) -> Response {
    let h = res.headers_mut();
    for (k, v) in [
        ("content-security-policy", "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; font-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("x-frame-options", "DENY"),
    ] {
        h.insert(HeaderName::from_static(k), HeaderValue::from_static(v));
    }
    if !h.contains_key(header::CACHE_CONTROL) {
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    res
}

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

fn now_unix() -> i64 {
    Local::now().timestamp()
}

fn cookie_token(headers: &HeaderMap) -> Option<&str> {
    headers.get(header::COOKIE).and_then(|v| v.to_str().ok()).and_then(auth::cookie_value)
}

fn authenticated(state: &AppState, headers: &HeaderMap) -> bool {
    !state.auth.required() || cookie_token(headers).is_some_and(|t| state.auth.verify(t, now_unix()))
}

/// Reject cross-site writes: a browser always sends `Origin` on cross-origin POSTs.
async fn check_origin(State(state): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    // With no password the only protection is "you are on this machine". A web page can point its own
    // hostname at 127.0.0.1 (DNS rebinding) and then talk to us as if it were local, so insist on a local name.
    if state.local_hosts_only {
        let host = req.headers().get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("");
        let configured = state.allowed_origins.iter().any(|o| host_of(o).eq_ignore_ascii_case(host));
        if !is_local_host(host) && !configured {
            return error(
                StatusCode::FORBIDDEN,
                "this server has no password, so it only answers to localhost; set a password or LEDGERHOLD_ALLOWED_ORIGINS",
            );
        }
    }
    if !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS)
        && let Some(origin) = req.headers().get(header::ORIGIN).and_then(|v| v.to_str().ok())
    {
        let host = req.headers().get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("");
        let same = origin
            .strip_prefix("http://")
            .or_else(|| origin.strip_prefix("https://"))
            .is_some_and(|h| h.eq_ignore_ascii_case(host));
        let allowed = state.allowed_origins.iter().any(|o| o.eq_ignore_ascii_case(origin.trim_end_matches('/')));
        if !same && !allowed {
            return error(StatusCode::FORBIDDEN, "cross-site request refused");
        }
    }
    next.run(req).await
}

async fn require_auth(State(state): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    if !authenticated(&state, req.headers()) {
        return error(StatusCode::UNAUTHORIZED, "sign in required");
    }
    next.run(req).await
}

// ---- auth endpoints -----------------------------------------------------------------

async fn session(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Json<Value> {
    Json(json!({
        "authRequired": state.auth.required(),
        "authenticated": authenticated(&state, &headers),
        "setupRequired": state.auth.setup_required(),
        "canChangePassword": state.auth.can_change_password(),
    }))
}

#[derive(Deserialize)]
struct LoginBody {
    password: String,
}

fn session_cookie(state: &AppState, headers: &HeaderMap) -> Response {
    let secure = headers.get("x-forwarded-proto").and_then(|v| v.to_str().ok()).is_some_and(|v| v.eq_ignore_ascii_case("https"));
    let cookie = format!(
        "{}={}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}{}",
        auth::COOKIE,
        state.auth.issue(now_unix()),
        auth::SESSION_SECS,
        if secure { "; Secure" } else { "" }
    );
    (StatusCode::NO_CONTENT, [(header::SET_COOKIE, cookie)]).into_response()
}

/// Argon2 takes tens of milliseconds of CPU and ~19 MiB of memory: keep it off the async threads, and only
/// run a couple at a time so a flood of unauthenticated requests queues up instead of exhausting memory.
async fn blocking<T: Send + 'static>(state: &AppState, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let _slot = state.hash_slots.acquire().await.ok()?;
    tokio::task::spawn_blocking(f).await.ok()
}

async fn wrong(state: &AppState, message: &str) -> Response {
    state.auth.record_failure();
    // Slow down guessing without blocking other requests.
    tokio::time::sleep(Duration::from_millis(600)).await;
    error(StatusCode::UNAUTHORIZED, message)
}

const THROTTLED: &str = "too many attempts; wait a minute and try again";

async fn login(State(state): State<Arc<AppState>>, headers: HeaderMap, Json(body): Json<LoginBody>) -> Response {
    if state.auth.setup_required() {
        return error(StatusCode::CONFLICT, "this server has no password yet; finish setup first");
    }
    if state.auth.throttled() {
        return error(StatusCode::TOO_MANY_REQUESTS, THROTTLED);
    }
    let s = state.clone();
    let password = body.password;
    let verdict = blocking(&state, move || (!s.auth.throttled()).then(|| s.auth.check_password(&password))).await;
    match verdict {
        Some(Some(true)) => {}
        // Waited in the queue while others failed: the lockout applies to us too.
        Some(None) => return error(StatusCode::TOO_MANY_REQUESTS, THROTTLED),
        _ => return wrong(&state, "wrong password").await,
    }
    session_cookie(&state, &headers)
}

#[derive(Deserialize)]
struct SetupBody {
    code: String,
    password: String,
}

/// First-run only: whoever holds the one-time code from the server log chooses the household password.
async fn setup(State(state): State<Arc<AppState>>, headers: HeaderMap, Json(body): Json<SetupBody>) -> Response {
    if !state.auth.setup_required() {
        return error(StatusCode::CONFLICT, "setup is already complete");
    }
    if state.auth.throttled() {
        return error(StatusCode::TOO_MANY_REQUESTS, THROTTLED);
    }
    if !state.auth.check_setup_code(&body.code) {
        return wrong(&state, "that setup code is not right; look for \"Setup code\" in the server log").await;
    }
    if let Err(m) = auth::validate_password(&body.password) {
        return error(StatusCode::BAD_REQUEST, m);
    }
    let password = body.password;
    let Some(Ok(phc)) = blocking(&state, move || auth::hash_password(&password)).await else {
        return error(StatusCode::INTERNAL_SERVER_ERROR, "could not hash the password");
    };
    match state.auth.complete_setup(&body.code, phc) {
        Ok(()) => session_cookie(&state, &headers),
        Err(auth::SetupError::BadCode) => wrong(&state, "that setup code is not right").await,
        Err(auth::SetupError::AlreadyDone) => {
            error(StatusCode::CONFLICT, "setup could not be completed (already done, or the password could not be saved; see the server log)")
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangePasswordBody {
    current: String,
    new_password: String,
}

async fn change_password(State(state): State<Arc<AppState>>, headers: HeaderMap, Json(body): Json<ChangePasswordBody>) -> Response {
    if !state.auth.can_change_password() {
        return error(StatusCode::CONFLICT, "the password is set with LEDGERHOLD_PASSWORD; change it there");
    }
    if state.auth.throttled() {
        return error(StatusCode::TOO_MANY_REQUESTS, THROTTLED);
    }
    let s = state.clone();
    let current = body.current;
    if !blocking(&state, move || s.auth.check_password(&current)).await.unwrap_or(false) {
        return wrong(&state, "the current password is not right").await;
    }
    if let Err(m) = auth::validate_password(&body.new_password) {
        return error(StatusCode::BAD_REQUEST, m);
    }
    let new_password = body.new_password;
    let Some(Ok(phc)) = blocking(&state, move || auth::hash_password(&new_password)).await else {
        return error(StatusCode::INTERNAL_SERVER_ERROR, "could not hash the password");
    };
    if let Err(e) = state.auth.change_password(phc) {
        return error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}"));
    }
    // Every other session just became invalid; keep this one signed in.
    session_cookie(&state, &headers)
}

async fn logout() -> Response {
    let cookie = format!("{}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0", auth::COOKIE);
    (StatusCode::NO_CONTENT, [(header::SET_COOKIE, cookie)]).into_response()
}

// ---- command API --------------------------------------------------------------------

async fn rpc_handler(State(state): State<Arc<AppState>>, Path(command): Path<String>, body: Bytes) -> Response {
    let args: Value = if body.iter().all(|b| b.is_ascii_whitespace()) {
        json!({})
    } else {
        match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(e) => return error(StatusCode::BAD_REQUEST, format!("invalid JSON: {e}")),
        }
    };
    let s = state.clone();
    let imported = (command == "commit_import").then(|| args.get("path").and_then(Value::as_str).map(str::to_string)).flatten();
    let result = tokio::task::spawn_blocking(move || {
        let mut store = s.store.lock().unwrap_or_else(|e| e.into_inner());
        let resolve = |p: &str| s.files.resolve_upload(p);
        let ctx = RpcContext { now: Local::now().naive_local(), data_location: "Stored on the server".into(), resolve_path: &resolve };
        rpc::call(&mut store, &command, args, &ctx)
    })
    .await;
    match result {
        Ok(Ok(v)) => {
            // The imported copy is no longer needed once the data is in the database.
            if let Some(token) = imported {
                state.files.discard_upload(&token);
            }
            Json(v).into_response()
        }
        Ok(Err(e @ RpcError::UnknownCommand(_))) => error(StatusCode::NOT_FOUND, e.message()),
        Ok(Err(e @ RpcError::BadArgs(_))) => error(StatusCode::BAD_REQUEST, e.message()),
        Ok(Err(e @ RpcError::Failed(_))) => error(StatusCode::UNPROCESSABLE_ENTITY, e.message()),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "the command crashed; see the server log"),
    }
}

#[derive(Deserialize)]
struct UploadQuery {
    name: String,
}

async fn upload(State(state): State<Arc<AppState>>, Query(q): Query<UploadQuery>, body: Bytes) -> Response {
    if body.is_empty() {
        return error(StatusCode::BAD_REQUEST, "the file is empty");
    }
    match state.files.save_upload(&q.name, &body) {
        Ok(token) => {
            let name = token.rsplit('/').next().unwrap_or("").to_string();
            Json(json!({ "path": token, "fileName": name })).into_response()
        }
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportBody {
    account_ids: Vec<String>,
    start: NaiveDate,
    end: NaiveDate,
    /// A uuid chosen by the client; repeated exports with the same session end up in one zip.
    session: String,
}

async fn export(State(state): State<Arc<AppState>>, Json(body): Json<ExportBody>) -> Response {
    if body.account_ids.is_empty() {
        return error(StatusCode::BAD_REQUEST, "choose at least one account");
    }
    let s = state.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<Vec<Value>> {
        let dir = s.files.export_dir(&body.session)?;
        let store = s.store.lock().unwrap_or_else(|e| e.into_inner());
        let results = ledgerhold_app::export_ofx(&store, &body.account_ids, body.start, body.end, &dir, Local::now().naive_utc())?;
        results
            .into_iter()
            .map(|r| {
                let file = FsPath::new(&r.path).file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
                let mut v = serde_json::to_value(&r)?;
                v["path"] = Value::String(format!("{}/{file}", body.session));
                Ok(v)
            })
            .collect()
    })
    .await;
    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => error(StatusCode::UNPROCESSABLE_ENTITY, format!("{e:#}")),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "the export crashed; see the server log"),
    }
}

async fn download(State(state): State<Arc<AppState>>, Path((session, name)): Path<(String, String)>) -> Response {
    if name == ZIP_NAME {
        return match state.files.zip_session(&session) {
            Ok(bytes) => attachment(bytes, "ledgerhold-export.zip", "application/zip"),
            Err(_) => error(StatusCode::NOT_FOUND, "export not found (it may have expired)"),
        };
    }
    match state.files.export_file(&session, &name).and_then(|p| std::fs::read(p).map_err(Into::into)) {
        Ok(bytes) => attachment(bytes, &name, "application/x-ofx"),
        Err(_) => error(StatusCode::NOT_FOUND, "export not found (it may have expired)"),
    }
}

fn attachment(bytes: Vec<u8>, name: &str, content_type: &'static str) -> Response {
    // `name` is validated to a safe character set, so it can't break out of the quoted header value.
    let disposition = format!("attachment; filename=\"{name}\"");
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(content_type)),
            (header::CONTENT_DISPOSITION, HeaderValue::from_str(&disposition).unwrap_or(HeaderValue::from_static("attachment"))),
        ],
        bytes,
    )
        .into_response()
}

/// Spawn the periodic temp-file sweeper (call once at startup).
pub fn spawn_sweeper(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(600));
        loop {
            tick.tick().await;
            state.sweep();
        }
    });
}
