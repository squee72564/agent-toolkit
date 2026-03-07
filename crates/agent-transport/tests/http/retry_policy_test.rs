use std::time::Duration;

use agent_transport::RetryPolicy;

#[test]
fn retry_policy_backoff_caps_at_max() {
    let policy = RetryPolicy {
        max_attempts: 4,
        initial_backoff: Duration::from_millis(100),
        max_backoff: Duration::from_millis(300),
        retryable_status_codes: vec![],
    };

    assert_eq!(
        policy.backoff_duration_for_retry(0),
        Duration::from_millis(100)
    );
    assert_eq!(
        policy.backoff_duration_for_retry(1),
        Duration::from_millis(200)
    );
    assert_eq!(
        policy.backoff_duration_for_retry(2),
        Duration::from_millis(300)
    );
    assert_eq!(
        policy.backoff_duration_for_retry(10),
        Duration::from_millis(300)
    );
}
