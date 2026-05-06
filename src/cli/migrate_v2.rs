use crate::core::crypto::{
    delete_master_key_v1, read_master_key_v1, read_master_key_v2, write_master_key_v2,
};
use crate::core::master_password::{prompt_password_with_confirmation, wrap_master_key};
use anyhow::Result;

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
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut master);
    let wrapped = wrap_master_key(&master, &pw)?;
    write_master_key_v2(&wrapped)?;
    println!("New master key set under master-key-v2.");
    Ok(())
}
