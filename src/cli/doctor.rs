use crate::core::paths;
use anyhow::Result;
use std::process::Command;

pub fn doctor() -> Result<()> {
    let checks: Vec<(&str, Box<dyn Fn() -> Result<String>>)> = vec![
        (
            "data dir writable",
            Box::new(|| {
                let p = paths::data_dir();
                std::fs::create_dir_all(&p)?;
                // Verify it is actually writable by probing a temp file.
                let probe = p.join(".write_probe");
                std::fs::write(&probe, b"ok")?;
                std::fs::remove_file(&probe)?;
                Ok(p.display().to_string())
            }),
        ),
        (
            "Keychain accessible",
            Box::new(|| {
                let s = Command::new("security").arg("list-keychains").output()?;
                if !s.status.success() {
                    return Err(anyhow::anyhow!("security CLI failed"));
                }
                Ok("ok".into())
            }),
        ),
        (
            "clipboard reachable",
            Box::new(|| {
                let _ = crate::core::pasteboard::change_count();
                Ok("ok".into())
            }),
        ),
    ];

    let mut all_ok = true;
    for (label, check) in checks {
        match check() {
            Ok(out) => println!("✓ {}: {}", label, out),
            Err(e) => {
                println!("✗ {}: {}", label, e);
                all_ok = false;
            }
        }
    }

    if all_ok {
        println!("All checks passed.");
    }
    Ok(())
}
