#[test]
#[ignore] // touches real system clipboard
fn arboard_round_trip() {
    use clipboard_history_mcp::core::pasteboard::{read_clipboard, write_clipboard};

    let needle = format!("v4-smoke-{}", std::process::id());
    write_clipboard(&needle).unwrap();
    let got = read_clipboard().unwrap();
    assert_eq!(got, needle);
}
