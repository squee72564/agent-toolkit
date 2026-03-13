use agent_core::{ProviderInstanceId, ProviderKind};

use crate::runtime_error::{RuntimeError, RuntimeErrorKind};

/// Action taken when a [`FallbackRule`] matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackAction {
    /// Continue routed execution with the next configured target.
    RetryNextTarget,
    /// Stop fallback evaluation and surface the current error.
    Stop,
}

/// Normalized executed-failure context used for rule-driven fallback.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ExecutedFailureContext<'a> {
    pub(crate) error: &'a RuntimeError,
    pub(crate) provider_kind: ProviderKind,
    pub(crate) provider_instance: &'a ProviderInstanceId,
}

/// Match criteria for fallback rule evaluation.
///
/// All non-empty fields must match for the rule to apply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FallbackMatch {
    /// Accepted runtime error kinds.
    pub error_kinds: Vec<RuntimeErrorKind>,
    /// Accepted HTTP status codes.
    pub status_codes: Vec<u16>,
    /// Accepted provider-specific error codes after trimming whitespace.
    pub provider_codes: Vec<String>,
    /// Accepted concrete provider kinds.
    pub provider_kinds: Vec<ProviderKind>,
    /// Accepted registered provider instances.
    pub provider_instances: Vec<ProviderInstanceId>,
}

impl FallbackMatch {
    fn matches(&self, failure: &ExecutedFailureContext<'_>) -> bool {
        if !self.error_kinds.is_empty() && !self.error_kinds.contains(&failure.error.kind) {
            return false;
        }

        if !self.status_codes.is_empty() {
            let Some(status_code) = failure.error.status_code else {
                return false;
            };
            if !self.status_codes.contains(&status_code) {
                return false;
            }
        }

        if !self.provider_codes.is_empty() {
            let Some(provider_code) = failure
                .error
                .provider_code
                .as_deref()
                .and_then(trimmed_non_empty)
            else {
                return false;
            };
            if !self
                .provider_codes
                .iter()
                .filter_map(|code| trimmed_non_empty(code))
                .any(|code| code == provider_code)
            {
                return false;
            }
        }

        if !self.provider_kinds.is_empty() && !self.provider_kinds.contains(&failure.provider_kind)
        {
            return false;
        }

        if !self.provider_instances.is_empty()
            && !self.provider_instances.contains(failure.provider_instance)
        {
            return false;
        }

        true
    }
}

/// Ordered fallback rule evaluated against a normalized executed failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FallbackRule {
    /// Match criteria for the rule.
    pub when: FallbackMatch,
    /// Action to take when the matcher succeeds.
    pub action: FallbackAction,
}

impl FallbackRule {
    /// Creates a rule that retries the next target for a specific HTTP status.
    pub fn retry_on_status(status_code: u16) -> Self {
        Self {
            when: FallbackMatch {
                status_codes: vec![status_code],
                ..FallbackMatch::default()
            },
            action: FallbackAction::RetryNextTarget,
        }
    }

    /// Creates a rule that retries the next target for a runtime error kind.
    pub fn retry_on_kind(kind: RuntimeErrorKind) -> Self {
        Self {
            when: FallbackMatch {
                error_kinds: vec![kind],
                ..FallbackMatch::default()
            },
            action: FallbackAction::RetryNextTarget,
        }
    }

    /// Creates a rule that retries the next target for a provider error code.
    pub fn retry_on_provider_code(provider_code: impl Into<String>) -> Self {
        Self {
            when: FallbackMatch {
                provider_codes: vec![provider_code.into()],
                ..FallbackMatch::default()
            },
            action: FallbackAction::RetryNextTarget,
        }
    }

    /// Creates a rule that stops fallback when a runtime error kind matches.
    pub fn stop_on_kind(kind: RuntimeErrorKind) -> Self {
        Self {
            when: FallbackMatch {
                error_kinds: vec![kind],
                ..FallbackMatch::default()
            },
            action: FallbackAction::Stop,
        }
    }

    /// Restricts the rule to a concrete provider kind.
    pub fn for_provider_kind(mut self, provider_kind: ProviderKind) -> Self {
        if !self.when.provider_kinds.contains(&provider_kind) {
            self.when.provider_kinds.push(provider_kind);
        }
        self
    }

    /// Restricts the rule to a registered provider instance.
    pub fn for_provider_instance(mut self, provider_instance: ProviderInstanceId) -> Self {
        if !self.when.provider_instances.contains(&provider_instance) {
            self.when.provider_instances.push(provider_instance);
        }
        self
    }
}

/// Ordered fallback configuration for routed execution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FallbackPolicy {
    /// Explicit ordered rules.
    pub rules: Vec<FallbackRule>,
}

impl FallbackPolicy {
    /// Creates a fallback policy with no implicit retry behavior.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a fallback rule.
    pub fn with_rule(mut self, rule: FallbackRule) -> Self {
        self.rules.push(rule);
        self
    }

    pub(crate) fn should_retry_next_target(
        &self,
        error: &RuntimeError,
        provider_kind: ProviderKind,
        provider_instance: &ProviderInstanceId,
    ) -> bool {
        matches!(
            self.action_for(&ExecutedFailureContext {
                error,
                provider_kind,
                provider_instance,
            }),
            FallbackAction::RetryNextTarget
        )
    }

    fn action_for(&self, failure: &ExecutedFailureContext<'_>) -> FallbackAction {
        self.rules
            .iter()
            .find(|rule| rule.when.matches(failure))
            .map(|rule| rule.action)
            .unwrap_or(FallbackAction::Stop)
    }
}

fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
