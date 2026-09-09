#[cfg(test)]
use crate::agent::CodexAppServerBackend;

mod activity;
mod animation;
#[cfg(test)]
mod auto_approval_tests;
mod collaboration;
mod context;
mod conversation;
mod landing;
mod mcp;
mod media;
mod messages;
mod notices;
mod progress;
mod reasoning;
mod requests;
mod timeline;
mod tools;

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    Context, Div, Entity, FocusHandle, Focusable, KeyDownEvent, ListState, MouseButton, Render,
    ScrollHandle, Window, div, point, prelude::*, px, relative,
};

use crate::{
    agent::{AgentBackend, CommandExecutionStatus},
    components::{
        composer::{
            ComposerView, ConversationChanged, ConversationThreadCreated, ModelCatalogLoadFinished,
            RequestFullAccessConfirmation,
        },
        file_change::{DiffReviewPresentation, FileApprovalEvent, FileChangeActivityEvent},
        file_editor::FileEditor,
        icons::suggestion_icon,
        permissions_approval::PermissionApprovalEvent,
        user_input_request::UserInputRequestEvent,
    },
    conversation::{ConversationActivity, ConversationPhase},
    theme::{Theme, ThemeMode},
};
use context::{
    ConversationRenderContext, CurrentTurnRows, DisclosureRenderState, MainConversationSnapshot,
};

use animation::{
    reasoning_transition_ease, suggestion_transition_ease, thinking_shimmer_progress,
    tool_group_chevron_transition_ease,
};
use conversation::{
    conversation_list_state, scroll_should_follow_output, subagent_conversation,
    sync_list_item_count,
};
use landing::home;
use notices::ConfigWarningFile;
use reasoning::{ReasoningDisclosureTransition, reasoning_body_text};
use timeline::{
    ActivityStreamUnit, ConversationListRow, activity_stream_units, conversation_list_rows,
    conversation_status,
};
use tools::ToolGroupDisclosureTransition;

pub struct HomeView {
    mode: ThemeMode,
    presentation: HomePresentation,
    composer: Entity<ComposerView>,
    observed_composers: Vec<Entity<ComposerView>>,
    suggestion_scale: [f32; 2],
    suggestion_animation_from: [f32; 2],
    suggestion_animation_to: [f32; 2],
    suggestion_animation_started_at: [Option<Instant>; 2],
    suggestion_animation_duration: [Duration; 2],
    suggestion_animation_running: bool,
    thinking_shimmer_progress: f32,
    thinking_shimmer_cycle: u64,
    thinking_shimmer_running: bool,
    response_feedback: i8,
    response_feedback_menu: Option<u64>,
    user_message_actions_visible_for_capture: bool,
    conversation_rows: Rc<Vec<ConversationListRow>>,
    conversation_phase: ConversationPhase,
    conversation_activity: Rc<Vec<ConversationActivity>>,
    conversation_cache_dirty: bool,
    content_width: f32,
    conversation_list: ListState,
    conversation_scroll: ScrollHandle,
    expanded_reasoning: HashSet<String>,
    reasoning_disclosure_transitions: HashMap<String, ReasoningDisclosureTransition>,
    reasoning_transition_running: bool,
    reasoning_scroll_handles: HashMap<String, ScrollHandle>,
    expanded_tool_groups: HashSet<String>,
    collapsed_active_tool_groups: HashSet<String>,
    tool_group_disclosure_transitions: HashMap<String, ToolGroupDisclosureTransition>,
    tool_group_transition_running: bool,
    tool_group_scroll_handles: HashMap<String, ScrollHandle>,
    expanded_commands: HashSet<String>,
    expanded_resumed_turns: HashSet<String>,
    expanded_file_summaries: HashSet<String>,
    resumed_turn_focus: HashMap<String, FocusHandle>,
    command_scroll_handles: HashMap<String, ScrollHandle>,
    expanded_collaborations: HashSet<String>,
    auto_review_views: HashMap<
        crate::agent::AgentAutoApprovalReviewKey,
        Entity<crate::components::auto_approval::AutoApprovalReviewView>,
    >,
    approval_focus: FocusHandle,
    approval_previews: HashMap<String, Entity<FileEditor>>,
    focused_approval_request: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HomePresentation {
    Conversation,
    SideChat,
    Subagent,
}

impl gpui::EventEmitter<RequestFullAccessConfirmation> for HomeView {}
impl gpui::EventEmitter<ModelCatalogLoadFinished> for HomeView {}
impl gpui::EventEmitter<ConversationThreadCreated> for HomeView {}

pub struct OpenDiffReview(pub DiffReviewPresentation);
impl gpui::EventEmitter<OpenDiffReview> for HomeView {}

pub struct OpenImagePreview(pub PathBuf);
impl gpui::EventEmitter<OpenImagePreview> for HomeView {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenSubAgentPanel {
    pub thread_id: String,
    pub name: String,
}
impl gpui::EventEmitter<OpenSubAgentPanel> for HomeView {}

pub struct RetryImageGeneration;
impl gpui::EventEmitter<RetryImageGeneration> for HomeView {}

const SUGGESTION_PRESSED_SCALE: f32 = 0.99;
const SUGGESTION_TRANSITION_DURATION: Duration = Duration::from_millis(150);
const THINKING_SHIMMER_DURATION: Duration = Duration::from_secs(1);
const THINKING_SHIMMER_STEPS: f32 = 48.0;
const THINKING_SHIMMER_FRAME_INTERVAL: Duration = Duration::from_micros(20_833);
const THINKING_SHIMMER_WIDTH: f32 = 56.0;
const THINKING_SHIMMER_BAND_SCALE: f32 = 0.5;
const THINKING_SHIMMER_ALPHA_LEVELS: usize = 32;
const COMPOSER_BOTTOM_INSET: f32 = 15.0;
const CONVERSATION_TOP_INSET: f32 = 78.0;
const CONVERSATION_BOTTOM_INSET: f32 = 153.0;
const CONVERSATION_BOTTOM_EPSILON: f32 = 0.5;
const CONVERSATION_LIST_OVERDRAW: f32 = 256.0;
const CONVERSATION_CONTENT_MAX_WIDTH: f32 = 736.0;
const USER_MESSAGE_MAX_WIDTH_RATIO: f32 = 0.7;
const USER_MESSAGE_TEXT_LAYOUT_EPSILON: f32 = 1.0;
const USER_MESSAGE_HORIZONTAL_PADDING: f32 = 16.0;
const USER_MESSAGE_VERTICAL_PADDING: f32 = 10.0;
const USER_MESSAGE_TEXT_SIZE: f32 = 14.0;
const USER_MESSAGE_LINE_HEIGHT: f32 = 22.75;
const USER_MESSAGE_PARAGRAPH_GAP: f32 = 20.0;
const USER_MESSAGE_BUBBLE_RADIUS: f32 = 22.0;
const USER_MESSAGE_BUBBLE_SUPERELLIPSE: f32 = 1.5;
const USER_MESSAGE_FOOTER_OFFSET: f32 = 4.0;
const USER_MESSAGE_FOOTER_HEIGHT: f32 = 26.0;
const USER_MESSAGE_FOOTER_SIDE_MARGIN: f32 = 4.0;
const USER_MESSAGE_FOOTER_GAP: f32 = 8.0;
const USER_MESSAGE_TIME_SIZE: f32 = 12.0;
const USER_MESSAGE_TIME_LINE_HEIGHT: f32 = 16.0;
const RESPONSE_ACTION_ICON_SIZE: f32 = 16.0;
const RESPONSE_ACTION_FOOTER_OFFSET: f32 = 3.0;
const RESPONSE_ACTION_FOOTER_HEIGHT: f32 = 26.0;
const RESPONSE_ACTION_FOOTER_ELECTRON_SHIFT: f32 = -4.0;
const RESPONSE_ACTION_GAP: f32 = 2.0;
const RESPONSE_TIME_MARGIN: f32 = 6.0;
const RESPONSE_TIME_SIZE: f32 = 12.0;
const RESPONSE_TIME_LINE_HEIGHT: f32 = 16.0;
const REASONING_HEADER_HEIGHT: f32 = 21.0;
const REASONING_TEXT_SIZE: f32 = 14.0;
const REASONING_LINE_HEIGHT: f32 = 21.0;
const REASONING_CHEVRON_SIZE: f32 = 14.0;
const REASONING_BODY_MAX_HEIGHT: f32 = 140.0;
const REASONING_BODY_TOP_GAP: f32 = 4.0;
const REASONING_TRANSITION_DURATION: Duration = Duration::from_millis(300);
const DISCLOSURE_FOCUS_PADDING: f32 = 2.0;
const TOOL_GROUP_HEADER_HEIGHT: f32 = 21.0;
const TOOL_GROUP_TEXT_SIZE: f32 = 14.0;
const TOOL_GROUP_LINE_HEIGHT: f32 = 21.0;
const TOOL_GROUP_ICON_SIZE: f32 = 16.0;
const TOOL_GROUP_ICON_TEXT_GAP: f32 = 6.0;
const TOOL_GROUP_HEADER_CHEVRON_GAP: f32 = 4.0;
const TOOL_GROUP_CHEVRON_SIZE: f32 = 14.0;
const TOOL_GROUP_ITEM_GAP: f32 = 4.0;
const TOOL_GROUP_BODY_MAX_HEIGHT: f32 = 224.0;
const TOOL_GROUP_EDGE_FADE_DISTANCE: f32 = 24.0;
const TOOL_GROUP_TRANSITION_DURATION: Duration = Duration::from_millis(300);
const COMMAND_ACTIVITY_ICON_SIZE: f32 = 16.0;
const COMMAND_ACTIVITY_CONTENT_GAP: f32 = 6.0;
const COMMAND_ACTIVITY_CHEVRON_SIZE: f32 = 14.0;
const COMMAND_CARD_RADIUS: f32 = 12.5;
const COMMAND_CARD_HEADER_SIZE: f32 = 13.0;
const COMMAND_CARD_HEADER_LINE_HEIGHT: f32 = 18.5714;
const COMMAND_CARD_TEXT_SIZE: f32 = 13.0;
const COMMAND_CARD_LINE_HEIGHT: f32 = 19.5;
const COMMAND_CARD_COMMAND_MAX_HEIGHT: f32 = 39.0;
const COMMAND_CARD_OUTPUT_MAX_HEIGHT: f32 = 144.0;
// GPUI rounds this box one device pixel shorter than Chromium at 27 CSS px.
const COMMAND_CARD_STATUS_HEIGHT: f32 = 28.0;
const NOTICE_RADIUS: f32 = 20.0;
const NOTICE_TEXT_SIZE: f32 = 13.0;
const NOTICE_LINE_HEIGHT: f32 = 20.0;
const NOTICE_ICON_SIZE: f32 = 18.0;
const NOTICE_ERROR_GAP: f32 = 12.0;
const NOTICE_WARNING_GAP: f32 = 16.0;
const NOTICE_ERROR_CONTENT_GAP: f32 = 6.0;
const NOTICE_WARNING_CONTENT_GAP: f32 = 8.0;
const NOTICE_BUTTON_HEIGHT: f32 = 24.0;
const COLLABORATION_ROW_HEIGHT: f32 = 20.0;
const COLLABORATION_ICON_SIZE: f32 = 16.0;
const COLLABORATION_TEXT_SIZE: f32 = 14.0;
const COLLABORATION_DETAIL_MAX_WIDTH: f32 = 520.0;
const MCP_TOOL_CALL_ROW_HEIGHT: f32 = 21.0;
// GPUI's monochrome SVG rasterizer paints the ChatGPT 20 px MCP glyph at the
// same physical silhouette when the asset is laid out at 16 px. Preserve the
// reference's 22 px icon-to-label advance with the compensating 6 px gap.
const MCP_TOOL_CALL_ICON_SIZE: f32 = 16.0;
const MCP_TOOL_CALL_ICON_TEXT_GAP: f32 = 6.0;
const MCP_TOOL_CALL_TEXT_SIZE: f32 = 14.0;

impl HomeView {
    pub(crate) fn has_visible_request(&self, cx: &gpui::App) -> bool {
        self.composer.read(cx).has_visible_request()
    }

