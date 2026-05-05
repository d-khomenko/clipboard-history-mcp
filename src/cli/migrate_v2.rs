use anyhow::Result;
use std::path::PathBuf;

pub fn migrate_v2() -> Result<()> {
    let home = std::env::var("HOME")?;
    let v2_db = PathBuf::from(&home)
        .join("Library/Application Support/clipboard-history-mcp/history.db");
    if !v2_db.exists() {
        println!(
            "No v2 DB found at {} — nothing to migrate.",
            v2_db.display()
        );
        return Ok(());
    }
    println!(
        "v2 DB at {} — schema is shared with v3, no migration needed.",
        v2_db.display()
    );
    println!("Run `clipboard-history-mcp status` to see existing rows.");
    Ok(())
}
