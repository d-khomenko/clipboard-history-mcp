# T3 Pinned / Favorites Finish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the gaps between the README's pin contract ("survives `clear_history`", filterable via `pinned_only`) and the actual code, so pinned clips really do persist across all the prune / clear paths and are queryable.

**Architecture:** Five small fixes across `src/core/store.rs` and `src/mcp/tools.rs`, each with a corresponding test. No schema change (the `is_pinned` column already exists since v0.3). No MCP tool surface change (`pin_item` and the `pinned_only` parameter already declared — the latter just isn't wired). Spec source: [v0.6 roadmap §3 T3](../specs/2026-05-08-v0.6-adoption-roadmap-design.md). T3 is the smallest of the v0.6 tracks and ships entirely as bug fixes + tests + docs.

**Tech Stack:** Rust 1.95, `rusqlite`, existing test harness in `tests/store.rs` and `tests/mcp_smoke.rs`.

---

## Pre-flight

The branch `feat/v0.6-T3-pinned-favorites` already exists off `origin/main` (created at `90cb9d1`, T1's merge commit). Confirm:

```bash
cd /Users/stock/dev/clipboard-history-mcp
git status
git log --oneline -1
# Should show: 90cb9d1 Merge pull request #31 from d-khomenko/feat/v0.6-T1-quality-polish
```

**Note on T2 conflict risk.** T2 (PR #33) is still open as of T3 start and modifies `store.rs` (Item extensions, `add_image_clip`/`add_files_clip`, `delete` blob-cleanup) and `tools.rs` (`copy_item` payload dispatch, `get_item with_blob`). T3 modifies the SAME two files but DIFFERENT methods (`list_with`, `prune_oldest`, `clear_*`, `list_history` `pinned_only` wiring). When T2 merges before T3, T3 will need a rebase; conflicts are likely contained to the imports and SQL columns (T2 added 4 columns, T3's modified SQLs need to include them after rebase). Implementer should fold the column list update into the rebase resolution — not in T3 scope to anticipate.

---

## Task 1: Wire `pinned_only` filter through `list_history`

**Why.** `ListParams.pinned_only: Option<bool>` is parsed from the MCP request but `list_history` never passes it to the store layer. Today calling `list_history(pinned_only=true)` returns the same result as without — silent no-op, contract violation.

**Files:**
- Modify: `src/core/store.rs` — extend `list_with` signature with `pinned_only: bool`.
- Modify: `src/mcp/tools.rs` — pass `p.pinned_only.unwrap_or(false)` through.
- Modify: `tests/store.rs` — new test.

- [ ] **Step 1: Write failing test in `tests/store.rs`**

Append at the end of the file:

```rust
#[test]
fn list_with_pinned_only_returns_only_pinned() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db = tmp.path().join("t.db");
    let store = Store::open(&db, [0u8; 32]).unwrap();

    let pinned_id = store.add_clip(ClipInput {
        text: "pinned".into(),
        primary_kind: "text".into(),
        kinds: vec!["text".into()],
        source_app: None, window_title: None,
    }).unwrap();
    let _unpinned_id = store.add_clip(ClipInput {
        text: "unpinned".into(),
        primary_kind: "text".into(),
        kinds: vec!["text".into()],
        source_app: None, window_title: None,
    }).unwrap();
    store.pin(pinned_id, true).unwrap();

    let pinned = store.list_with(None, 100, 0, true).unwrap();
    assert_eq!(pinned.len(), 1);
    assert_eq!(pinned[0].id, pinned_id);

    let all = store.list_with(None, 100, 0, false).unwrap();
    assert_eq!(all.len(), 2);
}
```

- [ ] **Step 2: Run test — should fail to compile (`list_with` only has 3 args, not 4)**

```bash
cargo test --test store list_with_pinned_only_returns_only_pinned
```

Expected: compile error — `list_with` takes 3 args.

- [ ] **Step 3: Extend `list_with` signature in `src/core/store.rs`**

Change the function signature from:

```rust
pub fn list_with(&self, kind: Option<&str>, limit: i64, offset: i64) -> Result<Vec<Item>> {
```

to:

```rust
pub fn list_with(&self, kind: Option<&str>, limit: i64, offset: i64, pinned_only: bool) -> Result<Vec<Item>> {
```

Update both SQL branches inside the function. The `kind.is_some()` branch becomes (note the new `WHERE is_pinned = 1` clause when `pinned_only`):

```rust
        let sql = match (kind.is_some(), pinned_only) {
            (true, true) => {
                "SELECT id, uuid, text, preview, length, primary_kind, source_app, window_title,
                        first_copied_at, last_copied_at, copy_count, paste_count, is_pinned
                 FROM clips WHERE is_pinned = 1
                   AND id IN (SELECT clip_id FROM kinds WHERE kind = ?1)
                 ORDER BY last_copied_at DESC LIMIT ?2 OFFSET ?3"
            }
            (true, false) => {
                "SELECT id, uuid, text, preview, length, primary_kind, source_app, window_title,
                        first_copied_at, last_copied_at, copy_count, paste_count, is_pinned
                 FROM clips WHERE id IN (SELECT clip_id FROM kinds WHERE kind = ?1)
                 ORDER BY is_pinned DESC, last_copied_at DESC LIMIT ?2 OFFSET ?3"
            }
            (false, true) => {
                "SELECT id, uuid, text, preview, length, primary_kind, source_app, window_title,
                        first_copied_at, last_copied_at, copy_count, paste_count, is_pinned
                 FROM clips WHERE is_pinned = 1
                 ORDER BY last_copied_at DESC LIMIT ?1 OFFSET ?2"
            }
            (false, false) => {
                "SELECT id, uuid, text, preview, length, primary_kind, source_app, window_title,
                        first_copied_at, last_copied_at, copy_count, paste_count, is_pinned
                 FROM clips ORDER BY is_pinned DESC, last_copied_at DESC LIMIT ?1 OFFSET ?2"
            }
        };
```

The `query_map` calls below still use `params![k, limit, offset]` and `params![limit, offset]` respectively — `is_pinned = 1` is a literal in the SQL, so no extra parameter binding needed.

- [ ] **Step 4: Update `Store::list` and the `list_history` MCP tool to pass `false` / `p.pinned_only`**

In `src/core/store.rs`, change `Store::list`:

```rust
    pub fn list(&self, limit: i64) -> Result<Vec<Item>> {
        self.list_with(None, limit, 0, false)
    }
```

In `src/mcp/tools.rs`, change `list_history` (around line 95-101):

```rust
    #[tool(description = "List recent clipboard entries, newest first. Secret values are never returned — only metadata.")]
    async fn list_history(&self, Parameters(p): Parameters<ListParams>) -> String {
        let limit = p.limit.unwrap_or(20);
        let pinned_only = p.pinned_only.unwrap_or(false);
        match self.store.list_with(p.kind.as_deref(), limit, p.offset.unwrap_or(0), pinned_only) {
            Ok(items) => serde_json::json!({ "count": items.len(), "items": items }).to_string(),
            Err(e) => format!("error: {}", e),
        }
    }
```

Search the rest of `tools.rs` for any other `list_with` call site and add `false` as the new fourth arg. Likely sites: `get_urls` (`list_with(Some("url"), 200, 0)` → `list_with(Some("url"), 200, 0, false)`), `get_code` (`list_with(Some(&k), limit, 0)` → add `false`), `get_json` (`list_with(Some("json"), limit, 0)` → add `false`), `get_secrets_index` (`list_with(Some(k), 200, 0)` → add `false`).

`grep -n 'list_with' src/` finds them all.

- [ ] **Step 5: Run all tests**

```bash
cargo test
```

Expected: green. The new test passes; existing tests still pass.

- [ ] **Step 6: Commit**

```bash
git add src/core/store.rs src/mcp/tools.rs tests/store.rs
git commit -m "fix(store,mcp): wire pinned_only filter through list_history

ListParams.pinned_only was parsed but never passed to the store.
Today list_history(pinned_only=true) silently returns the same set
as without — contract violation per the README's pin documentation.

Extends Store::list_with with a pinned_only: bool parameter, threads
it through every call site (list_history, get_urls, get_code, get_json,
get_secrets_index, Store::list). When true, the SELECT adds
WHERE is_pinned = 1 and drops the ORDER BY is_pinned (only one bucket
left to sort).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: `prune_oldest` strict pinned exclusion

**Why.** Today's `prune_oldest` SQL orders by `is_pinned DESC, last_copied_at DESC` and deletes everything beyond the `keep` offset. If a user has more pinned clips than `keep` (1000+), some pinned clips get pruned despite their pin. The README contract says pinned clips survive `clear_history` — and `prune_oldest` is the implicit ring-buffer enforcer. They should always survive.

**Files:**
- Modify: `src/core/store.rs` — change `prune_oldest` SQL to exclude pinned from the deletion candidates.
- Modify: `tests/store.rs` — new test asserting pinned survive prune even when pinned > keep.

- [ ] **Step 1: Write failing test in `tests/store.rs`**

Append:

```rust
#[test]
fn prune_oldest_never_deletes_pinned_even_when_pinned_exceeds_keep() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db = tmp.path().join("t.db");
    let store = Store::open(&db, [0u8; 32]).unwrap();

    // Pin 5 items, leave 5 unpinned. keep=2.
    let mut pinned_ids = Vec::new();
    for i in 0..5 {
        let id = store.add_clip(ClipInput {
            text: format!("pinned {}", i),
            primary_kind: "text".into(),
            kinds: vec!["text".into()],
            source_app: None, window_title: None,
        }).unwrap();
        store.pin(id, true).unwrap();
        pinned_ids.push(id);
    }
    for i in 0..5 {
        store.add_clip(ClipInput {
            text: format!("unpinned {}", i),
            primary_kind: "text".into(),
            kinds: vec!["text".into()],
            source_app: None, window_title: None,
        }).unwrap();
    }

    // Prune to keep=2. Today this would drop 3 pinned items (degenerate case).
    let removed = store.prune_oldest(2).unwrap();
    assert!(removed >= 5, "expected at least 5 unpinned items removed, got {}", removed);

    // All 5 pinned should still be there.
    for id in &pinned_ids {
        assert!(
            store.get_item(*id).unwrap().is_some(),
            "pinned item {} was deleted by prune_oldest",
            id
        );
    }
}
```

- [ ] **Step 2: Run test — should fail (some pinned items get deleted)**

```bash
cargo test --test store prune_oldest_never_deletes_pinned_even_when_pinned_exceeds_keep
```

Expected: assertion failure on one of the pinned items.

- [ ] **Step 3: Fix `prune_oldest` SQL in `src/core/store.rs`**

Replace the existing function with:

```rust
    pub fn prune_oldest(&self, keep: i64) -> Result<usize> {
        // Pinned clips are NEVER pruned — the ring buffer applies only to
        // unpinned items. The user's pin list is a contract.
        Ok(self.conn.execute(
            "DELETE FROM clips WHERE is_pinned = 0 AND id IN (
               SELECT id FROM clips WHERE is_pinned = 0
                 ORDER BY last_copied_at DESC LIMIT -1 OFFSET ?1
             )",
            params![keep],
        )?)
    }
```

Note: the inner SELECT also needs `WHERE is_pinned = 0` so the OFFSET counts only the unpinned items. Without that, the offset would skip pinned items first and undercount the deletable set.

- [ ] **Step 4: Run test — must pass**

```bash
cargo test --test store prune_oldest_never_deletes_pinned_even_when_pinned_exceeds_keep
```

Expected: pass.

- [ ] **Step 5: Run full store test suite**

```bash
cargo test --test store
```

Expected: all green; no regression on other prune-related tests.

- [ ] **Step 6: Commit**

```bash
git add src/core/store.rs tests/store.rs
git commit -m "fix(store): prune_oldest never deletes pinned items

Previous SQL ordered by is_pinned DESC then took everything beyond
keep offset. If pinned > keep (degenerate case but possible), the
oldest pinned items got pruned despite the pin. The README's pin
contract says pinned clips survive — make it true unconditionally.

The new SQL filters DELETE candidates to is_pinned = 0 in both the
outer and inner SELECT, so pinned items are never in the deletion
set regardless of count.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: `clear_all` / `clear_older_than_days` / `clear_kind` preserve pinned

**Why.** Currently all three `clear_*` paths delete pinned clips. The README says `pin_item` makes a clip "survive `clear_history`" — but the underlying `clear_all` is `DELETE FROM clips`, no pin protection. Same for the other two. Make pinned items always survive any `clear_*` call (no opt-in flag — the simpler contract).

**Files:**
- Modify: `src/core/store.rs` — three SQL changes.
- Modify: `tests/store.rs` — three new tests.

- [ ] **Step 1: Write three failing tests**

Append to `tests/store.rs`:

```rust
fn make_pinned_and_unpinned(store: &Store) -> (i64, i64) {
    let pinned_id = store.add_clip(ClipInput {
        text: "pinned".into(),
        primary_kind: "text".into(),
        kinds: vec!["text".into()],
        source_app: None, window_title: None,
    }).unwrap();
    let unpinned_id = store.add_clip(ClipInput {
        text: "unpinned".into(),
        primary_kind: "text".into(),
        kinds: vec!["text".into()],
        source_app: None, window_title: None,
    }).unwrap();
    store.pin(pinned_id, true).unwrap();
    (pinned_id, unpinned_id)
}

#[test]
fn clear_all_preserves_pinned() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path().join("t.db"), [0u8; 32]).unwrap();
    let (pinned_id, unpinned_id) = make_pinned_and_unpinned(&store);

    let removed = store.clear_all().unwrap();
    assert_eq!(removed, 1, "only the unpinned item should be removed");
    assert!(store.get_item(pinned_id).unwrap().is_some(), "pinned should survive");
    assert!(store.get_item(unpinned_id).unwrap().is_none(), "unpinned should be gone");
}

#[test]
fn clear_older_than_days_preserves_pinned() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path().join("t.db"), [0u8; 32]).unwrap();
    let (pinned_id, _unpinned_id) = make_pinned_and_unpinned(&store);

    // -1 days = future cutoff so EVERY clip is "older than" — only pinning saves the pinned one.
    let removed = store.clear_older_than_days(-1).unwrap();
    assert_eq!(removed, 1);
    assert!(store.get_item(pinned_id).unwrap().is_some());
}

#[test]
fn clear_kind_preserves_pinned() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = Store::open(tmp.path().join("t.db"), [0u8; 32]).unwrap();
    let (pinned_id, _unpinned_id) = make_pinned_and_unpinned(&store);

    let removed = store.clear_kind("text").unwrap();
    assert_eq!(removed, 1, "only the unpinned text item should be removed");
    assert!(store.get_item(pinned_id).unwrap().is_some());
}
```

- [ ] **Step 2: Run tests — all three should fail**

```bash
cargo test --test store clear_all_preserves_pinned clear_older_than_days_preserves_pinned clear_kind_preserves_pinned
```

Expected: 3 failures.

- [ ] **Step 3: Fix the three `clear_*` methods in `src/core/store.rs`**

Replace each with the pinned-aware version:

```rust
    pub fn clear_all(&self) -> Result<usize> {
        Ok(self.conn.execute("DELETE FROM clips WHERE is_pinned = 0", [])?)
    }
    pub fn clear_older_than_days(&self, days: i64) -> Result<usize> {
        let cutoff = now_ms() - days * 86_400_000;
        Ok(self.conn.execute(
            "DELETE FROM clips WHERE is_pinned = 0 AND last_copied_at < ?1",
            params![cutoff],
        )?)
    }
    pub fn clear_kind(&self, kind: &str) -> Result<usize> {
        Ok(self.conn.execute(
            "DELETE FROM clips WHERE is_pinned = 0 AND (
               primary_kind = ?1 OR id IN (SELECT clip_id FROM kinds WHERE kind = ?1)
             )",
            params![kind],
        )?)
    }
```

- [ ] **Step 4: Run all 3 tests — must pass**

```bash
cargo test --test store clear_all_preserves_pinned clear_older_than_days_preserves_pinned clear_kind_preserves_pinned
```

Expected: pass.

- [ ] **Step 5: Run full test suite**

```bash
cargo test
```

Expected: green.

- [ ] **Step 6: Commit**

```bash
git add src/core/store.rs tests/store.rs
git commit -m "fix(store): clear_* paths preserve pinned items

clear_all, clear_older_than_days, and clear_kind now all add
'WHERE is_pinned = 0' to their DELETE so pinned clips survive every
\"clear history\" entry point. The README documents pin_item as
\"survives clear_history\" — making it true.

There is no opt-out: pinning is the user's explicit do-not-delete
signal, and a force-everything escape hatch isn't worth the API
complexity. To delete a pinned clip the caller unpins it first
(pin_item(id, false)) then clears or deletes.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: README + CHANGELOG + final integration + push + PR

**Why.** Pin behavior is now consistent with the README's existing claims; document the new `pinned_only` MCP filter and the strict prune semantics in CHANGELOG so users see what changed.

**Files:**
- Modify: `README.md` — extend the Tools (15) row for `list_history` to mention `pinned_only`. Optionally add a one-liner under "Try asking Claude" about pinning.
- Modify: `CHANGELOG.md` — `[Unreleased] ### Fixed` block.

- [ ] **Step 1: Update `README.md`**

Find the Tools (15) Read table (around line 187-194). Locate the `list_history` row and extend its description:

```markdown
| `list_history(limit?, kind?, source_app?, since?, pinned_only?)` | Paginated history, newest first. `pinned_only=true` returns only pinned clips. |
```

In the "Try asking Claude" example block (around line 144-159), the existing `Pin the JSON I just copied — I'll need it again.` line is already there. Add a sibling:

```
Show me only my pinned clips.
```

- [ ] **Step 2: Update `CHANGELOG.md`**

Find the `[Unreleased]` section. Add (or extend) `### Fixed`:

```markdown
### Fixed
- **Pin contract honoured across all clear / prune paths.** `clear_all`, `clear_older_than_days`, `clear_kind`, and the daemon's `prune_oldest` ring-buffer now skip pinned clips unconditionally. Previously pinned items could be deleted by `clear_history(scope='all')` or, in degenerate cases (more pinned than `CLIPBOARD_HISTORY_MAX`), by the prune itself. The README has documented `pin_item` as "survives `clear_history`" since v0.3 — now it actually does.
- **`list_history(pinned_only=true)` filter wired through.** The parameter was parsed from MCP requests but never passed to the store layer, so pinned-only listings silently returned everything. Threads through `Store::list_with(..., pinned_only: bool)` and every call site (`get_urls`, `get_code`, `get_json`, `get_secrets_index`, internal `Store::list`).
```

- [ ] **Step 3: Run the full test suite**

```bash
cargo test
```

Expected: all green.

- [ ] **Step 4: Run clippy**

```bash
cargo clippy --all-targets -- -D warnings
```

Expected: clean.

- [ ] **Step 5: Sanity-grep for production unwraps**

```bash
grep -nE '\.(expect|unwrap)\(' src/main.rs src/daemon/ src/mcp/ src/core/{crypto,store,vault,pasteboard,biometry,blobs}.rs 2>/dev/null | grep -v 'unwrap_or' | grep -v '#\[cfg(test)' | grep -v '// '
```

Expected: only the three pre-acknowledged init-time hits (`paths.rs:10`, `pasteboard.rs:6`, `crypto.rs:22`). T3 adds zero new production unwraps — every change is SQL or single-line wiring.

- [ ] **Step 6: Commit**

```bash
git add README.md CHANGELOG.md
git commit -m "docs(t3): document pin behavior + pinned_only filter

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

- [ ] **Step 7: Push branch and open PR**

```bash
git push -u origin feat/v0.6-T3-pinned-favorites
gh pr create --base main --title "fix(v0.6): T3 pinned-favorites finish" --body "$(cat <<'EOF'
## Summary

T3 from the [v0.6 adoption-first roadmap](docs/superpowers/specs/2026-05-08-v0.6-adoption-roadmap-design.md). Closes the pin-related contract gaps audit'd before T3 started: pinned clips now actually survive every \`clear_*\` and \`prune_oldest\` path, and \`list_history(pinned_only=true)\` actually filters.

Three of the four tasks are pure bug fixes against pre-existing behaviour; the fourth is docs.

### Fixed
- \`clear_all\` / \`clear_older_than_days\` / \`clear_kind\` add \`WHERE is_pinned = 0\` — the README has promised this since v0.3 but the SQL didn't enforce it.
- \`prune_oldest\` strictly excludes pinned clips from DELETE candidates (both outer and inner SELECT). Previously, more pinned than \`keep\` could prune the oldest pinned items.
- \`list_history(pinned_only=true)\` filter wired through. Was parsed from \`ListParams\` but never reached \`Store::list_with\`; now threads through every call site.

### Out of scope
Visual distinction of pinned items in vault daily-notes (roadmap §3 T3 marked low priority).

### Test plan

- [x] \`cargo test\` — 79+5 tests pass (5 new T3 tests for the four fixes)
- [x] \`cargo clippy --all-targets -- -D warnings\` — clean
- [x] Sanity-grep: no new production \`.expect()\` / \`.unwrap()\`
- [ ] **Manual:** \`pin_item(N)\` then \`clear_history(scope='all')\` — clip N still listed
- [ ] **Manual:** \`pin_item(N)\` then 1500 captures (>= HISTORY_MAX) — clip N still listed

## Rebase note

If T2 (PR #33) lands before this is merged, rebase will likely conflict in \`src/core/store.rs\` (T2 added 4 columns to the SELECTs in \`list_with\`; T3 added a \`pinned_only\` branch parameter). Resolution is mechanical: each new branch's SELECT in T3 needs the four T2 columns appended.
EOF
)"
```

Expected: PR URL printed.

---

## Done-criteria for T3

T3 ships when **all** are true:

1. PR merged into `main`.
2. `cargo test` and `cargo clippy --all-targets -- -D warnings` are green on the merge commit.
3. `pin_item(N)` followed by `clear_history(scope='all')`: clip N survives. Verified by `clear_all_preserves_pinned`.
4. `pin_item(N)` followed by `prune_oldest(keep=2)` with 5 pinned + 5 unpinned: all 5 pinned survive. Verified by `prune_oldest_never_deletes_pinned_even_when_pinned_exceeds_keep`.
5. `list_history(pinned_only=true)` returns only pinned items. Verified by `list_with_pinned_only_returns_only_pinned`.
6. CHANGELOG `[Unreleased]` documents both fixes.
7. README Tools (15) row for `list_history` mentions `pinned_only`.