    pub(crate) fn needs_live_interaction_render(&self, cx: &gpui::App) -> bool {
        let interactive = |activity: &ConversationActivity| {
            matches!(
                activity,
                ConversationActivity::Plan(_)
                    | ConversationActivity::TurnPlan(_)
                    | ConversationActivity::AutoApprovalReview(_)
                    | ConversationActivity::StrictReview(_)
                    | ConversationActivity::GuardianWarning(_)
                    | ConversationActivity::UserMessage { .. }
            )
        };
        self.has_visible_request(cx)
            || self.conversation_activity.iter().any(interactive)
            || self.conversation_rows.iter().any(|row| match row {
                ConversationListRow::Activity {
                    unit: ActivityStreamUnit::Standalone(activity),
                    ..
                } => interactive(activity),
                ConversationListRow::Activity {
                    unit: ActivityStreamUnit::ToolGroup(group),
                    ..
                } => group.activities.iter().any(interactive),
                _ => false,
            })
    }
    #[cfg(feature = "screenshot")]
    pub fn replay_approvals(
        &mut self,
        run: crate::agent::AgentRun,
        user_message: &str,
        assistant_message: &str,
        cwd: PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.composer.update(cx, |composer, cx| {
            composer.replay_approvals(run, user_message, assistant_message, cwd, cx)
        });
    }
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
        let composer = cx.new(|cx| ComposerView::new_with_backend(mode, backend, cx));
        Self::with_composer(mode, HomePresentation::Conversation, composer, cx)
    }

    pub(crate) fn new_side_chat(
        mode: ThemeMode,
        composer: Entity<ComposerView>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_composer(mode, HomePresentation::SideChat, composer, cx)
    }

    pub fn new_subagent(
        mode: ThemeMode,
        composer: Entity<ComposerView>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_composer(mode, HomePresentation::Subagent, composer, cx)
    }

    fn sync_auto_review_view(
        &mut self,
        review: &crate::conversation::AutoApprovalReviewPresentation,
        cx: &mut Context<Self>,
    ) {
        let view = self
            .auto_review_views
            .entry(review.review.key.clone())
            .or_insert_with(|| {
                let target = cx.weak_entity();
                cx.new(|cx| {
                    let mut view = crate::components::auto_approval::AutoApprovalReviewView::new(
                        review.clone(),
                        self.mode,
                        cx,
                    );
                    view.on_change(crate::components::callback::UiCallback::new(
                        move |(), _, cx| {
                            let _ = target.update(cx, |home, cx| {
                                home.conversation_list.remeasure();
                                home.conversation_cache_dirty = true;
                                cx.notify();
                            });
                        },
                    ));
                    view
                })
            });
        view.update(cx, |view, cx| view.sync(review.clone(), self.mode, cx));
    }

