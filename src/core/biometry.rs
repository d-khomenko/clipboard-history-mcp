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
        // Check if user is on the v1 compat path and warn them to migrate.
        // The LAContext gate still applies even in v1 mode, but the master key
        // itself is not password-protected, so we emit a deprecation warning.
        if crate::core::crypto::read_master_key_v2()?.is_none() {
            tracing::warn!(
                "master-key-v1 compat mode active. Run `clipboard-history-mcp migrate-v2` \
                 to set a master password and enable v4 secret protections."
            );
        }
        macos::evaluate_la_context(reason)
    }

    #[cfg(target_os = "linux")]
    fn evaluate_inner(&self, _reason: &str) -> Result<bool> {
        use crate::core::crypto::read_master_key_v2;
        use crate::core::master_password::{prompt_password, unwrap_master_key, WrappedKey};

        let Some(entry) = read_master_key_v2()? else {
            // No v2 entry → user is on v1 raw-key compat mode → no biometry gate.
            tracing::warn!(
                "master-key-v1 compat mode active. Run `clipboard-history-mcp migrate-v2` \
                 to set a master password and enable v4 secret protections."
            );
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
        tracing::warn!(
            "biometry gate is not implemented on this platform; \
             secret unlock is permitted without authentication. \
             Track at https://github.com/d-khomenko/clipboard-history-mcp/issues"
        );
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

    /// Thin `Send` wrapper around `RcBlock`.
    ///
    /// `block2` 0.6 does not yet expose an `ArcBlock` type for thread-safe
    /// blocks. The LAContext reply callback is invoked on an arbitrary GCD
    /// queue (i.e. a different thread), so we must ensure the block is `Send`.
    ///
    /// Safety: the closure captured inside the block contains only
    /// `mpsc::Sender<bool>`, which is `Send`. ObjC's block reference counting
    /// uses atomic operations, so it is safe to share the pointer across
    /// threads. We never call the block from Rust — we only pass it to the
    /// ObjC runtime, which takes ownership and calls it once on its own queue.
    struct SendBlock(block2::RcBlock<dyn Fn(objc2::runtime::Bool, *mut NSError)>);
    // SAFETY: see doc comment above.
    unsafe impl Send for SendBlock {}

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
            let block = SendBlock(block2::RcBlock::<dyn Fn(Bool, *mut NSError)>::new(
                move |success: Bool, _err: *mut NSError| {
                    let _ = sender.send(success.as_bool());
                },
            ));
            ctx.evaluatePolicy_localizedReason_reply(policy, &reason_ns, &block.0);
            receiver.recv_timeout(Duration::from_secs(60)).unwrap_or(false)
        });
        Ok(ok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// Serializes tests that mutate the process-global `AUTH_CACHE`.
    ///
    /// `cargo test` runs unit tests in parallel within a single process. Because
    /// the cache is a `OnceLock<Mutex<Option<Instant>>>` shared across the whole
    /// crate, parallel tests would race on it: one test might prime the cache
    /// just as another asserts emptiness. This mutex forces strict ordering for
    /// the subset of tests that touch the cache. Tests that don't touch the
    /// cache (e.g. constructor checks) don't need to hold it.
    static CACHE_TEST_LOCK: StdMutex<()> = StdMutex::new(());

    /// Clears `AUTH_CACHE` so a test can start from a known-cold state.
    ///
    /// Test-only — production code never resets the cache; the 5-minute TTL is
    /// the sole expiry mechanism in real use.
    fn reset_cache() {
        let cache = AUTH_CACHE.get_or_init(|| Mutex::new(None));
        let mut g = cache.lock().expect("cache mutex");
        *g = None;
    }

    /// Manually sets the cache's last-success timestamp.
    ///
    /// Test-only — lets a test simulate "the cache was primed N seconds ago"
    /// without having to call the platform `evaluate_inner` (which prompts the
    /// user). Using `Instant::now() - dur` synthesizes an aged entry.
    fn set_cache_at(t: Instant) {
        let cache = AUTH_CACHE.get_or_init(|| Mutex::new(None));
        let mut g = cache.lock().expect("cache mutex");
        *g = Some(t);
    }

    // --- construction ---

    #[test]
    fn new_is_zero_sized_and_constructible() {
        let _g = BiometryGate::new();
        // Sanity: the marker struct holds no state.
        assert_eq!(std::mem::size_of::<BiometryGate>(), 0);
    }

    #[test]
    #[allow(clippy::default_constructed_unit_structs)]
    fn default_matches_new() {
        // Both paths must produce an equivalent gate; the type has no fields
        // to compare directly, so we assert size invariance instead.
        // The `Default::default()` call is intentional: it's the whole point of
        // this test (verify the `Default` impl is wired up), so we silence the
        // unit-struct lint that would otherwise nudge us to drop it.
        let _a = BiometryGate::default();
        let _b = BiometryGate::new();
        assert_eq!(std::mem::size_of_val(&_a), std::mem::size_of_val(&_b));
    }

    #[test]
    fn gate_is_send_and_sync() {
        // The gate is shared across MCP request handlers and must be safe to
        // move between threads. The marker assertion fails at compile time if
        // the bounds regress.
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<BiometryGate>();
    }

    // --- cache TTL constant ---

    #[test]
    fn cache_ttl_is_five_minutes() {
        // The 5-minute window is part of the security contract: longer windows
        // weaken the biometric gate; shorter ones harm UX. Pin the value so a
        // future tweak fails this test and forces an intentional decision.
        assert_eq!(CACHE_TTL, Duration::from_secs(300));
    }

    // --- cache_hit / mark_cache primitives ---

    #[test]
    fn cache_hit_false_when_empty() {
        let _g = CACHE_TEST_LOCK.lock().unwrap();
        reset_cache();
        assert!(!cache_hit(), "empty cache must not register as a hit");
    }

    #[test]
    fn mark_cache_then_hit_returns_true() {
        let _g = CACHE_TEST_LOCK.lock().unwrap();
        reset_cache();
        mark_cache();
        assert!(cache_hit(), "fresh mark must register as a hit");
    }

    #[test]
    fn cache_hit_false_when_entry_older_than_ttl() {
        let _g = CACHE_TEST_LOCK.lock().unwrap();
        // Plant an entry that's already past the 5-minute window. We add an
        // extra second so we're definitively over the boundary regardless of
        // sub-second timing on slow CI.
        let aged = Instant::now() - (CACHE_TTL + Duration::from_secs(1));
        set_cache_at(aged);
        assert!(
            !cache_hit(),
            "entry older than CACHE_TTL must not register as a hit"
        );
    }

    #[test]
    fn cache_hit_true_just_under_ttl() {
        let _g = CACHE_TEST_LOCK.lock().unwrap();
        // 60s in the past — well within the 5-minute window.
        let recent = Instant::now() - Duration::from_secs(60);
        set_cache_at(recent);
        assert!(cache_hit(), "entry inside CACHE_TTL must register as a hit");
    }

    #[test]
    fn mark_cache_is_idempotent_and_refreshes_timestamp() {
        let _g = CACHE_TEST_LOCK.lock().unwrap();
        reset_cache();
        mark_cache();
        let cache = AUTH_CACHE.get_or_init(|| Mutex::new(None));
        let t1 = cache.lock().unwrap().expect("primed");
        // A second call must overwrite the prior timestamp, not append or panic.
        std::thread::sleep(Duration::from_millis(2));
        mark_cache();
        let t2 = cache.lock().unwrap().expect("re-primed");
        assert!(t2 >= t1, "second mark_cache should refresh the timestamp");
    }

    // --- evaluate() short-circuit path ---
    //
    // `BiometryGate::evaluate` has two branches: a cache-hit fast-path that
    // returns Ok(true) without ever touching the platform layer, and the
    // platform-dispatch slow path. We can exercise the first branch in CI
    // because it's pure Rust; the second still requires Touch ID and stays
    // behind `#[ignore]`.

    #[test]
    fn evaluate_short_circuits_on_cache_hit() {
        let _g = CACHE_TEST_LOCK.lock().unwrap();
        // Prime the cache with a fresh timestamp — `evaluate` should bypass
        // every platform-specific code path and return Ok(true) directly.
        set_cache_at(Instant::now());
        let gate = BiometryGate::new();
        let r = gate.evaluate("unit-test: should not prompt").expect("ok");
        assert!(r, "primed cache must short-circuit evaluate() to true");
    }

    #[test]
    fn evaluate_does_not_consume_cache() {
        let _g = CACHE_TEST_LOCK.lock().unwrap();
        // The cache is a TTL window, not a single-use ticket: multiple
        // evaluate() calls inside the window must all succeed without ever
        // re-prompting.
        set_cache_at(Instant::now());
        let gate = BiometryGate::new();
        for i in 0..3 {
            let r = gate.evaluate(&format!("call {i}")).expect("ok");
            assert!(r, "call {i} should short-circuit on the still-valid cache");
        }
    }

    /// Verifies that two rapid calls within the 5-minute window short-circuit on
    /// the second attempt (cache hit). Because the *first* call triggers the
    /// system Touch ID / password UI this test is `#[ignore]`d by default — run
    /// it manually with `cargo test -- --ignored` on a machine with biometrics.
    #[test]
    #[ignore] // requires user interaction and Touch ID / password prompt
    fn cache_hit_on_second_call_end_to_end() {
        let _g = CACHE_TEST_LOCK.lock().unwrap();
        reset_cache();
        let gate = BiometryGate::new();
        let r1 = gate.evaluate("test: first call");
        assert!(r1.unwrap(), "first call should succeed with user interaction");

        // Second call within TTL: should return Ok(true) without a new prompt.
        let r2 = gate.evaluate("test: second call (should hit cache)");
        assert!(r2.unwrap(), "second call should hit the cache");
    }
}
