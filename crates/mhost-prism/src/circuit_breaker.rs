use std::collections::HashMap;
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// BreakerState
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState {
    /// Normal operation — queries proceed, error rate is tracked.
    Closed,
    /// Tripped — new queries are rejected immediately.
    Open,
    /// Cooldown elapsed — one probe query is allowed through.
    HalfOpen,
}

// ---------------------------------------------------------------------------
// SlidingWindow
// ---------------------------------------------------------------------------

struct SlidingWindow {
    successes: Vec<Instant>,
    failures: Vec<Instant>,
    window: Duration,
}

impl SlidingWindow {
    fn new(window: Duration) -> Self {
        Self {
            successes: Vec::new(),
            failures: Vec::new(),
            window,
        }
    }

    /// Remove entries older than the sliding window.
    fn prune(&mut self, now: Instant) {
        let cutoff = now.checked_sub(self.window).unwrap_or(now);
        self.successes.retain(|t| *t >= cutoff);
        self.failures.retain(|t| *t >= cutoff);
    }

    fn record_success(&mut self, now: Instant) {
        self.prune(now);
        self.successes.push(now);
    }

    fn record_failure(&mut self, now: Instant) {
        self.prune(now);
        self.failures.push(now);
    }

    fn total(&self) -> usize {
        self.successes.len() + self.failures.len()
    }

    /// Failure ratio over the current window. Returns 0.0 when there are no
    /// entries so that an empty window never trips the breaker.
    fn error_rate(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        self.failures.len() as f64 / total as f64
    }

    /// Reset counters (used on state transitions to Closed).
    fn reset(&mut self) {
        self.successes.clear();
        self.failures.clear();
    }
}

// ---------------------------------------------------------------------------
// CircuitBreaker (per-provider)
// ---------------------------------------------------------------------------

struct CircuitBreaker {
    state: BreakerState,
    window: SlidingWindow,
    opened_at: Option<Instant>,
    cooldown: Duration,
    failure_threshold: f64,
    min_requests: usize,
}

impl CircuitBreaker {
    fn new(
        window: Duration,
        cooldown: Duration,
        failure_threshold: f64,
        min_requests: usize,
    ) -> Self {
        Self {
            state: BreakerState::Closed,
            window: SlidingWindow::new(window),
            opened_at: None,
            cooldown,
            failure_threshold,
            min_requests,
        }
    }

    /// Check whether this breaker allows a request through.
    ///
    /// Returns `Ok(Some((Open, HalfOpen)))` on a state transition,
    /// `Ok(None)` when no transition occurs, or `Err(Open)` when blocked.
    fn check(&mut self, now: Instant) -> Result<Option<(BreakerState, BreakerState)>, BreakerState> {
        match self.state {
            BreakerState::Closed => Ok(None),
            BreakerState::Open => {
                let opened_at = self
                    .opened_at
                    .expect("opened_at must be set when state is Open");
                if now.duration_since(opened_at) >= self.cooldown {
                    self.state = BreakerState::HalfOpen;
                    Ok(Some((BreakerState::Open, BreakerState::HalfOpen)))
                } else {
                    Err(BreakerState::Open)
                }
            }
            BreakerState::HalfOpen => Ok(None),
        }
    }

    fn record_success(&mut self, now: Instant) -> Option<(BreakerState, BreakerState)> {
        self.window.record_success(now);
        if self.state == BreakerState::HalfOpen {
            let old = self.state;
            self.state = BreakerState::Closed;
            self.opened_at = None;
            self.window.reset();
            Some((old, self.state))
        } else {
            None
        }
    }

    fn record_failure(&mut self, now: Instant) -> Option<(BreakerState, BreakerState)> {
        self.window.record_failure(now);
        match self.state {
            BreakerState::HalfOpen => {
                let old = self.state;
                self.state = BreakerState::Open;
                self.opened_at = Some(now);
                Some((old, self.state))
            }
            BreakerState::Closed => {
                if self.window.total() >= self.min_requests
                    && self.window.error_rate() > self.failure_threshold
                {
                    let old = self.state;
                    self.state = BreakerState::Open;
                    self.opened_at = Some(now);
                    Some((old, self.state))
                } else {
                    None
                }
            }
            BreakerState::Open => None,
        }
    }
}

// ---------------------------------------------------------------------------
// CircuitBreakerRegistry (shared app state)
// ---------------------------------------------------------------------------

