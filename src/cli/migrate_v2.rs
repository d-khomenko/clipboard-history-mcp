use crate::core::crypto::{
    delete_master_key_v1, read_master_key_v1, read_master_key_v2, write_master_key_v2,
};
use crate::core::master_password::{prompt_password_with_confirmation, wrap_master_key};
use anyhow::Result;
use rand::Rng;

pub fn migrate_v2() -> Result<()> {
    // macOS: v3 used ~/Library/Application Support/clipboard-history-mcp/
    // v4 uses ~/Library/Application Support/kz.me.clipboard-history-mcp/ (via directories-next)
    // Move the directory if it exists and the v4 dir doesn't yet.
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME")?;
        let v3_dir = std::path::PathBuf::from(&home)
            .join("Library/Application Support/clipboard-history-mcp");
        let v4_dir = crate::core::paths::data_dir();
        if v3_dir.exists() && v3_dir != v4_dir && !v4_dir.exists() {
            if let Some(parent) = v4_dir.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(&v3_dir, &v4_dir)?;
            println!("Moved v3 data dir → {}", v4_dir.display());
        }
    }

    if read_master_key_v2()?.is_some() {
        println!("v4 master-key-v2 already present — nothing to migrate.");
        return Ok(());
    }
    let legacy = match read_master_key_v1()? {
        Some(k) => k,
        None => {
            println!("No v3 master-key-v1 found. Set a fresh master password instead.");
            return set_fresh_password();
        }
    };
    println!("Found v3 master-key-v1. Setting up v4 master-password wrap.");
    let pw = prompt_password_with_confirmation()?;
    let wrapped = wrap_master_key(&legacy, &pw)?;
    write_master_key_v2(&wrapped)?;
    delete_master_key_v1()?;
    println!("Migrated. Master key is now password-wrapped under master-key-v2.");
    Ok(())
}

fn set_fresh_password() -> Result<()> {
    let pw = prompt_password_with_confirmation()?;
    let mut master = [0u8; 32];
    rand::rng().fill_bytes(&mut master);
    let wrapped = wrap_master_key(&master, &pw)?;
    write_master_key_v2(&wrapped)?;
    println!("New master key set under master-key-v2.");
    Ok(())
}

#[cfg(test)]
mod tests {
    // Notes on coverage scope:
    //
    // `migrate_v2()` itself calls `read_master_key_v2()` and (potentially)
    // `prompt_password_with_confirmation()` — both hit the OS keyring and tty
    // respectively. Invoking the top-level function in a unit test would
    // either prompt for a password or contaminate the real keychain. So
    // these tests cover the **macOS v3 → v4 directory-rename block** in
    // isolation: same preconditions, same `fs::rename` call, against mock
    // paths under tempdirs.
    //
    // The rename block lives at the top of `migrate_v2()`:
    //
    //     if v3_dir.exists() && v3_dir != v4_dir && !v4_dir.exists() {
    //         create_dir_all(parent_of_v4)?;
    //         fs::rename(&v3_dir, &v4_dir)?;
    //     }
    //
    // The tests below reproduce that exact predicate against tempdir paths.

    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Re-implements the rename precondition + action verbatim from
    /// `migrate_v2`. If the logic in `migrate_v2.rs` changes, this helper
    /// must change in lockstep — that's intentional: a contract test.
    fn run_rename_block(v3_dir: &PathBuf, v4_dir: &PathBuf) -> anyhow::Result<bool> {
        if v3_dir.exists() && v3_dir != v4_dir && !v4_dir.exists() {
            if let Some(parent) = v4_dir.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::rename(v3_dir, v4_dir)?;
            return Ok(true);
        }
        Ok(false)
    }

    #[test]
    fn rename_moves_v3_when_v4_absent() {
        let tmp = TempDir::new().unwrap();
        let v3 = tmp.path().join("Library/Application Support/clipboard-history-mcp");
        let v4 = tmp.path().join("Library/Application Support/kz.me.clipboard-history-mcp");
        fs::create_dir_all(&v3).unwrap();
        fs::write(v3.join("history.db"), b"old data").unwrap();

        let renamed = run_rename_block(&v3, &v4).unwrap();
        assert!(renamed, "should have performed the rename");
        assert!(!v3.exists(), "v3 dir should be gone after rename");
        assert!(v4.exists(), "v4 dir should exist after rename");
        assert_eq!(fs::read(v4.join("history.db")).unwrap(), b"old data");
    }

    #[test]
    fn rename_skipped_when_v4_already_exists() {
        let tmp = TempDir::new().unwrap();
        let v3 = tmp.path().join("Library/Application Support/clipboard-history-mcp");
        let v4 = tmp.path().join("Library/Application Support/kz.me.clipboard-history-mcp");
        fs::create_dir_all(&v3).unwrap();
        fs::write(v3.join("history.db"), b"old data").unwrap();
        fs::create_dir_all(&v4).unwrap();
        fs::write(v4.join("history.db"), b"new data").unwrap();

        let renamed = run_rename_block(&v3, &v4).unwrap();
        assert!(!renamed, "should NOT rename when v4 already exists");
        // Both dirs untouched.
        assert_eq!(fs::read(v3.join("history.db")).unwrap(), b"old data");
        assert_eq!(fs::read(v4.join("history.db")).unwrap(), b"new data");
    }

    #[test]
    fn rename_skipped_when_v3_absent() {
        let tmp = TempDir::new().unwrap();
        let v3 = tmp.path().join("Library/Application Support/clipboard-history-mcp");
        let v4 = tmp.path().join("Library/Application Support/kz.me.clipboard-history-mcp");
        // Neither exists.
        let renamed = run_rename_block(&v3, &v4).unwrap();
        assert!(!renamed);
        assert!(!v3.exists());
        assert!(!v4.exists());
    }

    #[test]
    fn rename_skipped_when_paths_equal() {
        // Defensive: if someone misconfigures `CLIPBOARD_DATA_DIR` so the
        // computed v4 dir matches the legacy v3 location, the `v3_dir != v4_dir`
        // guard must skip the rename rather than collapse the directory onto
        // itself.
        let tmp = TempDir::new().unwrap();
        let same = tmp.path().join("data");
        fs::create_dir_all(&same).unwrap();
        let renamed = run_rename_block(&same, &same).unwrap();
        assert!(!renamed);
        assert!(same.exists(), "shared path must survive");
    }

    #[test]
    fn rename_creates_parent_of_v4() {
        // The v4 path nests under `Library/Application Support/` — that
        // parent dir won't exist on a clean rename. The block must
        // `create_dir_all(parent)` first.
        let tmp = TempDir::new().unwrap();
        let v3 = tmp.path().join("flat-v3");
        let v4 = tmp.path().join("deep/nested/v4");
        fs::create_dir_all(&v3).unwrap();

        let renamed = run_rename_block(&v3, &v4).unwrap();
        assert!(renamed);
        assert!(v4.exists());
        assert!(v4.parent().unwrap().exists());
    }
}
