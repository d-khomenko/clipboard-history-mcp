use anyhow::{anyhow, Result};
use super::store_helper;

/// Clear history by scope.
/// Scope: `all` | `older-than-days:N` | `kind:K`.
pub fn run(scope: &str, yes: bool) -> Result<()> {
    if !yes {
        eprint!("Clear {}? [y/N]: ", scope);
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        let answer = line.trim().to_ascii_lowercase();
        if answer != "y" && answer != "yes" {
            eprintln!("Aborted.");
            std::process::exit(1);
        }
    }

    let store = store_helper::open()?;

    let removed = if scope == "all" {
        store.clear_all()?
    } else if let Some(rest) = scope.strip_prefix("older-than-days:") {
        let days: i64 = rest
            .parse()
            .map_err(|_| anyhow!("invalid days value: '{}'", rest))?;
        store.clear_older_than_days(days)?
    } else if let Some(rest) = scope.strip_prefix("kind:") {
        store.clear_kind(rest)?
    } else {
        return Err(anyhow!(
            "unknown scope '{}'. Use: all | older-than-days:N | kind:K",
            scope
        ));
    };

    println!("{}", serde_json::json!({ "ok": true, "scope": scope, "removed": removed }));
    Ok(())
}
