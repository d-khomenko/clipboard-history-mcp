use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

/// End-to-end MCP smoke test.
///
/// Spawns the binary in `serve` mode, sends the MCP handshake
/// (initialize → notifications/initialized → tools/list), then asserts
/// that the server replied with an initialize result and a tools/list result
/// that mentions at least one of our known tool names.
///
/// Marked `#[ignore]` because it:
/// - hits the real macOS Keychain (get_or_create_master_key)
/// - requires the release binary to already be built
///
/// Run with:
///   cargo test --release --test mcp_smoke -- --ignored
#[test]
#[ignore]
fn mcp_handshake_and_list_tools() {
    let bin = env!("CARGO_BIN_EXE_clipboard-history-mcp");

    let mut child = Command::new(bin)
        .arg("serve")
        .env(
            "CLIPBOARD_DATA_DIR",
            std::env::temp_dir().join("cbhist-rs-int"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn MCP server binary");

    let stdin = child.stdin.as_mut().expect("stdin not available");

    // 1. initialize
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2024-11-05","capabilities":{{}},"clientInfo":{{"name":"t","version":"1"}}}}}}"#
    )
    .unwrap();

    // 2. notifications/initialized  (no id — notification, no response expected)
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#
    )
    .unwrap();

    // 3. tools/list
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/list"}}"#
    )
    .unwrap();

    // Give the server time to process and write responses, then kill it.
    std::thread::sleep(Duration::from_millis(2000));
    let _ = child.kill();
    let out = child.wait_with_output().expect("wait_with_output failed");

    let stdout = String::from_utf8_lossy(&out.stdout);

    // --- assertions ---

    // initialize reply must be present
    assert!(
        stdout.contains(r#""id":1"#),
        "missing initialize response (id=1)\n\nstdout:\n{}",
        stdout
    );

    // tools/list reply must be present
    assert!(
        stdout.contains(r#""id":2"#),
        "missing tools/list response (id=2)\n\nstdout:\n{}",
        stdout
    );

    // at least one known tool name must appear in tools/list
    assert!(
        stdout.contains("list_history") || stdout.contains("get_stats"),
        "tools/list response does not mention expected tools\n\nstdout:\n{}",
        stdout
    );

    // make sure we're not just seeing 0 tools
    assert!(
        !stdout.contains(r#""tools":[]"#),
        "tools/list returned an empty tools array\n\nstdout:\n{}",
        stdout
    );
}
