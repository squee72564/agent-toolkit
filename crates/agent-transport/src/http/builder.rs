use std::time::Duration;

use crate::http::retry_policy::RetryPolicy;
use crate::http::sse::SseLimits;
use crate::http::transport::HttpTransport;

#[derive(Clone)]
pub struct HttpTransportBuilder {
    pub(crate) client: reqwest::Client,
    pub(crate) retry_policy: RetryPolicy,
    pub(crate) request_timeout: Duration,
    pub(crate) stream_timeout: Duration,
    pub(crate) sse_limits: SseLimits,
}

impl HttpTransportBuilder {
    pub fn retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    pub fn stream_timeout(mut self, timeout: Duration) -> Self {
        self.stream_timeout = timeout;
        self
    }

    pub fn sse_limits(mut self, sse_limits: SseLimits) -> Self {
        self.sse_limits = sse_limits;
        self
    }

    pub fn build(self) -> HttpTransport {
        HttpTransport {
            client: self.client,
            retry_policy: self.retry_policy,
            request_timeout: self.request_timeout,
            stream_timeout: self.stream_timeout,
            sse_limits: self.sse_limits,
        }
    }
}