    fn with_composer(
        mode: ThemeMode,
        presentation: HomePresentation,
        composer: Entity<ComposerView>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self {
            mode,
            presentation,
            composer: composer.clone(),
            observed_composers: Vec::new(),
            suggestion_scale: [1.0; 2],
            suggestion_animation_from: [1.0; 2],
            suggestion_animation_to: [1.0; 2],
            suggestion_animation_started_at: [None; 2],
            suggestion_animation_duration: [Duration::ZERO; 2],
            suggestion_animation_running: false,
            thinking_shimmer_progress: 0.0,
            thinking_shimmer_cycle: 0,
            thinking_shimmer_running: false,
            response_feedback: 0,
            response_feedback_menu: None,
            user_message_actions_visible_for_capture: false,
            conversation_rows: Rc::new(Vec::new()),
            conversation_phase: ConversationPhase::Empty,
            conversation_activity: Rc::new(Vec::new()),
            conversation_cache_dirty: false,
            content_width: CONVERSATION_CONTENT_MAX_WIDTH,
            conversation_list: conversation_list_state(0),
            conversation_scroll: ScrollHandle::new(),
            expanded_reasoning: HashSet::new(),
            reasoning_disclosure_transitions: HashMap::new(),
            reasoning_transition_running: false,
            reasoning_scroll_handles: HashMap::new(),
            expanded_tool_groups: HashSet::new(),
            collapsed_active_tool_groups: HashSet::new(),
            tool_group_disclosure_transitions: HashMap::new(),
            tool_group_transition_running: false,
            tool_group_scroll_handles: HashMap::new(),
            expanded_commands: HashSet::new(),
            expanded_resumed_turns: HashSet::new(),
            expanded_file_summaries: HashSet::new(),
            resumed_turn_focus: HashMap::new(),
            command_scroll_handles: HashMap::new(),
            expanded_collaborations: HashSet::new(),
            auto_review_views: HashMap::new(),
            approval_focus: cx.focus_handle(),
            approval_previews: HashMap::new(),
            focused_approval_request: None,
        };
        view.refresh_conversation_cache(cx);
        view.observe_composer(composer, cx);
        view
    }

    fn refresh_conversation_cache(&mut self, cx: &mut Context<Self>) {
        let transcript = self.composer.read(cx).transcript_render_snapshot();
        let (
            phase,
            user_message,
            user_message_time,
            assistant_message,
            assistant_message_time,
            conversation_activity,
        ) = self.composer.read(cx).conversation_render_snapshot();
        self.conversation_rows = Rc::new(conversation_list_rows(
            transcript,
            CurrentTurnRows {
                phase,
                user_message: user_message.unwrap_or_default(),
                user_images: self.composer.read(cx).user_images(),
                user_message_time: user_message_time.unwrap_or_default(),
                assistant_message,
                assistant_message_time,
                conversation_activity: &conversation_activity,
                resumed_turn: self.composer.read(cx).resumed_turn(),
            },
            &self.expanded_resumed_turns,
        ));
        self.conversation_phase = phase;
        for row in self.conversation_rows.iter() {
            if let ConversationListRow::ResumedWork { id, .. } = row {
                self.resumed_turn_focus
                    .entry(id.clone())
                    .or_insert_with(|| cx.focus_handle());
            }
        }
        self.conversation_activity = Rc::new(conversation_activity);
        self.conversation_cache_dirty = true;
        sync_list_item_count(&self.conversation_list, self.conversation_rows.len());
    }

    fn toggle_resumed_turn(&mut self, id: &str, cx: &mut Context<Self>) {
        let offset = self.conversation_list.logical_scroll_top();
        if !self.expanded_resumed_turns.remove(id) {
            self.expanded_resumed_turns.insert(id.to_owned());
        }
        self.refresh_conversation_cache(cx);
        self.conversation_list.remeasure();
        self.conversation_list.scroll_to(offset);
        cx.notify();
    }

    fn observe_composer(&mut self, composer: Entity<ComposerView>, cx: &mut Context<Self>) {
        if self
            .observed_composers
            .iter()
            .any(|observed| observed == &composer)
        {
            return;
        }
        cx.subscribe(&composer, |_, _, _: &RequestFullAccessConfirmation, cx| {
            cx.emit(RequestFullAccessConfirmation);
        })
        .detach();
        cx.subscribe(&composer, |_, _, _: &ModelCatalogLoadFinished, cx| {
            cx.emit(ModelCatalogLoadFinished);
        })
        .detach();
        cx.subscribe(&composer, |_, _, event: &ConversationThreadCreated, cx| {
            cx.emit(event.clone());
        })
        .detach();
        cx.subscribe(&composer, |this, composer, _: &ConversationChanged, cx| {
            if *this.composer == *composer {
                this.refresh_conversation_cache(cx);
                let composer = composer.read(cx);
                let item_count = this.conversation_list.item_count();
                if item_count > 0 {
                    this.conversation_list
                        .remeasure_items(item_count.saturating_sub(2)..item_count);
                }
                let needs_shimmer = composer.conversation_phase() == ConversationPhase::Thinking
                    || composer.has_active_context_compaction()
                    || composer.has_active_image_generation()
                    || composer.has_active_plan();
                this.sync_thinking_shimmer(needs_shimmer, cx);
                cx.notify();
            }
        })
        .detach();
        self.observed_composers.push(composer);
    }

    pub fn composer_entity(&self) -> Entity<ComposerView> {
        self.composer.clone()
    }

    pub fn set_composer(&mut self, composer: Entity<ComposerView>, cx: &mut Context<Self>) {
        self.approval_previews.clear();
        self.focused_approval_request = None;
        self.observe_composer(composer.clone(), cx);
        self.composer = composer;
        self.conversation_list = conversation_list_state(0);
        self.refresh_conversation_cache(cx);
        let composer = self.composer.read(cx);
        self.conversation_scroll = ScrollHandle::new();
        self.expanded_reasoning.clear();
        self.reasoning_disclosure_transitions.clear();
        self.reasoning_scroll_handles.clear();
        self.expanded_tool_groups.clear();
        self.collapsed_active_tool_groups.clear();
        self.tool_group_disclosure_transitions.clear();
        self.tool_group_scroll_handles.clear();
        self.expanded_commands.clear();
        self.command_scroll_handles.clear();
        self.expanded_collaborations.clear();
        let needs_shimmer = composer.conversation_phase() == ConversationPhase::Thinking
            || composer.has_active_context_compaction()
            || composer.has_active_image_generation()
            || composer.has_active_plan();
        self.sync_thinking_shimmer(needs_shimmer, cx);
        cx.notify();
    }

