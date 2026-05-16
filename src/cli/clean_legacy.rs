use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Remove the legacy macOS data directory left behind by v0.3.x and earlier
/// installs (`~/Library/Application Support/clipboard-history-mcp/`).
///
/// v0.4 switched the data directory to the qualified XDG-style path
/// `~/Library/Application Support/kz.me.clipboard-history-mcp/` via
/// `directories-next`. `migrate-v2` renames the old directory in place — but
/// only if the new directory does not yet exist. When both exist (e.g. after a
/// fresh reinstall over an existing v0.3.x install), the old directory becomes
/// orphaned: it still holds an obsolete `history.db` schema and consumes disk
/// without ever being read.
///
/// On Linux, every release used the qualified XDG path, so there is no legacy
/// directory to clean up — the command is a no-op.
pub fn clean_legacy(yes: bool) -> Result<()> {
    let Some(legacy) = legacy_path() else {
        println!("No legacy data directory exists on this platform.");
        return Ok(());
    };

    let current = crate::core::paths::data_dir();
    if legacy == current {
        anyhow::bail!(
            "legacy path equals current data dir ({}); refusing to delete",
            legacy.display()
        );
    }

    if !legacy.exists() {
        println!("No legacy directory at {} — nothing to do.", legacy.display());
        return Ok(());
    }

    let bytes = dir_size(&legacy).unwrap_or(0);
    println!(
        "Found legacy data dir: {} ({})",
        legacy.display(),
        format_size(bytes)
    );

    if !yes {
        print!("Delete it? [y/N] ");
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
            println!("Aborted.");
            return Ok(());
        }
    }

    std::fs::remove_dir_all(&legacy)
        .with_context(|| format!("failed to remove {}", legacy.display()))?;
    println!("Removed {}", legacy.display());
    Ok(())
}

#[cfg(target_os = "macos")]
fn legacy_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join("Library/Application Support/clipboard-history-mcp"))
}

#[cfg(not(target_os = "macos"))]
fn legacy_path() -> Option<PathBuf> {
    None
}

fn dir_size(path: &Path) -> Result<u64> {
    let mut total = 0;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_dir() {
            total += dir_size(&entry.path()).unwrap_or(0);
        } else {
            total += meta.len();
        }
    }
    Ok(total)
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// `clean_legacy()` reads `$HOME` (for `legacy_path()`) and
    /// `CLIPBOARD_DATA_DIR` (via `paths::data_dir()` for the
    /// legacy == current safety check). Use the shared `test_util::ENV_LOCK`
    /// so we serialise against every other module's env-touching tests.
    use crate::test_util::ENV_LOCK;

    #[test]
    fn dir_size_sums_files_recursively() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), b"hello").unwrap(); // 5 bytes
        fs::create_dir(tmp.path().join("nested")).unwrap();
        fs::write(tmp.path().join("nested/b.bin"), vec![0u8; 100]).unwrap(); // 100 bytes

        assert_eq!(dir_size(tmp.path()).unwrap(), 105);
    }

    #[test]
    fn format_size_bytes_kb_mb() {
        assert_eq!(format_size(500), "500 bytes");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(2 * 1024 * 1024), "2.0 MB");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn clean_legacy_no_op_when_legacy_absent() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home_tmp = TempDir::new().unwrap();
        let data_tmp = TempDir::new().unwrap();
        let saved_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", home_tmp.path());
        std::env::set_var("CLIPBOARD_DATA_DIR", data_tmp.path());

        // No `~/Library/Application Support/clipboard-history-mcp/` exists
        // under our tempdir HOME → function should return Ok and touch nothing.
        let r = clean_legacy(true);

        if let Some(h) = saved_home {
            std::env::set_var("HOME", h);
        } else {
            std::env::remove_var("HOME");
        }
        std::env::remove_var("CLIPBOARD_DATA_DIR");
        assert!(r.is_ok(), "clean_legacy returned Err: {:?}", r);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn clean_legacy_removes_legacy_with_yes_flag() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home_tmp = TempDir::new().unwrap();
        let data_tmp = TempDir::new().unwrap();
        let saved_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", home_tmp.path());
        std::env::set_var("CLIPBOARD_DATA_DIR", data_tmp.path());

        // Manufacture a fake legacy dir at the exact path `legacy_path()`
        // computes from our tempdir HOME.
        let legacy = home_tmp
            .path()
            .join("Library/Application Support/clipboard-history-mcp");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("history.db"), b"legacy bytes").unwrap();

        let r = clean_legacy(true);

        let legacy_still_there = legacy.exists();
        if let Some(h) = saved_home {
            std::env::set_var("HOME", h);
        } else {
            std::env::remove_var("HOME");
        }
        std::env::remove_var("CLIPBOARD_DATA_DIR");

        assert!(r.is_ok(), "clean_legacy returned Err: {:?}", r);
        assert!(!legacy_still_there, "legacy dir should have been removed");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn clean_legacy_refuses_when_legacy_equals_current() {
        // Safety check: if `CLIPBOARD_DATA_DIR` happens to coincide with the
        // computed legacy path, the function must bail rather than nuke the
        // user's current data dir.
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home_tmp = TempDir::new().unwrap();
        let legacy = home_tmp
            .path()
            .join("Library/Application Support/clipboard-history-mcp");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("sentinel"), b"do not delete").unwrap();

        let saved_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", home_tmp.path());
        // Point CLIPBOARD_DATA_DIR at the legacy path itself.
        std::env::set_var("CLIPBOARD_DATA_DIR", &legacy);

        let r = clean_legacy(true);

        let sentinel_intact = legacy.join("sentinel").exists();
        if let Some(h) = saved_home {
            std::env::set_var("HOME", h);
        } else {
            std::env::remove_var("HOME");
        }
        std::env::remove_var("CLIPBOARD_DATA_DIR");

        assert!(r.is_err(), "expected refusal when legacy == current data dir");
        assert!(sentinel_intact, "data must NOT be deleted on overlap");
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn clean_legacy_is_noop_on_non_macos() {
        // `legacy_path()` returns None outside macOS — function prints a
        // notice and exits Ok regardless of `yes`.
        assert!(clean_legacy(true).is_ok());
        assert!(clean_legacy(false).is_ok());
    }
}
