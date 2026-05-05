use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;

pub fn uninstall(keep_data: bool) -> Result<()> {
    let home = std::env::var("HOME")?;
    let plist = PathBuf::from(&home)
        .join("Library/LaunchAgents/me.kz.clipboard-history-rs.plist");
    let data_dir =
        PathBuf::from(&home).join("Library/Application Support/clipboard-history-mcp");

    if plist.exists() {
        let _ = Command::new("launchctl")
            .args(["unload", plist.to_str().unwrap()])
            .status();
        std::fs::remove_file(&plist)?;
        println!("Removed {}", plist.display());
    } else {
        println!("Plist not found (already uninstalled?)");
    }

    if !keep_data && data_dir.exists() {
        std::fs::remove_dir_all(&data_dir)?;
        println!("Removed {}", data_dir.display());
    }
    Ok(())
}
