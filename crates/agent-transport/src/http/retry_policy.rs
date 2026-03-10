use reqwest::StatusCode;
use std::time::Duration;

/// Retry settings applied before a response body is consumed.
#[derive(Debug, Clone, PartialEq)]
pub struct RetryPolicy {
    /// Total number of attempts, including the initial request.
    pub max_attempts: u8,
    /// Base delay used for exponential backoff.
    pub initial_backoff: Duration,
    /// Maximum delay allowed for any retry backoff.
    pub max_backoff: Duration,
    /// HTTP statuses that should trigger another attempt when seen before body handling begins.
    pub retryable_status_codes: Vec<StatusCode>,
}

impl RetryPolicy {
    /// Returns `true` when `status_code` should trigger a retry.
    pub fn should_retry_status(&self, status_code: StatusCode) -> bool {
        self.retryable_status_codes.contains(&status_code)
    }

    /// Returns the backoff for the retry at `retry_index` using capped exponential growth.
    pub fn backoff_duration_for_retry(&self, retry_index: u8) -> Duration {
        // Exponential backoff: initial_backoff * (2 ^ retry_index), capped.
        let shift = u32::from(retry_index.min(31));
        let multiplier = 1_u32 << shift;
        self.initial_backoff
            .saturating_mul(multiplier)
            .min(self.max_backoff)
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_millis(2_000),
            retryable_status_codes: vec![
                StatusCode::REQUEST_TIMEOUT,
                StatusCode::TOO_MANY_REQUESTS,
                StatusCode::INTERNAL_SERVER_ERROR,
                StatusCode::BAD_GATEWAY,
                StatusCode::SERVICE_UNAVAILABLE,
                StatusCode::GATEWAY_TIMEOUT,
            ],
        }
    }
}