/// Thread-safe registry of per-provider circuit breakers.
///
/// Intended to be wrapped in `Arc` and stored in axum app state so every
/// request handler can check / record results cheaply.
pub struct CircuitBreakerRegistry {
    breakers: RwLock<HashMap<String, Mutex<CircuitBreaker>>>,
    window: Duration,
    cooldown: Duration,
    failure_threshold: f64,
    min_requests: usize,
}

impl CircuitBreakerRegistry {
    /// Create a registry with production defaults (60 s window, 30 s cooldown,
    /// 50 % failure threshold, 5 minimum requests).
    pub fn new() -> Self {
        Self {
            breakers: RwLock::new(HashMap::new()),
            window: Duration::from_secs(60),
            cooldown: Duration::from_secs(30),
            failure_threshold: 0.5,
            min_requests: 5,
        }
    }

    /// Create a registry with custom parameters (useful for testing).
    pub fn with_params(
        window: Duration,
        cooldown: Duration,
        failure_threshold: f64,
        min_requests: usize,
    ) -> Self {
        Self {
            breakers: RwLock::new(HashMap::new()),
            window,
            cooldown,
            failure_threshold,
            min_requests,
        }
    }

    /// Check whether `provider` is available.
    ///
    /// Returns `Ok(())` when the circuit is closed or half-open (probe
    /// allowed). Returns `Err(BreakerState::Open)` when queries should be
    /// rejected.
    pub fn check(&self, provider: &str) -> Result<(), BreakerState> {
        let now = Instant::now();
        self.check_at(provider, now)
    }

    /// Record a successful query to `provider`.
    pub fn record_success(&self, provider: &str) {
        let now = Instant::now();
        self.record_success_at(provider, now);
    }

    /// Record a failed query to `provider`.
    pub fn record_failure(&self, provider: &str) {
        let now = Instant::now();
        self.record_failure_at(provider, now);
    }

    /// Get the current breaker state for `provider`. Returns `Closed` for
    /// unknown providers (no breaker allocated yet).
    pub fn state(&self, provider: &str) -> BreakerState {
        let breakers = self.breakers.read().expect("breaker lock poisoned");
        match breakers.get(provider) {
            Some(mutex) => {
                let breaker = mutex.lock().expect("breaker lock poisoned");
                breaker.state
            }
            None => BreakerState::Closed,
        }
    }

    // -- internal helpers with explicit `now` for deterministic testing ------

