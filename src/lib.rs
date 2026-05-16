pub mod core;
pub mod cli;
pub mod daemon;
pub mod mcp;

/// Test-only shared utilities. Process-global `Mutex<()>` to serialise
/// tests that mutate `CLIPBOARD_DATA_DIR` / `HOME` (or any other
/// process-wide env var). Per-module mutexes do not protect against
/// cross-module races — when the lib-test binary runs tests in parallel,
/// every test that touches env state must lock this same handle.
#[cfg(test)]
pub(crate) mod test_util {
    use std::sync::Mutex;
    pub static ENV_LOCK: Mutex<()> = Mutex::new(());
}
