#[cfg(test)]
use crate::agent::CodexAppServerBackend;

mod capture;
mod context;
mod dictation;
mod layout;
mod permissions;
mod picker;
mod render;
mod requests;
mod runtime;
mod side_chat;

use std::{path::PathBuf, sync::Arc};

use gpui::{Context, Entity, FocusHandle, Focusable, prelude::*};

use crate::{
    agent::{
        AgentBackend, AgentConnectionEvent, AgentEvent, AgentPermissionMode, ProjectId,
        ThreadHistory,
    },
    components::{
        prompt_input::{PromptChanged, PromptInput, PromptSubmitted},
        user_input_request::UserInputRequestEvent,
    },
    conversation::{
        ConversationActivity, ConversationPhase, ConversationState, ConversationTranscriptTurn,
        ResumedTurnPresentation,
    },
    theme::ThemeMode,
};

pub(crate) const COMPOSER_CORNER_RADIUS: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PickerSubmenu {
    Model,
    Effort,
    ServiceTier,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DictationState {
    #[default]
    Idle,
    Recording,
    Transcribing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PermissionMode {
    Request,
    Assist,
    Full,
    Custom,
}

impl PermissionMode {
    const fn at_menu_index(index: usize) -> Self {
        match index {
            0 => Self::Request,
            1 => Self::Assist,
            2 => Self::Full,
            _ => Self::Custom,
        }
    }

    const fn agent_mode(self) -> AgentPermissionMode {
        match self {
            Self::Request => AgentPermissionMode::Request,
            Self::Assist => AgentPermissionMode::Assist,
            Self::Full => AgentPermissionMode::Full,
            Self::Custom => AgentPermissionMode::Custom,
        }
    }
}

/// Opens the native full-access confirmation dialog.
///
/// Permission selection is currently local Composer state; changing the real
/// app-server policy remains a separate protocol operation.
pub struct RequestFullAccessConfirmation;
impl gpui::EventEmitter<RequestFullAccessConfirmation> for ComposerView {}

pub struct ModelCatalogLoadFinished;
impl gpui::EventEmitter<ModelCatalogLoadFinished> for ComposerView {}

pub struct ConversationChanged;
impl gpui::EventEmitter<ConversationChanged> for ComposerView {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationThreadCreated {
    pub thread_id: String,
}
impl gpui::EventEmitter<ConversationThreadCreated> for ComposerView {}
pub struct OpenReviewComments;
impl gpui::EventEmitter<OpenReviewComments> for ComposerView {}

pub struct ReviewCommentsSubmitted;
impl gpui::EventEmitter<ReviewCommentsSubmitted> for ComposerView {}

const MODEL_PICKER_WIDTH: f32 = 224.0;
const MODEL_PICKER_SUBMENU_GAP: f32 = 1.0;
const MODEL_PICKER_TRIGGER_GAP: f32 = 4.0;
// The open trigger reserves 224px and ends immediately before the 64px
// dictation/send group inside the composer's 8px trailing inset.
const MODEL_PICKER_RIGHT_INSET: f32 = 72.0;
const MODEL_PICKER_MIN_SUBMENU_WIDTH: f32 = 180.0;
const MODEL_PICKER_ROW_HEIGHT: f32 = 28.5625;
const MODEL_PICKER_DETAIL_ROW_HEIGHT: f32 = 47.125;
const MODEL_PICKER_SUBMENU_HEADER_HEIGHT: f32 = 26.0;
const MODEL_PICKER_SUBMENU_VERTICAL_PADDING: f32 = 8.0;
// Radix collision handling in the ChatGPT desktop app keeps each submenu's
// lower edge at the same viewport inset. Relative to this composer's anchored
// main menu, CDP resolves that edge to 184px below the main menu's top.
const MODEL_PICKER_SUBMENU_BOTTOM_OFFSET: f32 = 184.0;
const MODEL_PICKER_SUBMENU_MAX_HEIGHT: f32 = 420.0;
const HOME_COMPOSER_MAX_WIDTH: f32 = 748.0;
const APP_SIDEBAR_WIDTH: f32 = 256.125;
const PARTICLE_TIMELINE_MS: f32 = 120_000.0;
#[derive(Clone, Copy, Debug, PartialEq)]
struct SubmenuLayout {
    open_left: bool,
    width: f32,
}

pub struct ComposerView {
    conversation: ConversationState,
    backend: Arc<dyn AgentBackend>,
    mode: ThemeMode,
    prompt_input: Entity<PromptInput>,
    side_editor: Option<Entity<crate::components::file_editor::FileEditor>>,
    side_ready: bool,
    available_width: Option<f32>,
    trailing_margin: Option<f32>,
    prompt_context: crate::agent::AgentPromptContext,
    context_menu_open: bool,
    context_focus: FocusHandle,
    focus_prompt_pending: bool,
    review_comments: Vec<crate::git_review::ReviewComment>,
    user_input_other_input: Entity<PromptInput>,
    model_menu_focus: FocusHandle,
    model_menu_focused_item: usize,
    model_menu_keyboard_focus: bool,
    submenu_focused_item: usize,
    submenu_keyboard_focus: bool,
    menu_open: bool,
    advanced_expanded: bool,
    submenu: Option<PickerSubmenu>,
    slider_dragging: bool,
    dictation_state: DictationState,
    dictation_cycle: u64,
    /// The permission selector is part of the normal Composer UI. Capture
    /// helpers still use this flag to make fixture setup explicit, but product
    /// launches enable it by default; visual similarity is no longer a
    /// visibility gate.
    permission_ui_enabled: bool,
    permission_mode: PermissionMode,
    permission_update_cycle: u64,
    permission_menu_focus: FocusHandle,
    permission_menu_focused_item: usize,
    permission_menu_keyboard_focus: bool,
    permission_menu_open: bool,
    approval_resolved_capture: bool,
}

impl ComposerView {
    #[cfg(test)]
    pub fn new(mode: ThemeMode, cx: &mut Context<Self>) -> Self {
        let backend: Arc<dyn AgentBackend> = Arc::new(CodexAppServerBackend::new());
        Self::new_with_backend(mode, backend, cx)
    }

    pub fn new_with_backend(
        mode: ThemeMode,
        backend: Arc<dyn AgentBackend>,
        cx: &mut Context<Self>,
    ) -> Self {
        let connection_events = backend.subscribe_connection_events();
        let prompt_input = cx.new(|cx| PromptInput::new(mode, cx));
        let user_input_other_input = cx.new(|cx| {
            PromptInput::inline_other(mode, "否，并告诉 ChatGPT 应该如何做得不同", false, cx)
        });
        cx.subscribe(&prompt_input, |_, _, _: &PromptChanged, cx| {
            cx.notify();
        })
        .detach();
        cx.subscribe(&prompt_input, |this, _, event: &PromptSubmitted, cx| {
            this.submit_prompt(event.0.clone(), cx);
        })
        .detach();
        cx.subscribe(
            &user_input_other_input,
            |this, input, _: &PromptChanged, cx| {
                let answer = input.read(cx).text().to_owned();
                if let Some(model) = this
                    .conversation
                    .activities
                    .iter_mut()
                    .find_map(|activity| {
                        let ConversationActivity::UserInput(model) = activity else {
                            return None;
                        };
                        model.is_interactive().then_some(model)
                    })
                {
                    model.save_other_answer(answer);
                    model.focus_other_answer();
                    cx.emit(ConversationChanged);
                }
                cx.notify();
            },
        )
        .detach();
        cx.subscribe(
            &user_input_other_input,
            |this, _, event: &PromptSubmitted, cx| {
                let pending = this.conversation.activities.iter().find_map(|activity| {
                    let ConversationActivity::UserInput(model) = activity else {
                        return None;
                    };
                    let question = model.current_question()?;
                    model.is_interactive().then(|| {
                        (
                            model.request_id.clone(),
                            question.id.clone(),
                            event.0.clone(),
                        )
                    })
                });
                if let Some((request_id, question_id, answer)) = pending {
                    this.handle_user_input_request_event(
                        &request_id,
                        UserInputRequestEvent::SubmitOtherAnswer {
                            question_id,
                            answer,
                        },
                        cx,
                    );
                }
            },
        )
        .detach();
        let mut view = Self {
            conversation: ConversationState::default(),
            backend,
            mode,
            prompt_input,
            side_editor: None,
            side_ready: true,
            available_width: None,
            trailing_margin: None,
            prompt_context: Default::default(),
            context_menu_open: false,
            context_focus: cx.focus_handle(),
            focus_prompt_pending: false,
            review_comments: Vec::new(),
            user_input_other_input,
            model_menu_focus: cx.focus_handle(),
            model_menu_focused_item: 0,
            model_menu_keyboard_focus: false,
            submenu_focused_item: 0,
            submenu_keyboard_focus: false,
            menu_open: false,
            advanced_expanded: true,
            submenu: None,
            slider_dragging: false,
            dictation_state: DictationState::Idle,
            dictation_cycle: 0,
            permission_ui_enabled: true,
            permission_mode: PermissionMode::Full,
            permission_update_cycle: 0,
            permission_menu_focus: cx.focus_handle().tab_stop(true),
            permission_menu_focused_item: 0,
            permission_menu_keyboard_focus: false,
            permission_menu_open: false,
            approval_resolved_capture: false,
        };
        view.consume_connection_events(connection_events, cx);
        #[cfg(not(test))]
        view.load_model_catalog(cx);
        view
    }

    #[cfg(test)]
    pub fn conversation_snapshot(
        &self,
    ) -> (
        ConversationPhase,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
    ) {
        self.conversation.conversation_snapshot()
    }

    #[cfg(test)]
    pub fn conversation_activity_snapshot(&self) -> Vec<ConversationActivity> {
        self.conversation.activity_snapshot()
    }

    pub fn conversation_phase(&self) -> ConversationPhase {
        self.conversation.phase()
    }

    pub fn has_active_context_compaction(&self) -> bool {
        self.conversation.has_active_context_compaction()
    }

    pub fn has_active_image_generation(&self) -> bool {
        self.conversation.has_active_image_generation()
    }

    pub fn transcript_render_snapshot(&self) -> Vec<ConversationTranscriptTurn> {
        self.conversation.transcript_render_snapshot()
    }

    pub fn thread_id(&self) -> Option<&str> {
        self.conversation.thread_id()
    }

    pub fn history_needs_retry(&self) -> bool {
        self.conversation.history_needs_retry()
    }

    #[cfg(feature = "screenshot")]
    pub fn history_loading(&self) -> bool {
        self.conversation.history_loading()
    }

    #[cfg(feature = "screenshot")]
    pub fn history_error(&self) -> Option<&str> {
        self.conversation.history_error()
    }

    pub fn set_workspace_context(
        &mut self,
        cwd: PathBuf,
        project_id: Option<ProjectId>,
        thread_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.conversation
            .set_workspace_context(cwd, project_id, thread_id);
        cx.notify();
    }

    pub fn set_history_loading(&mut self, loading: bool, cx: &mut Context<Self>) {
        self.conversation.set_history_loading(loading);
        cx.emit(ConversationChanged);
        cx.notify();
    }

    pub fn set_history_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.conversation.set_history_error(error);
        cx.emit(ConversationChanged);
        cx.notify();
    }

    pub fn hydrate_history(&mut self, history: ThreadHistory, cx: &mut Context<Self>) {
        self.conversation.hydrate_history(history);
        cx.emit(ConversationChanged);
        cx.notify();
    }

    pub fn user_images(&self) -> Vec<crate::agent::UserMessageImage> {
        self.conversation.user_images()
    }

    pub fn resumed_turn(&self) -> Option<ResumedTurnPresentation> {
        self.conversation.resumed_turn()
    }

    pub fn conversation_render_snapshot(
        &self,
    ) -> (
        ConversationPhase,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        Vec<ConversationActivity>,
    ) {
        self.conversation.conversation_render_snapshot()
    }

    fn apply_connection_event(&mut self, event: AgentConnectionEvent) -> bool {
        self.conversation.apply_connection_event(event)
    }

    fn apply_agent_event_batch(&mut self, events: Vec<AgentEvent>) -> bool {
        self.conversation.apply_agent_event_batch(events)
    }

    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.prompt_input
            .update(cx, |input, cx| input.set_mode(mode, cx));
        if let Some(editor) = &self.side_editor {
            editor.update(cx, |editor, cx| editor.set_mode(mode, cx));
        }
        self.user_input_other_input
            .update(cx, |input, cx| input.set_mode(mode, cx));
        cx.notify();
    }

    pub fn set_review_comments(
        &mut self,
        comments: Vec<crate::git_review::ReviewComment>,
        cx: &mut Context<Self>,
    ) {
        self.review_comments = comments;
        self.prompt_input.update(cx, |input, _| {
            input.set_submit_empty(!self.review_comments.is_empty())
        });
        cx.notify();
    }

    pub fn latest_review(&self) -> Option<crate::components::file_change::DiffReviewPresentation> {
        let groups = std::iter::once(&self.conversation.activities).chain(
            self.conversation
                .transcript
                .iter()
                .rev()
                .map(|t| &t.activities),
        );
        for activities in groups {
            if let Some(review) = activities.iter().rev().find_map(|a| {
                if let crate::conversation::ConversationActivity::FileChange(c) = a {
                    c.review
                        .review_id
                        .starts_with("turn-diff-")
                        .then(|| c.review.clone())
                } else {
                    None
                }
            }) {
                return Some(review);
            }
            let mut raw = String::new();
            let mut files = Vec::new();
            for activity in activities {
                if let crate::conversation::ConversationActivity::FileChange(change) = activity
                    && change.status == crate::agent::AgentFileChangeStatus::Completed
                {
                    files.extend(change.review.files.clone());
                    if let Some(patch) = &change.review.raw_diff {
                        raw.push_str(patch);
                    }
                }
            }
            if !files.is_empty() {
                let mut review = crate::components::file_change::DiffReviewPresentation::new(
                    format!(
                        "latest-{}-{}",
                        self.conversation.thread_id.as_deref().unwrap_or("draft"),
                        self.conversation.cycle
                    ),
                    "上一轮",
                    files,
                );
                review.raw_diff = (!raw.is_empty()).then_some(raw);
                return Some(review);
            }
        }
        None
    }

    pub fn prompt_focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.side_editor.as_ref().map_or_else(
            || self.prompt_input.read(cx).focus_handle(cx),
            |editor| editor.read(cx).focus_handle(cx),
        )
    }

    pub fn user_input_other_entity(&self) -> Entity<PromptInput> {
        self.user_input_other_input.clone()
    }

    pub fn user_input_other_focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.user_input_other_input.read(cx).focus_handle(cx)
    }
}

impl ComposerView {}

#[cfg(test)]
mod approval_tests;
#[cfg(test)]
mod tests;
