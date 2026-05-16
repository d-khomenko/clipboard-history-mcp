use crate::cli::install::UninstallOpts;
use anyhow::Result;

pub fn uninstall(opts: UninstallOpts) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        uninstall_macos(opts)
    }
    #[cfg(target_os = "linux")]
    {
        crate::cli::install::install_linux::uninstall_linux(opts)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = opts;
        anyhow::bail!("uninstall is not supported on this platform")
    }
}

#[cfg(target_os = "macos")]
fn uninstall_macos(opts: UninstallOpts) -> Result<()> {
    use crate::core::paths;
    use std::process::Command;

    let home = std::env::var("HOME")?;
    let plist = std::path::PathBuf::from(&home)
        .join("Library/LaunchAgents/me.kz.clipboard-history-rs.plist");
    let data_dir = paths::data_dir();

    if plist.exists() {
        let _ = Command::new("launchctl")
            .args(["unload", plist.to_str().unwrap()])
            .status();
        std::fs::remove_file(&plist)?;
        println!("Removed {}", plist.display());
    } else {
        println!("Plist not found (already uninstalled?)");
    }

    if !opts.keep_data && data_dir.exists() {
        std::fs::remove_dir_all(&data_dir)?;
        println!("Removed {}", data_dir.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;
    /// Both `uninstall_macos` and `uninstall_linux` read `$HOME` plus
    /// `CLIPBOARD_DATA_DIR` (via `paths::data_dir`). Use the shared
    /// `test_util::ENV_LOCK` so we serialise against every other module's
    /// env-touching tests.
    use crate::test_util::ENV_LOCK;

    #[cfg(target_os = "macos")]
    #[test]
    fn uninstall_no_plist_no_data_is_ok() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home_tmp = tempfile::TempDir::new().unwrap();
        let data_tmp = tempfile::TempDir::new().unwrap();
        let saved_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", home_tmp.path());
        std::env::set_var("CLIPBOARD_DATA_DIR", data_tmp.path());

        // No plist installed, but `data_dir` does exist (the TempDir itself).
        // Default opts (keep_data=false) — directory should be removed.
        let r = uninstall(UninstallOpts { keep_data: false });

        let data_removed = !data_tmp.path().exists();
        if let Some(h) = saved_home {
            std::env::set_var("HOME", h);
        } else {
            std::env::remove_var("HOME");
        }
        std::env::remove_var("CLIPBOARD_DATA_DIR");

        assert!(r.is_ok(), "uninstall returned Err: {:?}", r);
        assert!(data_removed, "data dir should be removed with keep_data=false");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn uninstall_keep_data_preserves_data_dir() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home_tmp = tempfile::TempDir::new().unwrap();
        let data_tmp = tempfile::TempDir::new().unwrap();
        // Drop a sentinel inside the data dir to prove the directory survives.
        let sentinel = data_tmp.path().join("history.db");
        std::fs::write(&sentinel, b"keep me").unwrap();

        let saved_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", home_tmp.path());
        std::env::set_var("CLIPBOARD_DATA_DIR", data_tmp.path());

        let r = uninstall(UninstallOpts { keep_data: true });

        let sentinel_intact = sentinel.exists();
        if let Some(h) = saved_home {
            std::env::set_var("HOME", h);
        } else {
            std::env::remove_var("HOME");
        }
        std::env::remove_var("CLIPBOARD_DATA_DIR");

        assert!(r.is_ok());
        assert!(sentinel_intact, "keep_data must preserve history.db");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn uninstall_is_idempotent() {
        // Calling uninstall twice in a row must not error — second call hits
        // the "Plist not found / data dir absent" branch.
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home_tmp = tempfile::TempDir::new().unwrap();
        let data_tmp = tempfile::TempDir::new().unwrap();
        let saved_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", home_tmp.path());
        std::env::set_var("CLIPBOARD_DATA_DIR", data_tmp.path());

        let r1 = uninstall(UninstallOpts { keep_data: false });
        let r2 = uninstall(UninstallOpts { keep_data: false });

        if let Some(h) = saved_home {
            std::env::set_var("HOME", h);
        } else {
            std::env::remove_var("HOME");
        }
        std::env::remove_var("CLIPBOARD_DATA_DIR");

        assert!(r1.is_ok() && r2.is_ok(), "uninstall not idempotent: {:?} {:?}", r1, r2);
    }
}
