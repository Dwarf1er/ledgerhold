use anyhow::Result;
use ledgerhold_server::{AppState, Config, reset_password, router, spawn_sweeper};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const HELP: &str = "ledgerhold-server: self-hosted Ledgerhold

Configuration is read from environment variables:
  LEDGERHOLD_PASSWORD         optional: set the household password here instead of in the web UI
  LEDGERHOLD_LISTEN           address to listen on            [127.0.0.1:3000]
  LEDGERHOLD_DB               SQLite database file            [data/ledgerhold.sqlite]
  LEDGERHOLD_STATIC           built web UI directory          [ui/dist]
  LEDGERHOLD_SECRET           session signing key (default: generated and stored beside the database)
  LEDGERHOLD_ALLOWED_ORIGINS  extra comma-separated origins allowed to call the API
  LEDGERHOLD_ALLOW_NO_AUTH    set to 1 to serve with no password at all (something else must protect it)

First run: with no password configured on a network address the server prints a one-time setup
code in its log; open the web UI and enter it to choose the household password.

Commands:
  reset-password  forget the stored password; the next start offers first-run setup again

Options:
  --healthcheck   probe a running server and exit 0 if it is healthy
  --version, --help";

/// Tiny dependency-free HTTP probe so a container needs no curl/wget.
fn healthcheck() -> Result<()> {
    let cfg = Config::from_env()?;
    let addr = std::net::SocketAddr::new(
        if cfg.listen.ip().is_unspecified() { std::net::Ipv4Addr::LOCALHOST.into() } else { cfg.listen.ip() },
        cfg.listen.port(),
    );
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;
    s.set_read_timeout(Some(Duration::from_secs(3)))?;
    s.write_all(format!("GET /healthz HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n").as_bytes())?;
    let mut out = String::new();
    s.read_to_string(&mut out)?;
    if out.starts_with("HTTP/1.1 200") {
        Ok(())
    } else {
        anyhow::bail!("unexpected response: {}", out.lines().next().unwrap_or(""))
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
}

#[tokio::main]
async fn main() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("--help" | "-h") => {
            println!("{HELP}");
            return Ok(());
        }
        Some("--version" | "-V") => {
            println!("ledgerhold-server {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--healthcheck") => {
            return healthcheck().map_err(|e| {
                eprintln!("unhealthy: {e:#}");
                e
            });
        }
        Some("reset-password") => {
            let cfg = Config::from_env()?;
            if cfg.password.is_some() {
                println!("The password comes from LEDGERHOLD_PASSWORD, so there is nothing stored to reset. Change that variable instead.");
            } else if reset_password(&cfg)? {
                println!("Password forgotten. Restart the server, then open it in a browser to set a new one.");
            } else {
                println!("No password was stored in {}.", cfg.data_dir().display());
            }
            return Ok(());
        }
        Some(other) => anyhow::bail!("unknown argument {other:?}; try --help"),
        None => {}
    }

    let cfg = Config::from_env()?;
    let state = AppState::open(&cfg)?;
    spawn_sweeper(state.clone());
    if !cfg.static_dir.join("index.html").is_file() {
        eprintln!("warning: no web UI found at {} (set LEDGERHOLD_STATIC); the API will still work", cfg.static_dir.display());
    }
    let listener = tokio::net::TcpListener::bind(cfg.listen).await?;
    println!(
        "ledgerhold-server {} listening on http://{} (database: {}, password: {})",
        env!("CARGO_PKG_VERSION"),
        cfg.listen,
        cfg.db_path.display(),
        state.auth_summary()
    );
    if let Some(code) = state.setup_code() {
        println!();
        println!("  First run: open Ledgerhold in your browser and choose a household password.");
        println!("  Setup code: {code}");
        println!("  (It changes on every restart and stops working once the password is set.)");
        println!();
    }
    axum::serve(listener, router(state)).with_graceful_shutdown(shutdown_signal()).await?;
    Ok(())
}
