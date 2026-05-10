pub mod tools;

use crate::core::{biometry::BiometryGate, crypto::get_or_create_master_key, paths, store::Store};
use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};
use std::rc::Rc;

pub async fn run_server() -> Result<()> {
    // Skip the master-key load when running in introspection-only mode
    // (e.g. Glama's listing checker, which boots the server inside a
    // container with no keyring/TTY and only probes the MCP `initialize`
    // request). The ephemeral [0u8; 32] key is NEVER safe for real clip
    // data — guard with a loud warning so it can't slip into production.
    let key = if std::env::var("CLIPBOARD_EPHEMERAL_KEY").as_deref() == Ok("1") {
        tracing::warn!(
            "CLIPBOARD_EPHEMERAL_KEY=1 — using ephemeral [0u8; 32] key \
             (Glama listing / introspection only; NEVER use with real clip data)"
        );
        [0u8; 32]
    } else {
        get_or_create_master_key(|| {
            crate::core::master_password::prompt_password("Master password (5-min cache): ")
        })?
    };
    let dbp = paths::db_path();
    std::fs::create_dir_all(dbp.parent().unwrap_or(&dbp))?;
    let store = Rc::new(Store::open(&dbp, key)?);
    let biometry = Rc::new(BiometryGate::new());
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
