//! OCR helper: wraps the `klipta-ocr` Swift subprocess to extract text from
//! image clips via Apple VisionKit.
//!
//! # Subprocess protocol
//! The helper binary is invoked as:
//!   `klipta-ocr <absolute-image-path>`
//! and writes the extracted text to stdout (UTF-8, may contain newlines).
//! Exit code 0 = success; any other code = failure (stderr is logged at WARN).
//!
//! # Graceful degradation
//! If the binary is not present at the expected location, `run_ocr` returns
//! `Ok(None)` rather than an error. This keeps ingestion working on machines
//! where the helper has not yet been compiled/installed.
//!
//! # TODO(manual): install the helper
//!
//! The `install` subcommand should be extended to:
//!
//! - Compile `tools/klipta-ocr.swift` via `swiftc`.
//! - Place the resulting binary in
//!   `~/.local/share/clipboard-history-mcp/bin/klipta-ocr`
//!   (or the platform data dir returned by `paths::data_dir()`).
//!
//! Until that is done, all OCR calls return `Ok(None)` gracefully.

use anyhow::Result;
use std::path::Path;

/// Locate the `klipta-ocr` helper binary.
///
/// Resolution order:
///   1. `CLIPBOARD_OCR_BINARY` env var (for tests / custom installs).
///   2. `<data_dir>/bin/klipta-ocr`.
fn helper_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("CLIPBOARD_OCR_BINARY") {
        return std::path::PathBuf::from(p);
    }
    crate::core::paths::data_dir()
        .join("bin")
        .join("klipta-ocr")
}

/// Run OCR on the image at `image_path` and return the extracted text.
///
/// Returns `Ok(None)` when:
/// - `CLIPBOARD_OCR_DISABLED=1` is set in the environment.
/// - The `klipta-ocr` helper binary is not found.
/// - The binary produces no output (blank image / no text found).
///
/// Returns `Err` only on I/O errors (spawn failure, stdout read failure).
pub fn run_ocr(image_path: &Path) -> Result<Option<String>> {
    // Respect the privacy kill-switch.
    if std::env::var("CLIPBOARD_OCR_DISABLED").as_deref() == Ok("1") {
        return Ok(None);
    }

    let binary = helper_path();
    if !binary.exists() {
        // Helper not installed yet — silent no-op.
        tracing::debug!(
            "klipta-ocr binary not found at {}, skipping OCR",
            binary.display()
        );
        return Ok(None);
    }

    let output = std::process::Command::new(&binary)
        .arg(image_path)
        .output()
        .map_err(|e| anyhow::anyhow!("failed to spawn klipta-ocr: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(
            "klipta-ocr exited with {}: {}",
            output.status,
            stderr.trim()
        );
        return Ok(None);
    }

    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_none_when_ocr_disabled() {
        // Safety: env mutation in tests. Each test binary is single-process
        // so this is fine as long as tests don't run this in parallel with
        // tests that depend on OCR being enabled.
        let _guard = crate::test_util::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CLIPBOARD_OCR_DISABLED", "1");
        let tmp_img = tempfile::NamedTempFile::new().unwrap();
        let result = run_ocr(tmp_img.path()).unwrap();
        assert!(result.is_none(), "OCR should be a no-op when disabled");
        std::env::remove_var("CLIPBOARD_OCR_DISABLED");
    }

    #[test]
    fn returns_none_when_binary_missing() {
        let _guard = crate::test_util::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Point to a path that definitely doesn't exist.
        std::env::set_var("CLIPBOARD_OCR_BINARY", "/nonexistent/klipta-ocr");
        let tmp_img = tempfile::NamedTempFile::new().unwrap();
        let result = run_ocr(tmp_img.path()).unwrap();
        assert!(result.is_none(), "should return None, not Err, when binary is missing");
        std::env::remove_var("CLIPBOARD_OCR_BINARY");
    }
}
