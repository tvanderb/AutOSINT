use std::collections::HashMap;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

/// Per-domain rate limiter using a token bucket algorithm.
pub struct DomainRateLimiter {
    buckets: Mutex<HashMap<String, TokenBucket>>,
    default_rate: f64,
}

struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
    rate: f64, // tokens per second
}

impl TokenBucket {
    fn new(rate: f64) -> Self {
        Self {
            tokens: rate, // Start with a full bucket.
            last_refill: Instant::now(),
            rate,
        }
    }

    fn refill(&mut self) {
        let elapsed = self.last_refill.elapsed().as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.rate).min(self.rate * 2.0);
        self.last_refill = Instant::now();
    }

    fn try_acquire(&mut self) -> bool {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn time_until_available(&mut self) -> Duration {
        self.refill();
        if self.tokens >= 1.0 {
            Duration::ZERO
        } else {
            let needed = 1.0 - self.tokens;
            Duration::from_secs_f64(needed / self.rate)
        }
    }
}

impl DomainRateLimiter {
    /// Create a new rate limiter with the given default rate (requests per second).
    pub fn new(default_rate: f64) -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
            default_rate,
        }
    }

    /// Acquire a permit for the given domain, blocking until one is available.
    /// Returns after waiting at most `timeout`.
    pub async fn acquire(&self, domain: &str, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;

        loop {
            let wait_time = {
                let mut buckets = self.buckets.lock().await;
                let bucket = buckets
                    .entry(domain.to_string())
                    .or_insert_with(|| TokenBucket::new(self.default_rate));

                if bucket.try_acquire() {
                    return Ok(());
                }

                bucket.time_until_available()
            };

            if Instant::now() + wait_time > deadline {
                return Err(format!("Rate limit timeout for domain: {}", domain));
            }

            tokio::time::sleep(wait_time).await;
        }
    }
}
