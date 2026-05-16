use anyhow::Result;
use crate::core::{
    crypto::get_or_create_master_key,
    master_password::prompt_password,
    paths::db_path,
    store::Store,
};

/// Opens the production Store the same way `Cmd::Daemon` does.
/// Single source of truth for write-subcommand bootstrap.
///
/// When `CLIPBOARD_EPHEMERAL_KEY=1` is set the zero key `[0u8; 32]` is used
/// instead of the real keychain key. This matches the MCP server's test path
/// and allows integration tests to exercise CLI logic against a tmp DB without
/// a real keychain entry.
pub fn open() -> Result<Store> {
    let key = if std::env::var("CLIPBOARD_EPHEMERAL_KEY").as_deref() == Ok("1") {
        [0u8; 32]
    } else {
        get_or_create_master_key(|| prompt_password("Master password (5-min cache): "))?
    };
    let path = db_path();
    Store::open(path, key)
}
