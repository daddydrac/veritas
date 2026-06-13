use serde::{Deserialize, Serialize};
use std::env;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GovernanceMode {
    Enforce,
    Advisory,
    Disabled,
}

impl GovernanceMode {
    pub(crate) fn from_env() -> Self {
        if let Ok(raw) = env::var("VERITAS_GOVERNANCE_MODE") {
            return Self::parse(&raw).unwrap_or_else(|| Self::default_for_profile());
        }
        if let Ok(raw) = env::var("VERITAS_SHACL_ENFORCE") {
            return if matches!(raw.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on" | "enforce" | "enforced") {
                Self::Enforce
            } else {
                Self::Advisory
            };
        }
        Self::default_for_profile()
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "enforce" | "enforced" | "required" | "strict" | "production" | "prod" => Some(Self::Enforce),
            "advisory" | "warn" | "warning" | "dev" | "development" | "observe" | "observe_only" => Some(Self::Advisory),
            "disabled" | "disable" | "off" | "none" => Some(Self::Disabled),
            _ => None,
        }
    }

    pub(crate) fn default_for_profile() -> Self {
        let profile = env::var("VERITAS_PROFILE")
            .or_else(|_| env::var("VERITAS_DEPLOYMENT_PROFILE"))
            .unwrap_or_else(|_| "local".to_string())
            .trim()
            .to_ascii_lowercase();
        match profile.as_str() {
            "dev" | "development" | "test" | "unit" => Self::Advisory,
            _ => Self::Enforce,
        }
    }

    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Enforce => "enforce",
            Self::Advisory => "advisory",
            Self::Disabled => "disabled",
        }
    }

    pub(crate) fn enforces(&self) -> bool {
        matches!(self, Self::Enforce)
    }

    pub(crate) fn disabled(&self) -> bool {
        matches!(self, Self::Disabled)
    }

    pub(crate) fn allows_execution_after_findings(&self) -> bool {
        matches!(self, Self::Advisory | Self::Disabled)
    }
}
