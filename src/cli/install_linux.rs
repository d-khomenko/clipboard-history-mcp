use super::{InstallOpts, UninstallOpts};
use crate::core::paths::{data_dir, log_path};
use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::process::Command;

const SERVICE_NAME: &str = "clipboard-history-mcp.service";

pub fn install_linux(opts: InstallOpts) -> Result<()> {
    let home = std::env::var("HOME")?;
    let unit_dir = PathBuf::from(&home).join(".config/systemd/user");
    std::fs::create_dir_all(&unit_dir)?;
    let unit_path = unit_dir.join(SERVICE_NAME);

    std::fs::create_dir_all(data_dir())?;
    let log = log_path();
    let bin = std::env::current_exe()?;

    let template = include_str!("../../scripts/systemd.service.template");
    // Empty string when no vault — leaves a blank line that systemd ignores.
    let vault_env_line = match &opts.vault {
        Some(p) => format!(r#"Environment="CLIPBOARD_VAULT_PATH={}""#, p.display()),
        None => String::new(),
    };
    let unit = template
        .replace("__BINARY__", bin.to_str().unwrap())
        .replace("__LOG__", log.to_str().unwrap())
        .replace(
            "__CAPTURE_WINDOW_TITLE__",
            if opts.window_titles { "1" } else { "0" },
        )
        .replace("__VAULT_ENV_LINE__", &vault_env_line);
    std::fs::write(&unit_path, unit)?;

    let s = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status()?;
    if !s.success() {
        return Err(anyhow!("systemctl daemon-reload failed"));
    }
    let s = Command::new("systemctl")
        .args(["--user", "enable", "--now", SERVICE_NAME])
        .status()?;
    if !s.success() {
        return Err(anyhow!("systemctl enable --now failed"));
    }

    if opts.linger {
        let user = std::env::var("USER")?;
        let _ = Command::new("loginctl")
            .args(["enable-linger", &user])
            .status();
    }

    println!("Installed → {}", unit_path.display());
    println!("Logs    → {}", log.display());
    Ok(())
}

pub fn uninstall_linux(opts: UninstallOpts) -> Result<()> {
    let home = std::env::var("HOME")?;
    let unit_path =
        PathBuf::from(&home).join(".config/systemd/user").join(SERVICE_NAME);

    if unit_path.exists() {
        let _ = Command::new("systemctl")
            .args(["--user", "disable", "--now", SERVICE_NAME])
            .status();
        std::fs::remove_file(&unit_path)?;
        println!("Removed {}", unit_path.display());
    }
    if !opts.keep_data {
        let dir = data_dir();
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
            println!("Removed {}", dir.display());
        }
    }
    Ok(())
}
