use std::time::Duration;

use crate::provider::ProviderConfig;

#[test]
fn provider_config_debug_redacts_api_key() {
    let config = ProviderConfig::new("super-secret-key")
        .with_base_url("https://example.test/v1")
        .with_default_model("gpt-5-mini")
        .with_request_timeout(Duration::from_secs(10))
        .with_stream_timeout(Duration::from_secs(20));

    let debug = format!("{config:?}");

    assert!(debug.contains("ProviderConfig"));
    assert!(debug.contains("api_key: \"<redacted>\""));
    assert!(debug.contains("base_url: Some(\"https://example.test/v1\")"));
    assert!(debug.contains("default_model: Some(\"gpt-5-mini\")"));
    assert!(!debug.contains("super-secret-key"));
}
