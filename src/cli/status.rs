use anyhow::Result;
use serde_json::json;
use std::path::PathBuf;

pub fn status() -> Result<()> {
    let home = std::env::var("HOME")?;
    let data_dir =
        PathBuf::from(&home).join("Library/Application Support/clipboard-history-mcp");
    let pid_file = data_dir.join("daemon.pid");
    let db = data_dir.join("history.db");

    let daemon = if pid_file.exists() {
        let pid = std::fs::read_to_string(&pid_file)?
            .trim()
            .parse::<i32>()
            .ok();
        match pid {
            Some(p) if unsafe { libc::kill(p, 0) } == 0 => {
                json!({"running": true, "pid": p})
            }
            Some(p) => json!({"running": false, "stalePid": p}),
            None => json!({"running": false}),
        }
    } else {
        json!({"running": false})
    };

    let db_size = std::fs::metadata(&db).ok().map(|m| m.len());
    println!(
        "{}",
        json!({
            "dataDir": data_dir,
            "dbPath": db,
            "dbExists": db.exists(),
            "dbSizeBytes": db_size,
            "daemon": daemon,
        })
    );
    Ok(())
}
