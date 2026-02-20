use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// State of a circuit breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation — all calls pass through.
    Closed,
    /// Breaker tripped — calls are rejected.
    Open,
    /// Cooldown elapsed — one probe call allowed.
    HalfOpen,
}

/// A circuit breaker that opens after consecutive failures exceed a threshold,
/// and closes again after a successful probe during half-open state.
pub struct CircuitBreaker {
    name: String,
    failure_count: AtomicU32,
    failure_threshold: u32,
    cooldown: Duration,
    /// Guards (state, last_failure_time). Uses std::sync::Mutex because
    /// this is never held across await points.
    inner: Mutex<CircuitInner>,
}

struct CircuitInner {
    state: CircuitState,
    last_failure: Option<Instant>,
}

impl CircuitBreaker {
    pub fn new(name: &str, failure_threshold: u32, cooldown_seconds: u64) -> Self {
        Self {
            name: name.to_string(),
            failure_count: AtomicU32::new(0),
            failure_threshold,
            cooldown: Duration::from_secs(cooldown_seconds),
            inner: Mutex::new(CircuitInner {
                state: CircuitState::Closed,
                last_failure: None,
            }),
        }
    }

    /// Check whether a call should be allowed.
    pub fn allow(&self) -> bool {
        let mut inner = self.inner.lock().unwrap();

        match inner.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if cooldown has elapsed → transition to HalfOpen.
                if let Some(last) = inner.last_failure {
                    if last.elapsed() >= self.cooldown {
                        inner.state = CircuitState::HalfOpen;
                        tracing::info!(
                            circuit = %self.name,
                            "Circuit breaker transitioning to half-open"
                        );
                        true
                    } else {
                        false
                    }
                } else {
                    // Shouldn't happen, but be safe.
                    inner.state = CircuitState::Closed;
                    true
                }
            }
            CircuitState::HalfOpen => {
                // Allow one probe call (already transitioned).
                true
            }
        }
    }

    /// Record a successful call — reset failure count, close circuit.
    pub fn record_success(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
        let mut inner = self.inner.lock().unwrap();

        if inner.state != CircuitState::Closed {
            tracing::info!(
                circuit = %self.name,
                previous_state = ?inner.state,
                "Circuit breaker closing after success"
            );
            inner.state = CircuitState::Closed;
            metrics::counter!("circuit_breaker.recoveries", "circuit" => self.name.clone())
                .increment(1);
        }
    }

    /// Record a failed call — increment failure count, potentially open circuit.
    pub fn record_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        let mut inner = self.inner.lock().unwrap();

        inner.last_failure = Some(Instant::now());

        if count >= self.failure_threshold && inner.state != CircuitState::Open {
            tracing::warn!(
                circuit = %self.name,
                failures = count,
                threshold = self.failure_threshold,
                "Circuit breaker OPEN"
            );
            inner.state = CircuitState::Open;
            metrics::counter!("circuit_breaker.trips", "circuit" => self.name.clone()).increment(1);
        }
    }

    /// Get the current state of the circuit breaker.
    pub fn current_state(&self) -> CircuitState {
        self.inner.lock().unwrap().state
    }

    /// Get the circuit breaker name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Registry of circuit breakers for all external dependencies.
pub struct CircuitBreakerRegistry {
    pub neo4j: CircuitBreaker,
    pub postgres: CircuitBreaker,
    pub redis: CircuitBreaker,
    pub llm_api: CircuitBreaker,
    pub fetch: CircuitBreaker,
}

impl CircuitBreakerRegistry {
    /// Create registry with default thresholds.
    pub fn new() -> Self {
        Self {
            neo4j: CircuitBreaker::new("neo4j", 5, 60),
            postgres: CircuitBreaker::new("postgres", 5, 60),
            redis: CircuitBreaker::new("redis", 5, 60),
            llm_api: CircuitBreaker::new("llm_api", 3, 120),
            fetch: CircuitBreaker::new("fetch", 5, 60),
        }
    }

    /// Check if any hard dependency (Neo4j, Postgres, Redis, LLM) has an open circuit.
    /// Returns the name of the first open circuit, or None.
    pub fn any_hard_open(&self) -> Option<&str> {
        let hard = [&self.neo4j, &self.postgres, &self.redis, &self.llm_api];

        for cb in &hard {
            if cb.current_state() == CircuitState::Open {
                return Some(cb.name());
            }
        }

        None
    }

    /// Emit gauge metrics for all circuit breaker states.
    pub fn report_metrics(&self) {
        let all = [
            &self.neo4j,
            &self.postgres,
            &self.redis,
            &self.llm_api,
            &self.fetch,
        ];

        for cb in &all {
            let state_value = match cb.current_state() {
                CircuitState::Closed => 0.0,
                CircuitState::HalfOpen => 0.5,
                CircuitState::Open => 1.0,
            };
            metrics::gauge!("circuit_breaker.state", "circuit" => cb.name().to_string())
                .set(state_value);
        }
    }
}

impl Default for CircuitBreakerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
