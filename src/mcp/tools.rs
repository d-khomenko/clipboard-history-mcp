use crate::core::{
    biometry::BiometryGate,
    pasteboard,
    store::{Store},
};
use rmcp::{
    handler::server::wrapper::Parameters,
    schemars,
    tool, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone)]
pub struct ClipboardServer {
    pub store: Arc<Store>,
    pub biometry: Arc<BiometryGate>,
    pub db_path: std::path::PathBuf,
}

// ── Parameter types ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct ListParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub kind: Option<String>,
    pub source_app: Option<String>,
    pub since: Option<i64>,
    pub pinned_only: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetItemParams {
    pub id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    pub query: String,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct GetCodeParams {
    pub language: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct LimitParams {
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct SecretsIndexParams {
    pub kind: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UnlockParams {
    pub id: i64,
    pub reason: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IdParams {
    pub id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PinParams {
    pub id: i64,
    pub pinned: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TagParams {
    pub id: i64,
    pub tag: String,
    pub remove: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ClearParams {
    pub scope: String,
}

// ── Tool impl block ──────────────────────────────────────────────────────────

#[tool_router(server_handler)]
impl ClipboardServer {
    #[tool(description = "List recent clipboard entries, newest first. Secret values are never returned — only metadata.")]
    async fn list_history(&self, Parameters(p): Parameters<ListParams>) -> String {
        let limit = p.limit.unwrap_or(20);
        match self.store.list_with(p.kind.as_deref(), limit, p.offset.unwrap_or(0)) {
            Ok(items) => serde_json::json!({ "count": items.len(), "items": items }).to_string(),
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Fetch one clipboard entry by id. For secrets returns metadata only.")]
    async fn get_item(&self, Parameters(p): Parameters<GetItemParams>) -> String {
        match self.store.get_item(p.id) {
            Ok(Some(item)) => {
                let requires_unlock = item.primary_kind.starts_with("secret:");
                let mut v = serde_json::to_value(&item).unwrap_or(serde_json::Value::Null);
                if requires_unlock {
                    v["requiresUnlock"] = serde_json::json!(true);
                }
                v.to_string()
            }
            Ok(None) => format!("error: not found id={}", p.id),
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Full-text search clipboard history (FTS5 BM25).")]
    async fn search_history(&self, Parameters(p): Parameters<SearchParams>) -> String {
        let limit = p.limit.unwrap_or(20);
        match self.store.search(&p.query, limit) {
            Ok(items) => serde_json::json!({ "count": items.len(), "items": items }).to_string(),
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Return URL clips, deduped by host.")]
    async fn get_urls(&self, Parameters(p): Parameters<LimitParams>) -> String {
        let limit = p.limit.unwrap_or(20);
        match self.store.list_with(Some("url"), 200, 0) {
            Ok(items) => {
                let mut by_host: std::collections::HashMap<String, usize> =
                    std::collections::HashMap::new();
                let items_ref: Vec<_> = items.iter().collect();
                for (idx, item) in items_ref.iter().enumerate() {
                    if let Some(t) = &item.text {
                        if let Ok(u) = url::Url::parse(t) {
                            let host = u.host_str().unwrap_or("").to_string();
                            by_host
                                .entry(host)
                                .and_modify(|cur_idx| {
                                    if item.last_copied_at > items[*cur_idx].last_copied_at {
                                        *cur_idx = idx;
                                    }
                                })
                                .or_insert(idx);
                        }
                    }
                }
                let result: Vec<_> = by_host
                    .values()
                    .take(limit as usize)
                    .map(|&idx| &items[idx])
                    .collect();
                serde_json::json!({ "count": result.len(), "items": result }).to_string()
            }
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Return code clips, optionally filtered by language.")]
    async fn get_code(&self, Parameters(p): Parameters<GetCodeParams>) -> String {
        let limit = p.limit.unwrap_or(20);
        let kind = p.language.as_ref().map(|l| format!("code:{}", l.to_lowercase()));
        let res = match kind {
            Some(k) => self.store.list_with(Some(&k), limit, 0),
            None => self.store.list(limit).map(|all| {
                all.into_iter()
                    .filter(|i| i.primary_kind.starts_with("code:"))
                    .collect()
            }),
        };
        match res {
            Ok(items) => serde_json::json!({ "count": items.len(), "items": items }).to_string(),
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Return JSON clips with parsed structure preview.")]
    async fn get_json(&self, Parameters(p): Parameters<LimitParams>) -> String {
        let limit = p.limit.unwrap_or(20);
        match self.store.list_with(Some("json"), limit, 0) {
            Ok(items) => {
                let parsed: Vec<_> = items
                    .into_iter()
                    .map(|i| {
                        let parsed_val = i
                            .text
                            .as_ref()
                            .and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok());
                        let mut v =
                            serde_json::to_value(&i).unwrap_or(serde_json::Value::Null);
                        v["parsed"] = parsed_val.unwrap_or(serde_json::Value::Null);
                        v
                    })
                    .collect();
                serde_json::json!({ "count": parsed.len(), "items": parsed }).to_string()
            }
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "List secret clip metadata only — no values.")]
    async fn get_secrets_index(&self, Parameters(p): Parameters<SecretsIndexParams>) -> String {
        let filter = p.kind.as_ref().map(|k| format!("secret:{}", k));
        let res = match filter.as_deref() {
            Some(k) => self.store.list_with(Some(k), 200, 0),
            None => self.store.list(500).map(|all| {
                all.into_iter()
                    .filter(|i| i.primary_kind.starts_with("secret:"))
                    .collect()
            }),
        };
        match res {
            Ok(items) => {
                let stripped: Vec<_> = items
                    .into_iter()
                    .map(|i| {
                        serde_json::json!({
                            "id": i.id,
                            "secretKind": i.primary_kind.trim_start_matches("secret:"),
                            "sourceApp": i.source_app,
                            "windowTitle": i.window_title,
                            "firstCopiedAt": i.first_copied_at,
                            "lastCopiedAt": i.last_copied_at,
                        })
                    })
                    .collect();
                serde_json::json!({ "count": stripped.len(), "items": stripped }).to_string()
            }
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Decrypt and return a stored secret. Reason argument is mandatory and audited. Touch ID gated.")]
    async fn unlock_secret(&self, Parameters(p): Parameters<UnlockParams>) -> String {
        tracing::info!("unlock_secret id={} reason={:?}", p.id, p.reason);
        match self
            .biometry
            .evaluate(&format!("Reveal stored secret #{}: {}", p.id, p.reason))
        {
            Ok(false) => return "error: biometric authentication failed".into(),
            Err(e) => return format!("error: biometry error: {}", e),
            Ok(true) => {}
        }
        match self.store.unlock_secret(p.id) {
            Ok(value) => {
                let last_chars: String = value
                    .chars()
                    .rev()
                    .take(6)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect();
                serde_json::json!({ "value": value, "lastChars": last_chars }).to_string()
            }
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Restore a clip to the system clipboard. Refuses secrets — use unlock_secret first.")]
    async fn copy_item(&self, Parameters(p): Parameters<IdParams>) -> String {
        match self.store.get_item(p.id) {
            Ok(Some(item)) => {
                if item.text.is_none() {
                    return serde_json::json!({
                        "error": "cannot restore secret directly",
                        "requiresUnlock": true
                    })
                    .to_string();
                }
                if let Err(e) =
                    pasteboard::write_clipboard(item.text.as_deref().unwrap())
                {
                    return format!("error: {}", e);
                }
                let _ = self.store.bump_paste(p.id);
                serde_json::json!({ "ok": true, "id": p.id, "length": item.length })
                    .to_string()
            }
            Ok(None) => format!("error: not found id={}", p.id),
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Pin or unpin a clip.")]
    async fn pin_item(&self, Parameters(p): Parameters<PinParams>) -> String {
        match self.store.pin(p.id, p.pinned) {
            Ok(_) => r#"{"ok":true}"#.into(),
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Add or remove a tag on a clip.")]
    async fn tag_item(&self, Parameters(p): Parameters<TagParams>) -> String {
        let r = if p.remove.unwrap_or(false) {
            self.store.untag(p.id, &p.tag)
        } else {
            self.store.tag(p.id, &p.tag)
        };
        match r {
            Ok(_) => r#"{"ok":true}"#.into(),
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Hard-delete a clip (and its secrets row if applicable).")]
    async fn delete_item(&self, Parameters(p): Parameters<IdParams>) -> String {
        match self.store.delete(p.id) {
            Ok(_) => r#"{"ok":true}"#.into(),
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Clear history. scope: 'all' | 'older_than_days:N' | 'kind:K'")]
    async fn clear_history(&self, Parameters(p): Parameters<ClearParams>) -> String {
        let r = if p.scope == "all" {
            self.store.clear_all()
        } else if let Some(rest) = p.scope.strip_prefix("older_than_days:") {
            rest.parse::<i64>()
                .map_err(anyhow::Error::from)
                .and_then(|d| Ok(self.store.clear_older_than_days(d)?))
        } else if let Some(rest) = p.scope.strip_prefix("kind:") {
            self.store.clear_kind(rest)
        } else {
            return format!("error: unknown scope '{}'", p.scope);
        };
        match r {
            Ok(n) => serde_json::json!({"ok": true, "removed": n}).to_string(),
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Counts by kind, oldest/newest, db size.")]
    async fn get_stats(&self, _: Parameters<()>) -> String {
        match self.store.stats() {
            Ok(s) => {
                let size = std::fs::metadata(&self.db_path)
                    .ok()
                    .map(|m| m.len())
                    .unwrap_or(0);
                serde_json::json!({
                    "count": s.count,
                    "oldest": s.oldest,
                    "newest": s.newest,
                    "dbPath": self.db_path,
                    "sizeBytes": size,
                })
                .to_string()
            }
            Err(e) => format!("error: {}", e),
        }
    }

    #[tool(description = "Daemon status (running, pid).")]
    async fn daemon_status(&self, _: Parameters<()>) -> String {
        let pid_file = self.db_path.parent().unwrap().join("daemon.pid");
        if !pid_file.exists() {
            return r#"{"running":false}"#.into();
        }
        let Ok(s) = std::fs::read_to_string(&pid_file) else {
            return r#"{"running":false}"#.into();
        };
        let Ok(pid) = s.trim().parse::<i32>() else {
            return r#"{"running":false}"#.into();
        };
        let alive = unsafe { libc::kill(pid, 0) } == 0;
        if alive {
            format!(r#"{{"running":true,"pid":{}}}"#, pid)
        } else {
            r#"{"running":false}"#.into()
        }
    }
}