    fn handle_approval_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(feature = "screenshot")]
        if std::env::var_os("GPUI_CAPTURE_OUTPUT").is_some()
            && matches!(
                event.keystroke.key.as_str(),
                "enter" | "escape" | "tab" | "space" | "up" | "down"
            )
        {
            eprintln!(
                "approval-key: {} shift={}",
                event.keystroke.key, event.keystroke.modifiers.shift
            );
        }
        let other_focus = self.composer.read(cx).user_input_other_focus_handle(cx);
        let preview_focused = self
            .approval_previews
            .values()
            .any(|preview| preview.focus_handle(cx).is_focused(window));
        if preview_focused && !matches!(event.keystroke.key.as_str(), "tab" | "escape") {
            return;
        }
        if other_focus.is_focused(window)
            && !matches!(event.keystroke.key.as_str(), "tab" | "escape")
        {
            return;
        }
        let handled = self.composer.update(cx, |composer, cx| {
            composer.handle_approval_key(event, cx) || composer.handle_user_input_key(event, cx)
        });
        if handled {
            if preview_focused
                || other_focus.is_focused(window) && event.keystroke.key.as_str() == "tab"
            {
                window.focus(&self.approval_focus, cx);
            }
            cx.stop_propagation();
        } else if self.conversation_rows.iter().any(|row| match row {
            ConversationListRow::Activity {
                unit: ActivityStreamUnit::Standalone(ConversationActivity::AutoApprovalReview(_)),
                ..
            } => true,
            ConversationListRow::Activity {
                unit: ActivityStreamUnit::ToolGroup(group),
                ..
            } => group
                .activities
                .iter()
                .any(|a| matches!(a, ConversationActivity::AutoApprovalReview(_))),
            _ => false,
        }) {
            crate::components::auto_approval::navigate_tab(event, window, cx);
        }
    }

    fn sync_thinking_shimmer(&mut self, needs_shimmer: bool, cx: &mut Context<Self>) {
        if needs_shimmer {
            if cx.reduce_motion() {
                if self.thinking_shimmer_running || self.thinking_shimmer_progress != 0.5 {
                    self.thinking_shimmer_cycle = self.thinking_shimmer_cycle.wrapping_add(1);
                    self.thinking_shimmer_progress = 0.5;
                    self.thinking_shimmer_running = false;
                    cx.notify();
                }
            } else if !self.thinking_shimmer_running {
                self.start_thinking_shimmer(cx);
            }
        } else if self.thinking_shimmer_running || self.thinking_shimmer_progress != 0.0 {
            self.thinking_shimmer_cycle = self.thinking_shimmer_cycle.wrapping_add(1);
            self.thinking_shimmer_progress = 0.0;
            self.thinking_shimmer_running = false;
        }
    }

    fn start_thinking_shimmer(&mut self, cx: &mut Context<Self>) {
        self.thinking_shimmer_cycle = self.thinking_shimmer_cycle.wrapping_add(1);
        let cycle = self.thinking_shimmer_cycle;
        self.thinking_shimmer_progress = 0.0;
        self.thinking_shimmer_running = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let mut step = 1usize;
            loop {
                cx.background_executor()
                    .timer(THINKING_SHIMMER_FRAME_INTERVAL)
                    .await;
                let should_continue = this
                    .update(cx, |this, cx| {
                        if this.thinking_shimmer_cycle != cycle || !this.thinking_shimmer_running {
                            return false;
                        }
                        if cx.reduce_motion() {
                            this.thinking_shimmer_progress = 0.5;
                            this.thinking_shimmer_running = false;
                            cx.notify();
                            return false;
                        }
                        let elapsed =
                            THINKING_SHIMMER_DURATION.mul_f32(step as f32 / THINKING_SHIMMER_STEPS);
                        this.thinking_shimmer_progress = thinking_shimmer_progress(elapsed);
                        // A scheduled frame alone reuses the previous element
                        // tree. Notify the view so the Canvas captures the new
                        // cadence step before it paints.
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !should_continue {
                    return;
                }
                step = step % THINKING_SHIMMER_STEPS as usize + 1;
            }
        })
        .detach();
    }

    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.composer
            .update(cx, |composer, cx| composer.set_mode(mode, cx));
        cx.notify();
    }

    pub fn close_model_picker(&mut self, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |composer, cx| composer.close_picker(cx));
    }

    pub fn enable_permission_ui_for_capture(&mut self, cx: &mut Context<Self>) {
        self.composer.update(cx, |composer, cx| {
            composer.enable_permission_ui_for_capture(cx)
        });
    }

    pub fn set_permission_mode_for_capture(&mut self, mode: &str, cx: &mut Context<Self>) {
        self.composer.update(cx, |composer, cx| {
            composer.set_permission_mode_for_capture(mode, cx)
        });
    }

    pub fn confirm_full_access(&mut self, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |composer, cx| composer.confirm_full_access(cx));
    }

    pub fn open_permission_menu_for_capture(&mut self, cx: &mut Context<Self>) {
        self.composer.update(cx, |composer, cx| {
            composer.open_permission_menu_for_capture(cx)
        });
    }

    pub fn set_permission_menu_capture_state(&mut self, state: &str, cx: &mut Context<Self>) {
        self.composer.update(cx, |composer, cx| {
            composer.set_permission_menu_capture_state(state, cx)
        });
    }

    pub fn open_model_picker(&mut self, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |composer, cx| composer.open_picker(cx));
    }

    pub fn open_model_picker_submenu(&mut self, name: &str, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |composer, cx| composer.open_picker_submenu(name, cx));
    }

    pub fn open_model_picker_slider_at(
        &mut self,
        index: usize,
        fast: bool,
        cx: &mut Context<Self>,
    ) {
        self.composer.update(cx, |composer, cx| {
            composer.open_picker_slider_at(index, fast, cx)
        });
    }

    pub fn set_dictation_state_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.composer.update(cx, |composer, cx| {
            composer.set_dictation_state_for_capture(state, cx)
        });
    }

    pub fn submit_prompt_for_capture(&mut self, prompt: &str, cx: &mut Context<Self>) {
        self.composer.update(cx, |composer, cx| {
            composer.submit_prompt_for_capture(prompt, cx)
        });
    }

    pub fn show_user_message_actions_for_capture(&mut self, cx: &mut Context<Self>) {
        self.user_message_actions_visible_for_capture = true;
        cx.notify();
    }

    #[cfg(feature = "screenshot")]
    pub fn set_conversation_scroll_from_bottom_for_capture(
        &mut self,
        distance: f32,
        cx: &mut Context<Self>,
    ) {
        if self.presentation != HomePresentation::Subagent {
            self.conversation_list.scroll_to_end();
            self.conversation_list.scroll_by(px(-distance.max(0.0)));
        } else {
            let max_scroll = f32::from(self.conversation_scroll.max_offset().y).max(0.0);
            let scroll_top = (max_scroll - distance.max(0.0)).max(0.0);
            self.conversation_scroll
                .set_offset(point(px(0.0), px(-scroll_top)));
        }
        cx.notify();
    }

    pub fn set_command_tool_for_capture(
        &mut self,
        running: bool,
        expanded: bool,
        cx: &mut Context<Self>,
    ) {
        self.expanded_commands.clear();
        self.expanded_tool_groups.clear();
        if expanded {
            self.expanded_commands
                .insert("exec-command-ui-capture".to_owned());
            self.expanded_tool_groups
                .insert("exec-command-ui-capture".to_owned());
        }
        self.composer.update(cx, |composer, cx| {
            composer.set_command_tool_for_capture(running, cx)
        });
        cx.notify();
    }

    pub fn set_context_compaction_for_capture(&mut self, running: bool, cx: &mut Context<Self>) {
        self.composer.update(cx, |composer, cx| {
            composer.set_context_compaction_for_capture(running, cx)
        });
        cx.notify();
    }

    pub fn set_collaboration_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.expanded_collaborations.clear();
        let expanded = state.ends_with("-expanded");
        let state = state.strip_suffix("-expanded").unwrap_or(state);
        self.composer.update(cx, |composer, cx| {
            composer.set_collaboration_for_capture(state, cx)
        });
        if expanded {
            self.expanded_collaborations.insert(if state == "failed" {
                "collaboration-ui-capture".to_owned()
            } else {
                "legacy-01a06b7a-14c2-73b3-9c62-b29e27bd8689".to_owned()
            });
        }
        cx.notify();
    }

    pub fn set_mcp_tool_call_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.composer.update(cx, |composer, cx| {
            composer.set_mcp_tool_call_for_capture(state, cx)
        });
        cx.notify();
    }

    pub fn set_image_generation_for_capture(
        &mut self,
        state: &str,
        path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.composer.update(cx, |composer, cx| {
            composer.set_image_generation_for_capture(state, path, cx)
        });
        cx.notify();
    }

    #[cfg(test)]
    pub fn set_image_view_for_capture(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.expanded_tool_groups.clear();
        self.collapsed_active_tool_groups.clear();
        self.composer.update(cx, |composer, cx| {
            composer.set_image_view_for_capture(path, cx)
        });
        cx.notify();
    }

    pub fn set_tool_group_for_capture(
        &mut self,
        running: bool,
        expanded: bool,
        cx: &mut Context<Self>,
    ) {
        self.expanded_tool_groups.clear();
        self.collapsed_active_tool_groups.clear();
        self.expanded_commands.clear();
        if expanded {
            self.expanded_tool_groups
                .insert("tool-group-ui-capture".to_owned());
        }
        self.composer.update(cx, |composer, cx| {
            composer.set_tool_group_for_capture(running, cx)
        });
        cx.notify();
    }

    pub fn set_reasoning_for_capture(
        &mut self,
        state: &str,
        expanded: bool,
        cx: &mut Context<Self>,
    ) {
        self.expanded_reasoning.clear();
        if expanded {
            self.expanded_reasoning
                .insert("reasoning-ui-capture".to_owned());
        }
        self.composer.update(cx, |composer, cx| {
            composer.set_reasoning_for_capture(state, cx)
        });
        cx.notify();
    }

    pub fn set_approval_for_capture(&mut self, kind: &str, state: &str, cx: &mut Context<Self>) {
        self.composer.update(cx, |composer, cx| {
            composer.set_approval_for_capture(kind, state, cx)
        });
        cx.notify();
    }

    pub fn set_file_approval_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.composer.update(cx, |composer, cx| {
            composer.set_file_approval_for_capture(state, cx)
        });
        cx.notify();
    }

    pub fn set_permissions_approval_for_capture(
        &mut self,
        kind: &str,
        state: &str,
        cx: &mut Context<Self>,
    ) {
        self.composer.update(cx, |composer, cx| {
            composer.set_permissions_approval_for_capture(kind, state, cx)
        });
        cx.notify();
    }

    pub fn set_file_change_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.expanded_tool_groups.clear();
        self.expanded_commands.clear();
        if state.contains("expanded") {
            self.expanded_tool_groups
                .insert("file-change-ui-capture".to_owned());
            self.expanded_commands
                .insert("file-change-ui-capture".to_owned());
        }
        self.composer.update(cx, |composer, cx| {
            composer.set_file_change_for_capture(state, cx)
        });
        cx.notify();
    }

    pub fn set_user_input_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.composer.update(cx, |composer, cx| {
            composer.set_user_input_for_capture(state, cx)
        });
        cx.notify();
    }

    fn handle_approval_card_event(
        &mut self,
        request_id: &str,
        event: crate::components::approval::ApprovalCardEvent,
        cx: &mut Context<Self>,
    ) {
        self.composer.update(cx, |composer, cx| {
            composer.handle_approval_card_event(request_id, event, cx)
        });
    }

    fn handle_file_approval_event(
        &mut self,
        request_id: &str,
        event: FileApprovalEvent,
        cx: &mut Context<Self>,
    ) {
        if let FileApprovalEvent::ReviewFile(index) = event {
            if let Some(review) = self
                .composer
                .read(cx)
                .file_approval_review(request_id, index)
            {
                cx.emit(OpenDiffReview(review));
            }
            return;
        }
        self.composer.update(cx, |composer, cx| {
            composer.handle_file_approval_event(request_id, event, cx)
        });
    }

    fn handle_permissions_approval_event(
        &mut self,
        request_id: &str,
        event: PermissionApprovalEvent,
        cx: &mut Context<Self>,
    ) {
        self.composer.update(cx, |composer, cx| {
            composer.handle_permissions_approval_event(request_id, event, cx)
        });
    }

    fn handle_file_change_activity_event(
        &mut self,
        event: FileChangeActivityEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            FileChangeActivityEvent::ToggleDetails { item_id } => {
                if !self.expanded_commands.remove(&item_id) {
                    self.expanded_commands.insert(item_id);
                }
                cx.notify();
            }
        }
    }

    fn handle_user_input_request_event(
        &mut self,
        request_id: &str,
        event: UserInputRequestEvent,
        cx: &mut Context<Self>,
    ) {
        self.composer.update(cx, |composer, cx| {
            composer.handle_user_input_request_event(request_id, event, cx)
        });
    }

    fn set_suggestion_pressed(
        &mut self,
        index: usize,
        pressed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = if pressed {
            SUGGESTION_PRESSED_SCALE
        } else {
            1.0
        };

        if cx.reduce_motion() || (target - self.suggestion_scale[index]).abs() <= f32::EPSILON {
            self.suggestion_scale[index] = target;
            self.suggestion_animation_from[index] = target;
            self.suggestion_animation_to[index] = target;
            self.suggestion_animation_started_at[index] = None;
            self.suggestion_animation_duration[index] = Duration::ZERO;
            cx.notify();
            return;
        }

        self.suggestion_animation_from[index] = self.suggestion_scale[index];
        self.suggestion_animation_to[index] = target;
        self.suggestion_animation_started_at[index] = Some(cx.background_executor().now());
        self.suggestion_animation_duration[index] = Duration::from_secs_f32(
            SUGGESTION_TRANSITION_DURATION.as_secs_f32()
                * (target - self.suggestion_scale[index]).abs()
                / (1.0 - SUGGESTION_PRESSED_SCALE),
        );

        let was_running = self.suggestion_animation_running;
        self.suggestion_animation_running = true;
        cx.notify();
        if !was_running {
            cx.on_next_frame(window, |this, window, cx| {
                this.advance_suggestion_animations(window, cx)
            });
        }
    }

    fn advance_suggestion_animations(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.suggestion_animation_running {
            return;
        }

        let now = cx.background_executor().now();
        let mut still_running = false;
        for index in 0..self.suggestion_scale.len() {
            let Some(started_at) = self.suggestion_animation_started_at[index] else {
                continue;
            };
            let duration = self.suggestion_animation_duration[index];
            let progress = if duration.is_zero() {
                1.0
            } else {
                now.saturating_duration_since(started_at).as_secs_f32() / duration.as_secs_f32()
            }
            .clamp(0.0, 1.0);
            self.suggestion_scale[index] = self.suggestion_animation_from[index]
                + (self.suggestion_animation_to[index] - self.suggestion_animation_from[index])
                    * suggestion_transition_ease(progress);

            if progress >= 1.0 || cx.reduce_motion() {
                self.suggestion_scale[index] = self.suggestion_animation_to[index];
                self.suggestion_animation_started_at[index] = None;
            } else {
                still_running = true;
            }
        }

        self.suggestion_animation_running = still_running;
        cx.notify();
        if still_running {
            cx.on_next_frame(window, |this, window, cx| {
                this.advance_suggestion_animations(window, cx)
            });
        }
    }

    fn sync_reasoning_disclosure_transitions(
        &mut self,
        units: &[ActivityStreamUnit],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = cx.background_executor().now();
        let mut present_items = HashSet::new();
        let mut should_animate = false;

        let visible_reasoning = units.iter().filter_map(|unit| match unit {
            ActivityStreamUnit::Standalone(ConversationActivity::Reasoning(reasoning)) => {
                Some(reasoning)
            }
            ActivityStreamUnit::Standalone(_) | ActivityStreamUnit::ToolGroup(_) => None,
        });
        for reasoning in visible_reasoning {
            present_items.insert(reasoning.item_id.clone());
            self.reasoning_scroll_handles
                .entry(reasoning.item_id.clone())
                .or_default();
            let has_content = !reasoning_body_text(reasoning).trim().is_empty();
            let expanded = has_content
                && (reasoning.is_active() || self.expanded_reasoning.contains(&reasoning.item_id));
            let target = if expanded { 1.0 } else { 0.0 };
            let transition = self
                .reasoning_disclosure_transitions
                .entry(reasoning.item_id.clone())
                .or_insert_with(|| ReasoningDisclosureTransition::settled(expanded));

            if (transition.target - target).abs() > f32::EPSILON {
                if cx.reduce_motion() {
                    *transition = ReasoningDisclosureTransition::settled(expanded);
                } else {
                    transition.from = transition.progress;
                    transition.target = target;
                    transition.started_at = Some(now);
                }
            }
            should_animate |= transition.started_at.is_some();
        }

        self.reasoning_disclosure_transitions
            .retain(|item_id, _| present_items.contains(item_id));
        self.reasoning_scroll_handles
            .retain(|item_id, _| present_items.contains(item_id));
        if should_animate && !self.reasoning_transition_running {
            self.reasoning_transition_running = true;
            cx.on_next_frame(window, |this, window, cx| {
                this.advance_reasoning_disclosure_transitions(window, cx)
            });
        }
    }

    fn advance_reasoning_disclosure_transitions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.reasoning_transition_running {
            return;
        }

        let now = cx.background_executor().now();
        let mut still_running = false;
        for transition in self.reasoning_disclosure_transitions.values_mut() {
            let Some(started_at) = transition.started_at else {
                continue;
            };
            let progress = (now.saturating_duration_since(started_at).as_secs_f32()
                / REASONING_TRANSITION_DURATION.as_secs_f32())
            .clamp(0.0, 1.0);
            transition.progress = transition.from
                + (transition.target - transition.from) * reasoning_transition_ease(progress);

            if progress >= 1.0 || cx.reduce_motion() {
                transition.progress = transition.target;
                transition.started_at = None;
            } else {
                still_running = true;
            }
        }

        self.reasoning_transition_running = still_running;
        cx.notify();
        if still_running {
            cx.on_next_frame(window, |this, window, cx| {
                this.advance_reasoning_disclosure_transitions(window, cx)
            });
        }
    }

    fn sync_tool_group_disclosure_transitions(
        &mut self,
        units: &[ActivityStreamUnit],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = cx.background_executor().now();
        let groups = units
            .iter()
            .filter_map(|unit| match unit {
                ActivityStreamUnit::ToolGroup(group) => Some(group),
                ActivityStreamUnit::Standalone(_) => None,
            })
            .collect::<Vec<_>>();
        let image_views = units
            .iter()
            .filter_map(|unit| match unit {
                ActivityStreamUnit::Standalone(ConversationActivity::ImageView(image)) => {
                    Some(image)
                }
                ActivityStreamUnit::Standalone(ConversationActivity::ImageViews(images)) => {
                    images.first()
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let present_groups = groups
            .iter()
            .map(|group| group.id.clone())
            .collect::<HashSet<_>>();
        let present_disclosures = present_groups
            .iter()
            .cloned()
            .chain(image_views.iter().map(|image| image.id.clone()))
            .collect::<HashSet<_>>();
        let active_groups = groups
            .iter()
            .filter(|group| group.is_active())
            .map(|group| group.id.clone())
            .collect::<HashSet<_>>();
        self.expanded_tool_groups
            .retain(|group_id| present_disclosures.contains(group_id));
        self.collapsed_active_tool_groups.retain(|group_id| {
            active_groups.contains(group_id)
                || image_views.iter().any(|image| image.id == *group_id)
        });
        self.tool_group_scroll_handles
            .retain(|group_id, _| present_groups.contains(group_id));

        let mut should_animate = false;
        for group in groups {
            self.tool_group_scroll_handles
                .entry(group.id.clone())
                .or_default();
            let expanded = if group.is_active() {
                !self.collapsed_active_tool_groups.contains(&group.id)
            } else {
                self.expanded_tool_groups.contains(&group.id)
            };
            let target = if expanded { 1.0 } else { 0.0 };
            let transition = self
                .tool_group_disclosure_transitions
                .entry(group.id.clone())
                .or_insert_with(|| ToolGroupDisclosureTransition::settled(expanded));
            if (transition.target - target).abs() > f32::EPSILON {
                if cx.reduce_motion() {
                    *transition = ToolGroupDisclosureTransition::settled(expanded);
                } else {
                    transition.from = transition.progress;
                    transition.chevron_from = transition.chevron_progress;
                    transition.target = target;
                    transition.started_at = Some(now);
                }
            }
            should_animate |= transition.started_at.is_some();
        }

        self.tool_group_disclosure_transitions
            .retain(|group_id, _| present_groups.contains(group_id));
        if should_animate && !self.tool_group_transition_running {
            self.tool_group_transition_running = true;
            cx.on_next_frame(window, |this, window, cx| {
                this.advance_tool_group_disclosure_transitions(window, cx)
            });
        }
    }

    fn advance_tool_group_disclosure_transitions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.tool_group_transition_running {
            return;
        }

        let now = cx.background_executor().now();
        let mut still_running = false;
        for transition in self.tool_group_disclosure_transitions.values_mut() {
            let Some(started_at) = transition.started_at else {
                continue;
            };
            let progress = (now.saturating_duration_since(started_at).as_secs_f32()
                / TOOL_GROUP_TRANSITION_DURATION.as_secs_f32())
            .clamp(0.0, 1.0);
            transition.progress = transition.from
                + (transition.target - transition.from) * reasoning_transition_ease(progress);
            transition.chevron_progress = transition.chevron_from
                + (transition.target - transition.chevron_from)
                    * tool_group_chevron_transition_ease(progress);

            if progress >= 1.0 || cx.reduce_motion() {
                transition.progress = transition.target;
                transition.chevron_progress = transition.target;
                transition.started_at = None;
            } else {
                still_running = true;
            }
        }

        self.tool_group_transition_running = still_running;
        cx.notify();
        if still_running {
            cx.on_next_frame(window, |this, window, cx| {
                this.advance_tool_group_disclosure_transitions(window, cx)
            });
        }
    }

    fn suggestion(
        &self,
        index: usize,
        label: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let hover_group = format!("home-suggestion-{index}");
        let scale = self.suggestion_scale[index];

        div()
            .id(("home-suggestion-hit-area", index))
            .relative()
            .top(px(-11.0))
            .left(px(7.0))
            .h(px(40.0))
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.set_suggestion_pressed(index, true, window, cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.set_suggestion_pressed(index, false, window, cx);
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.set_suggestion_pressed(index, false, window, cx);
                }),
            )
            .child(
                div()
                    .id(("home-suggestion", index))
                    .group(hover_group.clone())
                    // GPUI does not expose a transform for arbitrary elements,
                    // so scale the same geometry around a fixed 40px hit area.
                    // The hit target therefore never moves while the rendered
                    // row matches the reference's active:scale-[0.99].
                    .w(relative(scale))
                    .h(px(40.0 * scale))
                    .px(px(6.0 * scale))
                    .rounded(px(8.0 * scale))
                    .flex()
                    .items_center()
                    .gap(px(7.0 * scale))
                    .text_size(px(13.0 * scale))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .text_color(theme.text_tertiary)
                    .hover(move |style| style.text_color(theme.text))
                    .child(
                        suggestion_icon(theme.text_tertiary.into())
                            .w(px(14.0 * scale))
                            .h(px(12.0 * scale))
                            .group_hover(hover_group, move |style| style.text_color(theme.text)),
                    )
                    .child(label),
            )
    }
}

