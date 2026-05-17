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
    let store = crate::cli::store_helper::open()?;
    let entries = store.audit_recent(args.limit, args.since)?;
    let json = serde_json::json!({ "count": entries.len(), "entries": entries });
    println!("{}", json);
    Ok(())
}
