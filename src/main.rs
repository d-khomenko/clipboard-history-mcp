use anyhow::Result;
use clap::Parser;
use clipboard_history_mcp::cli::{Cli, Cmd};
use clipboard_history_mcp::core::crypto::get_or_create_master_key;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

fn data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("CLIPBOARD_DATA_DIR") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join("Library/Application Support/clipboard-history-mcp")
}

fn db_path() -> PathBuf {
    if let Ok(p) = std::env::var("CLIPBOARD_DB_PATH") {
        return PathBuf::from(p);
    }
    data_dir().join("history.db")
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    match cli.command {
        Cmd::Daemon => run_daemon().await,
        Cmd::Serve => clipboard_history_mcp::mcp::run_server().await,
        Cmd::Install { window_titles } => {
            clipboard_history_mcp::cli::install::install(window_titles)
        }
        Cmd::Uninstall { keep_data } => {
            clipboard_history_mcp::cli::uninstall::uninstall(keep_data)
        }
        Cmd::Status => clipboard_history_mcp::cli::status::status(),
        Cmd::Doctor => clipboard_history_mcp::cli::doctor::doctor(),
        Cmd::Vault { sub, id } => clipboard_history_mcp::cli::vault::vault(&sub, id),
        Cmd::MigrateV2 => clipboard_history_mcp::cli::migrate_v2::migrate_v2(),
    }
}

async fn run_daemon() -> Result<()> {
    use clipboard_history_mcp::daemon::watcher::{run_watcher, WatcherOptions};

    let key = get_or_create_master_key()?;
    let path = db_path();
    let stop = Arc::new(AtomicBool::new(false));

    let pid_file = data_dir().join("daemon.pid");
    std::fs::create_dir_all(data_dir())?;
    std::fs::write(&pid_file, std::process::id().to_string())?;

    let stop_clone = stop.clone();
    ctrlc::set_handler(move || stop_clone.store(true, Ordering::Relaxed))?;

    let opts = WatcherOptions {
        poll_ms: std::env::var("CLIPBOARD_POLL_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1500),
        capture_window_title: std::env::var("CLIPBOARD_CAPTURE_WINDOW_TITLE").as_deref()
            == Ok("1"),
        ignore_apps: std::env::var("CLIPBOARD_IGNORE_APPS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        never_store_secrets: std::env::var("CLIPBOARD_NEVER_STORE_SECRETS").as_deref()
            == Ok("1"),
        max_items: std::env::var("CLIPBOARD_HISTORY_MAX")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1000),
    };

    tracing::info!(
        "daemon started pid={} db={:?}",
        std::process::id(),
        db_path()
    );

    // Watcher runs on its own std::thread because NSPasteboard is !Send.
    // It opens its own Store (rusqlite::Connection is also !Send).
    let stop_for_watcher = stop.clone();
    let path_clone = path.clone();
    std::thread::spawn(move || run_watcher(path_clone, key, opts, stop_for_watcher));

    while !stop.load(Ordering::Relaxed) {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    let _ = std::fs::remove_file(&pid_file);
    Ok(())
}
