use anyhow::Result;
use super::store_helper;

/// Hard-delete a clip and its secrets row if applicable.
pub fn run(id: i64) -> Result<()> {
    let store = store_helper::open()?;
    store.delete(id)?;
    println!("{}", serde_json::json!({ "ok": true, "id": id, "deleted": true }));
    Ok(())
}
