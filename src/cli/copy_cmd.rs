use anyhow::{anyhow, Result};
use crate::core::{blobs, pasteboard};
use super::store_helper;

/// Restore a clip to the system pasteboard.
/// Mirrors the logic in `mcp/tools.rs::copy_item`.
pub fn run(id: i64) -> Result<()> {
    let store = store_helper::open()?;
    let item = store
        .get_item(id)?
        .ok_or_else(|| anyhow!("not found id={}", id))?;

    if item.primary_kind.starts_with("secret:") {
        let json = serde_json::json!({
            "error": "cannot restore secret directly",
            "requiresUnlock": true
        });
        eprintln!("{json}");
        std::process::exit(1);
    }

    let result = match item.payload_kind.as_str() {
        "text" => {
            let text = item
                .text
                .as_deref()
                .ok_or_else(|| anyhow!("text clip has no text"))?;
            pasteboard::write_clipboard(text)
        }
        "image" => {
            let rel = item
                .blob_path
                .as_deref()
                .ok_or_else(|| anyhow!("image clip missing blob_path"))?;
            let mime = item.mime_type.as_deref().unwrap_or("image/png");
            let bytes = blobs::read(rel)?;
            pasteboard::write_clip(&pasteboard::Clip::Image {
                bytes,
                mime: if mime == "image/tiff" { "image/tiff" } else { "image/png" },
            })
        }
        "file" => {
            let rel = item
                .blob_path
                .as_deref()
                .ok_or_else(|| anyhow!("file clip missing blob_path"))?;
            let abs = blobs::absolute_path(rel);
            if !abs.exists() {
                return Err(anyhow!(
                    "file no longer exists at original path: {}",
                    abs.display()
                ));
            }
            pasteboard::write_clip(&pasteboard::Clip::Files(vec![abs]))
        }
        other => return Err(anyhow!("unknown payload_kind: {}", other)),
    };

    result?;
    let _ = store.bump_paste(id);

    println!(
        "{}",
        serde_json::json!({ "ok": true, "id": id, "payload_kind": item.payload_kind })
    );
    Ok(())
}
