use std::time::Duration;

use crate::http::retry_policy::RetryPolicy;
use crate::http::transport::HttpTransport;

#[derive(Clone)]
pub struct HttpTransportBuilder {
    pub client: reqwest::Client,
    pub retry_policy: RetryPolicy,
    pub timeout: Duration,
}

impl HttpTransportBuilder {
    pub fn retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn build(self) -> HttpTransport {
        HttpTransport {
            client: self.client,
            retry_policy: self.retry_policy,
            timeout: self.timeout,
        }
    }
}
