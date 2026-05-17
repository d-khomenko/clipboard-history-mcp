use crate::core::{
    pasteboard,
    recipe_engine::RecipeEngine,
    secrets,
    store::{ClipInput, SecretInput, Store},
    types,
};
use crate::daemon::context;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{info, warn};

pub struct WatcherOptions {
    pub poll_ms: u64,
    pub capture_window_title: bool,
    pub ignore_apps: Vec<String>,
    pub never_store_secrets: bool,
    pub max_items: i64,
    pub max_blob_bytes: i64,
    /// Configured Obsidian vault for daemon-side mirroring of non-secret
    /// clips. `None` disables the feature; capture proceeds unchanged.
    pub vault_mirror: Option<crate::core::vault::VaultMirror>,
    /// Recipe engine. Shared across the watcher thread via `Arc<Mutex<...>>`.
    /// `None` disables the workflow engine entirely.
    pub recipe_engine: Option<Arc<Mutex<RecipeEngine>>>,
}

impl Default for WatcherOptions {
    fn default() -> Self {
        Self {
            poll_ms: 1500,
            capture_window_title: false,
            ignore_apps: vec![],
            never_store_secrets: false,
            max_items: 1000,
            max_blob_bytes: 26_214_400, // 25 MB
            vault_mirror: None,
            recipe_engine: None,
        }
    }
}

/// Run the clipboard poll loop on the calling thread.
///
/// Opens its own `Store` because `rusqlite::Connection` is `!Send` — the store
/// cannot be moved across thread boundaries.  The caller owns the DB path and
/// master key and passes them by value.
pub fn run_watcher(
    db_path: PathBuf,
    master_key: [u8; 32],
    opts: WatcherOptions,
    stop: Arc<AtomicBool>,
) {
    let store = match Store::open(&db_path, master_key) {
        Ok(s) => s,
        Err(e) => {
            warn!("watcher: failed to open store: {}", e);
            return;
        }
    };

    let mut last_seen: Option<String> = None;
    while !stop.load(Ordering::Relaxed) {
        match tick(&store, &opts, &mut last_seen) {
            Ok(_) => {}
            Err(e) => warn!("watcher tick: {}", e),
        }
        std::thread::sleep(Duration::from_millis(opts.poll_ms));
    }
    info!("watcher stopped");
}

