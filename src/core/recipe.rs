//! Recipe / Workflow data model.
//!
//! Recipes are JSON files read from
//! `~/Library/Application Support/clipboard-history-mcp/recipes/*.json`
//! (or `<data_dir>/recipes/*.json` on Linux).
//!
//! Each recipe has a trigger (optional filters on clip attributes) and an
//! action that fires when a newly ingested clip matches.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A named, optionally-enabled workflow recipe.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Recipe {
    /// Unique identifier used as the file stem for logging / dedup.
    pub id: String,
    /// Human-friendly display name.
    pub name: String,
    /// When false the recipe is loaded but never fired.
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub trigger: RecipeTrigger,
    pub action: RecipeAction,
}

fn default_true() -> bool {
    true
}

/// Conditions that must all match for the recipe to fire.
/// All fields are optional; an empty trigger matches every clip.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RecipeTrigger {
    /// Match clips whose `primary_kind` equals this value (e.g. `"code"`, `"url"`, `"image"`).
    pub kind: Option<String>,
    /// Match clips from this app (exact match, case-sensitive).
    pub source_app: Option<String>,
    /// Match clips whose `primary_kind` starts with this prefix.
    pub primary_kind_starts_with: Option<String>,
    /// Match clips whose byte_length exceeds this threshold.
    pub min_byte_length: Option<i64>,
}

impl RecipeTrigger {
    /// Returns `true` if the clip described by the supplied fields matches
    /// every non-None condition in this trigger.
    pub fn matches(
        &self,
        primary_kind: &str,
        source_app: Option<&str>,
        byte_length: i64,
    ) -> bool {
        if let Some(k) = &self.kind {
            if primary_kind != k {
                return false;
            }
        }
        if let Some(app) = &self.source_app {
            if source_app != Some(app.as_str()) {
                return false;
            }
        }
        if let Some(prefix) = &self.primary_kind_starts_with {
            if !primary_kind.starts_with(prefix.as_str()) {
                return false;
            }
        }
        if let Some(min) = self.min_byte_length {
            if byte_length < min {
                return false;
            }
        }
        true
    }
}

/// The action fired when a recipe's trigger matches.
///
/// Uses `#[serde(tag = "type")]` so JSON looks like:
/// ```json
/// { "type": "tag", "tag": "code" }
/// { "type": "webhook_post", "url": "https://…", "include_text": true }
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RecipeAction {
    /// Add a tag to the clip via `Store::tag`.
    Tag { tag: String },
    /// POST the clip preview (and optionally full text) to a webhook URL.
    WebhookPost {
        url: String,
        #[serde(default)]
        include_text: bool,
    },
    /// Append a markdown bullet to a file in an Obsidian vault.
    ///
    /// When `daily` is true the target file is `<vault_path>/<YYYY-MM-DD>.md`.
    /// When false the path is used verbatim.
    ///
    /// TODO(manual): `vault_path` must be configured by the user to point at
    /// their actual Obsidian vault directory.
    VaultAppend {
        vault_path: PathBuf,
        #[serde(default)]
        daily: bool,
    },
    /// Send a macOS (or Linux) system notification.
    Notify { title: String, body: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_matches_kind() {
        let t = RecipeTrigger { kind: Some("code".into()), ..Default::default() };
        assert!(t.matches("code", None, 0));
        assert!(!t.matches("text", None, 0));
    }

    #[test]
    fn trigger_matches_source_app() {
        let t = RecipeTrigger { source_app: Some("Slack".into()), ..Default::default() };
        assert!(t.matches("text", Some("Slack"), 0));
        assert!(!t.matches("text", Some("VS Code"), 0));
        assert!(!t.matches("text", None, 0));
    }

    #[test]
    fn trigger_matches_prefix() {
        let t = RecipeTrigger {
            primary_kind_starts_with: Some("secret".into()),
            ..Default::default()
        };
        assert!(t.matches("secret:aws", None, 0));
        assert!(!t.matches("code", None, 0));
    }

    #[test]
    fn trigger_matches_min_byte_length() {
        let t = RecipeTrigger { min_byte_length: Some(1_000_000), ..Default::default() };
        assert!(t.matches("text", None, 2_000_000));
        assert!(!t.matches("text", None, 500));
    }

    #[test]
    fn empty_trigger_matches_everything() {
        let t = RecipeTrigger::default();
        assert!(t.matches("code", Some("Xcode"), 99));
    }

    #[test]
    fn recipe_roundtrips_json() {
        let json = r#"{
            "id": "tag-code",
            "name": "Tag code",
            "enabled": true,
            "trigger": { "kind": "code" },
            "action": { "type": "tag", "tag": "code" }
        }"#;
        let recipe: Recipe = serde_json::from_str(json).unwrap();
        assert_eq!(recipe.id, "tag-code");
        assert!(recipe.enabled);
        assert!(matches!(recipe.action, RecipeAction::Tag { tag } if tag == "code"));
    }
}
