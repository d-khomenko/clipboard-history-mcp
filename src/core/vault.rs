//! Daemon-side mirror of captured non-secret clips into an Obsidian vault.
//!
//! Hooked from `src/daemon/watcher.rs` after `store.add_clip()` returns.
//! Secrets never reach this module — they take the `add_secret` branch.
//!
//! See `docs/superpowers/specs/2026-05-06-vault-mirror-design.md` for design.

use anyhow::Result;
use chrono::{DateTime, Local};
use std::path::{Path, PathBuf};

/// One captured clip, in the shape the vault writer needs.
///
/// Borrowed from the watcher to avoid cloning the clip text just to format it.
pub struct MirrorItem<'a> {
    pub id: i64,
    pub primary_kind: &'a str,
    pub source_app: Option<&'a str>,
    pub window_title: Option<&'a str>,
    pub text: &'a str,
    pub captured: DateTime<Local>,
    /// "text" | "image" | "file"
    pub payload_kind: &'a str,
    /// Relative path under `data_dir/blobs/` for image/file clips; None
    /// for text. The vault writer reads this blob and copies it next to
    /// the sidecar.
    pub blob_relative_path: Option<&'a str>,
    /// MIME type for image/file clips; None for text.
    pub mime_type: Option<&'a str>,
}

/// Configured mirror writer rooted at an Obsidian vault directory.
///
/// `root` is created lazily on first write — install does not require the
/// directory to exist yet.
pub struct VaultMirror {
    root: PathBuf,
}

impl VaultMirror {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Write sidecar + append to daily note. Errors are returned but the
    /// daemon's caller is expected to log + continue, not crash.
    pub fn write(&self, item: &MirrorItem<'_>) -> Result<()> {
        let slug = slug_for_filename(item);

        let blob_link = if let (Some(rel), Some(_)) = (item.blob_relative_path, item.mime_type) {
            // Resolve the source blob in the daemon's data_dir, then copy it
            // to the vault month folder under a name that mirrors the sidecar.
            let src = crate::core::blobs::absolute_path(rel);
            let extension = std::path::Path::new(rel)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("bin")
                .to_string();
            let blob_filename = format!("{}.{}", slug, extension);
            // Compute month_folder eagerly so we have a destination for the
            // blob copy. (The sidecar path computed later uses the same
            // month_folder.)
            let month_folder_for_blob = self
                .root
                .join("clipboard")
                .join(item.captured.format("%Y-%m").to_string());
            std::fs::create_dir_all(&month_folder_for_blob)?;
            let dst = month_folder_for_blob.join(&blob_filename);
            // Copy the blob into the vault. We copy (not symlink) so the
            // vault is portable: a user can sync the vault folder without
            // dragging the daemon's data_dir along.
            std::fs::copy(&src, &dst)?;
            Some(blob_filename)
        } else {
            None
        };

        // Sidecar at <root>/clipboard/YYYY-MM/<id>-<kind>-<slug>.md
        let month_folder = self
            .root
            .join("clipboard")
            .join(item.captured.format("%Y-%m").to_string());
        let sidecar_path = month_folder.join(format!("{}.md", slug));

        let content = format!(
            "{}\n\n{}\n",
            format_frontmatter(item),
            format_body(item, blob_link.as_deref())
        );
        atomic_write(&sidecar_path, &content)?;

        // Daily note bullet: `- HH:MM:SS [[<slug>|<kind> from <source>]]`
        let daily_path = self
            .root
            .join("daily")
            .join(format!("{}.md", item.captured.format("%Y-%m-%d")));
        let source = item.source_app.unwrap_or("unknown");
        let bullet = format!(
            "- {} [[{}|{} from {}]]",
            item.captured.format("%H:%M:%S"),
            slug,
            item.primary_kind,
            source
        );
        append_daily(&daily_path, &bullet)?;

        Ok(())
    }
}

// --- internal helpers (private — exercised via the public `write` method
//     and the unit tests below) ---

