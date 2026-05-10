//! Filesystem storage for binary clip payloads (images, files).
//!
//! Layout: `data_dir/blobs/<aa>/<sha256>.<ext>` where `<aa>` is the first
//! two hex characters of the hash (git-style loose-object sharding so
//! directory listings stay flat as content grows).
//!
//! All paths returned to callers are RELATIVE to `data_dir/blobs/` so the
//! database survives a `data_dir` move. Resolve to absolute via
//! `blob_path_absolute`.

use anyhow::Result;
use std::path::PathBuf;

/// Root directory for all clip blobs under `data_dir`.
pub fn blobs_dir() -> PathBuf {
    crate::core::paths::data_dir().join("blobs")
}
