//! Model catalog selection and effective model lookup.

use super::state::ConversationState;
use crate::agent::{AgentModel, AgentModelCatalog};

impl ConversationState {
    pub(crate) fn apply_config_defaults(&mut self, config: &crate::agent::AgentConfigSnapshot) {
        if self.thread_id.is_some() || self.model_user_selected || self.user_message.is_some() {
            return;
        }
        let requirements = config.requirements.as_ref();
        let string = |key: &str| {
            requirements
                .and_then(|requirements| requirements.enforced.get(key))
                .filter(|value| !value.is_null())
                .or_else(|| config.effective.get(key))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };
        if let Some(model) = string("model") {
            self.selected_model = model;
        }
        if let Some(effort) = string("model_reasoning_effort") {
            self.selected_effort = effort;
        }
        self.selected_service_tier = string("service_tier");
        self.plan_default_effort = config
            .effective
            .get("plan_mode_reasoning_effort")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        if let Some(index) = self.selected_model_entry().and_then(|model| {
            model
                .supported_reasoning_efforts
                .iter()
                .position(|effort| effort.id == self.selected_effort)
        }) {
            self.slider_index = index;
        }
    }

    pub(crate) fn apply_model_catalog(&mut self, catalog: AgentModelCatalog) {
        let previous_model = self.selected_model.clone();
        let previous_effort = self.selected_effort.clone();
        let previous_service_tier = self.selected_service_tier.clone();

        self.models = catalog.models;
        self.model_catalog_error = None;
        if self.models.is_empty() {
            self.set_model_catalog_error("Codex 未返回可用模型".to_owned());
            return;
        }

        let selected_index = self
            .models
            .iter()
            .position(|model| model.model == previous_model)
            .or_else(|| self.models.iter().position(|model| model.is_default))
            .unwrap_or(0);
        let preserve_options = self.models[selected_index].model == previous_model;
        self.apply_model_selection(
            selected_index,
            preserve_options.then_some(previous_effort),
            preserve_options.then_some(previous_service_tier).flatten(),
            preserve_options,
        );
    }
    pub(crate) fn set_model_catalog_error(&mut self, error: String) {
        self.models.clear();
        self.model_catalog_error = Some(error);
        self.selected_model.clear();
        self.selected_effort.clear();
        self.selected_service_tier = None;
        self.actual_model = None;
        self.model_status = None;
        self.safety_buffering = false;
        self.slider_index = 0;
    }
    pub(crate) fn apply_model_selection(
        &mut self,
        index: usize,
        preferred_effort: Option<String>,
        preferred_service_tier: Option<String>,
        preserve_standard_tier: bool,
    ) {
        let Some(model) = self.models.get(index).cloned() else {
            return;
        };
        self.selected_model = model.model;
        self.selected_effort = preferred_effort
            .filter(|effort| {
                model
                    .supported_reasoning_efforts
                    .iter()
                    .any(|option| option.id == *effort)
            })
            .or_else(|| {
                model
                    .supported_reasoning_efforts
                    .iter()
                    .any(|option| option.id == model.default_reasoning_effort)
                    .then(|| model.default_reasoning_effort.clone())
            })
            .or_else(|| {
                model
                    .supported_reasoning_efforts
                    .first()
                    .map(|option| option.id.clone())
            })
            .unwrap_or_else(|| model.default_reasoning_effort.clone());

        self.selected_service_tier = if preserve_standard_tier && preferred_service_tier.is_none() {
            None
        } else {
            preferred_service_tier
                .filter(|tier| model.service_tiers.iter().any(|option| option.id == *tier))
                .or_else(|| {
                    model
                        .default_service_tier
                        .filter(|tier| model.service_tiers.iter().any(|option| option.id == *tier))
                })
        };
        self.slider_index = model
            .supported_reasoning_efforts
            .iter()
            .position(|option| option.id == self.selected_effort)
            .unwrap_or(0);
        self.actual_model = None;
        self.model_status = None;
        self.safety_buffering = false;
    }
    pub(crate) fn selected_model_entry(&self) -> Option<&AgentModel> {
        self.models
            .iter()
            .find(|model| model.model == self.selected_model)
    }
    pub(crate) fn model_display_name<'a>(&'a self, model_name: &'a str) -> &'a str {
        self.models
            .iter()
            .find(|model| model.model == model_name || model.id == model_name)
            .map(|model| model.display_name.as_str())
            .unwrap_or(model_name)
    }
}

#[cfg(test)]
mod config_default_tests {
    use super::*;
    #[test]
    fn session_static_defaults_apply_to_new_untouched_drafts_only() {
        let mut state = ConversationState::default();
        let mut config = crate::agent::AgentConfigSnapshot {
            generation: 1,
            cwd: "/work".into(),
            effective: serde_json::json!({"model":"model-a","model_reasoning_effort":"ultra","plan_mode_reasoning_effort":"max","service_tier":"priority"}),
            origins: Default::default(),
            layers: None,
            requirements: None,
            value_aliases: Default::default(),
            value_defaults: Default::default(),
            profile_parents: Default::default(),
            required_fields: Default::default(),
        };
        state.apply_config_defaults(&config);
        assert_eq!(state.selected_model, "model-a");
        assert_eq!(state.selected_effort, "ultra");
        assert_eq!(state.plan_default_effort.as_deref(), Some("max"));
        assert_eq!(state.selected_service_tier.as_deref(), Some("priority"));
        state.thread_id = Some("existing".into());
        config.effective["model"] = serde_json::json!("model-b");
        state.apply_config_defaults(&config);
        assert_eq!(state.selected_model, "model-a");
        state.thread_id = None;
        state.model_user_selected = true;
        state.apply_config_defaults(&config);
        assert_eq!(state.selected_model, "model-a");
    }
}
