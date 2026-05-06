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
    let mut env_dict = format!(
        r#"    <key>CLIPBOARD_CAPTURE_WINDOW_TITLE</key>
    <string>{}</string>"#,
        if opts.window_titles { "1" } else { "0" }
    );
    if let Some(vault) = &opts.vault {
        // XML-escape the path because launchd plist is XML and a vault dir
        // named e.g. "Work & Personal/Obsidian" or `my<vault>` would
        // produce malformed XML and `launchctl load` would silently fail.
        env_dict.push_str(&format!(
            "\n    <key>CLIPBOARD_VAULT_PATH</key>\n    <string>{}</string>",
            xml_escape(&vault.display().to_string())
        ));
    }
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

/// Minimal XML escape for plist `<string>` values. Covers the five chars
/// that are illegal inside XML element text content (`&`, `<`, `>`) plus
/// the two that need escaping inside attribute values (`"`, `'`). Apple's
/// plist format follows XML 1.0 rules.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_escape_handles_ampersand_and_brackets() {
        assert_eq!(
            xml_escape("Work & Personal/Obsidian"),
            "Work &amp; Personal/Obsidian"
        );
        assert_eq!(xml_escape("my<vault>"), "my&lt;vault&gt;");
    }

    #[test]
    fn xml_escape_passes_through_safe_paths() {
        assert_eq!(
            xml_escape("/Users/stock/Documents/Obsidian/MyVault"),
            "/Users/stock/Documents/Obsidian/MyVault"
        );
    }

    #[test]
    fn xml_escape_handles_quotes() {
        assert_eq!(
            xml_escape(r#"path "with quotes""#),
            "path &quot;with quotes&quot;"
        );
        assert_eq!(xml_escape("path 'with apos'"), "path &apos;with apos&apos;");
    }
}
