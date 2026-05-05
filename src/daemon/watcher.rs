use crate::core::{
    pasteboard,
    secrets,
    store::{ClipInput, SecretInput, Store},
    types,
};
use crate::daemon::context;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

pub struct WatcherOptions {
    pub poll_ms: u64,
    pub capture_window_title: bool,
    pub ignore_apps: Vec<String>,
    pub never_store_secrets: bool,
    pub max_items: i64,
}

impl Default for WatcherOptions {
    fn default() -> Self {
        Self {
            poll_ms: 1500,
            capture_window_title: false,
            ignore_apps: vec![],
            never_store_secrets: false,
            max_items: 1000,
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
    let text = pasteboard::read_clipboard()?;
    if text.is_empty() {
        return Ok(());
    }
    if last_seen.as_deref() == Some(text.as_str()) {
        return Ok(());
    }
    *last_seen = Some(text.clone());

    if pasteboard::is_transient() {
        info!("skip: transient pasteboard type");
        return Ok(());
    }

    let ctx = context::capture(opts.capture_window_title);
    if let Some(app) = &ctx.front_app {
        if opts.ignore_apps.iter().any(|a| a == app) {
            info!("skip: ignored app {}", app);
            return Ok(());
        }
    }

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
        store.add_clip(ClipInput {
            text: text.clone(),
            primary_kind: cls.primary_kind.clone(),
            kinds: cls.kinds,
            source_app: ctx.front_app.clone(),
            window_title: ctx.window_title.clone(),
        })?;
        info!("clip captured: {} from {:?}", cls.primary_kind, ctx.front_app);
    }

    store.prune_oldest(opts.max_items)?;
    Ok(())
}
