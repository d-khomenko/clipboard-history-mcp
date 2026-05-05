use anyhow::Result;

pub struct BiometryGate;

impl BiometryGate {
    pub fn new() -> Self { Self }

    pub fn evaluate(&self, _reason: &str) -> Result<bool> {
        // Stub: always succeeds. Real LocalAuthentication wiring lands in Task 22.
        Ok(true)
    }
}
