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

#[cfg(test)]
mod tests {
    use super::*;
    /// `status()` resolves paths via `CLIPBOARD_DATA_DIR`. Use the shared
    /// `test_util::ENV_LOCK` so we serialise against every other module's
    /// env-touching tests.
    use crate::test_util::ENV_LOCK;

    #[test]
    fn status_no_daemon_no_db_returns_ok() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::TempDir::new().unwrap();
        std::env::set_var("CLIPBOARD_DATA_DIR", tmp.path());

        // Fresh tempdir → no pid file, no db. `status()` must still succeed
        // (prints `{"running": false}` daemon + `dbExists: false`).
        let r = status();

        std::env::remove_var("CLIPBOARD_DATA_DIR");
        assert!(r.is_ok(), "status() returned Err: {:?}", r);
    }

    #[test]
    fn status_stale_pid_returns_ok() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::TempDir::new().unwrap();
        std::env::set_var("CLIPBOARD_DATA_DIR", tmp.path());

        // Write a pid that almost certainly isn't alive. `i32::MAX` is well
        // beyond what any kernel hands out — `kill(p, 0)` will return -1 and
        // the daemon branch becomes `{"running": false, "stalePid": p}`.
        let pid_file = tmp.path().join("daemon.pid");
        std::fs::write(&pid_file, format!("{}", i32::MAX)).unwrap();

        let r = status();

        std::env::remove_var("CLIPBOARD_DATA_DIR");
        assert!(r.is_ok(), "status() with stale pid returned Err: {:?}", r);
    }

    #[test]
    fn status_garbage_pid_file_returns_ok() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::TempDir::new().unwrap();
        std::env::set_var("CLIPBOARD_DATA_DIR", tmp.path());

        // Non-numeric pid → `.parse::<i32>().ok()` is None → daemon branch
        // is `{"running": false}` without panicking.
        let pid_file = tmp.path().join("daemon.pid");
        std::fs::write(&pid_file, b"not-a-pid").unwrap();

        let r = status();

        std::env::remove_var("CLIPBOARD_DATA_DIR");
        assert!(r.is_ok(), "status() with garbage pid returned Err: {:?}", r);
    }

    #[test]
    fn status_with_empty_db_file_returns_ok() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::TempDir::new().unwrap();
        std::env::set_var("CLIPBOARD_DATA_DIR", tmp.path());

        let db = tmp.path().join("history.db");
        std::fs::write(&db, b"").unwrap();

        let r = status();

        std::env::remove_var("CLIPBOARD_DATA_DIR");
        assert!(r.is_ok());
        assert!(db.exists());
    }
}
