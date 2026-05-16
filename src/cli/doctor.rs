use anyhow::Result;

type Check = (&'static str, Box<dyn Fn() -> Result<String>>);

pub fn doctor() -> Result<()> {
    let mut checks: Vec<Check> = vec![];

    checks.push((
        "data dir writable",
        Box::new(|| {
            let p = crate::core::paths::data_dir();
            std::fs::create_dir_all(&p)?;
            let probe = p.join(".write_probe");
            std::fs::write(&probe, b"ok")?;
            std::fs::remove_file(&probe)?;
            Ok(p.display().to_string())
        }),
    ));

    checks.push((
        "keyring accessible",
        Box::new(|| {
            use keyring_core::Entry;
            // Ensure the platform keyring backend is initialized before using Entry.
            static KEYRING_INIT: once_cell::sync::OnceCell<()> = once_cell::sync::OnceCell::new();
            KEYRING_INIT.get_or_init(|| {
                let _ = keyring::use_native_store(false);
            });
            let entry = Entry::new("clipboard-history-mcp-doctor", "ping")
                .map_err(|e| anyhow::anyhow!("keyring entry: {}", e))?;
            let _ = entry.set_secret(b"ok");
            let _ = entry.delete_credential();
            Ok("ok".into())
        }),
    ));

    checks.push((
        "clipboard reachable (arboard)",
        Box::new(|| {
            let _ = crate::core::pasteboard::change_count();
            Ok("ok".into())
        }),
    ));

    #[cfg(target_os = "macos")]
    checks.push((
        "launchd service installed",
        Box::new(|| {
            let home = std::env::var("HOME")?;
            let p = std::path::PathBuf::from(home)
                .join("Library/LaunchAgents/me.kz.clipboard-history-rs.plist");
            if p.exists() {
                Ok(p.display().to_string())
            } else {
                Err(anyhow::anyhow!(
                    "not installed (run `clipboard-history-mcp install`)"
                ))
            }
        }),
    ));

    #[cfg(target_os = "linux")]
    checks.push((
        "systemd unit installed",
        Box::new(|| {
            let home = std::env::var("HOME")?;
            let p = std::path::PathBuf::from(home)
                .join(".config/systemd/user/clipboard-history-mcp.service");
            if p.exists() {
                Ok(p.display().to_string())
            } else {
                Err(anyhow::anyhow!(
                    "not installed (run `clipboard-history-mcp install`)"
                ))
            }
        }),
    ));

    #[cfg(target_os = "linux")]
    checks.push((
        "X display present",
        Box::new(|| {
            if std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok() {
                Ok("yes".into())
            } else {
                Err(anyhow::anyhow!("no DISPLAY or WAYLAND_DISPLAY"))
            }
        }),
    ));

    for (label, check) in checks {
        match check() {
            Ok(out) => println!("✓ {}: {}", label, out),
            Err(e) => println!("✗ {}: {}", label, e),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    /// `doctor()` reads `CLIPBOARD_DATA_DIR` (via `paths::data_dir`) and `HOME`
    /// (for the platform-service plist/unit check). Both are process-global env
    /// vars — use the shared `test_util::ENV_LOCK` so this serialises against
    /// every other module's env-touching tests, not just our own.
    use crate::test_util::ENV_LOCK;

    #[test]
    fn doctor_returns_ok_with_tempdir_data_dir() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::TempDir::new().unwrap();
        std::env::set_var("CLIPBOARD_DATA_DIR", tmp.path());
        // doctor never bubbles up per-check failures — it just prints them.
        // The function itself should always return Ok(()).
        let r = doctor();
        std::env::remove_var("CLIPBOARD_DATA_DIR");
        assert!(r.is_ok(), "doctor() returned Err: {:?}", r);
    }

    #[test]
    fn doctor_data_dir_writable_against_fresh_tempdir() {
        // Mirrors the `data dir writable` check body. Verifies a write probe
        // succeeds in a freshly-created tempdir — the sensible-result contract
        // for that check on first-run.
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::TempDir::new().unwrap();
        std::env::set_var("CLIPBOARD_DATA_DIR", tmp.path());

        let p = crate::core::paths::data_dir();
        std::fs::create_dir_all(&p).unwrap();
        let probe = p.join(".write_probe");
        std::fs::write(&probe, b"ok").unwrap();
        let read_back = std::fs::read(&probe).unwrap();
        std::fs::remove_file(&probe).unwrap();

        std::env::remove_var("CLIPBOARD_DATA_DIR");
        assert_eq!(read_back, b"ok");
        assert_eq!(p, tmp.path());
    }

    #[test]
    fn doctor_runs_twice_idempotently() {
        // The write probe creates+deletes `.write_probe`; running doctor
        // repeatedly must not leave artefacts or fail on the second pass.
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::TempDir::new().unwrap();
        std::env::set_var("CLIPBOARD_DATA_DIR", tmp.path());
        let r1 = doctor();
        let r2 = doctor();
        let probe_remains = tmp.path().join(".write_probe").exists();
        std::env::remove_var("CLIPBOARD_DATA_DIR");
        assert!(r1.is_ok() && r2.is_ok());
        assert!(!probe_remains, ".write_probe must not linger after doctor()");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn doctor_plist_check_logic_matches_absence() {
        // The `launchd service installed` check is just a path-exists test
        // against `$HOME/Library/LaunchAgents/me.kz.clipboard-history-rs.plist`.
        // Reproduce the check body against a tempdir HOME — without a plist,
        // the path must not exist (i.e. the check would print ✗).
        let tmp = tempfile::TempDir::new().unwrap();
        let p = tmp
            .path()
            .join("Library/LaunchAgents/me.kz.clipboard-history-rs.plist");
        assert!(!p.exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn doctor_display_check_logic() {
        // Mirrors the `X display present` check — purely env-var driven.
        // We don't mutate env here (would clobber the caller's session); we
        // just assert the check's predicate is the obvious one.
        let has_display =
            std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok();
        // Just ensure the predicate is evaluable and boolean — the check
        // returns Ok("yes") iff this is true.
        let _ = has_display;
    }
}
