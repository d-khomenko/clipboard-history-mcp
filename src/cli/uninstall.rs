use crate::cli::install::UninstallOpts;
use anyhow::Result;

pub fn uninstall(opts: UninstallOpts) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        return uninstall_macos(opts);
    }
    #[cfg(target_os = "linux")]
    {
        return crate::cli::install::install_linux::uninstall_linux(opts);
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
