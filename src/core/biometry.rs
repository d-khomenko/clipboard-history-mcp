use anyhow::Result;
use objc2::runtime::Bool;
use objc2_foundation::{NSError, NSString};
use objc2_local_authentication::{LAContext, LAPolicy};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(300);

static AUTH_CACHE: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

pub struct BiometryGate;

impl BiometryGate {
    pub fn new() -> Self {
        Self
    }

    /// Evaluate the device-owner authentication policy (biometry or password).
    ///
    /// Results are cached for 5 minutes: a successful call within the TTL window
    /// returns `Ok(true)` immediately without re-prompting the user.
    pub fn evaluate(&self, reason: &str) -> Result<bool> {
        // --- 1. Check the 5-minute cache ---
        let cache = AUTH_CACHE.get_or_init(|| Mutex::new(None));
        if let Ok(guard) = cache.lock() {
            if let Some(ts) = *guard {
                if ts.elapsed() < CACHE_TTL {
                    return Ok(true);
                }
            }
        }

        // --- 2. Run LAContext evaluation ---
        // We use a channel to bridge the Objective-C reply block to Rust sync code.
        let (tx, rx) = std::sync::mpsc::channel::<bool>();

        // SAFETY: LAContext methods are documented unsafe due to ObjC message send;
        // we keep `ctx` alive for the duration of the evaluation by pinning it.
        let ok = unsafe {
            let ctx = LAContext::new();
            let policy = LAPolicy::DeviceOwnerAuthentication;

            // Check whether the policy can be evaluated at all.
            // In objc2-local-authentication 0.3.x the signature is:
            //   canEvaluatePolicy_error(policy) -> Result<(), Retained<NSError>>
            // (no &mut error out-param — the error is returned via Result).
            if ctx.canEvaluatePolicy_error(policy).is_err() {
                tracing::warn!("biometry unavailable on this device");
                return Ok(false);
            }

            let reason_ns = NSString::from_str(reason);

            // Build the reply block.
            // The expected type is &block2::DynBlock<dyn Fn(Bool, *mut NSError)>.
            // block2::RcBlock<dyn Fn(Bool, *mut NSError)> derefs to
            // block2::Block<dyn Fn(Bool, *mut NSError)> which is DynBlock<…>.
            // SAFETY: the closure captures only `tx` (an mpsc::Sender) which is
            // Send, satisfying the "reply block must be sendable" safety requirement.
            let block = block2::RcBlock::<dyn Fn(Bool, *mut NSError)>::new(
                move |success: Bool, _err: *mut NSError| {
                    let _ = tx.send(success.as_bool());
                },
            );

            ctx.evaluatePolicy_localizedReason_reply(policy, &reason_ns, &*block);

            // Block until the system calls back (or the 60-second watchdog fires).
            rx.recv_timeout(Duration::from_secs(60)).unwrap_or(false)
        };

        // --- 3. Update cache on success ---
        if ok {
            if let Ok(mut guard) = cache.lock() {
                *guard = Some(Instant::now());
            }
        }

        Ok(ok)
    }
}

impl Default for BiometryGate {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that two rapid calls within the 5-minute window short-circuit on
    /// the second attempt (cache hit).  Because this test triggers the system
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
