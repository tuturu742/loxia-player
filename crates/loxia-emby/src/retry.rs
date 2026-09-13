//! Backoff and retry policy for idempotent GETs.
//!
//! **Idempotent GETs only.** Never wrap a mutation in this — a retried playlist-add would
//! duplicate entries. The connectivity state machine (`08-06`), not this module, owns
//! reconnection after an `Offline` error; retrying here would just stall the UI.

use std::time::Duration;

use crate::error::EmbyError;

const MAX_ATTEMPTS: u32 = 3;
const BASE_BACKOFF: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(5);
const JITTER_FRACTION: f64 = 0.25;

/// Retries `op` up to [`MAX_ATTEMPTS`] times, but only when it fails with
/// [`EmbyError::Transient`]. Every other error — including `Offline` — returns immediately.
/// `op` receives the zero-based attempt number, useful for logging.
pub async fn with_retry<F, Fut, T>(op: F) -> Result<T, EmbyError>
where
    F: Fn(u32) -> Fut,
    Fut: Future<Output = Result<T, EmbyError>>,
{
    let mut attempt = 0;
    loop {
        match op(attempt).await {
            Ok(v) => return Ok(v),
            Err(EmbyError::Transient {
                status,
                retry_after,
            }) if attempt + 1 < MAX_ATTEMPTS => {
                let delay = backoff_delay(attempt, retry_after);
                tracing::debug!(attempt, status, ?delay, "retrying after transient error");
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

/// `250ms * 2^attempt`, jittered by ±25%, capped at 5s — or `retry_after` when it is larger.
fn backoff_delay(attempt: u32, retry_after: Option<Duration>) -> Duration {
    let exp = BASE_BACKOFF.saturating_mul(1u32 << attempt.min(6));
    let jittered = jitter(exp).min(MAX_BACKOFF);
    match retry_after {
        Some(ra) if ra > jittered => ra,
        _ => jittered,
    }
}

fn jitter(d: Duration) -> Duration {
    let factor = 1.0 + (rand::random::<f64>() * 2.0 - 1.0) * JITTER_FRACTION;
    Duration::from_secs_f64((d.as_secs_f64() * factor).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn retries_transient_three_times_then_gives_up() {
        let calls = AtomicU32::new(0);
        let result: Result<(), EmbyError> = with_retry(|_attempt| {
            calls.fetch_add(1, Ordering::SeqCst);
            async {
                Err(EmbyError::Transient {
                    status: 500,
                    retry_after: None,
                })
            }
        })
        .await;
        assert!(result.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), MAX_ATTEMPTS);
    }

    /// A real `reqwest::Error`, obtained from an actual failed connection attempt — the type has
    /// no public constructor, so this is the only way to get one for a test.
    async fn real_connect_error() -> reqwest::Error {
        reqwest::get("http://127.0.0.1:1/").await.unwrap_err()
    }

    #[tokio::test]
    async fn does_not_retry_offline() {
        let calls = AtomicU32::new(0);
        let result: Result<(), EmbyError> = with_retry(|_| async {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(EmbyError::Offline {
                source: real_connect_error().await,
            })
        })
        .await;
        assert!(result.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn does_not_retry_unauthorized() {
        let calls = AtomicU32::new(0);
        let result: Result<(), EmbyError> = with_retry(|_| {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err(EmbyError::Unauthorized) }
        })
        .await;
        assert!(result.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn succeeds_without_retry_when_ok_first_try() {
        let calls = AtomicU32::new(0);
        let result = with_retry(|_| {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Ok::<_, EmbyError>(42) }
        })
        .await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn honours_retry_after_header() {
        let big = Duration::from_secs(20);
        let delay = backoff_delay(0, Some(big));
        assert_eq!(delay, big);
    }

    #[test]
    fn backoff_is_jittered_and_capped() {
        for attempt in 0..6 {
            let base = BASE_BACKOFF.saturating_mul(1u32 << attempt);
            for _ in 0..100 {
                let d = jitter(base).min(MAX_BACKOFF);
                let lo = base.as_secs_f64() * 0.75;
                let hi = (base.as_secs_f64() * 1.25).max(lo);
                assert!(d.as_secs_f64() >= lo.min(d.as_secs_f64()));
                assert!(d <= MAX_BACKOFF);
                assert!(d.as_secs_f64() <= hi + 0.001 || d == MAX_BACKOFF);
            }
        }
    }
}
