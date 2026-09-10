//! A side composer shares the production turn and permission paths.

use super::{ComposerView, ConversationChanged, PermissionMode};
use crate::{
    agent::{AgentBackend, AgentEffectivePermissions, AgentModel, SideConversationRequest},
    conversation::ConversationPhase,
    theme::ThemeMode,
};
use gpui::{App, Context};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct SideChatConfiguration {
    pub request: SideConversationRequest,
    models: Vec<AgentModel>,
    permission_mode: PermissionMode,
    permission_profile: Option<String>,
    permissions: Option<AgentEffectivePermissions>,
}

impl ComposerView {
    pub(crate) fn side_chat_configuration(&self) -> Option<SideChatConfiguration> {
        Some(SideChatConfiguration {
            request: SideConversationRequest {
                parent_thread_id: self.conversation.thread_id.clone()?,
                cwd: self.conversation.cwd.clone(),
                model: (!self.conversation.selected_model.is_empty())
                    .then(|| self.conversation.selected_model.clone()),
                effort: (!self.conversation.selected_effort.is_empty())
                    .then(|| self.conversation.selected_effort.clone()),
                service_tier: self.conversation.selected_service_tier.clone(),
            },
            models: self.conversation.models.clone(),
            permission_mode: self.permission_mode,
            permission_profile: self.permission_selected_profile.clone(),
            permissions: self.conversation.effective_permissions.clone(),
        })
    }

    pub(crate) fn new_side_chat(
        mode: ThemeMode,
        backend: Arc<dyn AgentBackend>,
        config: SideChatConfiguration,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self::new_with_backend(mode, backend, cx);
        view.side_chat = true;
        view.prompt_editor
            .update(cx, |editor, _| editor.set_accessible_name("侧边聊天输入框"));
        view.side_ready = false;
        view.conversation.cwd = config.request.cwd;
        view.conversation.models = config.models;
        view.conversation.selected_model = config.request.model.unwrap_or_default();
        view.conversation.selected_effort = config.request.effort.unwrap_or_default();
        view.conversation.selected_service_tier = config.request.service_tier;
        view.permission_mode = config.permission_mode;
        view.permission_selected_profile = config.permission_profile;
        view.conversation.effective_permissions = config.permissions;
        view.conversation.slider_index = view
            .selected_model_entry()
            .and_then(|model| {
                model
                    .supported_reasoning_efforts
                    .iter()
                    .position(|effort| effort.id == view.conversation.selected_effort)
            })
            .unwrap_or(0);
        view
    }

    pub(crate) fn set_side_thread(&mut self, id: String, cx: &mut Context<Self>) {
        self.conversation.thread_id = Some(id);
        self.side_ready = true;
        self.load_permission_catalog(cx);
        cx.notify();
    }

    pub(crate) fn side_chat_title(&self) -> Option<String> {
        self.conversation
            .transcript
            .iter()
            .map(|turn| turn.user_message.clone())
            .find(|text| !text.is_empty())
            .or_else(|| self.conversation.user_message.clone())
    }

    pub(crate) fn has_messages(&self) -> bool {
        self.conversation.user_message.is_some() || !self.conversation.transcript.is_empty()
    }

    pub(crate) fn is_running(&self) -> bool {
        matches!(
            self.conversation.phase,
            ConversationPhase::Starting
                | ConversationPhase::Thinking
                | ConversationPhase::Streaming
                | ConversationPhase::Stopping
        )
    }

    pub(crate) fn close_side_chat(&mut self, cx: &mut Context<Self>) {
        self.side_ready = false;
        self.permission_update_cycle = self.permission_update_cycle.wrapping_add(1);
        self.conversation.permission_change = None;
        self.stop_generation(cx);
    }

    pub(crate) fn prompt_text<'a>(&'a self, cx: &'a App) -> &'a str {
        self.prompt_editor.read(cx).text()
    }

    pub(super) fn clear_prompt(&mut self, cx: &mut Context<Self>) {
        self.prompt_editor
            .update(cx, |editor, cx| editor.set_text_silently("", cx));
    }

    pub(super) fn submit_current_prompt(&mut self, cx: &mut Context<Self>) {
        let text = self.prompt_text(cx).to_owned();
        self.submit_prompt(text, cx);
    }

    pub(crate) fn close_side_menus(&mut self, cx: &mut Context<Self>) {
        if self.context_menu_open {
            self.focus_prompt_pending = true;
        }
        self.context_focus_pending = false;
        self.menu_open = false;
        self.permission_menu_open = false;
        self.submenu = None;
        self.context_menu_open = false;
        cx.notify();
    }

    pub(crate) fn side_composer_height(&self, cx: &App) -> f32 {
        self.composer_body_height(cx) + self.submission_feedback_height()
    }
    pub(super) fn composer_body_height(&self, cx: &App) -> f32 {
        self.prompt_editor.read(cx).composer_height()
            + 54.0
            + if self.prompt_context.files.is_empty() {
                0.0
            } else {
                32.0
            }
    }
    pub(crate) fn set_available_width(
        &mut self,
        width: f32,
        trailing_margin: f32,
        cx: &mut Context<Self>,
    ) {
        if self
            .available_width
            .is_none_or(|previous| (previous - width).abs() > 0.5)
            || self
                .trailing_margin
                .is_none_or(|previous| (previous - trailing_margin).abs() > 0.5)
        {
            self.available_width = Some(width);
            self.trailing_margin = Some(trailing_margin);
            cx.notify();
        }
    }

    pub(crate) fn retry_side_prompt(&mut self, cx: &mut Context<Self>) {
        if !self.is_running()
            && let Some(prompt) = self.conversation.user_message.clone()
        {
            self.submit_prompt(prompt, cx);
        }
    }

    pub(crate) fn side_conversation_disconnected(&mut self, cx: &mut Context<Self>) {
        self.side_ready = false;
        cx.emit(ConversationChanged);
        cx.notify();
    }
}
