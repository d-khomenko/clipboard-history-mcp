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
