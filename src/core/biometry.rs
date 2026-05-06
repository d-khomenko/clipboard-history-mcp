use anyhow::Result;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(300);
static AUTH_CACHE: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

pub struct BiometryGate;

impl BiometryGate {
    pub fn new() -> Self {
        Self
    }

    pub fn evaluate(&self, reason: &str) -> Result<bool> {
        if cache_hit() {
            return Ok(true);
        }
        let ok = self.evaluate_inner(reason)?;
        if ok {
            mark_cache();
        }
        Ok(ok)
    }

    #[cfg(target_os = "macos")]
    fn evaluate_inner(&self, reason: &str) -> Result<bool> {
        macos::evaluate_la_context(reason)
    }

    #[cfg(target_os = "linux")]
    fn evaluate_inner(&self, _reason: &str) -> Result<bool> {
        use crate::core::crypto::read_master_key_v2;
        use crate::core::master_password::{prompt_password, unwrap_master_key, WrappedKey};

        let Some(entry) = read_master_key_v2()? else {
            // No v2 entry → user is on v1 raw-key compat mode → no biometry, just allow.
            return Ok(true);
        };
        let pw = prompt_password("Master password to unlock: ")?;
        let wrapped = WrappedKey {
            salt: entry.salt,
            nonce: entry.nonce,
            ciphertext: entry.ciphertext,
        };
        Ok(unwrap_master_key(&wrapped, &pw).is_ok())
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn evaluate_inner(&self, _reason: &str) -> Result<bool> {
        Ok(true)
    }
}

impl Default for BiometryGate {
    fn default() -> Self {
        Self::new()
    }
}

fn cache_hit() -> bool {
    let cache = AUTH_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(g) = cache.lock() {
        if let Some(t) = *g {
            return t.elapsed() < CACHE_TTL;
        }
    }
    false
}

fn mark_cache() {
    let cache = AUTH_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(mut g) = cache.lock() {
        *g = Some(Instant::now());
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use anyhow::Result;
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{NSError, NSString};
    use objc2_local_authentication::{LAContext, LAPolicy};
    use std::time::Duration;

    pub fn evaluate_la_context(reason: &str) -> Result<bool> {
        let ok = autoreleasepool(|_| unsafe {
            let ctx = LAContext::new();
            let policy = LAPolicy::DeviceOwnerAuthentication;
            if ctx.canEvaluatePolicy_error(policy).is_err() {
                tracing::warn!("biometry unavailable on this device");
                return false;
            }

            let reason_ns = NSString::from_str(reason);
            let (sender, receiver) = std::sync::mpsc::channel::<bool>();

            use objc2::runtime::Bool;
            let block = block2::RcBlock::<dyn Fn(Bool, *mut NSError)>::new(
                move |success: Bool, _err: *mut NSError| {
                    let _ = sender.send(success.as_bool());
                },
            );
            ctx.evaluatePolicy_localizedReason_reply(policy, &reason_ns, &*block);
            receiver.recv_timeout(Duration::from_secs(60)).unwrap_or(false)
        });
        Ok(ok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that two rapid calls within the 5-minute window short-circuit on
    /// the second attempt (cache hit). Because this test triggers the system
    /// Touch ID / password UI it is `#[ignore]`d by default.
    #[test]
    #[ignore] // requires user interaction and Touch ID / password prompt
    fn cache_hit_on_second_call() {
        let gate = BiometryGate::new();
        let r1 = gate.evaluate("test: first call");
        assert!(r1.unwrap(), "first call should succeed with user interaction");

        // Second call within TTL: should return Ok(true) without a new prompt.
        let r2 = gate.evaluate("test: second call (should hit cache)");
        assert!(r2.unwrap(), "second call should hit the cache");
    }
}