fn tick(
    store: &Store,
    opts: &WatcherOptions,
    last_seen: &mut Option<String>,
) -> anyhow::Result<()> {
    if pasteboard::is_transient() {
        info!("skip: transient pasteboard type");
        return Ok(());
    }

    let clip = pasteboard::read_clip()?;

    // Dedup across all payload kinds. Without this, a stable image or file
    // selection on the pasteboard would re-run capture + vault mirror on
    // every poll. We pay one sha256 per tick on the image bytes (cheap on
    // Apple Silicon — ~50 ms per 25 MB) to skip an `add_image_clip` call
    // and a `std::fs::copy` of the blob into the vault, which is much
    // costlier. File dedup hashes the joined path list rather than the
    // bytes — re-reading every file every tick would defeat the purpose,
    // and "same selection in Finder" is the common case.
    let dedup_key = match &clip {
        pasteboard::Clip::Empty => None,
        pasteboard::Clip::Text(s) => Some(format!("text:{}", s)),
        pasteboard::Clip::Image { bytes, .. } => {
            Some(format!("image:{}", crate::core::blobs::sha256_hex(bytes)))
        }
        pasteboard::Clip::Files(paths) => {
            let joined = paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("|");
            Some(format!(
                "files:{}",
                crate::core::blobs::sha256_hex(joined.as_bytes())
            ))
        }
    };
    if let Some(key) = &dedup_key {
        if last_seen.as_deref() == Some(key.as_str()) {
            return Ok(());
        }
        *last_seen = Some(key.clone());
    }

    let ctx = context::capture(opts.capture_window_title);
    if let Some(app) = &ctx.front_app {
        if opts.ignore_apps.iter().any(|a| a == app) {
            info!("skip: ignored app {}", app);
            return Ok(());
        }
    }

    match clip {
        pasteboard::Clip::Empty => Ok(()),

        pasteboard::Clip::Text(text) => tick_text(store, opts, &ctx, text),

        pasteboard::Clip::Image { bytes, mime } => {
            if opts.never_store_secrets {
                info!("skip image (paranoid mode)");
                return Ok(());
            }
            if (bytes.len() as i64) > opts.max_blob_bytes {
                warn!(
                    "skip image: {} bytes exceeds CLIPBOARD_MAX_BLOB_BYTES={}",
                    bytes.len(),
                    opts.max_blob_bytes
                );
                return Ok(());
            }
            let id = store.add_image_clip(crate::core::store::ImageClipInput {
                bytes,
                mime: mime.into(),
                source_app: ctx.front_app.clone(),
                window_title: ctx.window_title.clone(),
            })?;
            info!("image captured: id={} mime={} from {:?}", id, mime, ctx.front_app);
            run_ocr_async(store, id);
            fire_recipes(opts, store, id);
            mirror_image_or_file(opts, &ctx, store, id)?;
            store.prune_oldest(opts.max_items)?;
            Ok(())
        }

        pasteboard::Clip::Files(paths) => {
            if opts.never_store_secrets {
                info!("skip files (paranoid mode)");
                return Ok(());
            }
            // Filter out files that are too large; keep the rest.
            let kept: Vec<_> = paths
                .into_iter()
                .filter(|p| match std::fs::metadata(p) {
                    Ok(m) if (m.len() as i64) <= opts.max_blob_bytes => true,
                    Ok(m) => {
                        warn!(
                            "skip file {}: {} bytes exceeds limit",
                            p.display(),
                            m.len()
                        );
                        false
                    }
                    Err(_) => false,
                })
                .collect();
            if kept.is_empty() {
                return Ok(());
            }
            let ids = store.add_files_clip(crate::core::store::FilesClipInput {
                paths: kept,
                source_app: ctx.front_app.clone(),
                window_title: ctx.window_title.clone(),
            })?;
            info!("files captured: {:?} from {:?}", ids, ctx.front_app);
            for id in ids {
                fire_recipes(opts, store, id);
                mirror_image_or_file(opts, &ctx, store, id)?;
            }
            store.prune_oldest(opts.max_items)?;
            Ok(())
        }
    }
}

/// Spawn an OS thread to run OCR on the blob just written for `clip_id` and
/// write the result back to `clips.ocr_text`. The schema `clips_au` trigger
/// propagates the change to `clips_fts` automatically.
///
/// Errors are logged at WARN and swallowed so OCR never blocks or breaks
/// the main capture loop.
fn run_ocr_async(store: &Store, clip_id: i64) {
    // Retrieve the blob_path so we can pass the absolute file path to OCR.
    // If the row is missing or has no blob (shouldn't happen for image clips
    // we just inserted), silently bail.
    let blob_rel = match store.get_item(clip_id) {
        Ok(Some(item)) => item.blob_path,
        _ => return,
    };
    let Some(rel) = blob_rel else { return };
    let abs_path = crate::core::blobs::absolute_path(&rel);
    let db_path = crate::core::paths::db_path();

    std::thread::spawn(move || {
        match crate::core::ocr::run_ocr(&abs_path) {
            Ok(Some(text)) => {
                // Open a fresh raw connection for the write-back — we only
                // need a plain UPDATE, no crypto. The primary Store is !Send
                // so we cannot reuse it across threads.
                match crate::core::db::open_db(&db_path) {
                    Ok(conn) => {
                        let result = conn.execute(
                            "UPDATE clips SET ocr_text = ?1 WHERE id = ?2",
                            rusqlite::params![text, clip_id],
                        );
                        match result {
                            Ok(_) => tracing::info!(
                                "OCR clip {}: {} chars extracted",
                                clip_id,
                                text.len()
                            ),
                            Err(e) => warn!("OCR write-back failed for clip {}: {}", clip_id, e),
                        }
                    }
                    Err(e) => warn!("OCR write-back: failed to open DB: {}", e),
                }
            }
            Ok(None) => {
                tracing::debug!("OCR clip {}: no text found", clip_id);
            }
            Err(e) => warn!("OCR clip {}: error: {}", clip_id, e),
        }
    });
}

