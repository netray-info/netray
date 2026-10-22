//! The site-wide budget of badge and OG fresh runs (`[admission] image_runs_per_minute`): a token
//! bucket holding at most one minute's tokens, refilled continuously. Its own type rather than a
//! governor limiter, because `gate` peeks it before the per-domain recompute limiter and takes it
//! only when the run starts: governor spends a token on every check.

use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;

pub struct ImageBudget {
    per_minute: f64,
    bucket: Mutex<(f64, Instant)>,
}

impl ImageBudget {
    /// A full bucket of `per_minute` tokens; `0` refuses every image run (the config refuses 0).
    pub fn new(per_minute: u32) -> Self {
        Self {
            per_minute: f64::from(per_minute),
            bucket: Mutex::new((f64::from(per_minute), Instant::now())),
        }
    }

    /// The tokens after refilling up to now.
    fn refill(&self) -> std::sync::MutexGuard<'_, (f64, Instant)> {
        let mut bucket = self.bucket.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(bucket.1).as_secs_f64();
        bucket.0 = (bucket.0 + elapsed * self.per_minute / 60.0).min(self.per_minute);
        bucket.1 = now;
        bucket
    }

    /// `Ok` when a token is there, else the whole seconds until one is (at least 1).
    pub fn peek(&self) -> Result<(), u64> {
        let bucket = self.refill();
        if bucket.0 >= 1.0 {
            return Ok(());
        }
        if self.per_minute <= 0.0 {
            return Err(60);
        }
        let wait = Duration::from_secs_f64((1.0 - bucket.0) * 60.0 / self.per_minute);
        Err(wait.as_secs_f64().ceil().max(1.0) as u64)
    }

    /// Takes one token; call it after `peek` answered `Ok` under the same lock of the caller.
    pub fn take(&self) {
        let mut bucket = self.refill();
        bucket.0 = (bucket.0 - 1.0).max(0.0);
    }
}