fn slug_for_filename(item: &MirrorItem<'_>) -> String {
    let kind = item.primary_kind.replace(':', "_");

    // First 60 chars of preview, then ASCII-fold + collapse non-alphanumerics.
    // Per spec 4.3 "Empty preview → omitted (filename ends at kind)" — there is
    // no behavioural distinction between "all whitespace" and "all non-ASCII":
    // both slugify to empty and both produce the two-part `<id>-<kind>` form.
    let mut buf = String::with_capacity(60);
    let mut last_was_dash = false;
    for ch in item.text.chars().take(60) {
        if ch.is_ascii_alphanumeric() {
            buf.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            buf.push('-');
            last_was_dash = true;
        }
    }
    let preview_slug = buf.trim_matches('-');

    if preview_slug.is_empty() {
        format!("{}-{}", item.id, kind)
    } else {
        format!("{}-{}-{}", item.id, kind, preview_slug)
    }
}

fn format_frontmatter(item: &MirrorItem<'_>) -> String {
    let mut s = String::new();
    s.push_str("---\n");
    s.push_str(&format!("id: {}\n", item.id));
    s.push_str(&format!("kind: {}\n", item.primary_kind));
    s.push_str(&format!("payload_kind: {}\n", item.payload_kind));
    if let Some(mime) = item.mime_type {
        s.push_str(&format!("mime_type: {}\n", mime));
    }
    // Reserved per spec 4.1; populated when classifier surfaces overlapping
    // kinds. Always emitted (even empty) so downstream Obsidian dataview
    // queries can rely on the field's presence.
    s.push_str("secondary_kinds: []\n");
    if let Some(app) = item.source_app {
        s.push_str(&format!("source: {}\n", app));
    }
    if let Some(title) = item.window_title {
        // YAML 1.2 §7.3.1 double-quoted scalar: escape `\` and `"`, plus the
        // C0 controls that can appear in real macOS window titles
        // (`\n`/`\r`/`\t` show up in terminal emulators and breadcrumb-style
        // titles). Unescaped `\n` would break the frontmatter block —
        // js-yaml-based parsers (including Obsidian's) silently drop all
        // metadata in that case. Order matters: backslash first, then the
        // remaining characters can be inserted as their escape sequences
        // without re-escaping the freshly-introduced backslashes.
        let escaped = title
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t");
        s.push_str(&format!("window_title: \"{}\"\n", escaped));
    }
    s.push_str(&format!("captured: {}\n", item.captured.to_rfc3339()));
    s.push_str(&format!("size: {}\n", item.text.len()));
    s.push_str("---\n");
    s
}

fn format_body(item: &MirrorItem<'_>, blob_filename: Option<&str>) -> String {
    if let Some(lang) = item.primary_kind.strip_prefix("code:") {
        return format!("```{}\n{}\n```", lang, item.text);
    }
    match item.payload_kind {
        "image" => {
            // Relative-path markdown image link (vault-relative).
            // Obsidian renders it inline.
            let link = blob_filename.unwrap_or("");
            format!("![Captured image]({})", link)
        }
        "file" => {
            format!(
                "- [`{}`]({})",
                blob_filename.unwrap_or("file"),
                blob_filename.unwrap_or("")
            )
        }
        _ => item.text.to_string(),
    }
}

