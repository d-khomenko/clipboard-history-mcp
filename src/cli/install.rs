use anyhow::Result;
use std::path::PathBuf;

#[cfg(target_os = "macos")]
#[path = "install_macos.rs"]
pub(super) mod install_macos;

#[cfg(target_os = "linux")]
#[path = "install_linux.rs"]
pub mod install_linux;

pub struct InstallOpts {
    pub window_titles: bool,
    pub linger: bool,
    /// Auto-mirror non-secret clips to this Obsidian vault path.
    /// Threaded through to launchd plist (macOS) or systemd unit (Linux)
    /// as `CLIPBOARD_VAULT_PATH` env var.
    pub vault: Option<PathBuf>,
}

pub struct UninstallOpts {
    pub keep_data: bool,
}

pub fn install(opts: InstallOpts) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let _ = opts.linger; // unused on macOS
        return install_macos::install_macos(opts);
    }
    #[cfg(target_os = "linux")]
    {
        return install_linux::install_linux(opts);
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = opts;
        anyhow::bail!("install is not supported on this platform")
    }
}
