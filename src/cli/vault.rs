use crate::core::{biometry::BiometryGate, crypto::get_or_create_master_key, store::Store};
use anyhow::{anyhow, Result};
use std::path::PathBuf;

pub fn vault(sub: &str, id: Option<i64>) -> Result<()> {
    let home = std::env::var("HOME")?;
    let db = PathBuf::from(&home)
        .join("Library/Application Support/clipboard-history-mcp/history.db");
    let key = get_or_create_master_key()?;
    let store = Store::open(&db, key)?;

    match sub {
        "list" => {
            let items = store.list(500)?;
            let secrets: Vec<_> = items
                .into_iter()
                .filter(|i| i.primary_kind.starts_with("secret:"))
                .collect();
            println!("{}", serde_json::json!({"count": secrets.len(), "items": secrets}));
        }
        "unlock" => {
            let id = id.ok_or_else(|| anyhow!("Usage: vault unlock <id>"))?;
            // Gate behind Touch ID before revealing the secret.
            let gate = BiometryGate::new();
            gate.evaluate(&format!("Reveal stored secret #{}", id))?;
            println!("{}", store.unlock_secret(id)?);
        }
        other => return Err(anyhow!("unknown vault subcommand: {}", other)),
    }
    Ok(())
}
