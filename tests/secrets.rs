use clipboard_history_mcp::core::secrets::detect_secret;

#[test]
fn detects_openai() {
    let key = format!("sk-{}T3BlbkFJ{}", "A".repeat(20), "B".repeat(20));
    let r = detect_secret(&key).expect("should detect");
    assert!(r.kind.contains("openai") || r.kind.contains("OpenAI"));
    assert!(r.value.starts_with("sk-"));
}

#[test]
fn detects_jwt() {
    let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.abc-_def123";
    let r = detect_secret(jwt).expect("jwt");
    assert_eq!(r.kind, "jwt");
}

#[test]
fn detects_credit_card_luhn_valid() {
    let r = detect_secret("4242 4242 4242 4242").expect("cc");
    assert_eq!(r.kind, "credit_card");
}

#[test]
fn rejects_credit_card_luhn_invalid() {
    let r = detect_secret("1234 5678 9012 3456");
    if let Some(hit) = r {
        assert_ne!(hit.kind, "credit_card");
    }
}

#[test]
fn returns_none_for_plain_text() {
    assert!(detect_secret("just hello world").is_none());
}
