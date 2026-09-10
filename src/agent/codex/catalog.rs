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
pub(super) struct PermissionProfileListResponse {
    pub(super) data: Vec<PermissionProfileListEntry>,
    #[serde(default)]
    pub(super) next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PermissionProfileListEntry {
    pub(super) id: String,
    #[serde(default)]
    pub(super) description: Option<String>,
    pub(super) allowed: bool,
    #[serde(default)]
    pub(super) extends: Option<String>,
}

pub(super) fn permission_profile_pages(
    cwd: &std::path::Path,
    mut request: impl FnMut(serde_json::Value) -> anyhow::Result<serde_json::Value>,
) -> anyhow::Result<Vec<crate::agent::AgentPermissionProfile>> {
    use anyhow::Context as _;
    use serde_json::json;
    let mut cursor: Option<String> = None;
    let mut seen_cursors = std::collections::HashSet::new();
    let mut profiles = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();
    loop {
        let response = request(json!({"cwd":cwd,"cursor":cursor,"limit":100}))?;
        let page: PermissionProfileListResponse = serde_json::from_value(
            response
                .get("result")
                .cloned()
                .context("permissionProfile/list 缺少 result")?,
        )?;
        for entry in page.data {
            if !seen_ids.insert(entry.id.clone()) {
                anyhow::bail!("权限配置出现重复 id：{}", entry.id);
            }
            profiles.push(crate::agent::AgentPermissionProfile {
                id: entry.id,
                description: entry.description,
                allowed: entry.allowed,
                extends: entry.extends,
            });
        }
        let Some(next) = page.next_cursor else {
            return Ok(profiles);
        };
        if !seen_cursors.insert(next.clone()) {
            anyhow::bail!("permissionProfile/list 返回循环分页 cursor：{next}");
        }
        cursor = Some(next);
    }
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
