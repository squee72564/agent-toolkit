use agent_core::ProviderId;

use crate::runtime_error::{RuntimeError, RuntimeErrorKind};
use crate::target::Target;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FallbackMode {
    LegacyOnly,
    RulesOnly,
    #[default]
    LegacyOrRules,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FallbackAction {
    RetryNextTarget,
    Stop,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FallbackMatch {
    pub error_kinds: Vec<RuntimeErrorKind>,
    pub status_codes: Vec<u16>,
    pub provider_codes: Vec<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FallbackRule {
    pub when: FallbackMatch,
    pub action: FallbackAction,
}

impl FallbackRule {
    pub fn retry_on_status(status_code: u16) -> Self {
        Self {
            when: FallbackMatch {
                status_codes: vec![status_code],
                ..FallbackMatch::default()
            },
            action: FallbackAction::RetryNextTarget,
        }
    }

    pub fn retry_on_kind(kind: RuntimeErrorKind) -> Self {
        Self {
            when: FallbackMatch {
                error_kinds: vec![kind],
                ..FallbackMatch::default()
            },
            action: FallbackAction::RetryNextTarget,
        }
    }

    pub fn retry_on_provider_code(provider_code: impl Into<String>) -> Self {
        Self {
            when: FallbackMatch {
                provider_codes: vec![provider_code.into()],
                ..FallbackMatch::default()
            },
            action: FallbackAction::RetryNextTarget,
        }
    }

    pub fn stop_on_kind(kind: RuntimeErrorKind) -> Self {
        Self {
            when: FallbackMatch {
                error_kinds: vec![kind],
                ..FallbackMatch::default()
            },
            action: FallbackAction::Stop,
        }
    }

    pub fn for_provider(mut self, provider: ProviderId) -> Self {
        if !self.when.providers.contains(&provider) {
            self.when.providers.push(provider);
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FallbackPolicy {
    pub targets: Vec<Target>,
    pub retry_on_status_codes: Vec<u16>,
    pub retry_on_transport_error: bool,
    pub rules: Vec<FallbackRule>,
    pub mode: FallbackMode,
}

impl FallbackPolicy {
    pub fn new(targets: Vec<Target>) -> Self {
        Self {
            targets,
            ..Self::default()
        }
    }

    pub fn with_mode(mut self, mode: FallbackMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_rule(mut self, rule: FallbackRule) -> Self {
        self.rules.push(rule);
        self
    }

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
