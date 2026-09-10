//! Model catalog and effective thread configuration.

use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentReasoningEffort {
    pub id: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentServiceTier {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentModel {
    pub id: String,
    pub model: String,
    pub display_name: String,
    pub description: String,
    pub supported_reasoning_efforts: Vec<AgentReasoningEffort>,
    pub default_reasoning_effort: String,
    pub service_tiers: Vec<AgentServiceTier>,
    pub default_service_tier: Option<String>,
    pub is_default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentModelCatalog {
    pub models: Vec<AgentModel>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentPermissionMode {
    Request,
    Assist,
    Full,
    Custom,
    Profile(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentActivePermissionProfile {
    pub id: String,
    pub extends: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentEffectivePermissions {
    pub approval_policy: Value,
    pub approvals_reviewer: String,
    pub sandbox_policy: Option<Value>,
    pub active_permission_profile: Option<AgentActivePermissionProfile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPermissionProfile {
    pub id: String,
    pub description: Option<String>,
    pub allowed: bool,
    pub extends: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadSettings {
    pub model: String,
    pub effort: Option<String>,
    pub service_tier: Option<String>,
    pub cwd: String,
    pub permissions: Option<AgentEffectivePermissions>,
}

/// A permission mutation is scoped to the original view operation and connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadPermissionUpdate {
    pub thread_id: String,
    pub cwd: std::path::PathBuf,
    pub mode: AgentPermissionMode,
    pub expected_generation: Option<u64>,
    pub operation_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadPermissionResult {
    pub thread_id: String,
    pub generation: u64,
    pub operation_id: u64,
    pub settings: AgentThreadSettings,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadSettingsSnapshot {
    pub thread_id: String,
    pub generation: u64,
    pub settings: AgentThreadSettings,
}
