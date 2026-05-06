use crate::core::paths;
use anyhow::Result;
use serde_json::json;

pub fn status() -> Result<()> {
    let data_dir = paths::data_dir();
    let pid_file = paths::pid_file_path();
    let db = paths::db_path();

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
