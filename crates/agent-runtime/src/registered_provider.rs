use agent_core::{PlatformConfig, ProviderDescriptor, ProviderInstanceId, ProviderKind};

use crate::provider_config::ProviderConfig;
use crate::runtime_error::RuntimeError;

/// Runtime registration record for one concrete provider instance.
#[derive(Debug, Clone)]
pub struct RegisteredProvider {
    /// Registered provider instance id.
    pub instance_id: ProviderInstanceId,
    /// Concrete provider kind used to resolve the adapter.
    pub kind: ProviderKind,
    /// Runtime-owned per-instance configuration.
    pub config: ProviderConfig,
}

impl RegisteredProvider {
    /// Creates a new registered provider record.
    pub fn new(
        instance_id: ProviderInstanceId,
        kind: ProviderKind,
        config: ProviderConfig,
    ) -> Self {
        Self {
            instance_id,
            kind,
            config,
        }
    }

    /// Resolves transport-facing platform configuration from descriptor + config.
    pub fn platform_config(
        &self,
        descriptor: &ProviderDescriptor,
    ) -> Result<PlatformConfig, RuntimeError> {
        if descriptor.kind != self.kind {
            return Err(RuntimeError::configuration(format!(
                "provider descriptor kind {:?} does not match registered provider kind {:?}",
                descriptor.kind, self.kind
            )));
        }

        let base_url = self
            .config
            .base_url
            .as_deref()
            .unwrap_or(descriptor.default_base_url)
            .trim()
            .to_string();
        if base_url.is_empty() {
            return Err(RuntimeError::configuration(format!(
                "base_url is empty for provider instance {}",
                self.instance_id
            )));
        }

        let parsed = reqwest::Url::parse(&base_url)
            .map_err(|error| RuntimeError::configuration(format!("invalid base_url: {error}")))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(RuntimeError::configuration(format!(
                "base_url must use http or https for provider instance {}",
                self.instance_id
            )));
        }

        Ok(PlatformConfig {
            protocol: descriptor.protocol.clone(),
            base_url: parsed.to_string().trim_end_matches('/').to_string(),
            auth_style: descriptor.default_auth_style.clone(),
            request_id_header: self
                .config
                .request_id_header
                .clone()
                .unwrap_or_else(|| descriptor.default_request_id_header.clone()),
            default_headers: descriptor.default_headers.clone(),
        })
    }
}
