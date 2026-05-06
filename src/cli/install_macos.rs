use super::InstallOpts;
use crate::core::paths;
use anyhow::{anyhow, Result};
use std::process::Command;

const LABEL: &str = "me.kz.clipboard-history-rs";

pub fn install_macos(opts: InstallOpts) -> Result<()> {
    let home = std::env::var("HOME")?;
    let plist_path =
        std::path::PathBuf::from(&home).join(format!("Library/LaunchAgents/{}.plist", LABEL));
    let data_dir = paths::data_dir();
    std::fs::create_dir_all(&data_dir)?;
    let log = paths::log_path();
    let bin = std::env::current_exe()?;

    let template = include_str!("../../scripts/launchd.plist.template");
    let env_dict = format!(
        r#"    <key>CLIPBOARD_CAPTURE_WINDOW_TITLE</key>
    <string>{}</string>"#,
        if opts.window_titles { "1" } else { "0" }
    );
    let plist = template
        .replace("__LABEL__", LABEL)
        .replace("__BINARY__", bin.to_str().unwrap())
        .replace("__ENV_DICT__", &env_dict)
        .replace("__LOG__", log.to_str().unwrap());
    std::fs::write(&plist_path, plist)?;

    // Unload first (ignore error — may not be loaded)
    let _ = Command::new("launchctl")
        .args(["unload", plist_path.to_str().unwrap()])
        .status();
    let s = Command::new("launchctl")
        .args(["load", "-w", plist_path.to_str().unwrap()])
        .status()?;
    if !s.success() {
        return Err(anyhow!("launchctl load failed"));
    }
    println!("Installed → {}", plist_path.display());
    println!("Logs    → {}", log.display());
    Ok(())
}