    fn check_at(&self, provider: &str, now: Instant) -> Result<(), BreakerState> {
        self.ensure_breaker(provider);
        let breakers = self.breakers.read().expect("breaker lock poisoned");
        let mutex = breakers.get(provider).expect("breaker just inserted");
        let mut breaker = mutex.lock().expect("breaker lock poisoned");
        match breaker.check(now) {
            Ok(Some((from, to))) => {
                tracing::warn!(
                    provider,
                    from = ?from,
                    to = ?to,
                    "circuit breaker state transition"
                );
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(state) => Err(state),
        }
    }

    fn record_success_at(&self, provider: &str, now: Instant) {
        self.ensure_breaker(provider);
        let breakers = self.breakers.read().expect("breaker lock poisoned");
        let mutex = breakers.get(provider).expect("breaker just inserted");
        let mut breaker = mutex.lock().expect("breaker lock poisoned");
        if let Some((from, to)) = breaker.record_success(now) {
            tracing::warn!(
                provider,
                from = ?from,
                to = ?to,
                "circuit breaker state transition"
            );
        }
    }

    fn record_failure_at(&self, provider: &str, now: Instant) {
        self.ensure_breaker(provider);
        let breakers = self.breakers.read().expect("breaker lock poisoned");
        let mutex = breakers.get(provider).expect("breaker just inserted");
        let mut breaker = mutex.lock().expect("breaker lock poisoned");
        if let Some((from, to)) = breaker.record_failure(now) {
            tracing::warn!(
                provider,
                from = ?from,
                to = ?to,
                "circuit breaker state transition"
            );
        }
    }

    /// Lazily insert a breaker for `provider` if one does not already exist.
    fn ensure_breaker(&self, provider: &str) {
        // Fast path: read lock only.
        {
            let breakers = self.breakers.read().expect("breaker lock poisoned");
            if breakers.contains_key(provider) {
                return;
            }
        }
        // Slow path: upgrade to write lock and insert.
        let mut breakers = self.breakers.write().expect("breaker lock poisoned");
        breakers.entry(provider.to_owned()).or_insert_with(|| {
            Mutex::new(CircuitBreaker::new(
                self.window,
                self.cooldown,
                self.failure_threshold,
                self.min_requests,
            ))
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a registry with short durations for fast, deterministic
    /// tests.
    fn test_registry(
        window: Duration,
        cooldown: Duration,
        failure_threshold: f64,
        min_requests: usize,
    ) -> CircuitBreakerRegistry {
        CircuitBreakerRegistry::with_params(window, cooldown, failure_threshold, min_requests)
    }

    // -- Closed state -------------------------------------------------------

    #[test]
    fn closed_stays_closed_with_successes() {
        let reg = test_registry(Duration::from_secs(60), Duration::from_secs(30), 0.5, 5);
        let now = Instant::now();

        for i in 0..20 {
            let t = now + Duration::from_millis(i * 100);
            reg.record_success_at("provider-a", t);
        }

        assert_eq!(reg.state("provider-a"), BreakerState::Closed);
        assert!(
            reg.check_at("provider-a", now + Duration::from_secs(3))
                .is_ok()
        );
    }

    #[test]
    fn closed_trips_to_open_on_high_failure_rate() {
        let reg = test_registry(Duration::from_secs(60), Duration::from_secs(30), 0.5, 5);
        let now = Instant::now();

        // 2 successes + 4 failures = 6 total, 66 % failure rate → trips.
        reg.record_success_at("provider-a", now);
        reg.record_success_at("provider-a", now + Duration::from_millis(10));
        for i in 0..4 {
            reg.record_failure_at("provider-a", now + Duration::from_millis(20 + i * 10));
        }

        assert_eq!(reg.state("provider-a"), BreakerState::Open);
    }

    #[test]
    fn min_requests_prevents_early_tripping() {
        let reg = test_registry(Duration::from_secs(60), Duration::from_secs(30), 0.5, 5);
        let now = Instant::now();

        // 4 failures, 0 successes → 100 % failure rate, but only 4 requests
        // (below the min_requests = 5 threshold).
        for i in 0..4 {
            reg.record_failure_at("provider-a", now + Duration::from_millis(i * 10));
        }

        assert_eq!(reg.state("provider-a"), BreakerState::Closed);
        assert!(
            reg.check_at("provider-a", now + Duration::from_secs(1))
                .is_ok()
        );
    }

    // -- Open state ---------------------------------------------------------

    #[test]
    fn open_rejects_queries() {
        let reg = test_registry(Duration::from_secs(60), Duration::from_secs(30), 0.5, 5);
        let now = Instant::now();

        // Trip the breaker.
        for i in 0..6 {
            reg.record_failure_at("provider-a", now + Duration::from_millis(i * 10));
        }
        assert_eq!(reg.state("provider-a"), BreakerState::Open);

        // Queries within cooldown are rejected.
        let result = reg.check_at("provider-a", now + Duration::from_secs(10));
        assert_eq!(result, Err(BreakerState::Open));
    }

    #[test]
    fn open_transitions_to_half_open_after_cooldown() {
        let cooldown = Duration::from_secs(5);
        let reg = test_registry(Duration::from_secs(60), cooldown, 0.5, 5);
        let now = Instant::now();

        // Trip the breaker.
        for i in 0..6 {
            reg.record_failure_at("provider-a", now + Duration::from_millis(i * 10));
        }
        assert_eq!(reg.state("provider-a"), BreakerState::Open);

        // After cooldown, check transitions to HalfOpen and allows the query.
        let after_cooldown = now + cooldown + Duration::from_secs(1);
        assert!(reg.check_at("provider-a", after_cooldown).is_ok());
        assert_eq!(reg.state("provider-a"), BreakerState::HalfOpen);
    }

    // -- HalfOpen state -----------------------------------------------------

    #[test]
    fn half_open_closes_on_success() {
        let cooldown = Duration::from_secs(5);
        let reg = test_registry(Duration::from_secs(60), cooldown, 0.5, 5);
        let now = Instant::now();

        // Trip → Open.
        for i in 0..6 {
            reg.record_failure_at("provider-a", now + Duration::from_millis(i * 10));
        }

        // Wait for cooldown → HalfOpen.
        let probe_time = now + cooldown + Duration::from_secs(1);
        assert!(reg.check_at("provider-a", probe_time).is_ok());
        assert_eq!(reg.state("provider-a"), BreakerState::HalfOpen);

        // Probe succeeds → Closed.
        reg.record_success_at("provider-a", probe_time + Duration::from_millis(100));
        assert_eq!(reg.state("provider-a"), BreakerState::Closed);
    }

    #[test]
    fn half_open_reopens_on_failure() {
        let cooldown = Duration::from_secs(5);
        let reg = test_registry(Duration::from_secs(60), cooldown, 0.5, 5);
        let now = Instant::now();

        // Trip → Open.
        for i in 0..6 {
            reg.record_failure_at("provider-a", now + Duration::from_millis(i * 10));
        }

        // Wait for cooldown → HalfOpen.
        let probe_time = now + cooldown + Duration::from_secs(1);
        assert!(reg.check_at("provider-a", probe_time).is_ok());
        assert_eq!(reg.state("provider-a"), BreakerState::HalfOpen);

        // Probe fails → back to Open.
        reg.record_failure_at("provider-a", probe_time + Duration::from_millis(100));
        assert_eq!(reg.state("provider-a"), BreakerState::Open);
    }

    #[test]
    fn half_open_allows_probe_query() {
        let cooldown = Duration::from_secs(5);
        let reg = test_registry(Duration::from_secs(60), cooldown, 0.5, 5);
        let now = Instant::now();

        // Trip → Open → wait → HalfOpen.
        for i in 0..6 {
            reg.record_failure_at("provider-a", now + Duration::from_millis(i * 10));
        }
        let probe_time = now + cooldown + Duration::from_secs(1);
        assert!(reg.check_at("provider-a", probe_time).is_ok());
        assert_eq!(reg.state("provider-a"), BreakerState::HalfOpen);

        // A second check while still HalfOpen also returns Ok (the probe is
        // in-flight).
        assert!(
            reg.check_at("provider-a", probe_time + Duration::from_millis(50))
                .is_ok()
        );
    }

    // -- Sliding window -----------------------------------------------------

    #[test]
    fn window_pruning_removes_old_entries() {
        let window = Duration::from_secs(2);
        let reg = test_registry(window, Duration::from_secs(30), 0.5, 5);
        let now = Instant::now();

        // Record 6 failures at t=0 (trips the breaker).
        for i in 0..6 {
            reg.record_failure_at("provider-a", now + Duration::from_millis(i * 10));
        }
        assert_eq!(reg.state("provider-a"), BreakerState::Open);

        // Manually reset to Closed to test pruning in isolation. We do this by
        // going through the HalfOpen → Closed path.
        let after_cooldown = now + Duration::from_secs(31);
        assert!(reg.check_at("provider-a", after_cooldown).is_ok()); // → HalfOpen
        reg.record_success_at("provider-a", after_cooldown); // → Closed

        // Now the window should have been reset on the Closed transition.
        // Record 3 failures well after the original window expired. Because
        // the old entries were pruned/reset and total < min_requests, the
        // breaker stays Closed.
        let later = after_cooldown + Duration::from_secs(1);
        for i in 0..3 {
            reg.record_failure_at("provider-a", later + Duration::from_millis(i * 10));
        }
        assert_eq!(reg.state("provider-a"), BreakerState::Closed);
    }

    #[test]
    fn old_failures_expire_from_window() {
        let window = Duration::from_secs(2);
        let reg = test_registry(window, Duration::from_secs(30), 0.5, 3);
        let now = Instant::now();

        // 3 failures at t=0 — trips the breaker (3 >= min_requests, 100 %).
        for i in 0..3 {
            reg.record_failure_at("provider-a", now + Duration::from_millis(i * 10));
        }
        assert_eq!(reg.state("provider-a"), BreakerState::Open);

        // Reset via HalfOpen → Closed.
        let after_cooldown = now + Duration::from_secs(31);
        assert!(reg.check_at("provider-a", after_cooldown).is_ok());
        reg.record_success_at("provider-a", after_cooldown);
        assert_eq!(reg.state("provider-a"), BreakerState::Closed);

        // Record 2 failures at t+40 s (old failures are outside the 2 s
        // window and have been pruned on reset). total = 2 < min_requests = 3.
        let much_later = now + Duration::from_secs(40);
        reg.record_failure_at("provider-a", much_later);
        reg.record_failure_at("provider-a", much_later + Duration::from_millis(10));
        assert_eq!(reg.state("provider-a"), BreakerState::Closed);
    }

    // -- Registry: unknown providers ----------------------------------------

    #[test]
    fn unknown_provider_returns_closed() {
        let reg = CircuitBreakerRegistry::new();
        assert_eq!(reg.state("never-seen"), BreakerState::Closed);
    }

    #[test]
    fn check_creates_breaker_lazily() {
        let reg = CircuitBreakerRegistry::new();
        assert!(reg.check("new-provider").is_ok());
        assert_eq!(reg.state("new-provider"), BreakerState::Closed);
    }

    // -- Multiple providers are independent ---------------------------------

    #[test]
    fn providers_are_independent() {
        let reg = test_registry(Duration::from_secs(60), Duration::from_secs(5), 0.5, 5);
        let now = Instant::now();

        // Trip provider-a.
        for i in 0..6 {
            reg.record_failure_at("provider-a", now + Duration::from_millis(i * 10));
        }
        assert_eq!(reg.state("provider-a"), BreakerState::Open);

        // provider-b is unaffected.
        assert_eq!(reg.state("provider-b"), BreakerState::Closed);
        assert!(
            reg.check_at("provider-b", now + Duration::from_secs(1))
                .is_ok()
        );
    }

    // -- Full lifecycle ------------------------------------------------------

    #[test]
    fn full_lifecycle_closed_open_half_open_closed() {
        let cooldown = Duration::from_secs(5);
        let reg = test_registry(Duration::from_secs(60), cooldown, 0.5, 5);
        let now = Instant::now();

        // 1. Start Closed.
        assert_eq!(reg.state("p"), BreakerState::Closed);

        // 2. Accumulate failures → Open.
        for i in 0..6 {
            reg.record_failure_at("p", now + Duration::from_millis(i * 10));
        }
        assert_eq!(reg.state("p"), BreakerState::Open);
        assert_eq!(
            reg.check_at("p", now + Duration::from_secs(1)),
            Err(BreakerState::Open)
        );

        // 3. Wait for cooldown → HalfOpen.
        let probe_time = now + cooldown + Duration::from_secs(1);
        assert!(reg.check_at("p", probe_time).is_ok());
        assert_eq!(reg.state("p"), BreakerState::HalfOpen);

        // 4. Probe succeeds → Closed.
        reg.record_success_at("p", probe_time + Duration::from_millis(100));
        assert_eq!(reg.state("p"), BreakerState::Closed);

        // 5. Subsequent queries proceed normally.
        assert!(
            reg.check_at("p", probe_time + Duration::from_secs(1))
                .is_ok()
        );
    }

    #[test]
    fn full_lifecycle_with_half_open_failure_retry() {
        let cooldown = Duration::from_secs(5);
        let reg = test_registry(Duration::from_secs(60), cooldown, 0.5, 5);
        let now = Instant::now();

        // Trip → Open.
        for i in 0..6 {
            reg.record_failure_at("p", now + Duration::from_millis(i * 10));
        }

        // Cooldown → HalfOpen → probe fails → Open again.
        let probe1 = now + cooldown + Duration::from_secs(1);
        assert!(reg.check_at("p", probe1).is_ok());
        reg.record_failure_at("p", probe1 + Duration::from_millis(100));
        assert_eq!(reg.state("p"), BreakerState::Open);

        // Still Open before second cooldown.
        let mid = probe1 + Duration::from_secs(2);
        assert_eq!(reg.check_at("p", mid), Err(BreakerState::Open));

        // Second cooldown → HalfOpen → probe succeeds → Closed.
        let probe2 = probe1 + cooldown + Duration::from_secs(1);
        assert!(reg.check_at("p", probe2).is_ok());
        assert_eq!(reg.state("p"), BreakerState::HalfOpen);
        reg.record_success_at("p", probe2 + Duration::from_millis(100));
        assert_eq!(reg.state("p"), BreakerState::Closed);
    }

    // -- Edge: exact threshold boundary -------------------------------------

    #[test]
    fn exact_threshold_does_not_trip() {
        // failure_threshold = 0.5, so >50 % is required. Exactly 50 % should
        // NOT trip (we use strict `>`, not `>=`).
        let reg = test_registry(Duration::from_secs(60), Duration::from_secs(30), 0.5, 4);
        let now = Instant::now();

        // 2 successes + 2 failures = 50 % — should stay Closed.
        reg.record_success_at("p", now);
        reg.record_success_at("p", now + Duration::from_millis(10));
        reg.record_failure_at("p", now + Duration::from_millis(20));
        reg.record_failure_at("p", now + Duration::from_millis(30));

        assert_eq!(reg.state("p"), BreakerState::Closed);
    }
}