/// Existing text capture flow. Kept as a private helper so tick stays
/// readable.
fn tick_text(
    store: &Store,
    opts: &WatcherOptions,
    ctx: &context::CaptureContext,
    text: String,
) -> anyhow::Result<()> {
    if let Some(hit) = secrets::detect_secret(&text) {
        if opts.never_store_secrets {
            info!(
                "secret detected (paranoid mode, ciphertext omitted): {}",
                hit.kind
            );
        } else {
            store.add_secret(SecretInput {
                text: hit.value.clone(),
                secret_kind: hit.kind.clone(),
                source_app: ctx.front_app.clone(),
                window_title: ctx.window_title.clone(),
            })?;
            info!("secret captured: {} from {:?}", hit.kind, ctx.front_app);
        }
    } else {
        let cls = types::classify(&text);
        let id = store.add_clip(ClipInput {
            text: text.clone(),
            primary_kind: cls.primary_kind.clone(),
            kinds: cls.kinds,
            source_app: ctx.front_app.clone(),
            window_title: ctx.window_title.clone(),
        })?;
        info!("clip captured: {} from {:?}", cls.primary_kind, ctx.front_app);
        fire_recipes(opts, store, id);

        if let Some(mirror) = &opts.vault_mirror {
            let item = crate::core::vault::MirrorItem {
                id,
                primary_kind: &cls.primary_kind,
                source_app: ctx.front_app.as_deref(),
                window_title: ctx.window_title.as_deref(),
                text: &text,
                captured: chrono::Local::now(),
                payload_kind: "text",
                blob_relative_path: None,
                mime_type: None,
            };
            if let Err(e) = mirror.write(&item) {
                warn!("vault mirror failed for clip {}: {}", id, e);
            }
        }
    }

    store.prune_oldest(opts.max_items)?;
    Ok(())
}

/// Dispatch a newly-ingested clip to the recipe engine (if configured).
///
/// Runs synchronously on the watcher thread. Recipe actions that do I/O
/// (webhook POST, vault append) have individual timeouts / best-effort
/// semantics so a slow external endpoint doesn't permanently stall the
/// poll loop — it delays at most one tick.
fn fire_recipes(opts: &WatcherOptions, store: &Store, clip_id: i64) {
    let Some(engine_arc) = &opts.recipe_engine else { return };
    match engine_arc.lock() {
        Ok(engine) => engine.handle_clip(store, clip_id),
        Err(_) => warn!("fire_recipes: recipe engine mutex poisoned, skipping"),
    }
}

/// Vault-mirror hook for image and file clips. Reads the just-inserted
/// row to populate `MirrorItem` with the blob path.
fn mirror_image_or_file(
    opts: &WatcherOptions,
    ctx: &context::CaptureContext,
    store: &Store,
    id: i64,
) -> anyhow::Result<()> {
    let Some(mirror) = &opts.vault_mirror else {
        return Ok(());
    };
    let Some(item) = store.get_item(id)? else {
        return Ok(());
    };
    let _ = ctx; // currently unused — placeholder for future per-app behaviour
    let mirror_item = crate::core::vault::MirrorItem {
        id: item.id,
        primary_kind: &item.primary_kind,
        source_app: item.source_app.as_deref(),
        window_title: item.window_title.as_deref(),
        text: item.preview.as_str(), // preview as the body — image/file have no text
        captured: chrono::Local::now(),
        payload_kind: &item.payload_kind,
        blob_relative_path: item.blob_path.as_deref(),
        mime_type: item.mime_type.as_deref(),
    };
    if let Err(e) = mirror.write(&mirror_item) {
        warn!("vault mirror failed for clip {}: {}", id, e);
    }
    Ok(())
}
