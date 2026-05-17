//! Workflow / Recipe engine.
//!
//! Loads `*.json` recipe files from `<data_dir>/recipes/`, watches the
//! directory for changes (via the `notify` crate), and fires matching recipe
//! actions whenever a new clip is ingested.
//!
//! # File-system watch
//! Call `RecipeEngine::start_fs_watch` once during daemon startup to set up
//! automatic reload on any change inside the recipes directory.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

use crate::core::recipe::{Recipe, RecipeAction};
use crate::core::store::Store;

pub struct RecipeEngine {
    recipes: Vec<Recipe>,
    recipes_dir: PathBuf,
}

impl RecipeEngine {
    /// Load recipes from `recipes_dir`. The directory is created if it does
    /// not yet exist.
    pub fn load(recipes_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&recipes_dir)
            .with_context(|| format!("create recipes dir {}", recipes_dir.display()))?;
        let mut engine = Self { recipes: vec![], recipes_dir };
        engine.reload()?;
        Ok(engine)
    }

    /// (Re)read all `*.json` files in the recipes directory.
    pub fn reload(&mut self) -> Result<()> {
        self.recipes.clear();
        let dir = match std::fs::read_dir(&self.recipes_dir) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e).context("read recipes dir"),
        };
        for entry in dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            match load_recipe_file(&path) {
                Ok(recipe) => {
                    info!("recipe loaded: {} ({})", recipe.name, recipe.id);
                    self.recipes.push(recipe);
                }
                Err(e) => warn!("recipe load error {}: {}", path.display(), e),
            }
        }
        Ok(())
    }

    /// Return a reference to the currently-loaded recipes (for testing).
    #[cfg(test)]
    pub fn recipes(&self) -> &[Recipe] {
        &self.recipes
    }

    /// Fire all matching, enabled recipes for the clip identified by `clip_id`.
    /// Errors from individual actions are logged at WARN and do not abort
    /// remaining actions.
    pub fn handle_clip(&self, store: &Store, clip_id: i64) {
        // Fetch the clip once so we can match triggers without hitting the DB
        // inside every recipe iteration.
        let item = match store.get_item(clip_id) {
            Ok(Some(i)) => i,
            Ok(None) => return,
            Err(e) => {
                warn!("recipe_engine: get_item({}) failed: {}", clip_id, e);
                return;
            }
        };

        for recipe in &self.recipes {
            if !recipe.enabled {
                continue;
            }
            let matches = recipe.trigger.matches(
                &item.primary_kind,
                item.source_app.as_deref(),
                item.byte_length,
            );
            if !matches {
                continue;
            }
            info!(
                "recipe '{}' fired for clip {} (kind={})",
                recipe.name, clip_id, item.primary_kind
            );
            if let Err(e) = fire_action(&recipe.action, store, clip_id, &item) {
                warn!("recipe '{}' action failed: {}", recipe.name, e);
            }
        }
    }

    /// Start a background thread that watches `recipes_dir` for file-system
    /// changes and triggers a reload whenever any change is detected.
    ///
    /// Returns the `notify::RecommendedWatcher` handle; the caller must keep
    /// it alive (store in a field or `Box::leak` it) — dropping it stops
    /// the watch.
    pub fn start_fs_watch(
        recipes_dir: PathBuf,
        reload_tx: std::sync::mpsc::Sender<()>,
    ) -> Result<notify::RecommendedWatcher> {
        use notify::Watcher;

        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            match event {
                Ok(e) if !matches!(e.kind, notify::EventKind::Access(_)) => {
                    let _ = reload_tx.send(());
                }
                _ => {}
            }
        })
        .context("create fs watcher for recipes dir")?;

        // Wrap in a mutable binding so we can call watch() without moving.
        let mut w = watcher;
        w.watch(&recipes_dir, notify::RecursiveMode::NonRecursive)
            .context("watch recipes dir")?;
        Ok(w)
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn load_recipe_file(path: &Path) -> Result<Recipe> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("read recipe {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("parse recipe {}", path.display()))
}

