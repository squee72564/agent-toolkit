use agent_core::ProviderId;

use crate::runtime_error::{RuntimeError, RuntimeErrorKind};
use crate::target::Target;

/// Strategy for combining legacy fallback toggles with explicit rules.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FallbackMode {
    /// Evaluate only legacy transport/status fallback settings and ignore rules.
    LegacyOnly,
    /// Evaluate only explicit rules and ignore legacy transport/status fallback settings.
    RulesOnly,
    /// Retry if either legacy settings or explicit rules allow fallback.
    #[default]
    LegacyOrRules,
}

/// Action taken when a [`FallbackRule`] matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FallbackAction {
    /// Continue routed execution with the next configured target.
    RetryNextTarget,
    /// Stop fallback evaluation and surface the current error.
    Stop,
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
    /// Accepted providers.
    pub providers: Vec<ProviderId>,
}

impl FallbackMatch {
    fn matches(&self, error: &RuntimeError) -> bool {
        if !self.error_kinds.is_empty() && !self.error_kinds.contains(&error.kind) {
            return false;
        }

        if !self.status_codes.is_empty() {
            let Some(status_code) = error.status_code else {
                return false;
            };
            if !self.status_codes.contains(&status_code) {
                return false;
            }
        }

        if !self.provider_codes.is_empty() {
            let Some(provider_code) = error.provider_code.as_deref().and_then(trimmed_non_empty)
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

        if !self.providers.is_empty() {
            let Some(provider) = error.provider else {
                return false;
            };
            if !self.providers.contains(&provider) {
                return false;
            }
        }

        true
    }
}

/// Ordered fallback rule evaluated against a [`RuntimeError`].
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

    /// Restricts the rule to errors originating from a specific provider.
    pub fn for_provider(mut self, provider: ProviderId) -> Self {
        if !self.when.providers.contains(&provider) {
            self.when.providers.push(provider);
        }
        self
    }
}

/// Ordered fallback configuration for routed execution.
///
/// `targets` supplies the fallback destinations after the primary target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FallbackPolicy {
    /// Additional targets attempted after the primary target fails.
    pub targets: Vec<Target>,
    /// Legacy HTTP status codes that trigger fallback.
    pub retry_on_status_codes: Vec<u16>,
    /// Whether transport errors trigger fallback under legacy behavior.
    pub retry_on_transport_error: bool,
    /// Explicit ordered rules.
    pub rules: Vec<FallbackRule>,
    /// How legacy settings and rules are combined.
    pub mode: FallbackMode,
}

impl FallbackPolicy {
    /// Creates a fallback policy with the supplied fallback targets.
    pub fn new(targets: Vec<Target>) -> Self {
        Self {
            targets,
            ..Self::default()
        }
    }

    /// Sets how legacy fallback settings and explicit rules are combined.
    pub fn with_mode(mut self, mode: FallbackMode) -> Self {
        self.mode = mode;
        self
    }

    /// Appends a fallback rule.
    pub fn with_rule(mut self, rule: FallbackRule) -> Self {
        self.rules.push(rule);
        self
    }

    /// Decide whether the given error should advance to the next fallback target.
    ///
    /// Legacy transport/status settings and explicit rules are evaluated according
    /// to [`FallbackMode`]. Rule evaluation is insertion-ordered: the first rule
    /// whose `when` matcher fully matches the error decides the rule-based outcome.
    pub fn should_fallback(&self, error: &RuntimeError) -> bool {
        let legacy_decision = self.should_fallback_legacy(error);
        let rules_decision = self.should_fallback_rules(error);

        match self.mode {
            FallbackMode::LegacyOnly => legacy_decision,
            FallbackMode::RulesOnly => rules_decision,
            FallbackMode::LegacyOrRules => legacy_decision || rules_decision,
        }
    }

    fn should_fallback_legacy(&self, error: &RuntimeError) -> bool {
        if self.retry_on_transport_error && error.kind == RuntimeErrorKind::Transport {
            return true;
        }

        if let Some(status_code) = error.status_code {
            return self.retry_on_status_codes.contains(&status_code);
        }

        false
    }

    fn should_fallback_rules(&self, error: &RuntimeError) -> bool {
        for rule in &self.rules {
            if rule.when.matches(error) {
                return matches!(rule.action, FallbackAction::RetryNextTarget);
            }
        }

        false
    }
}

impl Default for FallbackPolicy {
    fn default() -> Self {
        Self {
            targets: Vec::new(),
            retry_on_status_codes: vec![429, 500, 502, 503, 504],
            retry_on_transport_error: true,
            rules: Vec::new(),
            mode: FallbackMode::LegacyOrRules,
        }
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
