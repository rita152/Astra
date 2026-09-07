//! Codex model and permission catalog wire types.

use serde::Deserialize;

use crate::agent::{AgentModel, AgentReasoningEffort, AgentServiceTier};

pub(super) const MODEL_LIST_PAGE_SIZE: u32 = 50;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ModelListResponse {
    pub(super) data: Vec<ModelListEntry>,
    #[serde(default)]
    pub(super) next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub(super) struct PermissionProfileListResponse {
    pub(super) data: Vec<PermissionProfileListEntry>,
    #[serde(default)]
    pub(super) next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub(super) struct PermissionProfileListEntry {
    pub(super) id: String,
    pub(super) allowed: bool,
    #[serde(default)]
    pub(super) extends: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ModelListEntry {
    pub(super) id: String,
    pub(super) model: String,
    pub(super) display_name: String,
    pub(super) description: String,
    pub(super) hidden: bool,
    pub(super) supported_reasoning_efforts: Vec<ModelReasoningEffort>,
    pub(super) default_reasoning_effort: String,
    #[serde(default)]
    pub(super) service_tiers: Vec<ModelServiceTier>,
    #[serde(default)]
    pub(super) default_service_tier: Option<String>,
    pub(super) is_default: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ModelReasoningEffort {
    pub(super) reasoning_effort: String,
    pub(super) description: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct ModelServiceTier {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) description: String,
}

impl From<ModelListEntry> for AgentModel {
    fn from(entry: ModelListEntry) -> Self {
        Self {
            id: entry.id,
            model: entry.model,
            display_name: entry.display_name,
            description: entry.description,
            supported_reasoning_efforts: entry
                .supported_reasoning_efforts
                .into_iter()
                .map(|effort| AgentReasoningEffort {
                    id: effort.reasoning_effort,
                    description: effort.description,
                })
                .collect(),
            default_reasoning_effort: entry.default_reasoning_effort,
            service_tiers: entry
                .service_tiers
                .into_iter()
                .map(|tier| AgentServiceTier {
                    id: tier.id,
                    name: tier.name,
                    description: tier.description,
                })
                .collect(),
            default_service_tier: entry.default_service_tier,
            is_default: entry.is_default,
        }
    }
}