fn fire_action(
    action: &RecipeAction,
    store: &Store,
    clip_id: i64,
    item: &crate::core::store::Item,
) -> Result<()> {
    match action {
        RecipeAction::Tag { tag } => {
            store.tag(clip_id, tag)?;
            info!("recipe: tagged clip {} with '{}'", clip_id, tag);
        }

        RecipeAction::WebhookPost { url, include_text } => {
            webhook_post(url, clip_id, item, *include_text)?;
        }

        RecipeAction::VaultAppend { vault_path, daily } => {
            vault_append(vault_path, *daily, item)?;
        }

        RecipeAction::Notify { title, body } => {
            send_notification(title, body)?;
        }
    }
    Ok(())
}

// ── action implementations ────────────────────────────────────────────────────

fn webhook_post(
    url: &str,
    clip_id: i64,
    item: &crate::core::store::Item,
    include_text: bool,
) -> Result<()> {
    // TODO(manual): replace the placeholder URL in starter recipes with your
    // actual webhook endpoint before enabling WebhookPost recipes.
    let mut payload = serde_json::json!({
        "clip_id":    clip_id,
        "preview":    item.preview,
        "kind":       item.primary_kind,
        "source_app": item.source_app,
    });
    if include_text {
        payload["text"] = serde_json::json!(item.text);
    }

    // 5-second timeout to avoid blocking the capture loop.
    let config = ureq::config::Config::builder()
        .timeout_global(Some(std::time::Duration::from_secs(5)))
        .build();
    let agent: ureq::Agent = config.into();
    agent
        .post(url)
        .send_json(payload)
        .map_err(|e| anyhow::anyhow!("webhook POST to {}: {}", url, e))?;
    info!("recipe: webhook POST to {} for clip {}", url, clip_id);
    Ok(())
}

fn vault_append(
    vault_path: &Path,
    daily: bool,
    item: &crate::core::store::Item,
) -> Result<()> {
    // TODO(manual): set vault_path in recipe JSON to your actual Obsidian vault.
    let target = if daily {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        vault_path.join(format!("{}.md", today))
    } else {
        vault_path.to_path_buf()
    };
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).context("vault_append: create dir")?;
    }
    let bullet = format!(
        "- [{kind}] {preview} *(from {app})*\n",
        kind = item.primary_kind,
        preview = item.preview,
        app = item.source_app.as_deref().unwrap_or("unknown"),
    );
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&target)
        .with_context(|| format!("vault_append: open {}", target.display()))?;
    f.write_all(bullet.as_bytes())
        .context("vault_append: write bullet")?;
    info!("recipe: appended bullet to {}", target.display());
    Ok(())
}

