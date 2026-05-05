pub mod tools;

use crate::core::{biometry::BiometryGate, crypto::get_or_create_master_key, store::Store};
use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};
use std::path::PathBuf;
use std::sync::Arc;

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

pub async fn run_server() -> Result<()> {
    let key = get_or_create_master_key()?;
    let dbp = db_path();
    std::fs::create_dir_all(dbp.parent().unwrap_or(&dbp))?;
    let store = Arc::new(Store::open(&dbp, key)?);
    let biometry = Arc::new(BiometryGate::new());
    let server = tools::ClipboardServer { store, biometry, db_path: dbp };

    // rusqlite::Connection is !Sync, so ClipboardServer is !Send.
    // The `local` rmcp feature uses spawn_local instead of spawn, which only
    // works inside a tokio LocalSet. We create one here so the entire MCP
    // session runs on a single thread.
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let running = server.serve(stdio()).await?;
            running.waiting().await?;
            Ok::<_, anyhow::Error>(())
        })
        .await?;
    Ok(())
}
