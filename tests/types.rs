use clipboard_history_mcp::core::types::classify;

#[test]
fn detects_url() {
    let r = classify("https://api.openai.com/v1");
    assert_eq!(r.primary_kind, "url");
    assert!(r.kinds.contains(&"url".to_string()));
}

#[test]
fn detects_email() {
    assert_eq!(classify("hello@example.com").primary_kind, "email");
}

#[test]
fn detects_json() {
    assert_eq!(classify(r#"{"a":1}"#).primary_kind, "json");
}

#[test]
fn detects_sql() {
    assert_eq!(classify("SELECT * FROM users").primary_kind, "sql");
}

#[test]
fn detects_shell() {
    assert_eq!(classify("git checkout main").primary_kind, "shell");
}

#[test]
fn detects_python_code() {
    let r = classify("def foo():\n    return 42\n");
    assert!(r.primary_kind.starts_with("code:"));
    assert_eq!(r.code.unwrap().language, "python");
}

#[test]
fn falls_back_to_text() {
    assert_eq!(classify("just words").primary_kind, "text");
}