impl Render for HomeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_mode(self.mode);
        let (transcript, phase, assistant_message, conversation_activity) =
            if self.presentation != HomePresentation::Subagent {
                (
                    Vec::new(),
                    self.conversation_phase,
                    String::new(),
                    self.conversation_activity.clone(),
                )
            } else {
                let transcript = self.composer.read(cx).transcript_render_snapshot();
                let (phase, _, _, assistant_message, _, activity) =
                    self.composer.read(cx).conversation_render_snapshot();
                (transcript, phase, assistant_message, Rc::new(activity))
            };
        let user_input_other = self.composer.read(cx).user_input_other_entity();
        let user_input_other_focus = self.composer.read(cx).user_input_other_focus_handle(cx);
        let preview_requests = conversation_activity
            .iter()
            .filter_map(|activity| match activity {
                ConversationActivity::Approval(model) if model.should_render() => {
                    model.request.preview().map(|text| {
                        (
                            model.request_id.clone(),
                            text.to_owned(),
                            model.preview_expanded,
                        )
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        self.approval_previews
            .retain(|id, _| preview_requests.iter().any(|(request, _, _)| request == id));
        for (id, text, expanded) in preview_requests {
            if !self.approval_previews.contains_key(&id) {
                let preview =
                    cx.new(|cx| FileEditor::approval_preview(text.clone(), self.mode, cx));
                let owner = self.composer.clone();
                let request_id = id.clone();
                cx.observe(&preview, move |_, preview, cx| {
                    let lines = preview.read(cx).visual_line_count();
                    owner.update(cx, |composer, cx| {
                        composer.set_approval_preview_lines(&request_id, lines, cx)
                    });
                })
                .detach();
                self.approval_previews.insert(id.clone(), preview);
            }
            self.approval_previews[&id].update(cx, |preview, cx| {
                if preview.text() != text {
                    preview.reload(text, cx);
                }
                if preview.mode != self.mode {
                    preview.set_mode(self.mode, cx);
                }
                preview.set_preview_expanded(expanded, cx);
            });
        }
        let pending_request_id = conversation_activity
            .iter()
            .find_map(|activity| match activity {
                ConversationActivity::Approval(model) if model.should_render() => {
                    Some(model.request_id.clone())
                }
                ConversationActivity::FileApproval(model) if model.should_render() => {
                    Some(model.request_id.clone())
                }
                ConversationActivity::PermissionsApproval(model) if model.should_render() => {
                    Some(model.request_id.clone())
                }
                ConversationActivity::UserInput(model) if model.should_render() => {
                    Some(model.request_id.clone())
                }
                _ => None,
            });
        let blocking_keyboard_request_pending = self.presentation != HomePresentation::Subagent
            && conversation_activity.iter().any(|activity| {
                matches!(activity, ConversationActivity::Approval(model) if model.should_render())
                    || matches!(activity, ConversationActivity::FileApproval(model) if model.should_render())
                    || matches!(activity, ConversationActivity::PermissionsApproval(model) if model.should_render())
                    || matches!(activity, ConversationActivity::UserInput(model) if model.should_render())
            });
        if blocking_keyboard_request_pending
            && self.focused_approval_request != pending_request_id
            && !self.approval_focus.is_focused(window)
            && !user_input_other_focus.is_focused(window)
        {
            window.focus(&self.approval_focus, cx);
        } else if !blocking_keyboard_request_pending
            && (self.approval_focus.is_focused(window) || user_input_other_focus.is_focused(window))
        {
            let prompt_focus = self.composer.read(cx).prompt_focus_handle(cx);
            window.focus(&prompt_focus, cx);
        }
        self.focused_approval_request = pending_request_id;
        // Disclosure state belongs to every visible turn. On resume, all but
        // the last turn live in `transcript`; syncing only the current turn
        // immediately pruned a historical group id after its header was
        // clicked, making a valid command block appear inert.
        let disclosure_state_changed = self.conversation_rows.iter().any(|row| {
            let ConversationListRow::Activity { unit, .. } = row else {
                return false;
            };
            match unit {
                ActivityStreamUnit::Standalone(ConversationActivity::Reasoning(reasoning)) => {
                    let expanded = reasoning.is_active()
                        || self.expanded_reasoning.contains(&reasoning.item_id);
                    self.reasoning_disclosure_transitions
                        .get(&reasoning.item_id)
                        .is_some_and(|transition| {
                            (transition.target - if expanded { 1.0 } else { 0.0 }).abs()
                                > f32::EPSILON
                        })
                }
                ActivityStreamUnit::ToolGroup(group) => {
                    let expanded = if group.is_active() {
                        !self.collapsed_active_tool_groups.contains(&group.id)
                    } else {
                        self.expanded_tool_groups.contains(&group.id)
                    };
                    self.tool_group_disclosure_transitions
                        .get(&group.id)
                        .is_some_and(|transition| {
                            (transition.target - if expanded { 1.0 } else { 0.0 }).abs()
                                > f32::EPSILON
                        })
                }
                _ => false,
            }
        });
        let conversation_data_changed = self.conversation_cache_dirty || disclosure_state_changed;
        if self.presentation == HomePresentation::Subagent || conversation_data_changed {
            let visible_activity_units = if self.presentation != HomePresentation::Subagent {
                self.conversation_rows
                    .iter()
                    .filter_map(|row| match row {
                        ConversationListRow::Activity { unit, .. } => Some(unit.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            } else {
                transcript
                    .iter()
                    .flat_map(|turn| activity_stream_units(&turn.activities))
                    .chain(activity_stream_units(&conversation_activity))
                    .collect::<Vec<_>>()
            };
            self.sync_reasoning_disclosure_transitions(&visible_activity_units, window, cx);
            self.sync_tool_group_disclosure_transitions(&visible_activity_units, window, cx);
            for unit in &visible_activity_units {
                match unit {
                    ActivityStreamUnit::Standalone(ConversationActivity::AutoApprovalReview(
                        review,
                    )) => {
                        self.sync_auto_review_view(review, cx);
                    }
                    ActivityStreamUnit::Standalone(ConversationActivity::Command(command)) => {
                        self.command_scroll_handles
                            .entry(command.id.clone())
                            .or_default();
                    }
                    ActivityStreamUnit::ToolGroup(group) => {
                        for activity in &group.activities {
                            if let ConversationActivity::AutoApprovalReview(review) = activity {
                                self.sync_auto_review_view(review, cx);
                            }
                        }
                        for command in &group.commands {
                            self.command_scroll_handles
                                .entry(command.id.clone())
                                .or_default();
                        }
                    }
                    _ => {}
                }
            }
            self.conversation_cache_dirty = false;
        }
        let reasoning_disclosure_progress = self
            .reasoning_disclosure_transitions
            .iter()
            .map(|(item_id, transition)| (item_id.clone(), transition.progress))
            .collect();
        let tool_group_disclosure_progress = self
            .tool_group_disclosure_transitions
            .iter()
            .map(|(group_id, transition)| {
                (
                    group_id.clone(),
                    (transition.progress, transition.chevron_progress),
                )
            })
            .collect();
        if conversation_data_changed || conversation_status(phase).is_some() {
            for unit in activity_stream_units(&conversation_activity) {
                let ActivityStreamUnit::ToolGroup(group) = unit else {
                    continue;
                };
                let expanded = if group.is_active() {
                    !self.collapsed_active_tool_groups.contains(&group.id)
                } else {
                    self.expanded_tool_groups.contains(&group.id)
                };
                let scroll_handle = self
                    .tool_group_scroll_handles
                    .entry(group.id.clone())
                    .or_default();
                if group.is_active() && expanded && scroll_should_follow_output(scroll_handle) {
                    scroll_handle.scroll_to_bottom();
                }
            }
            for activity in conversation_activity.iter() {
                match activity {
                    ConversationActivity::Reasoning(reasoning) => {
                        let scroll_handle = self
                            .reasoning_scroll_handles
                            .entry(reasoning.item_id.clone())
                            .or_default();
                        if reasoning.is_active() && scroll_should_follow_output(scroll_handle) {
                            scroll_handle.scroll_to_bottom();
                        }
                    }
                    ConversationActivity::Command(command) => {
                        let scroll_handle = self
                            .command_scroll_handles
                            .entry(command.id.clone())
                            .or_default();
                        if command.status == CommandExecutionStatus::InProgress
                            && scroll_should_follow_output(scroll_handle)
                        {
                            scroll_handle.scroll_to_bottom();
                        }
                    }
                    _ => {}
                }
            }
        }
        if self.presentation == HomePresentation::Subagent {
            if phase == ConversationPhase::Empty && conversation_activity.is_empty() {
                self.conversation_scroll.set_offset(point(px(0.0), px(0.0)));
            } else if scroll_should_follow_output(&self.conversation_scroll) {
                self.conversation_scroll.scroll_to_bottom();
            }
        }
        let content = match self.presentation {
            HomePresentation::Conversation | HomePresentation::SideChat => home(
                ConversationRenderContext {
                    home_entity: cx.entity(),
                    approval_previews: self.approval_previews.clone(),
                    approval_border_offset: 0.5 / window.scale_factor(),
                    request_owner: context::RequestOwner::new(self.composer.clone(), cx),
                    theme,
                    thinking_shimmer_progress: self.thinking_shimmer_progress,
                    response_feedback: self.response_feedback,
                    user_message_actions_visible_for_capture: self
                        .user_message_actions_visible_for_capture,
                    disclosures: Rc::new(DisclosureRenderState {
                        expanded_reasoning: self.expanded_reasoning.clone(),
                        reasoning_disclosure_progress,
                        reasoning_scroll_handles: self.reasoning_scroll_handles.clone(),
                        expanded_tool_groups: self.expanded_tool_groups.clone(),
                        collapsed_active_tool_groups: self.collapsed_active_tool_groups.clone(),
                        tool_group_disclosure_progress,
                        tool_group_scroll_handles: self.tool_group_scroll_handles.clone(),
                        expanded_commands: self.expanded_commands.clone(),
                        command_scroll_handles: self.command_scroll_handles.clone(),
                        expanded_collaborations: self.expanded_collaborations.clone(),
                        auto_review_views: self.auto_review_views.clone(),
                    }),
                },
                self.composer.clone(),
                user_input_other,
                MainConversationSnapshot {
                    side_chat: self.presentation == HomePresentation::SideChat,
                    composer_height: self.composer.read(cx).side_composer_height(cx),
                    rows: self.conversation_rows.clone(),
                    phase,
                    activities: conversation_activity,
                    list: self.conversation_list.clone(),
                },
                self.suggestion(
                    0,
                    "Prove plugin upgrades never mutate an active run",
                    theme,
                    cx,
                ),
                self.suggestion(
                    1,
                    "Verify the full /plugins lifecycle in the interactive terminal",
                    theme,
                    cx,
                ),
            ),
            HomePresentation::Subagent => subagent_conversation(
                ConversationRenderContext {
                    home_entity: cx.entity(),
                    approval_previews: self.approval_previews.clone(),
                    approval_border_offset: 0.5 / window.scale_factor(),
                    request_owner: context::RequestOwner::new(self.composer.clone(), cx),
                    theme,
                    thinking_shimmer_progress: self.thinking_shimmer_progress,
                    response_feedback: self.response_feedback,
                    user_message_actions_visible_for_capture: false,
                    disclosures: Rc::new(DisclosureRenderState {
                        expanded_reasoning: self.expanded_reasoning.clone(),
                        reasoning_disclosure_progress,
                        reasoning_scroll_handles: self.reasoning_scroll_handles.clone(),
                        expanded_tool_groups: self.expanded_tool_groups.clone(),
                        collapsed_active_tool_groups: self.collapsed_active_tool_groups.clone(),
                        tool_group_disclosure_progress,
                        tool_group_scroll_handles: self.tool_group_scroll_handles.clone(),
                        expanded_commands: self.expanded_commands.clone(),
                        command_scroll_handles: self.command_scroll_handles.clone(),
                        expanded_collaborations: self.expanded_collaborations.clone(),
                        auto_review_views: self.auto_review_views.clone(),
                    }),
                },
                transcript,
                phase,
                assistant_message,
                conversation_activity.as_ref().clone(),
                self.conversation_scroll.clone(),
            ),
        };
        let measured_home = cx.entity();
        content
            .when(blocking_keyboard_request_pending, |content| {
                content.key_context(
                    if self
                        .approval_previews
                        .values()
                        .any(|preview| preview.focus_handle(cx).is_focused(window))
                    {
                        "ApprovalText"
                    } else {
                        "ApprovalCard"
                    },
                )
            })
            .track_focus(&self.approval_focus)
            .on_action(cx.listener(
                |home, action: &crate::components::approval::ApprovalShortcut, window, cx| {
                    home.handle_approval_key(
                        &KeyDownEvent {
                            keystroke: gpui::Keystroke::parse(action.0)
                                .expect("registered approval shortcut"),
                            is_held: false,
                            prefer_character_input: false,
                        },
                        window,
                        cx,
                    );
                },
            ))
            .on_key_down(cx.listener(Self::handle_approval_key))
            .child(
                gpui::canvas(
                    move |bounds, window, cx| {
                        let width = f32::from(bounds.size.width).max(1.0);
                        let composer_width = width.min(748.0);
                        let composer_right =
                            f32::from(bounds.left()) + (width + composer_width) * 0.5 - 6.0;
                        let trailing_margin =
                            (f32::from(window.viewport_size().width) - composer_right).max(0.0);
                        let width_changed = measured_home.update(cx, |home, cx| {
                            let width_changed = (home.content_width - width).abs() > 0.5;
                            if width_changed {
                                home.content_width = width;
                                home.conversation_list.remeasure();
                                cx.notify();
                            }
                            home.composer.update(cx, |composer, cx| {
                                composer.set_available_width(
                                    composer_width - 12.0,
                                    trailing_margin,
                                    cx,
                                )
                            });
                            width_changed
                        });
                        if width_changed {
                            let home = measured_home.downgrade();
                            // A size discovered during prepaint needs a subsequent
                            // frame: invalidation in the current frame may already
                            // have been consumed by the virtual list's cache.
                            window.on_next_frame(move |_, cx| {
                                let _ = home.update(cx, |home, cx| {
                                    home.conversation_list.remeasure();
                                    cx.notify();
                                });
                            });
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
    }
}

#[cfg(test)]
mod resume_activity_regression_tests;
#[cfg(test)]
mod resumed_history_tests;
#[cfg(test)]
mod tests;

#[cfg(feature = "screenshot")]
pub(crate) use timeline::resumed_activity_audit;

#[cfg(feature = "screenshot")]
impl HomeView {
    pub fn set_progress_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |view, cx| view.set_progress_for_capture(state, cx));
        cx.notify();
    }
}

#[derive(Clone)]
pub struct OpenPlan(pub crate::agent::AgentPlan);
impl gpui::EventEmitter<OpenPlan> for HomeView {}
#[derive(Clone)]
pub struct DownloadPlan(pub crate::agent::AgentPlan);
impl gpui::EventEmitter<DownloadPlan> for HomeView {}

impl HomeView {
    pub fn set_plan_panel_for_view(&mut self, item_id: Option<String>, cx: &mut Context<Self>) {
        let incoming = item_id.map(|id| format!("plan-in-panel:{id}"));
        let previous = self
            .expanded_commands
            .iter()
            .find(|key| key.starts_with("plan-in-panel:"))
            .cloned();
        if previous == incoming {
            return;
        }
        self.expanded_commands
            .retain(|key| !key.starts_with("plan-in-panel:"));
        if let Some(key) = incoming {
            self.expanded_commands.insert(key);
        }
        self.conversation_list.remeasure();
        self.conversation_cache_dirty = true;
        cx.notify();
    }
}

impl HomeView {
    pub fn dismiss_plan_popovers(&mut self, cx: &mut Context<Self>) {
        let count = self.expanded_commands.len();
        self.expanded_commands
            .retain(|key| !key.starts_with("turn-plan-") && !key.starts_with("plan-feedback-"));
        if self.expanded_commands.len() != count {
            cx.notify();
        }
    }
}
