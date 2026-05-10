//! Filesystem storage for binary clip payloads (images, files).
//!
//! Layout: `data_dir/blobs/<aa>/<sha256>.<ext>` where `<aa>` is the first
//! two hex characters of the hash (git-style loose-object sharding so
//! directory listings stay flat as content grows).
//!
//! All paths stored in the DB are RELATIVE to `data_dir/blobs/` so the
//! database survives a `data_dir` move.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::PathBuf;

/// Root directory for all clip blobs under `data_dir`.
pub fn blobs_dir() -> PathBuf {
    crate::core::paths::data_dir().join("blobs")
}

/// Compute the sha256 hex digest of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

/// Build the relative path that goes into `clips.blob_path`.
///
/// `extension` should be a short ASCII suffix without the leading dot
/// ("png", "tiff", "bin"). The function does not validate the extension —
/// callers pass MIME-derived strings.
pub fn relative_path(hash: &str, extension: &str) -> String {
    let prefix = &hash[..2];
    format!("{}/{}.{}", prefix, hash, extension)
}

/// Resolve a relative blob path to an absolute path under `data_dir/blobs/`.
pub fn absolute_path(relative: &str) -> PathBuf {
    blobs_dir().join(relative)
}

/// Atomically write `bytes` to the blob path derived from its sha256
/// digest. Idempotent — if the destination already exists with the same
/// hash, returns the existing relative path without rewriting.
///
/// Returns `(relative_path, hash_hex)`.
pub fn write(bytes: &[u8], extension: &str) -> Result<(String, String)> {
    let hash = sha256_hex(bytes);
    let rel = relative_path(&hash, extension);
    let abs = absolute_path(&rel);

    if abs.exists() {
        // Dedup hit — same hash means same content. No-op.
        return Ok((rel, hash));
    }

    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent).context("create blob parent dir")?;
    }
    let pid = std::process::id();
    let file_name = abs
        .file_name()
        .context("blob path missing file name component")?
        .to_string_lossy();
    let tmp = abs.with_file_name(format!("{}.{}.tmp", file_name, pid));
    {
        let mut f = std::fs::File::create(&tmp).context("create blob tmp")?;
        f.write_all(bytes).context("write blob bytes")?;
        f.sync_all().context("fsync blob")?;
    }
    std::fs::rename(&tmp, &abs).context("rename blob into place")?;
    Ok((rel, hash))
}

/// Read a blob's full contents into memory.
pub fn read(relative: &str) -> Result<Vec<u8>> {
    let abs = absolute_path(relative);
    std::fs::read(&abs).with_context(|| format!("read blob {}", abs.display()))
}

/// Best-effort delete. Logs a warn and returns `Ok(())` if the file is
/// missing; the row is gone anyway. Returns `Err` only on permission /
/// I/O failures.
pub fn delete(relative: &str) -> Result<()> {
    let abs = absolute_path(relative);
    match std::fs::remove_file(&abs) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::warn!("blob delete: {} already missing", abs.display());
            Ok(())
        }
        Err(e) => Err(e).with_context(|| format!("delete blob {}", abs.display())),
    }
}

/// Determine a sensible filename extension for a MIME type.
/// Returns `"bin"` for unknown MIMEs.
pub fn extension_for_mime(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/tiff" => "tiff",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "application/pdf" => "pdf",
        _ => "bin",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Mutex;

    /// Tests share an env-var override of CLIPBOARD_DATA_DIR. Serialise so
    /// they don't race on the global env.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_data_dir<F: FnOnce(&Path)>(f: F) {
        let _g = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::TempDir::new().unwrap();
        std::env::set_var("CLIPBOARD_DATA_DIR", tmp.path());
        f(tmp.path());
        std::env::remove_var("CLIPBOARD_DATA_DIR");
    }

    #[test]
    fn relative_path_uses_two_char_prefix() {
        assert_eq!(
            relative_path("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789", "png"),
            "ab/abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789.png"
        );
    }

    #[test]
    fn extension_for_mime_known_and_unknown() {
        assert_eq!(extension_for_mime("image/png"), "png");
        assert_eq!(extension_for_mime("image/tiff"), "tiff");
        assert_eq!(extension_for_mime("video/quicktime"), "bin");
    }

    #[test]
    fn write_creates_file_and_dedups_on_repeat() {
        with_temp_data_dir(|root| {
            let bytes = b"hello blob";
            let (rel1, hash1) = write(bytes, "bin").unwrap();
            let abs1 = absolute_path(&rel1);
            assert!(abs1.exists(), "first write should create file");
            assert_eq!(std::fs::read(&abs1).unwrap(), bytes);

            // Second write: same content → same path, no rewrite.
            let mtime1 = abs1.metadata().unwrap().modified().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
            let (rel2, hash2) = write(bytes, "bin").unwrap();
            assert_eq!(rel1, rel2);
            assert_eq!(hash1, hash2);
            let mtime2 = abs1.metadata().unwrap().modified().unwrap();
            assert_eq!(mtime1, mtime2, "dedup hit should not rewrite the file");

            assert!(root.join("blobs").exists());
        });
    }

    #[test]
    fn read_round_trips() {
        with_temp_data_dir(|_| {
            let bytes = b"round trip payload";
            let (rel, _) = write(bytes, "bin").unwrap();
            assert_eq!(read(&rel).unwrap(), bytes);
        });
    }

    #[test]
    fn delete_removes_existing_file() {
        with_temp_data_dir(|_| {
            let bytes = b"delete me";
            let (rel, _) = write(bytes, "bin").unwrap();
            assert!(absolute_path(&rel).exists());
            delete(&rel).unwrap();
            assert!(!absolute_path(&rel).exists());
        });
    }

    #[test]
    fn delete_is_ok_when_blob_missing() {
        with_temp_data_dir(|_| {
            // No write — delete should succeed (best-effort) and return Ok.
            assert!(delete("aa/missing.bin").is_ok());
        });
    }
}