fn send_notification(title: &str, body: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            r#"display notification "{body}" with title "{title}""#,
            body = body.replace('"', "'"),
            title = title.replace('"', "'"),
        );
        let status = std::process::Command::new("osascript")
            .args(["-e", &script])
            .status()
            .context("spawn osascript for notification")?;
        if !status.success() {
            warn!("recipe notify: osascript exited with {}", status);
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Linux: notify-send (libnotify). Best-effort — not installed everywhere.
        let _ = std::process::Command::new("notify-send")
            .args([title, body])
            .status();
    }
    info!("recipe: notification sent: '{}'", title);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_recipe(dir: &Path, name: &str, json: &str) {
        std::fs::write(dir.join(name), json).unwrap();
    }

    #[test]
    fn load_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let engine = RecipeEngine::load(tmp.path().to_path_buf()).unwrap();
        assert!(engine.recipes.is_empty());
    }

    #[test]
    fn load_valid_recipe() {
        let tmp = TempDir::new().unwrap();
        write_recipe(
            tmp.path(),
            "tag-code.json",
            r#"{
                "id": "tag-code",
                "name": "Tag code",
                "enabled": true,
                "trigger": { "kind": "code" },
                "action": { "type": "tag", "tag": "code" }
            }"#,
        );
        let engine = RecipeEngine::load(tmp.path().to_path_buf()).unwrap();
        assert_eq!(engine.recipes().len(), 1);
        assert_eq!(engine.recipes()[0].id, "tag-code");
    }

    #[test]
    fn skip_malformed_recipe_and_load_rest() {
        let tmp = TempDir::new().unwrap();
        write_recipe(tmp.path(), "bad.json", r#"{"invalid"}"#);
        write_recipe(
            tmp.path(),
            "good.json",
            r#"{
                "id": "good",
                "name": "Good",
                "enabled": true,
                "trigger": {},
                "action": { "type": "tag", "tag": "ok" }
            }"#,
        );
        let engine = RecipeEngine::load(tmp.path().to_path_buf()).unwrap();
        assert_eq!(engine.recipes().len(), 1, "bad recipe should be skipped");
    }

    #[test]
    fn disabled_recipe_not_fired() {
        let tmp = TempDir::new().unwrap();
        write_recipe(
            tmp.path(),
            "disabled.json",
            r#"{
                "id": "noop",
                "name": "Disabled",
                "enabled": false,
                "trigger": {},
                "action": { "type": "tag", "tag": "should-not-appear" }
            }"#,
        );
        let engine = RecipeEngine::load(tmp.path().to_path_buf()).unwrap();

        let db_tmp = TempDir::new().unwrap();
        let store = Store::open(db_tmp.path().join("t.db"), [0u8; 32]).unwrap();
        let id = store.add_clip(crate::core::store::ClipInput {
            text: "hello".into(),
            primary_kind: "text".into(),
            kinds: vec!["text".into()],
            source_app: None,
            window_title: None,
        }).unwrap();

        engine.handle_clip(&store, id);
        let item = store.get_item(id).unwrap().unwrap();
        assert!(
            item.tags.is_empty(),
            "disabled recipe must not tag the clip"
        );
    }

    #[test]
    fn tag_action_end_to_end() {
        let tmp_recipes = TempDir::new().unwrap();
        write_recipe(
            tmp_recipes.path(),
            "tag-code.json",
            r#"{
                "id": "tag-code",
                "name": "Tag code",
                "enabled": true,
                "trigger": { "kind": "code" },
                "action": { "type": "tag", "tag": "code" }
            }"#,
        );
        let engine = RecipeEngine::load(tmp_recipes.path().to_path_buf()).unwrap();

        let db_tmp = TempDir::new().unwrap();
        let store = Store::open(db_tmp.path().join("t.db"), [0u8; 32]).unwrap();
        let id = store.add_clip(crate::core::store::ClipInput {
            text: "fn main() {}".into(),
            primary_kind: "code".into(),
            kinds: vec!["code".into()],
            source_app: None,
            window_title: None,
        }).unwrap();

        engine.handle_clip(&store, id);
        let item = store.get_item(id).unwrap().unwrap();
        assert!(
            item.tags.contains(&"code".to_string()),
            "tag recipe should have tagged the clip"
        );
    }

    #[test]
    fn tag_action_does_not_fire_on_kind_mismatch() {
        let tmp_recipes = TempDir::new().unwrap();
        write_recipe(
            tmp_recipes.path(),
            "tag-code.json",
            r#"{
                "id": "tag-code",
                "name": "Tag code",
                "enabled": true,
                "trigger": { "kind": "code" },
                "action": { "type": "tag", "tag": "code" }
            }"#,
        );
        let engine = RecipeEngine::load(tmp_recipes.path().to_path_buf()).unwrap();

        let db_tmp = TempDir::new().unwrap();
        let store = Store::open(db_tmp.path().join("t.db"), [0u8; 32]).unwrap();
        let id = store.add_clip(crate::core::store::ClipInput {
            text: "https://example.com".into(),
            primary_kind: "url".into(),
            kinds: vec!["url".into()],
            source_app: None,
            window_title: None,
        }).unwrap();

        engine.handle_clip(&store, id);
        let item = store.get_item(id).unwrap().unwrap();
        assert!(item.tags.is_empty(), "url clip should not be tagged by code recipe");
    }

    #[test]
    fn reload_picks_up_new_recipes() {
        let tmp = TempDir::new().unwrap();
        let mut engine = RecipeEngine::load(tmp.path().to_path_buf()).unwrap();
        assert!(engine.recipes().is_empty());

        write_recipe(
            tmp.path(),
            "tag-new.json",
            r#"{
                "id": "tag-new",
                "name": "New",
                "enabled": true,
                "trigger": {},
                "action": { "type": "tag", "tag": "new" }
            }"#,
        );
        engine.reload().unwrap();
        assert_eq!(engine.recipes().len(), 1);
    }
}
