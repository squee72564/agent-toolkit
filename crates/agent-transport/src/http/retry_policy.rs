use reqwest::StatusCode;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct RetryPolicy {
    pub max_attempts: u8,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub retryable_status_codes: Vec<StatusCode>,
}

impl RetryPolicy {
    pub fn should_retry_status(&self, status_code: StatusCode) -> bool {
        self.retryable_status_codes.contains(&status_code)
    }

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
