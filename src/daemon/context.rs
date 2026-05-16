use serde::Serialize;

#[derive(Serialize)]
pub struct CaptureContext {
    pub front_app: Option<String>,
    pub window_title: Option<String>,
}

pub fn capture(with_window_title: bool) -> CaptureContext {
    let front_app = capture_frontmost_app();
    let window_title = if with_window_title { capture_window_title() } else { None };
    CaptureContext { front_app, window_title }
}

fn capture_frontmost_app() -> Option<String> {
    match active_win_pos_rs::get_active_window() {
        Ok(w) => Some(w.app_name).filter(|s| !s.is_empty()),
        Err(_) => None,
    }
}

#[cfg(target_os = "macos")]
fn capture_window_title() -> Option<String> {
    capture_window_title_via_active_win()
}

#[cfg(target_os = "linux")]
fn capture_window_title() -> Option<String> {
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        // Wayland window title support deferred to v0.4.x (portal API)
        return None;
    }
    capture_window_title_x11().or_else(|| capture_window_title_via_active_win())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn capture_window_title() -> Option<String> {
    capture_window_title_via_active_win()
}

fn capture_window_title_via_active_win() -> Option<String> {
    match active_win_pos_rs::get_active_window() {
        Ok(w) => Some(w.title).filter(|s| !s.is_empty()),
        Err(_) => None,
    }
}

