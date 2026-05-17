use anyhow::Result;
use super::store_helper;

/// Unpin a clip by id.
pub fn run(id: i64) -> Result<()> {
    let store = store_helper::open()?;
    store.pin(id, false)?;
    println!("{}", serde_json::json!({ "ok": true, "id": id, "pinned": false }));
    Ok(())
}
