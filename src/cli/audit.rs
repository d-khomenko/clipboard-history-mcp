use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct AuditArgs {
    /// Maximum number of entries to return.
    #[arg(long, default_value_t = 50)]
    pub limit: i64,
    /// Only return entries from the last N seconds.
    #[arg(long, value_name = "SECONDS")]
    pub since: Option<i64>,
}

pub fn run(args: AuditArgs) -> Result<()> {
    // TODO(post-merge): replace with cli::store_helper::open() once Track A's helper lands.
    use crate::core::crypto::get_or_create_master_key;
    use crate::core::paths::db_path;
    use crate::core::store::Store;

    // The audit subcommand does not decrypt secrets — it only reads audit_log rows.
    // We still need a master key to open the Store (the API requires it), but the
    // zero key is safe here because we never call unlock_secret from this path.
    let key = if std::env::var("CLIPBOARD_NEVER_STORE_SECRETS").as_deref() == Ok("1") {
        [0u8; 32]
    } else {
        // Try the cached key first; fall back to prompt if needed.
        get_or_create_master_key(|| {
            crate::core::master_password::prompt_password("Master password (5-min cache): ")
        })?
    };

    let store = Store::open(db_path(), key)?;
    let entries = store.audit_recent(args.limit, args.since)?;
    let json = serde_json::json!({ "count": entries.len(), "entries": entries });
    println!("{}", json);
    Ok(())
}
