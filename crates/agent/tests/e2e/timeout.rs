use std::future::Future;
use std::time::Duration;

const DEFAULT_TEST_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn with_test_timeout<F, T>(future: F) -> T
where
    F: Future<Output = T>,
{
    with_timeout(DEFAULT_TEST_TIMEOUT, future).await
}

pub async fn with_timeout<F, T>(duration: Duration, future: F) -> T
where
    F: Future<Output = T>,
{
    tokio::time::timeout(duration, future)
        .await
        .expect("test operation timed out")
}
