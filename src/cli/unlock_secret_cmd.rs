use anyhow::Result;
use crate::core::biometry::BiometryGate;
use super::store_helper;

/// Decrypt and emit a stored secret on stdout. Touch ID / password gated.
pub fn run(id: i64, reason: &str) -> Result<()> {
    let gate = BiometryGate::new();
    let prompt = format!("Reveal stored secret #{}: {}", id, reason);
    match gate.evaluate(&prompt) {
        Ok(false) => {
            let json = serde_json::json!({ "error": "biometric authentication failed" });
            eprintln!("{json}");
            std::process::exit(1);
        }
        Err(e) => {
            let json = serde_json::json!({ "error": format!("biometry error: {}", e) });
            eprintln!("{json}");
            std::process::exit(1);
        }
        Ok(true) => {}
    }

    let store = store_helper::open()?;
    let value = store.unlock_secret(id)?;
    let last_chars: String = value
        .chars()
        .rev()
        .take(6)
        .collect::<String>()
        .chars()
        .rev()
        .collect();

    println!(
        "{}",
        serde_json::json!({ "ok": true, "value": value, "last_chars": last_chars })
    );
    Ok(())
}
