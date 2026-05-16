use anyhow::Result;
use super::store_helper;

/// Pin a clip by id.
pub fn run(id: i64) -> Result<()> {
    let store = store_helper::open()?;
    store.pin(id, true)?;
    println!("{}", serde_json::json!({ "ok": true, "id": id, "pinned": true }));
    Ok(())
}
