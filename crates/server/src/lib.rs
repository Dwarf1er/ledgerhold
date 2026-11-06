//! Ledgerhold web server: serves the UI, a JSON command API, uploads and downloads.
//! All behaviour lives in `ledgerhold-app`; this crate adds HTTP, auth and temp files.

pub mod auth;
pub mod files;

use anyhow::{Context, Result, bail};
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

    /// Refuse to expose financial data on a network without a password.
    pub fn validate(&self) -> Result<()> {
        if self.password.is_none() && !self.listen.ip().is_loopback() && !self.allow_no_auth {
            bail!(
                "refusing to listen on {} without a password. Set LEDGERHOLD_PASSWORD, bind to 127.0.0.1, \
                 or set LEDGERHOLD_ALLOW_NO_AUTH=1 if something in front of Ledgerhold handles authentication",
                self.listen
            );
        }
        if self.password.as_deref().is_some_and(|p| p.chars().count() < 8) {
            bail!("LEDGERHOLD_PASSWORD must be at least 8 characters");
        }
        Ok(())
    }
}

pub struct AppState {
    store: Mutex<Store>,
    auth: Auth,
    files: Files,
    static_dir: PathBuf,
    allowed_origins: Vec<String>,
}

impl AppState {
    pub fn open(cfg: &Config) -> Result<Arc<AppState>> {
        cfg.validate()?;
        let data_dir = cfg.db_path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(FsPath::new(".")).to_path_buf();
        std::fs::create_dir_all(&data_dir).with_context(|| format!("creating {}", data_dir.display()))?;
        let store = Store::open(&cfg.db_path).map_err(|e| anyhow::anyhow!("opening {}: {e}", cfg.db_path.display()))?;
        let key = match &cfg.secret {
            Some(s) => s.as_bytes().to_vec(),
            None => load_or_create_key(&data_dir.join("session.key"))?,
        };
        Ok(Arc::new(AppState {
            store: Mutex::new(store),
            auth: Auth::new(cfg.password.as_deref(), key),
            files: Files::new(&data_dir)?,
            static_dir: cfg.static_dir.clone(),
            allowed_origins: cfg.allowed_origins.clone(),
        }))
    }

    /// Delete stale temporary uploads/exports; call periodically.
    pub fn sweep(&self) {
        self.files.sweep(Duration::from_secs(3600));
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
    std::fs::write(path, hex::encode(&key)).with_context(|| format!("writing {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(key)
}

// ---- router ------------------------------------------------------------------------

pub fn router(state: Arc<AppState>) -> Router {
    let protected = Router::new()
        .route("/api/rpc/{command}", post(rpc_handler))
        .route("/api/upload", post(upload).layer(DefaultBodyLimit::max(MAX_UPLOAD)))
        .route("/api/export", post(export))
        .route("/api/download/{session}/{name}", get(download))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/api/session", get(session))
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
    Json(json!({ "authRequired": state.auth.required(), "authenticated": authenticated(&state, &headers) }))
}

#[derive(Deserialize)]
struct LoginBody {
    password: String,
}

async fn login(State(state): State<Arc<AppState>>, headers: HeaderMap, Json(body): Json<LoginBody>) -> Response {
    if state.auth.throttled() {
        return error(StatusCode::TOO_MANY_REQUESTS, "too many attempts; wait a minute and try again");
    }
    if !state.auth.check_password(&body.password) {
        state.auth.record_failure();
        // Slow down guessing without blocking other requests.
        tokio::time::sleep(Duration::from_millis(600)).await;
        return error(StatusCode::UNAUTHORIZED, "wrong password");
    }
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
