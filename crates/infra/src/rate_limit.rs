use std::time::Duration;

use tokio::sync::{Mutex, Semaphore, SemaphorePermit};
use tokio::time::Instant;

/// Serializes requests so the configured requests-per-second limit is
/// respected across all concurrent lookups.
pub struct RateLimiter {
    semaphore: Semaphore,
    min_interval: Duration,
    last: Mutex<Instant>,
}

impl RateLimiter {
    pub fn new(requests_per_second: u32) -> Self {
        let rps = requests_per_second.clamp(1, 50);
        Self {
            semaphore: Semaphore::new(1),
            min_interval: Duration::from_micros(1_000_000 / u64::from(rps)),
            last: Mutex::new(Instant::now() - Duration::from_secs(1)),
        }
    }

    pub async fn acquire(&self) -> SemaphorePermit<'_> {
        let permit = self
            .semaphore
            .acquire()
            .await
            .expect("rate limiter semaphore is never closed");
        let mut last = self.last.lock().await;
        let elapsed = last.elapsed();
        if elapsed < self.min_interval {
            tokio::time::sleep(self.min_interval - elapsed).await;
        }
        *last = Instant::now();
        permit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn serializes_requests() {
        let limiter = RateLimiter::new(50);
        let start = Instant::now();
        for _ in 0..3 {
            let _permit = limiter.acquire().await;
        }
        assert!(start.elapsed() >= Duration::from_millis(35));
    }
}