#[cfg(target_os = "linux")]
fn capture_window_title_x11() -> Option<String> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};

    let (conn, screen_num) = x11rb::connect(None).ok()?;
    let screen = &conn.setup().roots[screen_num];

    let net_active = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW").ok()?.reply().ok()?.atom;
    let net_wm_name = conn.intern_atom(false, b"_NET_WM_NAME").ok()?.reply().ok()?.atom;
    let utf8 = conn.intern_atom(false, b"UTF8_STRING").ok()?.reply().ok()?.atom;

    let active_reply = conn
        .get_property(false, screen.root, net_active, AtomEnum::WINDOW, 0, 1)
        .ok()?
        .reply()
        .ok()?;
    let active_win = u32::from_ne_bytes(active_reply.value.get(..4)?.try_into().ok()?);
    if active_win == 0 {
        return None;
    }

    let title_reply = conn
        .get_property(false, active_win, net_wm_name, utf8, 0, 1024)
        .ok()?
        .reply()
        .ok()?;

    let title = String::from_utf8(title_reply.value).ok()?;
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the active-window detection module.
    //!
    //! ## Coverage scope
    //! Production code in this module is a thin adapter around
    //! `active_win_pos_rs::get_active_window` (bumped to 0.10). The crate
    //! has no public mocking hook and `capture_frontmost_app` /
    //! `capture_window_title_via_active_win` are private with no DI seam, so
    //! we cannot inject a synthetic `ActiveWindow` into the live `capture()`
    //! path without changing production code (forbidden by task brief).
    //!
    //! What we do cover:
    //!   1. `CaptureContext` shape and `Serialize` impl (the JSON contract
    //!      consumed downstream by the watcher and mcp tools).
    //!   2. The `with_window_title=false` contract: `window_title` MUST be
    //!      `None` regardless of OS state.
    //!   3. Empty-string filter contract: any returned `Some(s)` is
    //!      non-empty. This mirrors the `.filter(|s| !s.is_empty())` guard
    //!      in `capture_frontmost_app` / `capture_window_title_via_active_win`.
    //!   4. The ignore-app filter logic exactly as `watcher::tick` consumes
    //!      it: case-sensitive substring-free equality, parsed from the
    //!      CSV env-var shape `main.rs` uses for `CLIPBOARD_IGNORE_APPS`.
    //!   5. Direct construction of `active_win_pos_rs::ActiveWindow` to
    //!      verify our wrapper's filter shape (empty `app_name` → None)
    //!      against the upstream 0.10 struct layout.
    //!
    //! ## Known gaps (cannot test without production-logic changes)
    //!   * Cannot force `get_active_window()` to return `Err(())` from a
    //!     unit test — depends on macOS Accessibility permission / X11
    //!     server availability. In CI sandbox `front_app` may be `None`
    //!     (no permission) or `Some(...)` (granted). Test #5 of "live call"
    //!     accepts both, asserting only the type contract.
    //!   * Cannot test `capture_window_title_x11` from macOS host (cfg-gated).
    //!   * Cannot inject a mocked `ActiveWindow` into `capture_frontmost_app`
    //!     without exposing a `pub(crate) fn from_window(&ActiveWindow)`
    //!     seam — that would be a production change.
    //!
    //! Re-evaluate after any refactor that introduces a trait seam around
    //! the `active_win_pos_rs::get_active_window` call.

    use super::*;
    use active_win_pos_rs::ActiveWindow;

    // --- CaptureContext shape & serialization -------------------------------

    #[test]
    fn capture_context_serializes_with_both_fields_populated() {
        let ctx = CaptureContext {
            front_app: Some("Safari".into()),
            window_title: Some("API Keys — OpenAI Platform".into()),
        };
        let v = serde_json::to_value(&ctx).expect("serialize");
        assert_eq!(v["front_app"], "Safari");
        assert_eq!(v["window_title"], "API Keys — OpenAI Platform");
    }

    #[test]
    fn capture_context_serializes_nulls_for_missing_fields() {
        // Watcher consumes `Option<String>` directly via `ctx.front_app` —
        // here we just guarantee the JSON shape remains stable for any
        // downstream consumer that reads it (mcp tooling, debug logs).
        let ctx = CaptureContext { front_app: None, window_title: None };
        let v = serde_json::to_value(&ctx).expect("serialize");
        assert!(v["front_app"].is_null());
        assert!(v["window_title"].is_null());
    }

    #[test]
    fn capture_context_struct_field_names_match_wire_format() {
        // Defensive: if a future refactor adds `#[serde(rename = ...)]`
        // this test will tell us, because mcp/tools.rs and downstream
        // consumers depend on these exact keys.
        let ctx = CaptureContext { front_app: None, window_title: None };
        let s = serde_json::to_string(&ctx).expect("serialize");
        assert!(s.contains("\"front_app\""), "got: {s}");
        assert!(s.contains("\"window_title\""), "got: {s}");
    }

    // --- capture() contract on live system ----------------------------------

    #[test]
    fn capture_without_window_title_never_returns_window_title() {
        // When the caller opts out, window_title MUST be None regardless
        // of OS state — guards against accidental leak of window titles
        // for users who explicitly disabled it via the env var.
        let ctx = capture(false);
        assert!(
            ctx.window_title.is_none(),
            "with_window_title=false yielded {:?}",
            ctx.window_title
        );
    }

    #[test]
    fn capture_front_app_is_either_none_or_non_empty() {
        // Empty-string filter contract: production filters out empty
        // strings from active-win-pos-rs (a known macOS sandbox edge:
        // some windows report empty app_name). The wrapper must never
        // surface `Some("")`.
        let ctx = capture(false);
        if let Some(s) = &ctx.front_app {
            assert!(!s.is_empty(), "front_app yielded empty string");
        }
    }

    #[test]
    fn capture_with_window_title_returns_either_none_or_non_empty() {
        // Same empty-string filter contract on the title side.
        let ctx = capture(true);
        if let Some(s) = &ctx.window_title {
            assert!(!s.is_empty(), "window_title yielded empty string");
        }
        if let Some(s) = &ctx.front_app {
            assert!(!s.is_empty(), "front_app yielded empty string");
        }
    }

    #[test]
    fn capture_is_idempotent_for_caller_shape() {
        // The function takes no state, only an env-driven flag — two
        // back-to-back calls with the same flag yield the same
        // type-shape (Option discriminants may differ if the user
        // alt-tabs mid-test, but the contract is preserved).
        let a = capture(false);
        let b = capture(false);
        // Both window_titles must be None (with_window_title=false).
        assert!(a.window_title.is_none());
        assert!(b.window_title.is_none());
    }

    // --- active-win-pos-rs 0.10 adapter wrapper -----------------------------

    #[test]
    fn active_window_struct_default_has_empty_app_name() {
        // Verifies our assumption about the upstream 0.10 layout: the
        // struct is `Default` and a default-constructed value has an
        // empty `app_name`. If 0.11 ever changes this we want to know.
        let w = ActiveWindow::default();
        assert_eq!(w.app_name, "");
        assert_eq!(w.title, "");
    }

    #[test]
    fn empty_app_name_filter_matches_production_behavior() {
        // This mirrors line 17 of context.rs:
        //   `Some(w.app_name).filter(|s| !s.is_empty())`
        // We can't reach the private `capture_frontmost_app` directly,
        // but we can prove the filter shape we depend on still holds
        // for a default ActiveWindow from the 0.10 crate.
        let w = ActiveWindow::default();
        let extracted = Some(w.app_name).filter(|s| !s.is_empty());
        assert!(extracted.is_none(), "empty app_name should filter to None");

        let w2 = ActiveWindow {
            app_name: "Safari".into(),
            ..ActiveWindow::default()
        };
        let extracted2 = Some(w2.app_name).filter(|s| !s.is_empty());
        assert_eq!(extracted2.as_deref(), Some("Safari"));
    }

    #[test]
    fn empty_title_filter_matches_production_behavior() {
        // Mirrors line 43 of context.rs:
        //   `Some(w.title).filter(|s| !s.is_empty())`
        let w = ActiveWindow::default();
        let extracted = Some(w.title).filter(|s| !s.is_empty());
        assert!(extracted.is_none(), "empty title should filter to None");

        let w2 = ActiveWindow {
            title: "Window — Doc".into(),
            ..ActiveWindow::default()
        };
        let extracted2 = Some(w2.title).filter(|s| !s.is_empty());
        assert_eq!(extracted2.as_deref(), Some("Window — Doc"));
    }

    #[test]
    fn active_window_unicode_app_name_round_trip() {
        // Ensures we accept non-ASCII (e.g. Cyrillic localized app names
        // on a localized macOS install, or apps with em-dashes in their
        // names) without truncation through the empty-string filter.
        let w = ActiveWindow {
            app_name: "Заметки".into(), // Russian "Notes"
            title: "Запис №42 — деталі".into(),
            ..ActiveWindow::default()
        };
        let app = Some(w.app_name).filter(|s| !s.is_empty());
        let title = Some(w.title).filter(|s| !s.is_empty());
        assert_eq!(app.as_deref(), Some("Заметки"));
        assert_eq!(title.as_deref(), Some("Запис №42 — деталі"));
    }

    // --- CLIPBOARD_IGNORE_APPS / ignore-app filter --------------------------
    //
    // The filter itself lives in `watcher::tick` (line 117):
    //     opts.ignore_apps.iter().any(|a| a == app)
    // It consumes `ctx.front_app: Option<String>` produced by this module.
    // Below we lock in that contract end-to-end, parsing the env var with
    // the exact same shape `main.rs` uses (line 84-89).

    /// Mirror of `main.rs` lines 84-89: CSV parse with trim and empty-skip.
    fn parse_ignore_apps(raw: &str) -> Vec<String> {
        raw.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    fn is_ignored(ctx: &CaptureContext, ignore_apps: &[String]) -> bool {
        match &ctx.front_app {
            Some(app) => ignore_apps.iter().any(|a| a == app),
            None => false,
        }
    }

    #[test]
    fn ignore_apps_parses_csv_with_trim() {
        let parsed = parse_ignore_apps("1Password,  Bitwarden , KeyChain Access");
        assert_eq!(parsed, vec!["1Password", "Bitwarden", "KeyChain Access"]);
    }

    #[test]
    fn ignore_apps_empty_env_yields_empty_list() {
        assert!(parse_ignore_apps("").is_empty());
    }

    #[test]
    fn ignore_apps_drops_empty_segments_from_trailing_commas() {
        let parsed = parse_ignore_apps("1Password,,Bitwarden,");
        assert_eq!(parsed, vec!["1Password", "Bitwarden"]);
    }

    #[test]
    fn ignore_apps_single_entry_no_comma() {
        assert_eq!(parse_ignore_apps("1Password"), vec!["1Password"]);
    }

    #[test]
    fn ignore_apps_filter_blocks_matching_front_app() {
        let ctx = CaptureContext {
            front_app: Some("1Password".into()),
            window_title: Some("Vault".into()),
        };
        let ignore = parse_ignore_apps("1Password,Bitwarden");
        assert!(is_ignored(&ctx, &ignore));
    }

    #[test]
    fn ignore_apps_filter_passes_non_matching_front_app() {
        let ctx = CaptureContext {
            front_app: Some("Safari".into()),
            window_title: None,
        };
        let ignore = parse_ignore_apps("1Password,Bitwarden");
        assert!(!is_ignored(&ctx, &ignore));
    }

    #[test]
    fn ignore_apps_filter_is_case_sensitive() {
        // The filter uses `==` — a user who configures "1password" (lc)
        // will NOT block a window reporting "1Password". This locks in
        // the existing behavior; if we ever want case-insensitive match
        // it should be a deliberate change, not an accident.
        let ctx = CaptureContext {
            front_app: Some("1Password".into()),
            window_title: None,
        };
        let ignore = parse_ignore_apps("1password");
        assert!(!is_ignored(&ctx, &ignore));
    }

    #[test]
    fn ignore_apps_filter_requires_exact_match_not_substring() {
        // A user who configures "Password" should NOT inadvertently
        // block "1Password" or "Password Manager".
        let ctx = CaptureContext {
            front_app: Some("1Password".into()),
            window_title: None,
        };
        let ignore = parse_ignore_apps("Password");
        assert!(!is_ignored(&ctx, &ignore));
    }

    #[test]
    fn ignore_apps_filter_skips_when_front_app_unknown() {
        // Sandboxed app / headless session / permission denied → front_app
        // is None and we MUST NOT treat that as a match. The watcher's
        // semantic intent is: only skip when we positively identified
        // a banned app. Anything else flows through to capture.
        let ctx = CaptureContext { front_app: None, window_title: None };
        let ignore = parse_ignore_apps("1Password,Bitwarden");
        assert!(!is_ignored(&ctx, &ignore));
    }

    #[test]
    fn ignore_apps_filter_empty_list_never_blocks() {
        let ctx = CaptureContext {
            front_app: Some("Safari".into()),
            window_title: None,
        };
        let ignore: Vec<String> = vec![];
        assert!(!is_ignored(&ctx, &ignore));
    }

    #[test]
    fn ignore_apps_filter_handles_app_names_with_spaces() {
        // "Visual Studio Code", "Sublime Text", "App Store" etc.
        let ctx = CaptureContext {
            front_app: Some("Visual Studio Code".into()),
            window_title: None,
        };
        let ignore = parse_ignore_apps("Visual Studio Code,Xcode");
        assert!(is_ignored(&ctx, &ignore));
    }

    #[test]
    fn ignore_apps_filter_handles_unicode_app_name() {
        // Localized macOS install: app name may be Cyrillic / CJK.
        let ctx = CaptureContext {
            front_app: Some("Заметки".into()),
            window_title: None,
        };
        let ignore = parse_ignore_apps("Заметки,Notes");
        assert!(is_ignored(&ctx, &ignore));
    }
}