fn atomic_write(path: &Path, contents: &str) -> Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let pid = std::process::id();
    let tmp = path.with_file_name(format!(
        "{}.{}.tmp",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("write"),
        pid
    ));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(contents.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn append_daily(path: &Path, line: &str) -> Result<()> {
    use std::fmt::Write as _;

    // Parse the date from the filename stem (e.g. "2026-05-06").
    let date_label = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let header = format!("## Clipboard captures · {}", date_label);

    let existing = match std::fs::read_to_string(path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };

    let new_content = match existing {
        // Case 1: file doesn't exist
        None => format!("{}\n\n{}\n", header, line),

        // Case 3 (common): section already there
        Some(s) if s.contains(&header) => {
            // Find the section, then find the next H2 (or end of file).
            // Insert `line` immediately before the next H2 (or at EOF).
            let Some(section_start) = s.find(&header) else {
                // Unreachable given the if-guard above, but defensive against
                // future edits that drop the guard. Fall through to creating
                // the section from scratch.
                return atomic_write(
                    path,
                    &format!("{}\n\n{}\n{}", s.trim_end(), header, line),
                );
            };
            let after_header = section_start + header.len();
            let next_h2 = s[after_header..]
                .find("\n## ")
                .map(|i| after_header + i + 1) // position of '#' of next header
                .unwrap_or(s.len());
            // Trim trailing whitespace inside the section so we don't pile up
            // blank lines on repeated appends.
            let mut before = s[..next_h2].trim_end().to_string();
            let after = &s[next_h2..];
            let _ = writeln!(before, "\n{}", line);
            if after.is_empty() {
                before
            } else {
                format!("{}\n{}", before, after)
            }
        }

        // Case 2: file exists but no captures header — append a fresh section
        Some(s) => {
            let mut out = s;
            if !out.ends_with('\n') {
                out.push('\n');
            }
            let _ = writeln!(out, "\n{}\n\n{}", header, line);
            out
        }
    };

    atomic_write(path, &new_content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn item_with_text<'a>(text: &'a str, primary_kind: &'a str) -> MirrorItem<'a> {
        MirrorItem {
            id: 142,
            primary_kind,
            source_app: Some("Safari"),
            window_title: Some("API Keys — OpenAI Platform"),
            text,
            captured: Local
                .with_ymd_and_hms(2026, 5, 6, 4, 32, 3)
                .single()
                .unwrap(),
            payload_kind: "text",
            blob_relative_path: None,
            mime_type: None,
        }
    }

    // --- slug_for_filename ---

    #[test]
    fn slug_basic_url() {
        let item = item_with_text("https://platform.openai.com/api-keys", "url");
        assert_eq!(
            slug_for_filename(&item),
            "142-url-https-platform-openai-com-api-keys"
        );
    }

    #[test]
    fn slug_replaces_colon_in_kind() {
        let item = item_with_text("def authenticate():", "code:python");
        assert!(slug_for_filename(&item).starts_with("142-code_python-"));
    }

    #[test]
    fn slug_collapses_runs_of_special_chars() {
        let item = item_with_text("foo!!!  ???bar", "text");
        assert_eq!(slug_for_filename(&item), "142-text-foo-bar");
    }

    #[test]
    fn slug_caps_preview_at_60_chars() {
        let long = "a".repeat(200);
        let item = item_with_text(&long, "text");
        let s = slug_for_filename(&item);
        assert!(s.len() <= 69, "got {} chars: {}", s.len(), s);
    }

    #[test]
    fn slug_strips_leading_and_trailing_dashes() {
        let item = item_with_text("---hello---", "text");
        assert_eq!(slug_for_filename(&item), "142-text-hello");
    }

    #[test]
    fn slug_handles_unicode() {
        // All non-ASCII content slugifies to empty after the alphanumeric
        // filter. Per spec 4.3 "Empty preview → omitted", this collapses to
        // the two-part form with no trailing dash — same as whitespace-only
        // input. There is no behavioural distinction between "had content
        // that all dropped" vs "had no content at all".
        let item = item_with_text("Привіт світ", "text");
        assert_eq!(slug_for_filename(&item), "142-text");
    }

    #[test]
    fn slug_preserves_ascii_within_mixed_unicode() {
        // Mixed input: ASCII parts survive, non-ASCII collapse to dashes
        // which then collapse to single separators between ASCII runs.
        let item = item_with_text("Привіт hello world", "text");
        assert_eq!(slug_for_filename(&item), "142-text-hello-world");
    }

    #[test]
    fn slug_empty_preview_omits_suffix() {
        let item = item_with_text("   \n\t  ", "text");
        assert_eq!(slug_for_filename(&item), "142-text");
    }

    // --- format_frontmatter ---

    #[test]
    fn frontmatter_minimal_shape() {
        let item = item_with_text("hello", "text");
        let fm = format_frontmatter(&item);
        assert!(fm.starts_with("---\n"));
        assert!(fm.trim_end().ends_with("\n---"));
        assert!(fm.contains("id: 142"));
        assert!(fm.contains("kind: text"));
        assert!(fm.contains("source: Safari"));
        assert!(fm.contains(r#"window_title: "API Keys — OpenAI Platform""#));
        assert!(fm.contains("size: 5")); // "hello" = 5 bytes
    }

    #[test]
    fn frontmatter_omits_missing_optional_fields() {
        let item = MirrorItem {
            id: 1,
            primary_kind: "text",
            source_app: None,
            window_title: None,
            text: "x",
            captured: Local.with_ymd_and_hms(2026, 5, 6, 4, 32, 3).single().unwrap(),
            payload_kind: "text",
            blob_relative_path: None,
            mime_type: None,
        };
        let fm = format_frontmatter(&item);
        assert!(!fm.contains("source:"));
        assert!(!fm.contains("window_title:"));
    }

    #[test]
    fn frontmatter_quotes_window_title_with_double_quote() {
        let item = MirrorItem {
            id: 1,
            primary_kind: "text",
            source_app: None,
            window_title: Some(r#"He said "hi""#),
            text: "x",
            captured: Local.with_ymd_and_hms(2026, 5, 6, 4, 32, 3).single().unwrap(),
            payload_kind: "text",
            blob_relative_path: None,
            mime_type: None,
        };
        let fm = format_frontmatter(&item);
        assert!(fm.contains(r#"window_title: "He said \"hi\"""#));
    }

    #[test]
    fn frontmatter_escapes_backslash_and_quote_combined() {
        // Real-world Windows-path-like title: tests that backslash escape
        // happens BEFORE quote escape so we don't double-escape backslashes
        // introduced by the quote replacement.
        let item = MirrorItem {
            id: 1,
            primary_kind: "text",
            source_app: None,
            window_title: Some(r#"C:\path "quoted""#),
            text: "x",
            captured: Local.with_ymd_and_hms(2026, 5, 6, 4, 32, 3).single().unwrap(),
            payload_kind: "text",
            blob_relative_path: None,
            mime_type: None,
        };
        let fm = format_frontmatter(&item);
        assert!(
            fm.contains(r#"window_title: "C:\\path \"quoted\"""#),
            "got: {fm}"
        );
    }

    #[test]
    fn frontmatter_escapes_control_characters_in_title() {
        // Terminal emulators and some apps put tabs/newlines in window titles.
        // An unescaped `\n` breaks the YAML block — Obsidian's js-yaml parser
        // silently drops all metadata in that case.
        let item = MirrorItem {
            id: 1,
            primary_kind: "text",
            source_app: None,
            window_title: Some("line1\nline2\twith tab\rcr"),
            text: "x",
            captured: Local.with_ymd_and_hms(2026, 5, 6, 4, 32, 3).single().unwrap(),
            payload_kind: "text",
            blob_relative_path: None,
            mime_type: None,
        };
        let fm = format_frontmatter(&item);
        assert!(
            fm.contains(r#"window_title: "line1\nline2\twith tab\rcr""#),
            "got: {fm}"
        );
        let title_line = fm.lines().find(|l| l.starts_with("window_title:")).unwrap();
        assert!(!title_line.contains('\t'));
        assert!(!title_line.contains('\r'));
    }

    #[test]
    fn frontmatter_captured_is_rfc3339_with_local_offset() {
        let item = item_with_text("x", "text");
        let fm = format_frontmatter(&item);
        let captured_line = fm.lines().find(|l| l.starts_with("captured:")).unwrap();
        assert!(captured_line.contains("2026-05-06T04:32:03"));
        let offset = &captured_line[captured_line.len() - 6..];
        assert!(
            (offset.starts_with('+') || offset.starts_with('-')) && offset.contains(':'),
            "expected RFC3339 offset, got {:?}",
            offset
        );
    }

    #[test]
    fn frontmatter_size_is_byte_count_not_char_count() {
        // "Привіт" is 6 chars but 12 bytes in UTF-8.
        let item = item_with_text("Привіт", "text");
        let fm = format_frontmatter(&item);
        assert!(fm.contains("size: 12"), "got: {}", fm);
    }

    #[test]
    fn frontmatter_always_emits_secondary_kinds() {
        let item = item_with_text("hello", "text");
        let fm = format_frontmatter(&item);
        assert!(fm.contains("secondary_kinds: []"));
    }

    // --- format_body ---

    #[test]
    fn body_plain_text_is_literal() {
        let item = item_with_text("https://example.com/foo", "url");
        assert_eq!(format_body(&item, None), "https://example.com/foo");
    }

    #[test]
    fn body_code_kind_gets_fenced() {
        let item = item_with_text("def f(): pass", "code:python");
        assert_eq!(format_body(&item, None), "```python\ndef f(): pass\n```");
    }

    #[test]
    fn body_code_rust_kind() {
        let item = item_with_text("fn main() {}", "code:rust");
        assert_eq!(format_body(&item, None), "```rust\nfn main() {}\n```");
    }

    #[test]
    fn body_code_unknown_lang_after_colon() {
        let item = item_with_text("foo", "code:zzz");
        assert_eq!(format_body(&item, None), "```zzz\nfoo\n```");
    }

    #[test]
    fn body_does_not_escape_text() {
        let item = item_with_text("# header *bold* [link](url)", "text");
        assert_eq!(format_body(&item, None), "# header *bold* [link](url)");
    }

    // --- atomic_write ---

    #[test]
    fn atomic_write_creates_file_with_contents() {
        let tmp = tempfile::TempDir::new().unwrap();
        let target = tmp.path().join("subdir/note.md");
        atomic_write(&target, "hello world").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "hello world");
    }

    #[test]
    fn atomic_write_creates_parent_dirs() {
        let tmp = tempfile::TempDir::new().unwrap();
        let target = tmp.path().join("a/b/c/note.md");
        atomic_write(&target, "x").unwrap();
        assert!(target.exists());
    }

    #[test]
    fn atomic_write_overwrites_existing_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let target = tmp.path().join("note.md");
        std::fs::write(&target, "old").unwrap();
        atomic_write(&target, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
    }

    #[test]
    fn atomic_write_does_not_leave_tmp_files_on_success() {
        let tmp = tempfile::TempDir::new().unwrap();
        let target = tmp.path().join("note.md");
        atomic_write(&target, "x").unwrap();
        let entries: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().into_string().unwrap())
            .collect();
        assert_eq!(entries, vec!["note.md".to_string()]);
    }

    // --- append_daily ---

    #[test]
    fn daily_create_when_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("daily/2026-05-06.md");
        append_daily(&path, "- 04:32:03 [[142-url-foo|url from Safari]]").unwrap();
        let s = std::fs::read_to_string(&path).unwrap();
        assert!(s.contains("## Clipboard captures · 2026-05-06"));
        assert!(s.contains("- 04:32:03 [[142-url-foo|url from Safari]]"));
    }

    #[test]
    fn daily_append_to_existing_section() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("daily/2026-05-06.md");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "# Today\n\n## Clipboard captures · 2026-05-06\n\n- 04:32:03 [[1-url-a|a]]\n\n## Other section\n\nstuff\n",
        ).unwrap();
        append_daily(&path, "- 04:32:05 [[2-json-b|b]]").unwrap();
        let s = std::fs::read_to_string(&path).unwrap();
        assert!(s.contains("- 04:32:03 [[1-url-a|a]]"));
        assert!(s.contains("- 04:32:05 [[2-json-b|b]]"));
        let captures_idx = s.find("## Clipboard captures").unwrap();
        let other_idx = s.find("## Other section").unwrap();
        let new_bullet_idx = s.find("- 04:32:05").unwrap();
        assert!(captures_idx < new_bullet_idx);
        assert!(new_bullet_idx < other_idx);
        assert!(s.contains("# Today"));
        assert!(s.contains("stuff"));
    }

    #[test]
    fn daily_append_when_file_exists_without_header() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("daily/2026-05-06.md");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "# Today\n\nMy notes.\n").unwrap();
        append_daily(&path, "- 04:32:03 [[142-url-foo|url from Safari]]").unwrap();
        let s = std::fs::read_to_string(&path).unwrap();
        assert!(s.contains("## Clipboard captures"));
        assert!(s.contains("My notes."));
        assert!(s.find("My notes.").unwrap() < s.find("## Clipboard captures").unwrap());
    }
}
