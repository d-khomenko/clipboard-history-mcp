use crate::core::paths;
use anyhow::Result;

pub fn migrate_v2() -> Result<()> {
    let v2_db = paths::db_path();
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
