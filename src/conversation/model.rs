//! Model catalog selection and effective model lookup.

use super::state::ConversationState;
use crate::agent::{AgentModel, AgentModelCatalog};

impl ConversationState {
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
