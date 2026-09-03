use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    App, Bounds, BoxShadow, ContentMask, Context, Div, Entity, FocusHandle, FontWeight,
    KeyDownEvent, MouseButton, ObjectFit, PathBuilder, Pixels, Render, Role, ScrollDelta,
    ScrollHandle, ScrollWheelEvent, ShapedLine, SharedString, TextAlign, TextRun, Transformation,
    Window, canvas, div, linear_color_stop, linear_gradient, point, prelude::*, px, radians,
    relative, rgba,
};

use crate::{
    agent::{
        AgentBackend, AgentFileChangeStatus, AgentImageView, CodexAppServerBackend,
        CommandExecution, CommandExecutionAction, CommandExecutionStatus,
    },
    components::{
        approval::{ApprovalCardCallback, render_approval_card},
        composer::{
            ComposerView, ConversationActivity, ConversationChanged, ConversationPhase,
            ConversationThreadCreated, ConversationTranscriptTurn, ModelCatalogLoadFinished,
            ReasoningActivityPresentation, RequestFullAccessConfirmation,
        },
        file_change::{
            DiffReviewPresentation, FileApprovalCallback, FileApprovalEvent,
            FileApprovalPresentation, FileChangeActivityCallback, FileChangeActivityEvent,
            render_file_approval_card, render_file_change_activity,
        },
        icons::{icon, suggestion_icon},
        markdown::render_assistant_markdown,
        permissions_approval::{
            PermissionApprovalCallback, PermissionApprovalEvent, PermissionApprovalPresentation,
            render_permissions_approval,
        },
        prompt_input::PromptInput,
        user_input_request::{
            UserInputRequestCallback, UserInputRequestEvent, render_user_input_request,
        },
    },
    theme::{Theme, ThemeMode, UI_MONOSPACE_FONT_FAMILY},
};

pub struct HomeView {
    mode: ThemeMode,
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
    user_message_actions_visible_for_capture: bool,
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
    command_scroll_handles: HashMap<String, ScrollHandle>,
    approval_focus: FocusHandle,
}

impl gpui::EventEmitter<RequestFullAccessConfirmation> for HomeView {}
impl gpui::EventEmitter<ModelCatalogLoadFinished> for HomeView {}
impl gpui::EventEmitter<ConversationThreadCreated> for HomeView {}

pub struct OpenDiffReview(pub DiffReviewPresentation);
impl gpui::EventEmitter<OpenDiffReview> for HomeView {}

pub struct OpenImagePreview(pub PathBuf);
impl gpui::EventEmitter<OpenImagePreview> for HomeView {}

const SUGGESTION_PRESSED_SCALE: f32 = 0.99;
const SUGGESTION_TRANSITION_DURATION: Duration = Duration::from_millis(150);
const THINKING_SHIMMER_DURATION: Duration = Duration::from_secs(1);
const THINKING_SHIMMER_STEPS: f32 = 48.0;
const THINKING_SHIMMER_FRAME_INTERVAL: Duration = Duration::from_micros(20_833);
const THINKING_SHIMMER_WIDTH: f32 = 56.0;
const THINKING_SHIMMER_BAND_SCALE: f32 = 0.5;
const THINKING_SHIMMER_ALPHA_LEVELS: usize = 32;
const CONVERSATION_TOP_INSET: f32 = 78.0;
const CONVERSATION_BOTTOM_INSET: f32 = 153.0;
const CONVERSATION_BOTTOM_EPSILON: f32 = 0.5;
const USER_MESSAGE_BUBBLE_RADIUS: f32 = 22.0;
const USER_MESSAGE_BUBBLE_SUPERELLIPSE: f32 = 1.5;
const USER_MESSAGE_FOOTER_OFFSET: f32 = 3.0;
const USER_MESSAGE_FOOTER_HEIGHT: f32 = 26.0;
const USER_MESSAGE_FOOTER_SIDE_MARGIN: f32 = 4.0;
const USER_MESSAGE_FOOTER_GAP: f32 = 8.0;
const USER_MESSAGE_TIME_SIZE: f32 = 12.0;
const USER_MESSAGE_TIME_LINE_HEIGHT: f32 = 16.0;
const RESPONSE_ACTION_ICON_SIZE: f32 = 16.0;
const RESPONSE_ACTION_FOOTER_OFFSET: f32 = 6.0;
const RESPONSE_ACTION_FOOTER_HEIGHT: f32 = 20.0;
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

#[derive(Clone, Copy, Debug)]
struct ReasoningDisclosureTransition {
    progress: f32,
    from: f32,
    target: f32,
    started_at: Option<Instant>,
}

impl ReasoningDisclosureTransition {
    fn settled(expanded: bool) -> Self {
        let progress = if expanded { 1.0 } else { 0.0 };
        Self {
            progress,
            from: progress,
            target: progress,
            started_at: None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ToolGroupDisclosureTransition {
    progress: f32,
    chevron_progress: f32,
    from: f32,
    chevron_from: f32,
    target: f32,
    started_at: Option<Instant>,
}

impl ToolGroupDisclosureTransition {
    fn settled(expanded: bool) -> Self {
        let progress = if expanded { 1.0 } else { 0.0 };
        Self {
            progress,
            chevron_progress: progress,
            from: progress,
            chevron_from: progress,
            target: progress,
            started_at: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ToolActivityGroupPresentation {
    id: String,
    reasoning: Vec<ReasoningActivityPresentation>,
    commands: Vec<CommandExecution>,
    file_changes: Vec<crate::components::file_change::FileChangeActivityPresentation>,
}

impl ToolActivityGroupPresentation {
    fn is_active(&self) -> bool {
        self.reasoning
            .iter()
            .any(ReasoningActivityPresentation::is_active)
            || self
                .commands
                .iter()
                .any(|command| command.status == CommandExecutionStatus::InProgress)
            || self
                .file_changes
                .iter()
                .any(|change| change.status == AgentFileChangeStatus::InProgress)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ActivityStreamUnit {
    Standalone(ConversationActivity),
    ToolGroup(ToolActivityGroupPresentation),
}

#[derive(Default)]
struct PendingToolActivityGroup {
    id: Option<String>,
    reasoning: Vec<ReasoningActivityPresentation>,
    commands: Vec<CommandExecution>,
    file_changes: Vec<crate::components::file_change::FileChangeActivityPresentation>,
}

fn flush_pending_tool_activity_group(
    pending: &mut PendingToolActivityGroup,
    units: &mut Vec<ActivityStreamUnit>,
) {
    if pending.commands.is_empty() && pending.file_changes.is_empty() {
        // ChatGPT does not render completed reasoning as an independent
        // "思考了 …" row. It is presentation context for an adjacent tool
        // block and remains invisible when no command belongs to the group.
        pending.reasoning.clear();
        pending.id = None;
        return;
    }

    let id = pending.id.take().expect("a populated tool group has an id");
    units.push(ActivityStreamUnit::ToolGroup(
        ToolActivityGroupPresentation {
            id,
            reasoning: std::mem::take(&mut pending.reasoning),
            commands: std::mem::take(&mut pending.commands),
            file_changes: std::mem::take(&mut pending.file_changes),
        },
    ));
}

fn activity_stream_units(activities: &[ConversationActivity]) -> Vec<ActivityStreamUnit> {
    let mut units = Vec::new();
    let mut pending = PendingToolActivityGroup::default();
    let mut active_reasoning = Vec::new();

    for activity in activities {
        match activity {
            ConversationActivity::Reasoning(reasoning) if reasoning.is_active() => {
                // The desktop app treats the active reasoning row as a live
                // cursor: it follows every newer JSON-RPC item instead of
                // staying where reasoning/itemStarted first inserted it.
                flush_pending_tool_activity_group(&mut pending, &mut units);
                // Preserve the reasoning id as the stable disclosure key when
                // the next protocol items are commands from the same group.
                pending.id = Some(reasoning.item_id.clone());
                active_reasoning.push(reasoning.clone());
            }
            ConversationActivity::Reasoning(reasoning) => {
                pending.id.get_or_insert_with(|| reasoning.item_id.clone());
                pending.reasoning.push(reasoning.clone());
            }
            ConversationActivity::Command(command) => {
                pending.id.get_or_insert_with(|| command.id.clone());
                pending.commands.push(command.clone());
            }
            ConversationActivity::FileChange(change) => {
                pending.id.get_or_insert_with(|| change.item_id.clone());
                pending.file_changes.push(change.clone());
            }
            standalone => {
                flush_pending_tool_activity_group(&mut pending, &mut units);
                units.push(ActivityStreamUnit::Standalone(standalone.clone()));
            }
        }
    }
    flush_pending_tool_activity_group(&mut pending, &mut units);
    units.extend(active_reasoning.into_iter().map(|reasoning| {
        ActivityStreamUnit::Standalone(ConversationActivity::Reasoning(reasoning))
    }));
    units
}

fn reasoning_activity_title(reasoning: &ReasoningActivityPresentation) -> Option<String> {
    let candidate = reasoning
        .summary
        .iter()
        .find(|part| !part.trim().is_empty())
        .or_else(|| {
            reasoning
                .content
                .iter()
                .find(|part| !part.trim().is_empty())
        })?
        .trim();
    let candidate = if let Some(after_opening) = candidate.strip_prefix("**") {
        after_opening
            .find("**")
            .map(|closing| &after_opening[..closing])
            .unwrap_or(after_opening)
    } else {
        candidate.lines().next().unwrap_or(candidate)
    };
    let candidate = candidate
        .trim()
        .trim_start_matches('#')
        .trim_start_matches(['-', '*'])
        .trim();
    (!candidate.is_empty()).then(|| candidate.to_owned())
}

fn tool_group_reasoning_title(group: &ToolActivityGroupPresentation) -> Option<String> {
    group
        .reasoning
        .iter()
        .rev()
        .find_map(reasoning_activity_title)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CommandActivitySummary {
    icon: &'static str,
    text: String,
    reads_files: bool,
    runs_command: bool,
}

fn command_activity_summary(command: &CommandExecution) -> CommandActivitySummary {
    command_activity_summaries(command)
        .into_iter()
        .next()
        .expect("every command execution has at least one presentation row")
}

fn command_activity_summaries(command: &CommandExecution) -> Vec<CommandActivitySummary> {
    if command.actions.is_empty() {
        return vec![generic_command_activity_summary(command, &command.command)];
    }
    command
        .actions
        .iter()
        .map(|action| command_action_summary(command, action))
        .collect()
}

fn command_action_summary(
    command: &CommandExecution,
    action: &CommandExecutionAction,
) -> CommandActivitySummary {
    let completed = command.status == CommandExecutionStatus::Completed;
    let failed = command.status == CommandExecutionStatus::Failed;

    match action {
        CommandExecutionAction::Read { name, path, .. } => {
            let target = if name.trim().is_empty() { path } else { name };
            let text = if failed {
                format!("读取失败 {target}")
            } else if completed {
                format!("已读取 {target}")
            } else {
                format!("正在读取 {target}")
            };
            CommandActivitySummary {
                icon: "activity-read",
                text,
                reads_files: true,
                runs_command: false,
            }
        }
        CommandExecutionAction::ListFiles { path, .. } => {
            let target = path.as_deref().filter(|path| !path.trim().is_empty());
            let text = match (failed, completed, target) {
                (true, _, Some(path)) => format!("列出 {path} 中的文件失败"),
                (true, _, None) => "列出文件失败".to_owned(),
                (false, true, Some(path)) => format!("已列出 {path} 中的文件"),
                (false, true, None) => "已列出文件".to_owned(),
                (false, false, Some(path)) => format!("正在列出 {path} 中的文件"),
                (false, false, None) => "正在列出文件".to_owned(),
            };
            CommandActivitySummary {
                icon: "activity-read",
                text,
                reads_files: true,
                runs_command: false,
            }
        }
        CommandExecutionAction::Search { path, query, .. } => {
            let path = path.as_deref().filter(|path| !path.trim().is_empty());
            let query = query.as_deref().filter(|query| !query.trim().is_empty());
            let text = match (failed, completed, path, query) {
                (true, _, _, Some(query)) => format!("搜索“{query}”失败"),
                (true, _, _, None) => "搜索文件失败".to_owned(),
                (false, true, Some(path), Some(query)) => {
                    format!("已在 {path} 中搜索“{query}”")
                }
                (false, true, _, Some(query)) => format!("已对“{query}”进行搜索"),
                (false, true, _, None) => "已搜索文件".to_owned(),
                (false, false, Some(path), Some(query)) => {
                    format!("正在 {path} 中搜索“{query}”")
                }
                (false, false, _, Some(query)) => format!("正在搜索“{query}”"),
                (false, false, _, None) => "正在搜索文件".to_owned(),
            };
            CommandActivitySummary {
                icon: "search",
                text,
                reads_files: true,
                runs_command: false,
            }
        }
        CommandExecutionAction::Unknown {
            command: action, ..
        } => generic_command_activity_summary(command, action),
    }
}

fn generic_command_activity_summary(
    command: &CommandExecution,
    display_command: &str,
) -> CommandActivitySummary {
    // Browser text in ChatGPT's one-line activity label uses normal
    // whitespace collapsing. GPUI preserves embedded newlines, so a heredoc
    // command otherwise paints several lines through the fixed 21px row.
    let display_command = display_command
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let display_command = if display_command.is_empty() {
        "命令"
    } else {
        &display_command
    };
    let text = match command.status {
        CommandExecutionStatus::InProgress => format!("正在运行 {display_command}"),
        CommandExecutionStatus::Completed => format!("已运行 {display_command}"),
        CommandExecutionStatus::Failed => format!("运行失败 {display_command}"),
    };
    CommandActivitySummary {
        icon: "panel-terminal",
        text,
        reads_files: false,
        runs_command: true,
    }
}

fn completed_tool_group_summary(group: &ToolActivityGroupPresentation) -> CommandActivitySummary {
    let command_summaries = group
        .commands
        .iter()
        .flat_map(command_activity_summaries)
        .collect::<Vec<_>>();
    let reads_files = command_summaries.iter().any(|summary| summary.reads_files);
    let runs_command = command_summaries.iter().any(|summary| summary.runs_command);
    let edits_files = !group.file_changes.is_empty();
    let text = match (edits_files, reads_files, runs_command) {
        (true, true, true) => "编辑了文件读取文件运行了命令",
        (true, true, false) => "编辑了文件读取文件",
        (true, false, true) => "编辑了文件运行了命令",
        (true, false, false) => "编辑了文件",
        (false, true, true) => "已读取文件运行了命令",
        (false, true, false) => "已读取文件",
        (false, false, true) => "运行了命令",
        (false, false, false) => "已工作",
    };
    CommandActivitySummary {
        icon: if edits_files {
            "message-edit"
        } else if reads_files {
            "activity-read"
        } else {
            "panel-terminal"
        },
        text: text.to_owned(),
        reads_files,
        runs_command,
    }
}

fn command_activity_row_count(command: &CommandExecution) -> usize {
    command.actions.len().max(1)
}

fn tool_group_row_count(group: &ToolActivityGroupPresentation) -> usize {
    group
        .commands
        .iter()
        .map(command_activity_row_count)
        .sum::<usize>()
        + group.file_changes.len()
}

fn strip_terminal_line_ending(output: &str) -> &str {
    output
        .strip_suffix("\r\n")
        .or_else(|| output.strip_suffix('\n'))
        .unwrap_or(output)
}

fn superellipse_corner_points(
    center_x: f32,
    center_y: f32,
    radius: f32,
    start_angle: f32,
) -> impl Iterator<Item = gpui::Point<Pixels>> {
    const SEGMENTS: usize = 12;
    let exponent = 2.0_f32.powf(USER_MESSAGE_BUBBLE_SUPERELLIPSE);
    (1..=SEGMENTS).map(move |step| {
        let angle = start_angle + std::f32::consts::FRAC_PI_2 * step as f32 / SEGMENTS as f32;
        let cosine = angle.cos();
        let sine = angle.sin();
        point(
            px(center_x + cosine.signum() * cosine.abs().powf(2.0 / exponent) * radius),
            px(center_y + sine.signum() * sine.abs().powf(2.0 / exponent) * radius),
        )
    })
}

fn user_message_bubble_path(bounds: Bounds<Pixels>) -> gpui::Path<Pixels> {
    let left = f32::from(bounds.left());
    let top = f32::from(bounds.top());
    let right = f32::from(bounds.right());
    let bottom = f32::from(bounds.bottom());
    let radius = USER_MESSAGE_BUBBLE_RADIUS
        .min((right - left) * 0.5)
        .min((bottom - top) * 0.5);
    let mut builder = PathBuilder::fill();
    builder.move_to(point(px(left + radius), px(top)));
    builder.line_to(point(px(right - radius), px(top)));
    for point in superellipse_corner_points(
        right - radius,
        top + radius,
        radius,
        -std::f32::consts::FRAC_PI_2,
    ) {
        builder.line_to(point);
    }
    builder.line_to(point(px(right), px(bottom - radius)));
    for point in superellipse_corner_points(right - radius, bottom - radius, radius, 0.0) {
        builder.line_to(point);
    }
    builder.line_to(point(px(left + radius), px(bottom)));
    for point in superellipse_corner_points(
        left + radius,
        bottom - radius,
        radius,
        std::f32::consts::FRAC_PI_2,
    ) {
        builder.line_to(point);
    }
    builder.line_to(point(px(left), px(top + radius)));
    for point in
        superellipse_corner_points(left + radius, top + radius, radius, std::f32::consts::PI)
    {
        builder.line_to(point);
    }
    builder.close();
    builder
        .build()
        .expect("user message superellipse should tessellate")
}

fn thinking_shimmer_step(progress: f32) -> f32 {
    (progress.clamp(0.0, 1.0) * THINKING_SHIMMER_STEPS).floor() / THINKING_SHIMMER_STEPS
}

fn thinking_shimmer_progress(elapsed: Duration) -> f32 {
    (elapsed.as_secs_f32() / THINKING_SHIMMER_DURATION.as_secs_f32()).clamp(0.0, 1.0)
}

fn thinking_shimmer_band_left(progress: f32, text_width: f32) -> f32 {
    // CSS background-position percentages are relative to the remaining width.
    // With a 50%-wide image, -100%..250% resolves to -0.5w..1.25w.
    let remaining_width = text_width * (1.0 - THINKING_SHIMMER_BAND_SCALE);
    remaining_width * (-1.0 + 3.5 * thinking_shimmer_step(progress))
}

fn thinking_shimmer_alpha(position: f32) -> f32 {
    let position = position.clamp(0.0, 1.0);
    if position < 0.4 {
        position / 0.4 * 0.75
    } else if position <= 0.6 {
        0.75
    } else {
        (1.0 - position) / 0.4 * 0.75
    }
}

fn suggestion_transition_ease(progress: f32) -> f32 {
    fn bezier(t: f32, first: f32, second: f32) -> f32 {
        let inverse = 1.0 - t;
        3.0 * inverse * inverse * t * first + 3.0 * inverse * t * t * second + t * t * t
    }

    let progress = progress.clamp(0.0, 1.0);
    if progress == 0.0 || progress == 1.0 {
        return progress;
    }

    // chat-reference: --ease-enter-snappy: cubic-bezier(.23, 1, .32, 1)
    let (mut lower, mut upper) = (0.0, 1.0);
    for _ in 0..12 {
        let parameter = (lower + upper) * 0.5;
        if bezier(parameter, 0.23, 0.32) < progress {
            lower = parameter;
        } else {
            upper = parameter;
        }
    }
    bezier((lower + upper) * 0.5, 1.0, 1.0)
}

impl HomeView {
    #[allow(dead_code)]
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
        let mut view = Self {
            mode,
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
            user_message_actions_visible_for_capture: false,
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
            command_scroll_handles: HashMap::new(),
            approval_focus: cx.focus_handle(),
        };
        view.observe_composer(composer, cx);
        view
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
                let phase = composer.read(cx).conversation_phase();
                this.sync_thinking_shimmer(phase, cx);
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
        self.observe_composer(composer.clone(), cx);
        self.composer = composer;
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
        let phase = self.composer.read(cx).conversation_phase();
        self.sync_thinking_shimmer(phase, cx);
        cx.notify();
    }

    fn handle_approval_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let other_focus = self.composer.read(cx).user_input_other_focus_handle(cx);
        if other_focus.is_focused(window)
            && !matches!(event.keystroke.key.as_str(), "tab" | "escape")
        {
            return;
        }
        let handled = self.composer.update(cx, |composer, cx| {
            composer.handle_approval_key(event, cx) || composer.handle_user_input_key(event, cx)
        });
        if handled {
            if other_focus.is_focused(window) && event.keystroke.key.as_str() == "tab" {
                window.focus(&self.approval_focus, cx);
            }
            cx.stop_propagation();
        }
    }

    fn sync_thinking_shimmer(&mut self, phase: ConversationPhase, cx: &mut Context<Self>) {
        if phase == ConversationPhase::Thinking {
            if !self.thinking_shimmer_running {
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
        let max_scroll = f32::from(self.conversation_scroll.max_offset().y).max(0.0);
        let scroll_top = (max_scroll - distance.max(0.0)).max(0.0);
        self.conversation_scroll
            .set_offset(point(px(0.0), px(-scroll_top)));
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
                .or_insert_with(ScrollHandle::new);
            let has_content = !reasoning_body_text(&reasoning).trim().is_empty();
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
                .or_insert_with(ScrollHandle::new);
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
        let transcript = self.composer.read(cx).transcript_render_snapshot();
        let (
            phase,
            user_message,
            user_message_time,
            assistant_message,
            assistant_message_time,
            conversation_activity,
        ) = self.composer.read(cx).conversation_render_snapshot();
        let user_input_other = self.composer.read(cx).user_input_other_entity();
        let user_input_other_focus = self.composer.read(cx).user_input_other_focus_handle(cx);
        let blocking_keyboard_request_pending = conversation_activity.iter().any(|activity| {
            matches!(activity, ConversationActivity::Approval(model) if model.should_render())
                || matches!(activity, ConversationActivity::FileApproval(model) if model.should_render())
                || matches!(activity, ConversationActivity::PermissionsApproval(model) if model.should_render())
                || matches!(activity, ConversationActivity::UserInput(model) if model.should_render())
        });
        if blocking_keyboard_request_pending
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
        // Disclosure state belongs to every visible turn. On resume, all but
        // the last turn live in `transcript`; syncing only the current turn
        // immediately pruned a historical group id after its header was
        // clicked, making a valid command block appear inert.
        let visible_activity_units = transcript
            .iter()
            .flat_map(|turn| activity_stream_units(&turn.activities))
            .chain(activity_stream_units(&conversation_activity))
            .collect::<Vec<_>>();
        self.sync_reasoning_disclosure_transitions(&visible_activity_units, window, cx);
        self.sync_tool_group_disclosure_transitions(&visible_activity_units, window, cx);
        for activity in transcript
            .iter()
            .flat_map(|turn| turn.activities.iter())
            .chain(conversation_activity.iter())
        {
            if let ConversationActivity::Command(command) = activity {
                self.command_scroll_handles
                    .entry(command.id.clone())
                    .or_insert_with(ScrollHandle::new);
            }
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
                .or_insert_with(ScrollHandle::new);
            if group.is_active() && expanded && scroll_should_follow_output(scroll_handle) {
                scroll_handle.scroll_to_bottom();
            }
        }
        for activity in &conversation_activity {
            match activity {
                ConversationActivity::Reasoning(reasoning) => {
                    let scroll_handle = self
                        .reasoning_scroll_handles
                        .entry(reasoning.item_id.clone())
                        .or_insert_with(ScrollHandle::new);
                    if reasoning.is_active() && scroll_should_follow_output(scroll_handle) {
                        scroll_handle.scroll_to_bottom();
                    }
                }
                ConversationActivity::Command(command) => {
                    let scroll_handle = self
                        .command_scroll_handles
                        .entry(command.id.clone())
                        .or_insert_with(ScrollHandle::new);
                    if command.status == CommandExecutionStatus::InProgress
                        && scroll_should_follow_output(scroll_handle)
                    {
                        scroll_handle.scroll_to_bottom();
                    }
                }
                _ => {}
            }
        }
        if phase == ConversationPhase::Empty {
            self.conversation_scroll.set_offset(point(px(0.0), px(0.0)));
        } else if scroll_should_follow_output(&self.conversation_scroll) {
            self.conversation_scroll.scroll_to_bottom();
        }
        home(
            cx.entity(),
            theme,
            self.composer.clone(),
            user_input_other,
            transcript,
            phase,
            user_message,
            user_message_time,
            assistant_message,
            assistant_message_time,
            conversation_activity,
            self.conversation_scroll.clone(),
            self.thinking_shimmer_progress,
            self.response_feedback,
            self.user_message_actions_visible_for_capture,
            self.expanded_reasoning.clone(),
            reasoning_disclosure_progress,
            self.reasoning_scroll_handles.clone(),
            self.expanded_tool_groups.clone(),
            self.collapsed_active_tool_groups.clone(),
            tool_group_disclosure_progress,
            self.tool_group_scroll_handles.clone(),
            self.expanded_commands.clone(),
            self.command_scroll_handles.clone(),
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
        )
        .track_focus(&self.approval_focus)
        .on_key_down(cx.listener(Self::handle_approval_key))
    }
}

fn home(
    home_entity: Entity<HomeView>,
    theme: Theme,
    composer: Entity<ComposerView>,
    user_input_other: Entity<PromptInput>,
    transcript: Vec<ConversationTranscriptTurn>,
    phase: ConversationPhase,
    user_message: Option<String>,
    user_message_time: Option<String>,
    assistant_message: String,
    assistant_message_time: Option<String>,
    conversation_activity: Vec<ConversationActivity>,
    conversation_scroll: ScrollHandle,
    thinking_shimmer_progress: f32,
    response_feedback: i8,
    user_message_actions_visible_for_capture: bool,
    expanded_reasoning: HashSet<String>,
    reasoning_disclosure_progress: HashMap<String, f32>,
    reasoning_scroll_handles: HashMap<String, ScrollHandle>,
    expanded_tool_groups: HashSet<String>,
    collapsed_active_tool_groups: HashSet<String>,
    tool_group_disclosure_progress: HashMap<String, (f32, f32)>,
    tool_group_scroll_handles: HashMap<String, ScrollHandle>,
    expanded_commands: HashSet<String>,
    command_scroll_handles: HashMap<String, ScrollHandle>,
    first_suggestion: impl IntoElement,
    second_suggestion: impl IntoElement,
) -> Div {
    let pending_command_approval = conversation_activity.iter().find_map(|activity| {
        let ConversationActivity::Approval(model) = activity else {
            return None;
        };
        model.should_render().then(|| model.clone())
    });
    let pending_user_input = conversation_activity.iter().find_map(|activity| {
        let ConversationActivity::UserInput(model) = activity else {
            return None;
        };
        model.should_render().then(|| model.clone())
    });
    let pending_file_approval = conversation_activity.iter().find_map(|activity| {
        let ConversationActivity::FileApproval(model) = activity else {
            return None;
        };
        model.should_render().then(|| model.clone())
    });
    let pending_permissions_approval = conversation_activity.iter().find_map(|activity| {
        let ConversationActivity::PermissionsApproval(model) = activity else {
            return None;
        };
        model.should_render().then(|| model.clone())
    });
    let blocking_request_pending = pending_command_approval.is_some()
        || pending_user_input.is_some()
        || pending_file_approval.is_some()
        || pending_permissions_approval.is_some();

    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .relative()
        .when(phase == ConversationPhase::Empty, |root| {
            root.child(
                div()
                    .absolute()
                    // These are component boundaries, not a viewport-specific
                    // heading coordinate. GPUI centers the group in between them.
                    .top(px(46.0))
                    .bottom(px(153.0))
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(768.0))
                            .px(px(24.0))
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(12.0))
                            .child(
                                icon("home-mark", theme.home_mark.into())
                                    .size(px(56.0))
                                    .relative()
                                    .top(px(-2.0)),
                            )
                            .child(
                                div()
                                    .relative()
                                    .top(px(2.0))
                                    .left(px(-6.5))
                                    .text_size(px(29.2))
                                    .font_weight(gpui::FontWeight(350.0))
                                    .text_color(theme.text)
                                    .child("你想让我们在 coda 中构建什么？"),
                            ),
                    ),
            )
        })
        .when(phase != ConversationPhase::Empty, |root| {
            root.child(conversation(
                home_entity.clone(),
                theme,
                transcript,
                phase,
                user_message.unwrap_or_default(),
                user_message_time.unwrap_or_default(),
                assistant_message,
                assistant_message_time,
                conversation_activity,
                conversation_scroll,
                thinking_shimmer_progress,
                response_feedback,
                user_message_actions_visible_for_capture,
                expanded_reasoning,
                reasoning_disclosure_progress,
                reasoning_scroll_handles,
                expanded_tool_groups,
                collapsed_active_tool_groups,
                tool_group_disclosure_progress,
                tool_group_scroll_handles,
                expanded_commands,
                command_scroll_handles,
            ))
        })
        .when_some(pending_command_approval, |root, model| {
            let card = command_approval_card(home_entity.clone(), model, theme);
            root.when_some(card, |root, card| {
                root.child(
                    div()
                        .id("command-approval-overlay")
                        .debug_selector(|| "command-approval-overlay".to_owned())
                        .absolute()
                        .bottom(px(16.0))
                        .w_full()
                        .max_w(px(736.0))
                        .child(card),
                )
            })
        })
        .when_some(pending_user_input, |root, model| {
            let card = user_input_request_card(
                home_entity.clone(),
                model,
                theme,
                user_input_other.clone(),
            );
            root.when_some(card, |root, card| {
                root.child(
                    div()
                        .absolute()
                        .bottom(px(16.0))
                        .w_full()
                        .max_w(px(736.0))
                        .child(div().relative().left(px(2.671_875)).w_full().child(card)),
                )
            })
        })
        .when_some(pending_file_approval, |root, model| {
            let card = file_approval_card(home_entity.clone(), model, theme);
            root.when_some(card, |root, card| {
                root.child(
                    div()
                        .absolute()
                        .bottom(px(16.0))
                        .w_full()
                        .max_w(px(736.0))
                        .child(
                            // CDP 39-43 rasterize the left edge one pixel before
                            // GPUI at the same fractional CSS coordinate.
                            div().relative().left(px(1.671_875)).w_full().child(card),
                        ),
                )
            })
        })
        .when_some(pending_permissions_approval, |root, model| {
            let card = permissions_approval_card(home_entity.clone(), model, theme);
            root.when_some(card, |root, card| {
                root.child(
                    div()
                        .absolute()
                        .bottom(px(16.0))
                        .w_full()
                        .max_w(px(736.0))
                        .child(div().relative().left(px(0.671_875)).w_full().child(card)),
                )
            })
        })
        .child(
            div()
                .id("composer-overlay")
                .debug_selector(|| "composer-overlay".to_owned())
                .absolute()
                .bottom(px(15.0))
                .w_full()
                .max_w(px(748.0))
                // Match the reference composition at every window size: the
                // composer sits 6px inside its responsive container, while
                // its utility strip adds its own 14px inset.
                .px(px(6.0))
                .flex()
                .flex_col()
                .justify_end()
                .gap(px(8.0))
                .when(phase == ConversationPhase::Empty, |container| {
                    container.child(
                        div()
                            .min_h(px(80.0))
                            .px(px(19.0))
                            .flex()
                            .flex_col()
                            .justify_end()
                            .child(first_suggestion)
                            .child(second_suggestion),
                    )
                })
                .when(!blocking_request_pending, |container| {
                    container.child(composer)
                }),
        )
}

fn conversation(
    home_entity: Entity<HomeView>,
    theme: Theme,
    transcript: Vec<ConversationTranscriptTurn>,
    phase: ConversationPhase,
    user_message: String,
    user_message_time: String,
    assistant_message: String,
    assistant_message_time: Option<String>,
    conversation_activity: Vec<ConversationActivity>,
    conversation_scroll: ScrollHandle,
    thinking_shimmer_progress: f32,
    response_feedback: i8,
    user_message_actions_visible_for_capture: bool,
    expanded_reasoning: HashSet<String>,
    reasoning_disclosure_progress: HashMap<String, f32>,
    reasoning_scroll_handles: HashMap<String, ScrollHandle>,
    expanded_tool_groups: HashSet<String>,
    collapsed_active_tool_groups: HashSet<String>,
    tool_group_disclosure_progress: HashMap<String, (f32, f32)>,
    tool_group_scroll_handles: HashMap<String, ScrollHandle>,
    expanded_commands: HashSet<String>,
    command_scroll_handles: HashMap<String, ScrollHandle>,
) -> impl IntoElement {
    let has_active_reasoning = conversation_activity.iter().any(|activity| {
        matches!(activity, ConversationActivity::Reasoning(reasoning) if reasoning.is_active())
    });
    let show_thinking_tail = conversation_status(phase).is_some() && !has_active_reasoning;
    let user_message_hover_group: SharedString = "user-message-hover".into();
    let assistant_message_hover_group: SharedString = "assistant-message-hover".into();
    let copied_user_message = user_message.clone();
    let complete = matches!(
        phase,
        ConversationPhase::Complete | ConversationPhase::Failed
    );

    let mut historical_transcript = div().w_full().flex().flex_col().gap(px(24.0));
    for (index, turn) in transcript.into_iter().enumerate() {
        let user_message = turn.user_message;
        let answer = if turn.activities.is_empty() {
            render_assistant_markdown(
                &turn.assistant_message,
                theme,
                &format!("historical-assistant-{index}"),
            )
            .into_any_element()
        } else {
            activity_stream(
                home_entity.clone(),
                turn.activities,
                conversation_status(turn.phase).is_some(),
                thinking_shimmer_progress,
                expanded_reasoning.clone(),
                reasoning_disclosure_progress.clone(),
                reasoning_scroll_handles.clone(),
                expanded_tool_groups.clone(),
                collapsed_active_tool_groups.clone(),
                tool_group_disclosure_progress.clone(),
                tool_group_scroll_handles.clone(),
                expanded_commands.clone(),
                command_scroll_handles.clone(),
                theme,
            )
            .into_any_element()
        };
        historical_transcript = historical_transcript.child(
            div()
                .id(("transcript-turn", index))
                .w_full()
                .flex()
                .flex_col()
                .gap(px(16.0))
                .when(!user_message.is_empty(), |row| {
                    row.child(
                        div().w_full().flex().justify_end().child(
                            div()
                                .max_w(px(600.0))
                                .px(px(16.0))
                                .py(px(10.0))
                                .rounded(px(USER_MESSAGE_BUBBLE_RADIUS))
                                .bg(theme.text.alpha(0.05))
                                .text_size(px(14.0))
                                .line_height(px(22.0))
                                .text_color(theme.text)
                                .child(user_message),
                        ),
                    )
                })
                .child(answer),
        );
    }

    let conversation_body = div()
        .w_full()
        .max_w(px(736.0))
        .mx_auto()
        .pt(px(CONVERSATION_TOP_INSET))
        .pb(px(CONVERSATION_BOTTOM_INSET))
        .flex()
        .flex_col()
        .child(historical_transcript)
        .child(
            div()
                .h(px(72.0))
                .w_full()
                .flex()
                .flex_col()
                .items_end()
                .child(
                    div()
                        .group(user_message_hover_group.clone())
                        .flex()
                        .flex_col()
                        .items_end()
                        .child(
                            div()
                                .max_w(px(600.0))
                                .px(px(16.0))
                                .py(px(10.0))
                                .relative()
                                .text_size(px(14.0))
                                .line_height(px(22.0))
                                .text_color(theme.text)
                                // CDP reports border-radius:22px plus
                                // corner-shape:superellipse(1.5), which cannot be
                                // represented by GPUI's circular rounded corners.
                                .child(
                                    canvas(
                                        |bounds, _, _| user_message_bubble_path(bounds),
                                        move |_, path, window, _| {
                                            window.paint_path(path, theme.text.alpha(0.05));
                                        },
                                    )
                                    .absolute()
                                    .inset_0(),
                                )
                                .child(div().relative().child(user_message)),
                        )
                        .child(
                            div()
                                .mt(px(USER_MESSAGE_FOOTER_OFFSET))
                                .mx(px(USER_MESSAGE_FOOTER_SIDE_MARGIN))
                                .h(px(USER_MESSAGE_FOOTER_HEIGHT))
                                .flex()
                                .items_center()
                                .gap(px(USER_MESSAGE_FOOTER_GAP))
                                .child(
                                    div()
                                        .text_size(px(USER_MESSAGE_TIME_SIZE))
                                        .line_height(px(USER_MESSAGE_TIME_LINE_HEIGHT))
                                        .text_color(theme.text_tertiary)
                                        .opacity(if user_message_actions_visible_for_capture {
                                            1.0
                                        } else {
                                            0.0
                                        })
                                        .group_hover(user_message_hover_group.clone(), |time| {
                                            time.opacity(1.0)
                                        })
                                        .child(user_message_time),
                                )
                                .child(
                                    div()
                                        .id("user-message-copy")
                                        .size(px(26.0))
                                        .rounded(px(10.0))
                                        .opacity(if user_message_actions_visible_for_capture {
                                            1.0
                                        } else {
                                            0.0
                                        })
                                        .group_hover(user_message_hover_group, |button| {
                                            button.opacity(1.0)
                                        })
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .hover(move |button| button.bg(theme.sidebar_hover))
                                        .active(move |button| button.bg(theme.text.alpha(0.12)))
                                        .on_click(move |_, _, cx| {
                                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                                copied_user_message.clone(),
                                            ));
                                        })
                                        .child(
                                            icon("message-copy", theme.text_tertiary.into())
                                                .size(px(RESPONSE_ACTION_ICON_SIZE)),
                                        ),
                                ),
                        ),
                ),
        )
        .child(
            div()
                .group(assistant_message_hover_group.clone())
                .mt(px(16.0))
                .w_full()
                .min_h(px(48.0))
                // GPUI group hover is based on the group's own hitbox. The
                // reference's 26px buttons overflow a 20px footer by 3px, and
                // CSS :hover still includes those descendants. This invisible
                // trailing hit area preserves that behavior for every icon px.
                .pb(px(3.0))
                .text_size(px(14.0))
                .line_height(px(22.0))
                .text_color(theme.text)
                .when(
                    !assistant_message.is_empty()
                        || !conversation_activity.is_empty()
                        || show_thinking_tail,
                    |answer| {
                        if conversation_activity.is_empty() {
                            answer.child(
                                div()
                                    .w_full()
                                    .flex()
                                    .flex_col()
                                    .gap(px(16.0))
                                    .when(!assistant_message.is_empty(), |stream| {
                                        stream.child(render_assistant_markdown(
                                            &assistant_message,
                                            theme,
                                            "current-assistant",
                                        ))
                                    })
                                    .when(show_thinking_tail, |stream| {
                                        stream.child(thinking_shimmer(
                                            theme,
                                            thinking_shimmer_progress,
                                        ))
                                    }),
                            )
                        } else {
                            answer.child(activity_stream(
                                home_entity.clone(),
                                conversation_activity,
                                show_thinking_tail,
                                thinking_shimmer_progress,
                                expanded_reasoning,
                                reasoning_disclosure_progress,
                                reasoning_scroll_handles,
                                expanded_tool_groups,
                                collapsed_active_tool_groups,
                                tool_group_disclosure_progress,
                                tool_group_scroll_handles,
                                expanded_commands,
                                command_scroll_handles,
                                theme,
                            ))
                        }
                    },
                )
                .when(complete, |answer| {
                    answer.child(
                        div()
                            .relative()
                            .left(px(RESPONSE_ACTION_FOOTER_ELECTRON_SHIFT))
                            .mt(px(RESPONSE_ACTION_FOOTER_OFFSET))
                            .w_full()
                            .h(px(RESPONSE_ACTION_FOOTER_HEIGHT))
                            .flex()
                            .items_center()
                            .gap(px(RESPONSE_ACTION_GAP))
                            .child(
                                div()
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .gap(px(RESPONSE_ACTION_GAP))
                                    .child(message_action(
                                        "message-copy",
                                        "response-copy",
                                        0,
                                        false,
                                        assistant_message.clone(),
                                        home_entity.clone(),
                                        theme,
                                    ))
                                    .child(message_action(
                                        "message-thumb-up",
                                        "response-thumb-up",
                                        1,
                                        response_feedback == 1,
                                        assistant_message.clone(),
                                        home_entity.clone(),
                                        theme,
                                    ))
                                    .child(message_action(
                                        "message-thumb-down",
                                        "response-thumb-down",
                                        2,
                                        response_feedback == -1,
                                        assistant_message.clone(),
                                        home_entity.clone(),
                                        theme,
                                    ))
                                    .child(message_action(
                                        "message-branch",
                                        "response-branch",
                                        3,
                                        false,
                                        assistant_message,
                                        home_entity,
                                        theme,
                                    )),
                            )
                            .when_some(assistant_message_time, |footer, completed_at| {
                                footer.child(
                                    div()
                                        .ml(px(RESPONSE_TIME_MARGIN))
                                        .h_full()
                                        .flex()
                                        .items_center()
                                        .opacity(0.0)
                                        .group_hover(assistant_message_hover_group, |time| {
                                            time.opacity(1.0)
                                        })
                                        .child(
                                            div()
                                                .text_size(px(RESPONSE_TIME_SIZE))
                                                .line_height(px(RESPONSE_TIME_LINE_HEIGHT))
                                                .font_weight(gpui::FontWeight::NORMAL)
                                                .text_color(theme.text_tertiary)
                                                .child(completed_at),
                                        ),
                                )
                            }),
                    )
                }),
        );

    div()
        .id("conversation-scroll")
        .debug_selector(|| "conversation-scroll".to_owned())
        .absolute()
        .inset_0()
        .overflow_y_scroll()
        .restrict_scroll_to_axis()
        .scrollbar_width(px(0.0))
        .track_scroll(&conversation_scroll)
        .child(conversation_body)
}

fn scroll_should_follow_output(scroll_handle: &ScrollHandle) -> bool {
    let max_offset = f32::from(scroll_handle.max_offset().y).max(0.0);
    let offset = f32::from(scroll_handle.offset().y);
    max_offset <= CONVERSATION_BOTTOM_EPSILON || max_offset + offset <= CONVERSATION_BOTTOM_EPSILON
}

fn nested_scroll_consumed(
    scroll_handle: &ScrollHandle,
    event: &ScrollWheelEvent,
    window: &Window,
) -> bool {
    let delta_y = match event.delta {
        ScrollDelta::Pixels(delta) => f32::from(delta.y),
        ScrollDelta::Lines(delta) => f32::from(window.line_height()) * delta.y,
    };
    if delta_y.abs() <= f32::EPSILON {
        return false;
    }

    let max_offset = f32::from(scroll_handle.max_offset().y).max(0.0);
    if max_offset <= CONVERSATION_BOTTOM_EPSILON {
        return false;
    }

    // GPUI registers the built-in scroller after custom wheel listeners, so
    // bubble dispatch runs the built-in listener first. Reconstruct the
    // pre-gesture position to decide whether this nested viewport owned the
    // gesture; at an edge the event keeps bubbling to the conversation.
    let previous_offset = (f32::from(scroll_handle.offset().y) - delta_y).clamp(-max_offset, 0.0);
    if delta_y < 0.0 {
        previous_offset > -max_offset + CONVERSATION_BOTTOM_EPSILON
    } else {
        previous_offset < -CONVERSATION_BOTTOM_EPSILON
    }
}

fn activity_stream(
    home_entity: Entity<HomeView>,
    activities: Vec<ConversationActivity>,
    show_thinking_tail: bool,
    thinking_shimmer_progress: f32,
    expanded_reasoning: HashSet<String>,
    reasoning_disclosure_progress: HashMap<String, f32>,
    reasoning_scroll_handles: HashMap<String, ScrollHandle>,
    expanded_tool_groups: HashSet<String>,
    collapsed_active_tool_groups: HashSet<String>,
    tool_group_disclosure_progress: HashMap<String, (f32, f32)>,
    tool_group_scroll_handles: HashMap<String, ScrollHandle>,
    expanded_commands: HashSet<String>,
    command_scroll_handles: HashMap<String, ScrollHandle>,
    theme: Theme,
) -> Div {
    activity_stream_units(&activities)
        .into_iter()
        .enumerate()
        .fold(
            div().w_full().flex().flex_col().gap(px(16.0)),
            |stream, (index, unit)| match unit {
                ActivityStreamUnit::ToolGroup(group) => {
                    let active = group.is_active();
                    let expanded = if active {
                        !collapsed_active_tool_groups.contains(&group.id)
                    } else {
                        expanded_tool_groups.contains(&group.id)
                    };
                    let settled_progress = if expanded { 1.0 } else { 0.0 };
                    let (disclosure_progress, chevron_progress) = tool_group_disclosure_progress
                        .get(&group.id)
                        .copied()
                        .unwrap_or((settled_progress, settled_progress));
                    let scroll_handle = tool_group_scroll_handles
                        .get(&group.id)
                        .cloned()
                        .unwrap_or_else(ScrollHandle::new);
                    stream.child(tool_activity_group(
                        home_entity.clone(),
                        group,
                        expanded,
                        disclosure_progress,
                        chevron_progress,
                        scroll_handle,
                        &expanded_commands,
                        &command_scroll_handles,
                        theme,
                    ))
                }
                ActivityStreamUnit::Standalone(activity) => match activity {
                    ConversationActivity::AssistantMessage { item_id, text }
                        if !text.is_empty() =>
                    {
                        stream.child(render_assistant_markdown(&text, theme, &item_id))
                    }
                    ConversationActivity::Reasoning(reasoning) => {
                        let expanded = reasoning.is_active()
                            || expanded_reasoning.contains(&reasoning.item_id);
                        let disclosure_progress = reasoning_disclosure_progress
                            .get(&reasoning.item_id)
                            .copied()
                            .unwrap_or(if expanded { 1.0 } else { 0.0 });
                        let scroll_handle = reasoning_scroll_handles
                            .get(&reasoning.item_id)
                            .cloned()
                            .unwrap_or_else(ScrollHandle::new);
                        stream.child(reasoning_activity(
                            home_entity.clone(),
                            reasoning,
                            expanded,
                            disclosure_progress,
                            scroll_handle,
                            thinking_shimmer_progress,
                            theme,
                        ))
                    }
                    ConversationActivity::ImageView(image) => {
                        let expanded = if show_thinking_tail {
                            !collapsed_active_tool_groups.contains(&image.id)
                        } else {
                            expanded_tool_groups.contains(&image.id)
                        };
                        stream.child(image_view_activity(
                            home_entity.clone(),
                            image,
                            expanded,
                            show_thinking_tail,
                            theme,
                        ))
                    }
                    ConversationActivity::Command(command) => {
                        stream.child(command_execution_activity(
                            home_entity.clone(),
                            command,
                            &expanded_commands,
                            &command_scroll_handles,
                            theme,
                        ))
                    }
                    ConversationActivity::Approval(_) => stream,
                    ConversationActivity::FileApproval(_) => stream,
                    ConversationActivity::PermissionsApproval(_) => stream,
                    ConversationActivity::FileChange(model) => {
                        let target = home_entity.clone();
                        let callback = FileChangeActivityCallback::new(move |event, _, cx| {
                            target.update(cx, move |home, cx| {
                                home.handle_file_change_activity_event(event, cx)
                            });
                        });
                        let expanded = expanded_commands.contains(&model.item_id);
                        stream.child(render_file_change_activity(
                            &model, expanded, theme, callback,
                        ))
                    }
                    ConversationActivity::UserInput(_) => stream,
                    ConversationActivity::ProtocolError {
                        message,
                        details,
                        will_retry: true,
                    } => stream.child(retrying_error_activity(index, message, details, theme)),
                    ConversationActivity::ProtocolError {
                        message,
                        details,
                        will_retry: false,
                    } => stream.child(notice_activity(
                        message,
                        details,
                        None,
                        "Codex 错误",
                        NOTICE_ERROR_GAP,
                        NOTICE_ERROR_CONTENT_GAP,
                        index,
                        theme,
                    )),
                    ConversationActivity::Warning { message } => stream.child(notice_activity(
                        message,
                        None,
                        None,
                        "Codex 警告",
                        NOTICE_WARNING_GAP,
                        NOTICE_WARNING_CONTENT_GAP,
                        index,
                        theme,
                    )),
                    ConversationActivity::ConfigWarning(warning) => {
                        let file = warning.path.map(|path| ConfigWarningFile {
                            path,
                            line: warning.line,
                            column: warning.column,
                        });
                        stream.child(notice_activity(
                            warning.summary,
                            warning.details,
                            file,
                            "Codex 配置警告",
                            NOTICE_WARNING_GAP,
                            NOTICE_WARNING_CONTENT_GAP,
                            index,
                            theme,
                        ))
                    }
                    ConversationActivity::Error { message } => stream.child(notice_activity(
                        message,
                        None,
                        None,
                        "Codex turn 失败",
                        NOTICE_ERROR_GAP,
                        NOTICE_ERROR_CONTENT_GAP,
                        index,
                        theme,
                    )),
                    _ => stream,
                },
            },
        )
        // Keep the generic waiting state in the same 16px activity stream so
        // it always follows the latest rendered JSON-RPC item.
        .when(show_thinking_tail, |stream| {
            stream.child(thinking_shimmer(theme, thinking_shimmer_progress))
        })
}

fn image_view_activity(
    home_entity: Entity<HomeView>,
    image: AgentImageView,
    expanded: bool,
    active_turn: bool,
    theme: Theme,
) -> impl IntoElement {
    let item_id = image.id.clone();
    let path = image.path.clone();
    let click_home = home_entity.clone();
    let preview_home = home_entity.clone();
    let key_home = home_entity.clone();
    let preview_key_home = home_entity.clone();
    let click_item_id = item_id.clone();
    let key_item_id = item_id.clone();
    let hover_group: SharedString = format!("image-view-header-{item_id}").into();
    let label = if expanded {
        "已查看 1 张图像，折叠图像"
    } else {
        "已查看 1 张图像，展开图像"
    };
    let thumbnail_path = path.clone();
    let thumbnail_key_path = path.clone();

    div()
        .id(SharedString::from(format!("image-view-{item_id}")))
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .items_start()
        .child(
            div()
                .id(SharedString::from(format!("image-view-header-{item_id}")))
                .group(hover_group.clone())
                .h(px(21.0))
                .max_w_full()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(4.0))
                .rounded(px(6.0))
                .focusable()
                .tab_stop(true)
                .role(Role::Button)
                .aria_expanded(expanded)
                .aria_label(label)
                .cursor_pointer()
                .focus_visible(|style| {
                    style.px(px(2.0)).shadow(vec![
                        BoxShadow::new(px(0.0), px(0.0), rgba(0x3a83f7ff).into())
                            .spread_radius(px(2.0))
                            .inset(),
                    ])
                })
                .on_click(move |_, _, cx| {
                    toggle_tool_activity_group(
                        &click_home,
                        &click_item_id,
                        active_turn,
                        &ScrollHandle::new(),
                        cx,
                    );
                })
                .on_key_down(move |event, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        toggle_tool_activity_group(
                            &key_home,
                            &key_item_id,
                            active_turn,
                            &ScrollHandle::new(),
                            cx,
                        );
                        cx.stop_propagation();
                    }
                })
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_color(theme.text.alpha(0.60))
                        .child(
                            icon("activity-image", theme.text.alpha(0.60).into())
                                .size(px(21.0))
                                .flex_none(),
                        )
                        .child(
                            div()
                                .min_w(px(0.0))
                                .truncate()
                                .text_size(px(14.0))
                                .line_height(px(21.0))
                                .font_family(".SystemUIFont")
                                .font_weight(FontWeight::NORMAL)
                                .text_color(theme.text.alpha(0.40))
                                .child("已查看 1 张图像"),
                        ),
                )
                .child(
                    icon("settings-chevron-right", theme.text.alpha(0.60).into())
                        .size(px(20.0))
                        .flex_none()
                        .opacity(if expanded { 1.0 } else { 0.0 })
                        .group_hover(hover_group, |chevron| chevron.opacity(1.0))
                        .with_transformation(Transformation::rotate(radians(if expanded {
                            std::f32::consts::FRAC_PI_2
                        } else {
                            0.0
                        }))),
                ),
        )
        .when(expanded, |activity| {
            activity.child(
                div().pt(px(8.0)).pb(px(4.0)).flex().gap(px(8.0)).child(
                    div()
                        .id(SharedString::from(format!(
                            "image-view-thumbnail-{item_id}"
                        )))
                        .size(px(80.0))
                        .flex_none()
                        .rounded(px(8.0))
                        .border(px(1.0))
                        .border_color(theme.text.alpha(0.20))
                        .overflow_hidden()
                        .role(Role::Button)
                        .aria_label("已检查的图像")
                        .focusable()
                        .tab_stop(true)
                        .cursor_pointer()
                        .focus_visible(|style| {
                            style.shadow(vec![
                                BoxShadow::new(px(0.0), px(0.0), rgba(0x3a83f7ff).into())
                                    .spread_radius(px(2.0)),
                            ])
                        })
                        .on_click(move |_, _, cx| {
                            preview_home.update(cx, |_, cx| {
                                cx.emit(OpenImagePreview(thumbnail_path.clone()));
                            });
                            cx.stop_propagation();
                        })
                        .on_key_down(move |event, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                preview_key_home.update(cx, |_, cx| {
                                    cx.emit(OpenImagePreview(thumbnail_key_path.clone()));
                                });
                                cx.stop_propagation();
                            }
                        })
                        .child(
                            gpui::img(path)
                                .size_full()
                                .rounded(px(6.0))
                                .object_fit(ObjectFit::Cover),
                        ),
                ),
            )
        })
}

fn format_reasoning_elapsed(elapsed_ms: u64) -> String {
    let total_seconds = elapsed_ms.div_ceil(1_000).max(1);
    let hours = total_seconds / 3_600;
    let minutes = total_seconds % 3_600 / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        if minutes > 0 {
            format!("{hours}h {minutes}m")
        } else {
            format!("{hours}h")
        }
    } else if minutes > 0 {
        if seconds > 0 {
            format!("{minutes}m {seconds}s")
        } else {
            format!("{minutes}m")
        }
    } else {
        format!("{seconds}s")
    }
}

fn reasoning_header_label(reasoning: &ReasoningActivityPresentation) -> String {
    if reasoning.is_active() {
        "正在思考".to_owned()
    } else if let Some(elapsed_ms) = reasoning.elapsed_ms() {
        format!("思考了 {}", format_reasoning_elapsed(elapsed_ms))
    } else {
        "完成思考".to_owned()
    }
}

fn active_reasoning_body(text: &str) -> String {
    let trimmed = text.trim_start();
    let Some(after_opening) = trimmed.strip_prefix("**") else {
        return trimmed.to_owned();
    };
    let first_line = after_opening
        .split_once('\n')
        .map_or(after_opening, |(line, _)| line);
    let Some(closing) = first_line.find("**") else {
        return String::new();
    };
    after_opening[closing + 2..].trim_start().to_owned()
}

fn reasoning_body_text(reasoning: &ReasoningActivityPresentation) -> String {
    let display_text = reasoning.display_text();
    if reasoning.is_active() {
        active_reasoning_body(&display_text)
    } else {
        display_text
    }
}

fn completed_reasoning_body(text: &str) -> (Option<String>, String) {
    let trimmed = text.trim_start();
    let Some(after_opening) = trimmed.strip_prefix("**") else {
        return (None, trimmed.to_owned());
    };
    let first_line = after_opening
        .split_once('\n')
        .map_or(after_opening, |(line, _)| line);
    let Some(closing) = first_line.find("**") else {
        return (None, trimmed.to_owned());
    };
    let title = after_opening[..closing].trim().to_owned();
    let body = after_opening[closing + 2..].trim_start().to_owned();
    ((!title.is_empty()).then_some(title), body)
}

fn completed_reasoning_body_element(text: String) -> Div {
    let (title, body) = completed_reasoning_body(&text);
    let has_title = title.is_some();
    div()
        .w_full()
        .flex()
        .flex_col()
        .when_some(title, |content, title| {
            content.child(div().font_weight(FontWeight::SEMIBOLD).child(title))
        })
        .when(!body.is_empty(), |content| {
            content.child(
                div()
                    .when(has_title, |body| body.mt(px(REASONING_BODY_TOP_GAP)))
                    .font_weight(FontWeight::NORMAL)
                    .child(body),
            )
        })
}

fn reasoning_transition_ease(progress: f32) -> f32 {
    cubic_bezier_ease(progress, 0.19, 1.0, 0.22, 1.0)
}

fn tool_group_chevron_transition_ease(progress: f32) -> f32 {
    cubic_bezier_ease(progress, 0.4, 0.0, 0.2, 1.0)
}

fn cubic_bezier_ease(progress: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    if progress == 0.0 || progress == 1.0 {
        return progress;
    }

    // Invert x for the reference cubic Bézier, then evaluate y.
    let mut lower = 0.0;
    let mut upper = 1.0;
    for _ in 0..10 {
        let parameter = (lower + upper) * 0.5;
        let inverse = 1.0 - parameter;
        let x = 3.0 * inverse * inverse * parameter * x1
            + 3.0 * inverse * parameter * parameter * x2
            + parameter * parameter * parameter;
        if x < progress {
            lower = parameter;
        } else {
            upper = parameter;
        }
    }
    let parameter = (lower + upper) * 0.5;
    let inverse = 1.0 - parameter;
    3.0 * inverse * inverse * parameter * y1
        + 3.0 * inverse * parameter * parameter * y2
        + parameter * parameter * parameter
}

fn toggle_reasoning_item(
    home_entity: &Entity<HomeView>,
    item_id: &str,
    scroll_handle: &ScrollHandle,
    cx: &mut App,
) {
    let item_id = item_id.to_owned();
    home_entity.update(cx, |home, cx| {
        if !home.expanded_reasoning.remove(&item_id) {
            home.expanded_reasoning.insert(item_id);
            scroll_handle.scroll_to_bottom();
        }
        cx.notify();
    });
}

fn reasoning_activity(
    home_entity: Entity<HomeView>,
    reasoning: ReasoningActivityPresentation,
    expanded: bool,
    disclosure_progress: f32,
    scroll_handle: ScrollHandle,
    thinking_shimmer_progress: f32,
    theme: Theme,
) -> Div {
    let item_id = reasoning.item_id.clone();
    let hover_group: SharedString = format!("reasoning-activity-{item_id}").into();
    let active = reasoning.is_active();
    let body_text = reasoning_body_text(&reasoning);
    let has_content = !body_text.trim().is_empty();
    let can_toggle = !active && has_content;
    let header_label = reasoning_header_label(&reasoning);
    let accessible_label = if expanded {
        format!("{header_label}，折叠推理内容")
    } else {
        format!("{header_label}，展开推理内容")
    };
    let click_home_entity = home_entity.clone();
    let click_item_id = item_id.clone();
    let click_scroll_handle = scroll_handle.clone();
    let key_item_id = item_id.clone();
    let key_scroll_handle = scroll_handle.clone();
    let nested_scroll_handle = scroll_handle.clone();

    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .items_start()
        .child(
            div()
                .id(SharedString::from(format!("reasoning-activity-{item_id}")))
                .group(hover_group.clone())
                .h(px(REASONING_HEADER_HEIGHT))
                .max_w_full()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(4.0))
                .rounded(px(6.0))
                .when(can_toggle, |header| {
                    header
                        .focusable()
                        .tab_stop(true)
                        .role(Role::Button)
                        .aria_expanded(expanded)
                        .aria_label(accessible_label)
                        .focus_visible(|style| {
                            style.px(px(DISCLOSURE_FOCUS_PADDING)).shadow(vec![
                                BoxShadow::new(px(0.0), px(0.0), rgba(0x3a83f7ff).into())
                                    .spread_radius(px(2.0))
                                    .inset(),
                            ])
                        })
                        .cursor_pointer()
                        .on_click(move |_, _, cx| {
                            toggle_reasoning_item(
                                &click_home_entity,
                                &click_item_id,
                                &click_scroll_handle,
                                cx,
                            );
                        })
                        .on_key_down(move |event, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                toggle_reasoning_item(
                                    &home_entity,
                                    &key_item_id,
                                    &key_scroll_handle,
                                    cx,
                                );
                                cx.stop_propagation();
                            }
                        })
                })
                .child(if active {
                    div()
                        .child(thinking_shimmer(theme, thinking_shimmer_progress))
                        .into_any_element()
                } else {
                    div()
                        .min_w(px(0.0))
                        .max_w(px(718.0))
                        .truncate()
                        .text_size(px(REASONING_TEXT_SIZE))
                        .line_height(px(REASONING_LINE_HEIGHT))
                        .font_family(".SystemUIFont")
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.text.alpha(0.30))
                        .group_hover(hover_group.clone(), move |label| {
                            label.text_color(theme.text)
                        })
                        .child(header_label)
                        .into_any_element()
                })
                .when(can_toggle, |header| {
                    header.child(
                        icon("settings-chevron-right", theme.text.alpha(0.60).into())
                            .size(px(REASONING_CHEVRON_SIZE))
                            .flex_none()
                            .opacity(disclosure_progress.clamp(0.0, 1.0))
                            .group_hover(hover_group, |chevron| chevron.opacity(1.0))
                            .with_transformation(Transformation::rotate(radians(
                                std::f32::consts::FRAC_PI_2 * disclosure_progress.clamp(0.0, 1.0),
                            ))),
                    )
                }),
        )
        .when(has_content, |activity| {
            let visibility = disclosure_progress.clamp(0.0, 1.0);
            activity.child(
                div()
                    .w_full()
                    .overflow_hidden()
                    .max_h(px(
                        (REASONING_BODY_MAX_HEIGHT + REASONING_BODY_TOP_GAP) * visibility
                    ))
                    .opacity(visibility)
                    .when(visibility <= f32::EPSILON, |body| body.invisible())
                    .child(
                        div().w_full().pt(px(REASONING_BODY_TOP_GAP)).child(
                            div()
                                .id(SharedString::from(format!("reasoning-body-{item_id}")))
                                .w_full()
                                .max_h(px(REASONING_BODY_MAX_HEIGHT))
                                .overflow_scroll()
                                .restrict_scroll_to_axis()
                                .scrollbar_width(px(0.0))
                                .track_scroll(&scroll_handle)
                                .on_scroll_wheel(move |event, window, cx| {
                                    if nested_scroll_consumed(&nested_scroll_handle, event, window)
                                    {
                                        cx.stop_propagation();
                                    }
                                })
                                .text_size(px(REASONING_TEXT_SIZE))
                                .line_height(px(REASONING_LINE_HEIGHT))
                                .font_family(".SystemUIFont")
                                .font_weight(FontWeight::NORMAL)
                                .text_color(theme.text.alpha(0.50))
                                .child(if active {
                                    div().child(body_text)
                                } else {
                                    completed_reasoning_body_element(body_text)
                                }),
                        ),
                    ),
            )
        })
}

fn command_approval_card(
    home_entity: Entity<HomeView>,
    model: crate::components::approval::ApprovalCardViewModel,
    theme: Theme,
) -> Option<gpui::Stateful<Div>> {
    let request_id = model.request_id.clone();
    let target = home_entity;
    let callback = ApprovalCardCallback::new(move |event, _, cx| {
        let request_id = request_id.clone();
        target.update(cx, move |home, cx| {
            home.handle_approval_card_event(&request_id, event, cx)
        });
    });
    render_approval_card(&model, theme, callback)
}

fn file_approval_card(
    home_entity: Entity<HomeView>,
    model: FileApprovalPresentation,
    theme: Theme,
) -> Option<gpui::Stateful<Div>> {
    let request_id = model.request_id.clone();
    let target = home_entity;
    let callback = FileApprovalCallback::new(move |event, _, cx| {
        let request_id = request_id.clone();
        target.update(cx, move |home, cx| {
            home.handle_file_approval_event(&request_id, event, cx)
        });
    });
    render_file_approval_card(&model, theme, callback)
}

fn permissions_approval_card(
    home_entity: Entity<HomeView>,
    model: PermissionApprovalPresentation,
    theme: Theme,
) -> Option<gpui::Stateful<Div>> {
    let request_id = model.request_id.clone();
    let target = home_entity;
    let callback = PermissionApprovalCallback::new(move |event, _, cx| {
        let request_id = request_id.clone();
        target.update(cx, move |home, cx| {
            home.handle_permissions_approval_event(&request_id, event, cx)
        });
    });
    render_permissions_approval(&model, theme, callback)
}

fn user_input_request_card(
    home_entity: Entity<HomeView>,
    model: crate::components::user_input_request::UserInputRequestPresentation,
    theme: Theme,
    other_input: Entity<PromptInput>,
) -> Option<gpui::Stateful<Div>> {
    let request_id = model.request_id.clone();
    let target = home_entity;
    let callback = UserInputRequestCallback::new(move |event, window, cx| {
        let request_id = request_id.clone();
        if matches!(event, UserInputRequestEvent::BeginOtherAnswer { .. }) {
            let focus = target
                .read(cx)
                .composer
                .read(cx)
                .user_input_other_focus_handle(cx);
            window.focus(&focus, cx);
        }
        target.update(cx, move |home, cx| {
            home.handle_user_input_request_event(&request_id, event, cx)
        });
    });
    render_user_input_request(&model, theme, other_input, callback)
}

#[derive(Clone)]
struct ConfigWarningFile {
    path: String,
    line: Option<u64>,
    column: Option<u64>,
}

fn retrying_error_activity(
    index: usize,
    message: String,
    details: Option<String>,
    theme: Theme,
) -> impl IntoElement {
    let mut accessible_label = format!("Codex 错误，正在重试：{message}");
    if let Some(details) = details
        .as_deref()
        .filter(|details| !details.trim().is_empty())
    {
        accessible_label.push_str(&format!("；{details}"));
    }
    div()
        .id(("conversation-retrying-error", index))
        .role(Role::Alert)
        .aria_label(accessible_label)
        .w_full()
        .flex()
        .items_start()
        .gap(px(6.0))
        .text_size(px(14.0))
        .line_height(px(21.0))
        .text_color(theme.text_tertiary)
        .child(
            icon("settings-hooks-refresh", theme.text_tertiary.into())
                .size(px(16.0))
                .mt(px(2.0))
                .flex_none(),
        )
        .child(div().min_w(px(0.0)).flex_1().child(message).when_some(
            details.filter(|details| !details.trim().is_empty()),
            |text, details| {
                text.child(
                    div()
                        .text_size(px(NOTICE_TEXT_SIZE))
                        .line_height(px(NOTICE_LINE_HEIGHT))
                        .text_color(theme.text_tertiary)
                        .child(details),
                )
            },
        ))
}

fn notice_activity(
    summary: String,
    details: Option<String>,
    file: Option<ConfigWarningFile>,
    accessible_kind: &'static str,
    outer_gap: f32,
    content_gap: f32,
    index: usize,
    theme: Theme,
) -> impl IntoElement {
    let file_label = file.as_ref().map(|file| {
        let mut label = format!("文件：{}", file.path);
        match (file.line, file.column) {
            (Some(line), Some(column)) => {
                label.push_str(&format!("（第 {line} 行，第 {column} 列）"));
            }
            (Some(line), None) => label.push_str(&format!("（第 {line} 行）")),
            _ => {}
        }
        label
    });
    let mut accessible_label = format!("{accessible_kind}：{summary}");
    if let Some(details) = details
        .as_deref()
        .filter(|details| !details.trim().is_empty())
    {
        accessible_label.push_str(&format!("；{details}"));
    }
    if let Some(file_label) = &file_label {
        accessible_label.push_str(&format!("；{file_label}"));
    }
    let content = div()
        .min_w(px(0.0))
        .flex_1()
        .flex()
        .flex_col()
        .gap(px(content_gap))
        .text_size(px(NOTICE_TEXT_SIZE))
        .line_height(px(NOTICE_LINE_HEIGHT))
        .text_color(theme.text)
        .child(summary)
        .when_some(
            details.filter(|details| !details.trim().is_empty()),
            |content, details| content.child(div().text_color(theme.text_secondary).child(details)),
        )
        .when_some(file_label, |content, file_label| {
            content.child(div().text_color(theme.text_secondary).child(file_label))
        });

    div()
        .id(("conversation-notice", index))
        .role(Role::Alert)
        .aria_label(accessible_label)
        .w_full()
        .py(px(8.0))
        .pl(px(12.0))
        .pr(px(8.0))
        .flex()
        .items_center()
        .gap(px(outer_gap))
        .rounded(px(NOTICE_RADIUS))
        .bg(theme.surface)
        .shadow(vec![
            BoxShadow::new(px(0.0), px(0.0), theme.command_border.into()).spread_radius(px(0.5)),
            BoxShadow::new(px(0.0), px(1.0), rgba(0x0000000d).into()).blur_radius(px(2.0)),
        ])
        .child(
            icon("settings-warning", theme.warning.into())
                .size(px(NOTICE_ICON_SIZE))
                .flex_none(),
        )
        .child(content)
        .when_some(file, |notice, file| {
            let path = PathBuf::from(&file.path);
            notice.child(
                div()
                    .id(("config-warning-open", index))
                    .role(Role::Button)
                    .aria_label(format!("打开配置文件 {}", file.path))
                    .h(px(NOTICE_BUTTON_HEIGHT))
                    .px(px(8.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(9999.0))
                    .border(px(1.0))
                    .border_color(theme.border)
                    .bg(theme.command_surface)
                    .text_size(px(NOTICE_TEXT_SIZE))
                    .line_height(px(18.0))
                    .text_color(theme.text)
                    .cursor_pointer()
                    .hover(|button| button.bg(theme.sidebar_hover))
                    .on_click(move |_, _, cx| cx.open_with_system(&path))
                    .child("打开文件"),
            )
        })
}

fn toggle_tool_activity_group(
    home_entity: &Entity<HomeView>,
    group_id: &str,
    active: bool,
    scroll_handle: &ScrollHandle,
    cx: &mut App,
) {
    let group_id = group_id.to_owned();
    home_entity.update(cx, |home, cx| {
        let expanded = if active {
            if home.collapsed_active_tool_groups.remove(&group_id) {
                true
            } else {
                home.collapsed_active_tool_groups.insert(group_id.clone());
                false
            }
        } else if home.expanded_tool_groups.remove(&group_id) {
            false
        } else {
            home.expanded_tool_groups.insert(group_id);
            true
        };
        if expanded {
            scroll_handle.scroll_to_bottom();
        }
        cx.notify();
    });
}

fn tool_activity_group(
    home_entity: Entity<HomeView>,
    group: ToolActivityGroupPresentation,
    expanded: bool,
    disclosure_progress: f32,
    chevron_progress: f32,
    scroll_handle: ScrollHandle,
    expanded_commands: &HashSet<String>,
    command_scroll_handles: &HashMap<String, ScrollHandle>,
    theme: Theme,
) -> Div {
    let group_id = group.id.clone();
    let active = group.is_active();
    let reasoning_title = active.then(|| tool_group_reasoning_title(&group)).flatten();
    let summary = if let Some(title) = reasoning_title {
        CommandActivitySummary {
            icon: "",
            text: title,
            reads_files: false,
            runs_command: false,
        }
    } else if active {
        if group
            .file_changes
            .iter()
            .any(|change| change.status == AgentFileChangeStatus::InProgress)
        {
            CommandActivitySummary {
                icon: "message-edit",
                text: "正在编辑文件".to_owned(),
                reads_files: false,
                runs_command: false,
            }
        } else {
            group
                .commands
                .iter()
                .rev()
                .find(|command| command.status == CommandExecutionStatus::InProgress)
                .or_else(|| group.commands.last())
                .and_then(|command| command_activity_summaries(command).into_iter().last())
                .unwrap_or_else(|| CommandActivitySummary {
                    icon: "panel-terminal",
                    text: "正在工作".to_owned(),
                    reads_files: false,
                    runs_command: true,
                })
        }
    } else {
        completed_tool_group_summary(&group)
    };
    let has_header_icon = !summary.icon.is_empty();
    let accessible_label = if expanded {
        format!("{}，折叠工具调用", summary.text)
    } else {
        format!("{}，展开工具调用", summary.text)
    };
    let hover_group: SharedString = format!("tool-activity-group-{group_id}").into();
    let click_home = home_entity.clone();
    let click_group_id = group_id.clone();
    let click_scroll = scroll_handle.clone();
    let key_group_id = group_id.clone();
    let key_scroll = scroll_handle.clone();
    let scroll_home = home_entity.clone();
    let nested_scroll_handle = scroll_handle.clone();
    let visibility = disclosure_progress.clamp(0.0, 1.0);
    let chevron_visibility = chevron_progress.clamp(0.0, 1.0);
    let scroll_top = -f32::from(scroll_handle.offset().y);
    let max_scroll = f32::from(scroll_handle.max_offset().y);
    let row_count = tool_group_row_count(&group);
    let estimated_rows_height = TOOL_GROUP_ITEM_GAP
        + row_count as f32 * TOOL_GROUP_HEADER_HEIGHT
        + row_count.saturating_sub(1) as f32 * TOOL_GROUP_ITEM_GAP;
    let has_overflow = max_scroll > 0.5 || estimated_rows_height > TOOL_GROUP_BODY_MAX_HEIGHT;
    let show_top_fade = scroll_top > 0.5;
    let show_bottom_fade = has_overflow && (max_scroll <= 0.5 || scroll_top + 0.5 < max_scroll);

    let activity_rows = group.commands.into_iter().fold(
        div()
            .w_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(TOOL_GROUP_ITEM_GAP)),
        |rows, command| {
            rows.child(command_execution_activity(
                home_entity.clone(),
                command,
                expanded_commands,
                command_scroll_handles,
                theme,
            ))
        },
    );
    let activity_rows = group
        .file_changes
        .into_iter()
        .fold(activity_rows, |rows, file_change| {
            let expanded = expanded_commands.contains(&file_change.item_id);
            let target = home_entity.clone();
            let callback = FileChangeActivityCallback::new(move |event, _, cx| {
                target.update(cx, move |home, cx| {
                    home.handle_file_change_activity_event(event, cx)
                });
            });
            rows.child(
                div()
                    .w_full()
                    .flex_none()
                    .child(render_file_change_activity(
                        &file_change,
                        expanded,
                        theme,
                        callback,
                    )),
            )
        });

    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .items_start()
        .child(
            div()
                .id(SharedString::from(format!(
                    "tool-activity-group-{group_id}"
                )))
                .group(hover_group.clone())
                .h(px(TOOL_GROUP_HEADER_HEIGHT))
                .max_w_full()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(TOOL_GROUP_HEADER_CHEVRON_GAP))
                .rounded(px(6.0))
                .focusable()
                .tab_stop(true)
                .role(Role::Button)
                .aria_expanded(expanded)
                .aria_label(accessible_label)
                .focus_visible(|style| {
                    style.px(px(DISCLOSURE_FOCUS_PADDING)).shadow(vec![
                        BoxShadow::new(px(0.0), px(0.0), rgba(0x3a83f7ff).into())
                            .spread_radius(px(2.0))
                            .inset(),
                    ])
                })
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    toggle_tool_activity_group(
                        &click_home,
                        &click_group_id,
                        active,
                        &click_scroll,
                        cx,
                    );
                })
                .on_key_down(move |event, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        toggle_tool_activity_group(
                            &home_entity,
                            &key_group_id,
                            active,
                            &key_scroll,
                            cx,
                        );
                        cx.stop_propagation();
                    }
                })
                .child(
                    div()
                        .min_w(px(0.0))
                        .max_w(px(718.0))
                        .flex()
                        .items_center()
                        .gap(px(TOOL_GROUP_ICON_TEXT_GAP))
                        .text_color(theme.text.alpha(0.60))
                        .when(has_header_icon, |content| {
                            content.child(
                                icon(summary.icon, theme.text.alpha(0.60).into())
                                    .size(px(TOOL_GROUP_ICON_SIZE))
                                    .flex_none(),
                            )
                        })
                        .child(
                            div()
                                .min_w(px(0.0))
                                .max_w(px(if has_header_icon { 696.0 } else { 718.0 }))
                                .truncate()
                                .text_size(px(TOOL_GROUP_TEXT_SIZE))
                                .line_height(px(TOOL_GROUP_LINE_HEIGHT))
                                .font_family(".SystemUIFont")
                                .font_weight(FontWeight::NORMAL)
                                .child(summary.text),
                        ),
                )
                .child(
                    icon("settings-chevron-right", theme.text.alpha(0.60).into())
                        .size(px(TOOL_GROUP_CHEVRON_SIZE))
                        .flex_none()
                        .opacity(if expanded || visibility > f32::EPSILON {
                            1.0
                        } else {
                            0.0
                        })
                        .group_hover(hover_group, |chevron| chevron.opacity(1.0))
                        .with_transformation(Transformation::rotate(radians(
                            std::f32::consts::FRAC_PI_2 * chevron_visibility,
                        ))),
                ),
        )
        .child(
            div()
                .w_full()
                .max_h(px(TOOL_GROUP_BODY_MAX_HEIGHT * visibility))
                .overflow_hidden()
                .opacity(visibility)
                .when(visibility <= f32::EPSILON, |body| body.invisible())
                .child(
                    div()
                        .relative()
                        .w_full()
                        .max_h(px(TOOL_GROUP_BODY_MAX_HEIGHT))
                        .child(
                            div()
                                .id(SharedString::from(format!("tool-activity-body-{group_id}")))
                                .ml(px(-8.0))
                                .pl(px(8.0))
                                .w_full()
                                .max_h(px(TOOL_GROUP_BODY_MAX_HEIGHT))
                                .overflow_scroll()
                                .restrict_scroll_to_axis()
                                .scrollbar_width(px(0.0))
                                .track_scroll(&scroll_handle)
                                .pt(px(TOOL_GROUP_ITEM_GAP))
                                .on_scroll_wheel(move |event, window, cx| {
                                    if nested_scroll_consumed(&nested_scroll_handle, event, window)
                                    {
                                        cx.stop_propagation();
                                    }
                                    let home = scroll_home.clone();
                                    window.on_next_frame(move |_, cx| {
                                        home.update(cx, |_, cx| cx.notify());
                                    });
                                })
                                .child(activity_rows),
                        )
                        .when(show_top_fade, |body| {
                            body.child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .w_full()
                                    .h(px(TOOL_GROUP_EDGE_FADE_DISTANCE))
                                    .bg(linear_gradient(
                                        0.0,
                                        linear_color_stop(theme.surface.alpha(0.0), 0.0),
                                        linear_color_stop(theme.surface, 1.0),
                                    )),
                            )
                        })
                        .when(show_bottom_fade, |body| {
                            body.child(
                                div()
                                    .absolute()
                                    .bottom_0()
                                    .left_0()
                                    .w_full()
                                    .h(px(TOOL_GROUP_EDGE_FADE_DISTANCE))
                                    .bg(linear_gradient(
                                        180.0,
                                        linear_color_stop(theme.surface.alpha(0.0), 0.0),
                                        linear_color_stop(theme.surface, 1.0),
                                    )),
                            )
                        }),
                ),
        )
}

fn toggle_command_activity(
    home_entity: &Entity<HomeView>,
    item_id: &str,
    scroll_handle: &ScrollHandle,
    cx: &mut App,
) {
    let item_id = item_id.to_owned();
    home_entity.update(cx, |home, cx| {
        if !home.expanded_commands.remove(&item_id) {
            home.expanded_commands.insert(item_id);
            scroll_handle.scroll_to_bottom();
        }
        cx.notify();
    });
}

fn static_command_action_activity(
    row_id: String,
    summary: CommandActivitySummary,
    action: CommandExecutionAction,
    cwd: String,
    theme: Theme,
) -> impl IntoElement {
    let hover_group: SharedString = format!("command-action-{row_id}").into();
    let read_link = match action {
        CommandExecutionAction::Read { name, path, .. } => {
            let label = if name.trim().is_empty() {
                path.clone()
            } else {
                name
            };
            let path = PathBuf::from(path);
            let path = if path.is_absolute() {
                path
            } else {
                PathBuf::from(cwd).join(path)
            };
            Some((label, path))
        }
        _ => None,
    };
    let summary_text = summary.text.clone();
    div()
        .id(SharedString::from(format!("command-action-{row_id}")))
        .group(hover_group.clone())
        .h(px(TOOL_GROUP_HEADER_HEIGHT))
        .flex_none()
        .overflow_hidden()
        .max_w_full()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .gap(px(COMMAND_ACTIVITY_CONTENT_GAP))
        .text_color(theme.text.alpha(0.60))
        .child(
            icon(summary.icon, theme.text.alpha(0.60).into())
                .size(px(COMMAND_ACTIVITY_ICON_SIZE))
                .flex_none(),
        )
        .child(
            div()
                .min_w(px(0.0))
                .max_w(px(696.0))
                .truncate()
                .text_size(px(TOOL_GROUP_TEXT_SIZE))
                .line_height(px(TOOL_GROUP_LINE_HEIGHT))
                .font_family(".SystemUIFont")
                .text_color(theme.text.alpha(0.60))
                .group_hover(hover_group, move |label| label.text_color(theme.text))
                .child(if let Some((label, path)) = read_link {
                    let prefix = summary_text
                        .strip_suffix(&label)
                        .unwrap_or(&summary_text)
                        .to_owned();
                    let click_path = path.clone();
                    let key_path = path.clone();
                    div()
                        .min_w(px(0.0))
                        .flex()
                        .items_center()
                        .child(prefix)
                        .child(
                            div()
                                .id(SharedString::from(format!("command-read-link-{row_id}")))
                                .role(Role::Link)
                                .aria_label(format!("打开 {}", path.display()))
                                .focusable()
                                .tab_stop(true)
                                .min_w(px(0.0))
                                .max_w_full()
                                .truncate()
                                .rounded(px(4.0))
                                .cursor_pointer()
                                .underline()
                                .focus_visible(|style| {
                                    style.shadow(vec![
                                        BoxShadow::new(px(0.0), px(0.0), rgba(0x3a83f7ff).into())
                                            .spread_radius(px(2.0))
                                            .inset(),
                                    ])
                                })
                                .on_click(move |_, _, cx| cx.open_with_system(&click_path))
                                .on_key_down(move |event, _, cx| {
                                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                        cx.open_with_system(&key_path);
                                        cx.stop_propagation();
                                    }
                                })
                                .child(label),
                        )
                        .into_any_element()
                } else {
                    div().child(summary.text).into_any_element()
                }),
        )
}

fn command_execution_activity(
    home_entity: Entity<HomeView>,
    command: CommandExecution,
    expanded_commands: &HashSet<String>,
    command_scroll_handles: &HashMap<String, ScrollHandle>,
    theme: Theme,
) -> Div {
    let execution_id = command.id.clone();
    let actions = if command.actions.is_empty() {
        vec![CommandExecutionAction::Unknown {
            command: command.command.clone(),
        }]
    } else {
        command.actions.clone()
    };
    let action_count = actions.len();
    let scroll_handle = command_scroll_handles
        .get(&execution_id)
        .cloned()
        .unwrap_or_else(ScrollHandle::new);

    actions.into_iter().enumerate().fold(
        div()
            .w_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(TOOL_GROUP_ITEM_GAP)),
        |rows, (index, action)| {
            let row_id = if action_count == 1 {
                execution_id.clone()
            } else {
                format!("{execution_id}-action-{index}")
            };
            let summary = command_action_summary(&command, &action);
            match action {
                CommandExecutionAction::Unknown {
                    command: action_command,
                } => {
                    let mut row_command = command.clone();
                    row_command.id = row_id.clone();
                    row_command.command = action_command.clone();
                    row_command.actions = vec![CommandExecutionAction::Unknown {
                        command: action_command,
                    }];
                    rows.child(command_activity(
                        home_entity.clone(),
                        row_command,
                        expanded_commands.contains(&row_id),
                        scroll_handle.clone(),
                        theme,
                    ))
                }
                action => rows.child(static_command_action_activity(
                    row_id,
                    summary,
                    action,
                    command.cwd.clone(),
                    theme,
                )),
            }
        },
    )
}

fn command_activity(
    home_entity: Entity<HomeView>,
    command: CommandExecution,
    expanded: bool,
    scroll_handle: ScrollHandle,
    theme: Theme,
) -> Div {
    let item_id = command.id.clone();
    let output_scroll_id: SharedString = format!("command-output-{item_id}").into();
    let hover_group: SharedString = format!("command-activity-{item_id}").into();
    let summary = command_activity_summary(&command);
    let status_label = match command.status {
        CommandExecutionStatus::InProgress => "运行中",
        CommandExecutionStatus::Completed => "成功",
        CommandExecutionStatus::Failed => "失败",
    };
    let status_icon = match command.status {
        CommandExecutionStatus::Failed => "settings-warning",
        _ => "check",
    };
    let status_color = if command.status == CommandExecutionStatus::Failed {
        theme.warning
    } else {
        theme.command_muted
    };
    let display_command = if command.command.is_empty() {
        "命令".to_owned()
    } else {
        command.command.clone()
    };
    let output = if command.output.is_empty() {
        if command.status == CommandExecutionStatus::InProgress {
            "等待输出…".to_owned()
        } else {
            "（无输出）".to_owned()
        }
    } else {
        // A terminal normally returns one final line ending. Browsers do not
        // allocate another visible line for it in ChatGPT's shell card, while
        // GPUI's text layout does, so omit exactly that transport delimiter.
        strip_terminal_line_ending(&command.output).to_owned()
    };
    let command_for_body = display_command.clone();
    let accessible_label = if expanded {
        format!("{}，折叠详情", summary.text)
    } else {
        format!("{}，展开详情", summary.text)
    };
    let click_home = home_entity.clone();
    let click_item_id = item_id.clone();
    let click_scroll = scroll_handle.clone();
    let key_item_id = item_id.clone();
    let key_scroll = scroll_handle.clone();
    let nested_scroll_handle = scroll_handle.clone();

    div()
        .w_full()
        .min_w(px(0.0))
        .flex_none()
        .flex()
        .flex_col()
        .items_start()
        .child(
            div()
                .id(SharedString::from(format!("command-activity-{item_id}")))
                .group(hover_group.clone())
                .h(px(21.0))
                .flex_none()
                .overflow_hidden()
                .max_w_full()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(4.0))
                .rounded(px(6.0))
                .focusable()
                .tab_stop(true)
                .role(Role::Button)
                .aria_expanded(expanded)
                .aria_label(accessible_label)
                .focus_visible(|style| {
                    style.px(px(DISCLOSURE_FOCUS_PADDING)).shadow(vec![
                        BoxShadow::new(px(0.0), px(0.0), rgba(0x3a83f7ff).into())
                            .spread_radius(px(2.0))
                            .inset(),
                    ])
                })
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    toggle_command_activity(&click_home, &click_item_id, &click_scroll, cx);
                })
                .on_key_down(move |event, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        toggle_command_activity(&home_entity, &key_item_id, &key_scroll, cx);
                        cx.stop_propagation();
                    }
                })
                .child(
                    div()
                        .min_w(px(0.0))
                        .max_w(px(718.0))
                        .flex()
                        .items_center()
                        .gap(px(COMMAND_ACTIVITY_CONTENT_GAP))
                        .text_color(theme.text.alpha(0.60))
                        .child(
                            icon(summary.icon, theme.text.alpha(0.60).into())
                                .size(px(COMMAND_ACTIVITY_ICON_SIZE))
                                .flex_none(),
                        )
                        .child(
                            div()
                                .min_w(px(0.0))
                                .max_w(px(696.0))
                                .truncate()
                                .text_size(px(14.0))
                                .line_height(px(21.0))
                                .font_family(".SystemUIFont")
                                .child(summary.text),
                        ),
                )
                .child(
                    icon("settings-chevron-right", theme.text.alpha(0.60).into())
                        .size(px(COMMAND_ACTIVITY_CHEVRON_SIZE))
                        .flex_none()
                        .opacity(if expanded { 1.0 } else { 0.0 })
                        .group_hover(hover_group, |chevron| chevron.opacity(1.0))
                        .when(expanded, |chevron| {
                            chevron.with_transformation(Transformation::rotate(radians(
                                std::f32::consts::FRAC_PI_2,
                            )))
                        }),
                ),
        )
        .when(expanded, |activity| {
            activity.child(
                div().w_full().pt(px(8.0)).pb(px(4.0)).child(
                    div()
                        .w_full()
                        .overflow_hidden()
                        .rounded(px(COMMAND_CARD_RADIUS))
                        .border(px(1.0))
                        .border_color(theme.command_border)
                        .bg(theme.command_surface)
                        .child(
                            div()
                                .px(px(8.0))
                                .py(px(4.0))
                                .flex()
                                .items_center()
                                .text_size(px(COMMAND_CARD_HEADER_SIZE))
                                .line_height(px(COMMAND_CARD_HEADER_LINE_HEIGHT))
                                .font_weight(FontWeight::LIGHT)
                                .font_family(".SystemUIFont")
                                .text_color(theme.command_text)
                                .child("Shell"),
                        )
                        .child(
                            div()
                                .px(px(8.0))
                                .pt(px(8.0))
                                .text_size(px(COMMAND_CARD_TEXT_SIZE))
                                .line_height(px(COMMAND_CARD_LINE_HEIGHT))
                                .font_weight(FontWeight::LIGHT)
                                .font_family(UI_MONOSPACE_FONT_FAMILY)
                                .text_color(theme.command_text)
                                .child(
                                    div()
                                        .flex()
                                        .items_start()
                                        .pr(px(24.0))
                                        .child(
                                            div()
                                                .mr(px(8.0))
                                                .text_color(theme.command_muted)
                                                .child("$"),
                                        )
                                        .child(
                                            div()
                                                .min_w(px(0.0))
                                                .flex_1()
                                                .max_h(px(COMMAND_CARD_COMMAND_MAX_HEIGHT))
                                                .overflow_hidden()
                                                .child(command_for_body),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .id(output_scroll_id)
                                .max_h(px(COMMAND_CARD_OUTPUT_MAX_HEIGHT))
                                .overflow_scroll()
                                .restrict_scroll_to_axis()
                                .scrollbar_width(px(0.0))
                                .track_scroll(&scroll_handle)
                                .on_scroll_wheel(move |event, window, cx| {
                                    if nested_scroll_consumed(&nested_scroll_handle, event, window)
                                    {
                                        cx.stop_propagation();
                                    }
                                })
                                .p(px(8.0))
                                .text_size(px(COMMAND_CARD_TEXT_SIZE))
                                .line_height(px(COMMAND_CARD_LINE_HEIGHT))
                                .font_weight(FontWeight::LIGHT)
                                .font_family(UI_MONOSPACE_FONT_FAMILY)
                                .text_color(theme.command_text)
                                .child(output),
                        )
                        .child(
                            div()
                                .h(px(COMMAND_CARD_STATUS_HEIGHT))
                                .px(px(10.0))
                                .pt(px(2.0))
                                .pb(px(4.0))
                                .flex()
                                .items_center()
                                .justify_end()
                                .gap(px(4.0))
                                .text_size(px(14.0))
                                .line_height(px(21.0))
                                .font_weight(FontWeight::LIGHT)
                                .text_color(status_color)
                                .child(icon(status_icon, status_color.into()).size(px(12.0)))
                                .child(status_label),
                        ),
                ),
            )
        })
}

fn conversation_status(phase: ConversationPhase) -> Option<&'static str> {
    match phase {
        ConversationPhase::Thinking => Some("正在思考"),
        _ => None,
    }
}

fn thinking_shimmer(theme: Theme, progress: f32) -> impl IntoElement {
    div()
        .id("thinking-shimmer")
        .w(px(THINKING_SHIMMER_WIDTH))
        .h(px(21.0))
        .child(
            canvas(
                move |_, window, _| {
                    let mut font = window.text_style().font();
                    font.family = ".SystemUIFont".into();
                    font.weight = FontWeight::NORMAL;
                    let shape = |color| {
                        window.text_system().shape_line(
                            "正在思考".into(),
                            px(14.0),
                            &[TextRun {
                                len: "正在思考".len(),
                                font: font.clone(),
                                color,
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            }],
                            None,
                        )
                    };
                    let base = shape(theme.text.alpha(0.385).into());
                    let highlights = (1..=THINKING_SHIMMER_ALPHA_LEVELS)
                        .map(|level| {
                            shape(
                                rgba(0xffffff00)
                                    .alpha(
                                        0.75 * level as f32 / THINKING_SHIMMER_ALPHA_LEVELS as f32,
                                    )
                                    .into(),
                            )
                        })
                        .collect::<Vec<_>>();
                    (base, highlights)
                },
                move |bounds,
                      (base, highlights): (ShapedLine, Vec<ShapedLine>),
                      window: &mut Window,
                      cx: &mut App| {
                    let origin = bounds.origin;
                    base.paint(origin, px(21.0), TextAlign::Left, None, window, cx)
                        .expect("thinking shimmer base glyphs should paint");

                    let text_width = f32::from(bounds.size.width);
                    let band_width = text_width * THINKING_SHIMMER_BAND_SCALE;
                    let band_left = f32::from(bounds.origin.x)
                        + thinking_shimmer_band_left(progress, text_width);
                    let first_x = band_left.floor().max(f32::from(bounds.left()));
                    let last_x = (band_left + band_width)
                        .ceil()
                        .min(f32::from(bounds.right()));

                    for x in first_x as i32..last_x as i32 {
                        let band_position = (x as f32 + 0.5 - band_left) / band_width;
                        let alpha = thinking_shimmer_alpha(band_position);
                        if alpha <= 0.0 {
                            continue;
                        }
                        let level = ((alpha / 0.75 * THINKING_SHIMMER_ALPHA_LEVELS as f32).ceil()
                            as usize)
                            .clamp(1, THINKING_SHIMMER_ALPHA_LEVELS);
                        let mask = Bounds::from_corners(
                            point(px(x as f32), bounds.top()),
                            point(px(x as f32 + 1.0), bounds.bottom()),
                        );
                        window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
                            highlights[level - 1]
                                .paint(origin, px(21.0), TextAlign::Left, None, window, cx)
                                .expect("thinking shimmer highlight glyphs should paint");
                        });
                    }
                },
            )
            .size_full(),
        )
}

fn message_action(
    glyph: &'static str,
    id: &'static str,
    action: usize,
    active: bool,
    assistant_message: String,
    home_entity: Entity<HomeView>,
    theme: Theme,
) -> impl IntoElement {
    div()
        .id(id)
        .size(px(26.0))
        .rounded(px(10.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .when(active, |button| {
            button.bg(theme.sidebar_hover).text_color(theme.text)
        })
        .hover(move |style| style.bg(theme.sidebar_hover))
        .active(move |style| style.bg(theme.text.alpha(0.12)))
        .on_click(move |_, _, cx| match action {
            0 => cx.write_to_clipboard(gpui::ClipboardItem::new_string(assistant_message.clone())),
            1 | 2 => {
                let value = if action == 1 { 1 } else { -1 };
                home_entity.update(cx, |home, cx| {
                    home.response_feedback = if home.response_feedback == value {
                        0
                    } else {
                        value
                    };
                    cx.notify();
                });
            }
            _ => {}
        })
        .child(icon(glyph, theme.text_tertiary.into()).size(px(RESPONSE_ACTION_ICON_SIZE)))
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use gpui::{
        AppContext, Bounds, KeyBinding, MouseButton, TestApp, TestAppWindow, WindowBounds,
        WindowOptions, point, px, size,
    };

    use super::{
        ActivityStreamUnit, COMMAND_ACTIVITY_CHEVRON_SIZE, COMMAND_ACTIVITY_CONTENT_GAP,
        COMMAND_ACTIVITY_ICON_SIZE, COMMAND_CARD_COMMAND_MAX_HEIGHT,
        COMMAND_CARD_HEADER_LINE_HEIGHT, COMMAND_CARD_HEADER_SIZE, COMMAND_CARD_LINE_HEIGHT,
        COMMAND_CARD_OUTPUT_MAX_HEIGHT, COMMAND_CARD_RADIUS, COMMAND_CARD_STATUS_HEIGHT,
        COMMAND_CARD_TEXT_SIZE, CONVERSATION_BOTTOM_INSET, CONVERSATION_TOP_INSET,
        DISCLOSURE_FOCUS_PADDING, HomeView, NOTICE_BUTTON_HEIGHT, NOTICE_ERROR_CONTENT_GAP,
        NOTICE_ERROR_GAP, NOTICE_ICON_SIZE, NOTICE_LINE_HEIGHT, NOTICE_RADIUS, NOTICE_TEXT_SIZE,
        NOTICE_WARNING_CONTENT_GAP, NOTICE_WARNING_GAP, OpenImagePreview,
        REASONING_BODY_MAX_HEIGHT, REASONING_CHEVRON_SIZE, REASONING_HEADER_HEIGHT,
        REASONING_LINE_HEIGHT, REASONING_TEXT_SIZE, REASONING_TRANSITION_DURATION,
        RESPONSE_ACTION_FOOTER_ELECTRON_SHIFT, RESPONSE_ACTION_FOOTER_HEIGHT,
        RESPONSE_ACTION_FOOTER_OFFSET, RESPONSE_ACTION_GAP, RESPONSE_ACTION_ICON_SIZE,
        RESPONSE_TIME_LINE_HEIGHT, RESPONSE_TIME_MARGIN, RESPONSE_TIME_SIZE,
        SUGGESTION_PRESSED_SCALE, THINKING_SHIMMER_DURATION, THINKING_SHIMMER_FRAME_INTERVAL,
        THINKING_SHIMMER_STEPS, THINKING_SHIMMER_WIDTH, TOOL_GROUP_BODY_MAX_HEIGHT,
        TOOL_GROUP_CHEVRON_SIZE, TOOL_GROUP_EDGE_FADE_DISTANCE, TOOL_GROUP_HEADER_CHEVRON_GAP,
        TOOL_GROUP_HEADER_HEIGHT, TOOL_GROUP_ICON_SIZE, TOOL_GROUP_ICON_TEXT_GAP,
        TOOL_GROUP_ITEM_GAP, TOOL_GROUP_LINE_HEIGHT, TOOL_GROUP_TEXT_SIZE,
        TOOL_GROUP_TRANSITION_DURATION, USER_MESSAGE_BUBBLE_RADIUS,
        USER_MESSAGE_BUBBLE_SUPERELLIPSE, USER_MESSAGE_FOOTER_GAP, USER_MESSAGE_FOOTER_HEIGHT,
        USER_MESSAGE_FOOTER_OFFSET, USER_MESSAGE_FOOTER_SIDE_MARGIN, USER_MESSAGE_TIME_LINE_HEIGHT,
        USER_MESSAGE_TIME_SIZE, active_reasoning_body, activity_stream_units,
        command_activity_row_count, command_activity_summaries, command_activity_summary,
        completed_reasoning_body, completed_tool_group_summary, conversation_status,
        format_reasoning_elapsed, generic_command_activity_summary, reasoning_activity_title,
        reasoning_header_label, reasoning_transition_ease, scroll_should_follow_output,
        strip_terminal_line_ending, thinking_shimmer_alpha, thinking_shimmer_band_left,
        thinking_shimmer_progress, thinking_shimmer_step, toggle_reasoning_item,
        toggle_tool_activity_group, tool_group_chevron_transition_ease, tool_group_reasoning_title,
    };
    use crate::agent::{
        AgentImageView, CommandExecution, CommandExecutionAction, CommandExecutionStatus,
        HistoryItemDetail, HistoryTurnStatus, ThreadActivity, ThreadHistory, ThreadHistoryItem,
        ThreadSummary, ThreadTurn,
    };
    use crate::components::{
        composer::{ConversationActivity, ConversationPhase, ReasoningActivityPresentation},
        file_change::captured_file_change_activity_fixture,
        prompt_input::Submit,
        user_input_request::{UserInputKeyboardFocus, UserInputRequestStatus},
    };
    use crate::theme::ThemeMode;

    fn simulate_next_frame(app: &mut TestApp, window: &TestAppWindow<HomeView>, elapsed_ms: u64) {
        app.advance_clock(Duration::from_millis(elapsed_ms));
        let handle = window.handle();
        app.update(|cx| {
            cx.update_window(handle.into(), |_, window, cx| {
                window.simulate_next_frame(cx)
            })
            .unwrap()
        });
    }

    fn reasoning(id: &str, title: &str, active: bool) -> ReasoningActivityPresentation {
        ReasoningActivityPresentation {
            item_id: id.to_owned(),
            summary: vec![title.to_owned()],
            content: Vec::new(),
            started_at_ms: 1_000,
            completed_at_ms: (!active).then_some(2_000),
        }
    }

    fn command(
        id: &str,
        action: CommandExecutionAction,
        status: CommandExecutionStatus,
    ) -> CommandExecution {
        let command = match &action {
            CommandExecutionAction::Read { command, .. }
            | CommandExecutionAction::ListFiles { command, .. }
            | CommandExecutionAction::Search { command, .. }
            | CommandExecutionAction::Unknown { command } => command.clone(),
        };
        CommandExecution {
            id: id.to_owned(),
            command,
            actions: vec![action],
            cwd: "/tmp/project".to_owned(),
            output: String::new(),
            terminal_process_id: None,
            status,
            exit_code: (status == CommandExecutionStatus::Completed).then_some(0),
        }
    }

    #[test]
    fn consecutive_reasoning_and_commands_form_one_tool_activity_group() {
        let activities = vec![
            ConversationActivity::Reasoning(reasoning("reasoning_1", "Inspecting runtime", false)),
            ConversationActivity::Command(command(
                "read_1",
                CommandExecutionAction::Read {
                    command: "sed -n '1,20p' src/main.rs".into(),
                    name: "main.rs".into(),
                    path: "src/main.rs".into(),
                },
                CommandExecutionStatus::Completed,
            )),
            ConversationActivity::Reasoning(reasoning("reasoning_2", "Checking tests", false)),
            ConversationActivity::Command(command(
                "run_1",
                CommandExecutionAction::Unknown {
                    command: "cargo test".into(),
                },
                CommandExecutionStatus::Completed,
            )),
            ConversationActivity::AssistantMessage {
                item_id: "message_1".into(),
                text: "完成。".into(),
            },
        ];

        let units = activity_stream_units(&activities);
        assert_eq!(units.len(), 2);
        let ActivityStreamUnit::ToolGroup(group) = &units[0] else {
            panic!("expected grouped tool activity");
        };
        assert_eq!(group.id, "reasoning_1");
        assert_eq!(group.reasoning.len(), 2);
        assert_eq!(group.commands.len(), 2);
        assert_eq!(
            tool_group_reasoning_title(group).as_deref(),
            Some("Checking tests")
        );
        assert_eq!(
            completed_tool_group_summary(group).text,
            "已读取文件运行了命令"
        );
        assert!(matches!(units[1], ActivityStreamUnit::Standalone(_)));

        let separated = vec![
            activities[1].clone(),
            activities[4].clone(),
            activities[3].clone(),
        ];
        let separated_units = activity_stream_units(&separated);
        assert_eq!(separated_units.len(), 3);
        assert!(matches!(
            separated_units[0],
            ActivityStreamUnit::ToolGroup(_)
        ));
        assert!(matches!(
            separated_units[2],
            ActivityStreamUnit::ToolGroup(_)
        ));
    }

    #[test]
    fn image_view_stays_a_standalone_disclosure_between_tool_groups() {
        let activities = vec![
            ConversationActivity::Command(command(
                "read_1",
                CommandExecutionAction::Read {
                    command: "sed -n '1,20p' screenshot.png".into(),
                    name: "screenshot.png".into(),
                    path: "screenshot.png".into(),
                },
                CommandExecutionStatus::Completed,
            )),
            ConversationActivity::ImageView(AgentImageView {
                id: "image_1".into(),
                path: PathBuf::from("/tmp/screenshot.png"),
            }),
            ConversationActivity::Command(command(
                "test_1",
                CommandExecutionAction::Unknown {
                    command: "cargo test".into(),
                },
                CommandExecutionStatus::Completed,
            )),
        ];

        let units = activity_stream_units(&activities);
        assert_eq!(units.len(), 3);
        assert!(matches!(units[0], ActivityStreamUnit::ToolGroup(_)));
        assert!(matches!(
            &units[1],
            ActivityStreamUnit::Standalone(ConversationActivity::ImageView(image))
                if image.id == "image_1" && image.path == PathBuf::from("/tmp/screenshot.png")
        ));
        assert!(matches!(units[2], ActivityStreamUnit::ToolGroup(_)));
    }

    #[test]
    fn file_change_joins_the_tool_group_and_leads_its_completed_summary() {
        let activities = vec![
            ConversationActivity::Reasoning(reasoning("reasoning_1", "Applying changes", false)),
            ConversationActivity::Command(command(
                "read_1",
                CommandExecutionAction::Read {
                    command: "sed -n '1,20p' src/main.rs".into(),
                    name: "main.rs".into(),
                    path: "src/main.rs".into(),
                },
                CommandExecutionStatus::Completed,
            )),
            ConversationActivity::FileChange(captured_file_change_activity_fixture("completed")),
            ConversationActivity::Command(command(
                "run_1",
                CommandExecutionAction::Unknown {
                    command: "cargo test".into(),
                },
                CommandExecutionStatus::Completed,
            )),
        ];

        let units = activity_stream_units(&activities);
        assert_eq!(units.len(), 1);
        let ActivityStreamUnit::ToolGroup(group) = &units[0] else {
            panic!("expected fileChange inside the tool activity group");
        };
        assert_eq!(group.file_changes.len(), 1);
        assert_eq!(
            completed_tool_group_summary(group).text,
            "编辑了文件读取文件运行了命令"
        );
        assert_eq!(completed_tool_group_summary(group).icon, "message-edit");
    }

    #[test]
    fn active_reasoning_follows_the_latest_json_rpc_item_until_it_completes() {
        let later_items = || {
            vec![
                ConversationActivity::AssistantMessage {
                    item_id: "message_1".into(),
                    text: "先说明当前进度。".into(),
                },
                ConversationActivity::Command(command(
                    "read_1",
                    CommandExecutionAction::Read {
                        command: "sed -n '1,20p' src/main.rs".into(),
                        name: "main.rs".into(),
                        path: "src/main.rs".into(),
                    },
                    CommandExecutionStatus::Completed,
                )),
                ConversationActivity::AssistantMessage {
                    item_id: "message_2".into(),
                    text: "继续分析读取结果。".into(),
                },
            ]
        };

        let mut active_activities = vec![ConversationActivity::Reasoning(reasoning(
            "reasoning_1",
            "Inspecting the implementation",
            true,
        ))];
        active_activities.extend(later_items());
        let active_units = activity_stream_units(&active_activities);

        assert_eq!(active_units.len(), 4);
        assert!(matches!(
            &active_units[0],
            ActivityStreamUnit::Standalone(ConversationActivity::AssistantMessage {
                item_id,
                ..
            }) if item_id == "message_1"
        ));
        assert!(matches!(active_units[1], ActivityStreamUnit::ToolGroup(_)));
        assert!(matches!(
            &active_units[2],
            ActivityStreamUnit::Standalone(ConversationActivity::AssistantMessage {
                item_id,
                ..
            }) if item_id == "message_2"
        ));
        assert!(matches!(
            &active_units[3],
            ActivityStreamUnit::Standalone(ConversationActivity::Reasoning(reasoning))
                if reasoning.item_id == "reasoning_1" && reasoning.is_active()
        ));

        let mut completed_activities = vec![ConversationActivity::Reasoning(reasoning(
            "reasoning_1",
            "Inspecting the implementation",
            false,
        ))];
        completed_activities.extend(later_items());
        let completed_units = activity_stream_units(&completed_activities);
        assert_eq!(completed_units.len(), 3);
        assert!(matches!(
            &completed_units[0],
            ActivityStreamUnit::Standalone(ConversationActivity::AssistantMessage {
                item_id,
                ..
            }) if item_id == "message_1"
        ));
        assert!(!completed_units.iter().any(|unit| matches!(
            unit,
            ActivityStreamUnit::Standalone(ConversationActivity::Reasoning(_))
        )));
    }

    #[test]
    fn completed_reasoning_without_a_following_command_is_not_rendered() {
        let activities = vec![
            ConversationActivity::Reasoning(reasoning("reasoning_1", "Only reasoning", false)),
            ConversationActivity::AssistantMessage {
                item_id: "message_1".into(),
                text: "结论。".into(),
            },
        ];
        let units = activity_stream_units(&activities);
        assert_eq!(units.len(), 1);
        assert!(matches!(
            &units[0],
            ActivityStreamUnit::Standalone(ConversationActivity::AssistantMessage {
                item_id,
                ..
            }) if item_id == "message_1"
        ));

        let titled = ReasoningActivityPresentation {
            summary: vec!["**Header title**".into(), "Longer reasoning body".into()],
            ..reasoning("reasoning_title", "unused", false)
        };
        assert_eq!(
            reasoning_activity_title(&titled).as_deref(),
            Some("Header title")
        );
    }

    #[test]
    fn command_rows_use_the_app_server_action_semantics() {
        let read = command(
            "read",
            CommandExecutionAction::Read {
                command: "sed -n '1,20p' src/main.rs".into(),
                name: "main.rs".into(),
                path: "src/main.rs".into(),
            },
            CommandExecutionStatus::Completed,
        );
        assert_eq!(command_activity_summary(&read).text, "已读取 main.rs");

        let search = command(
            "search",
            CommandExecutionAction::Search {
                command: "rg needle src".into(),
                path: Some("src".into()),
                query: Some("needle".into()),
            },
            CommandExecutionStatus::InProgress,
        );
        assert_eq!(
            command_activity_summary(&search).text,
            "正在 src 中搜索“needle”"
        );

        let shell = command(
            "shell",
            CommandExecutionAction::Unknown {
                command: "cargo check".into(),
            },
            CommandExecutionStatus::Completed,
        );
        assert_eq!(command_activity_summary(&shell).text, "已运行 cargo check");
    }

    #[test]
    fn multiline_history_command_summary_collapses_to_one_activity_row() {
        let command = command(
            "multiline-command",
            CommandExecutionAction::Unknown {
                command: "python3 - <<'PY'\nfrom PIL import Image\nprint('done')\nPY".to_owned(),
            },
            CommandExecutionStatus::Completed,
        );

        let summary = generic_command_activity_summary(&command, &command.command);
        assert_eq!(
            summary.text,
            "已运行 python3 - <<'PY' from PIL import Image print('done') PY"
        );
        assert_eq!(summary.text.lines().count(), 1);
    }

    #[test]
    fn one_command_execution_renders_every_structured_action_as_its_own_row() {
        let command = CommandExecution {
            id: "exec_many".into(),
            command: "compound command".into(),
            actions: vec![
                CommandExecutionAction::Read {
                    command: "sed main.rs".into(),
                    name: "main.rs".into(),
                    path: "src/main.rs".into(),
                },
                CommandExecutionAction::Search {
                    command: "rg needle src".into(),
                    path: Some("src".into()),
                    query: Some("needle".into()),
                },
                CommandExecutionAction::Unknown {
                    command: "cargo check".into(),
                },
            ],
            cwd: "/tmp/project".into(),
            output: String::new(),
            terminal_process_id: None,
            status: CommandExecutionStatus::Completed,
            exit_code: Some(0),
        };

        let summaries = command_activity_summaries(&command);
        assert_eq!(command_activity_row_count(&command), 3);
        assert_eq!(
            summaries
                .into_iter()
                .map(|summary| summary.text)
                .collect::<Vec<_>>(),
            vec![
                "已读取 main.rs",
                "已在 src 中搜索“needle”",
                "已运行 cargo check",
            ]
        );
    }

    #[test]
    fn tool_group_matches_the_live_cdp_geometry() {
        assert_eq!(TOOL_GROUP_HEADER_HEIGHT, 21.0);
        assert_eq!(TOOL_GROUP_TEXT_SIZE, 14.0);
        assert_eq!(TOOL_GROUP_LINE_HEIGHT, 21.0);
        assert_eq!(TOOL_GROUP_ICON_SIZE, 16.0);
        assert_eq!(TOOL_GROUP_ICON_TEXT_GAP, 6.0);
        assert_eq!(TOOL_GROUP_HEADER_CHEVRON_GAP, 4.0);
        assert_eq!(TOOL_GROUP_CHEVRON_SIZE, 14.0);
        assert_eq!(TOOL_GROUP_ITEM_GAP, 4.0);
        assert_eq!(TOOL_GROUP_BODY_MAX_HEIGHT, 224.0);
        assert_eq!(TOOL_GROUP_EDGE_FADE_DISTANCE, 24.0);
        assert_eq!(DISCLOSURE_FOCUS_PADDING, 2.0);
        assert_eq!(TOOL_GROUP_TRANSITION_DURATION, Duration::from_millis(300));
        assert!(tool_group_chevron_transition_ease(0.5) > 0.7);
        assert!(tool_group_chevron_transition_ease(0.5) < 0.9);
    }

    #[test]
    fn conversation_viewport_scrolls_and_does_not_snap_back_after_user_scrolls_up() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(420.0)),
                })),
                ..Default::default()
            },
            |_, cx| HomeView::new(ThemeMode::Dark, cx),
        );

        window.update(|home, _, cx| home.set_tool_group_for_capture(true, false, cx));
        window.draw();

        let (initial_offset, max_offset) = window.read(|home, _| {
            (
                f32::from(home.conversation_scroll.offset().y),
                f32::from(home.conversation_scroll.max_offset().y),
            )
        });
        assert!(max_offset > 100.0, "long conversations must overflow");
        assert!((initial_offset + max_offset).abs() < 0.01);
        assert!(window.read(|home, _| { scroll_should_follow_output(&home.conversation_scroll) }));

        // Use the empty gutter beside the centered 736px message column so
        // only the main conversation viewport receives this gesture.
        window.simulate_scroll(point(px(20.0), px(200.0)), point(px(0.0), px(96.0)));
        let user_offset = window.read(|home, _| f32::from(home.conversation_scroll.offset().y));
        assert!(user_offset > initial_offset);
        assert!(!window.read(|home, _| { scroll_should_follow_output(&home.conversation_scroll) }));

        // A streaming repaint must preserve the user's reading position.
        window.update(|_, _, cx| cx.notify());
        window.draw();
        let repainted_offset =
            window.read(|home, _| f32::from(home.conversation_scroll.offset().y));
        assert!((repainted_offset - user_offset).abs() < 0.01);

        window.simulate_scroll(point(px(20.0), px(200.0)), point(px(0.0), px(-10_000.0)));
        window.draw();
        window.read(|home, _| {
            let offset = f32::from(home.conversation_scroll.offset().y);
            let max = f32::from(home.conversation_scroll.max_offset().y);
            assert!((offset + max).abs() < 0.01);
            assert!(scroll_should_follow_output(&home.conversation_scroll));
        });
    }

    #[test]
    fn conversation_insets_preserve_the_original_top_and_clear_the_fixed_composer() {
        assert_eq!(CONVERSATION_TOP_INSET, 78.0);
        assert_eq!(CONVERSATION_BOTTOM_INSET, 153.0);
        assert!(CONVERSATION_BOTTOM_INSET > 15.0 + 98.0);
    }

    #[test]
    fn nested_tool_scroll_does_not_move_the_conversation_until_it_reaches_an_edge() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(420.0)),
                })),
                ..Default::default()
            },
            |_, cx| HomeView::new(ThemeMode::Dark, cx),
        );

        window.update(|home, _, cx| home.set_tool_group_for_capture(true, false, cx));
        window.draw();
        let (outer_before, inner_before, inner_max, inner_position) = window.read(|home, _| {
            let inner = home
                .tool_group_scroll_handles
                .get("tool-group-ui-capture")
                .expect("tool group scroll handle");
            (
                f32::from(home.conversation_scroll.offset().y),
                f32::from(inner.offset().y),
                f32::from(inner.max_offset().y),
                inner.bounds().center(),
            )
        });
        assert!(inner_max > 0.0);
        assert!((inner_before + inner_max).abs() < 0.01);

        window.simulate_scroll(inner_position, point(px(0.0), px(48.0)));
        let (outer_after, inner_after) = window.read(|home, _| {
            let inner = home
                .tool_group_scroll_handles
                .get("tool-group-ui-capture")
                .expect("tool group scroll handle");
            (
                f32::from(home.conversation_scroll.offset().y),
                f32::from(inner.offset().y),
            )
        });
        assert!(
            inner_after > inner_before,
            "inner={inner_before}->{inner_after}, outer={outer_before}->{outer_after}, position={inner_position:?}"
        );
        assert!(
            (outer_after - outer_before).abs() < 0.01,
            "inner={inner_before}->{inner_after}, outer={outer_before}->{outer_after}, position={inner_position:?}"
        );

        window.simulate_scroll(inner_position, point(px(0.0), px(48.0)));
        let (outer_at_edge, inner_at_edge) = window.read(|home, _| {
            let inner = home
                .tool_group_scroll_handles
                .get("tool-group-ui-capture")
                .expect("tool group scroll handle");
            (
                f32::from(home.conversation_scroll.offset().y),
                f32::from(inner.offset().y),
            )
        });
        assert!(outer_at_edge > outer_after);
        assert!(inner_at_edge.abs() < 0.01);
    }

    #[test]
    fn tool_group_disclosure_uses_one_toggle_path_for_pointer_and_keyboard_activation() {
        let mut app = TestApp::new();
        let home = app.new_entity(|cx| HomeView::new(ThemeMode::Dark, cx));
        let scroll_handle = gpui::ScrollHandle::new();

        app.update(|cx| {
            toggle_tool_activity_group(&home, "group_1", false, &scroll_handle, cx);
        });
        assert!(app.read_entity(&home, |home, _| {
            home.expanded_tool_groups.contains("group_1")
        }));

        app.update(|cx| {
            toggle_tool_activity_group(&home, "group_1", false, &scroll_handle, cx);
        });
        assert!(!app.read_entity(&home, |home, _| {
            home.expanded_tool_groups.contains("group_1")
        }));

        app.update(|cx| {
            toggle_tool_activity_group(&home, "group_1", true, &scroll_handle, cx);
        });
        assert!(app.read_entity(&home, |home, _| {
            home.collapsed_active_tool_groups.contains("group_1")
        }));
    }

    #[test]
    fn resumed_historical_tool_group_keeps_its_disclosure_state() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| HomeView::new(ThemeMode::Dark, cx),
        );
        let command_turn = |turn_id: &str, item_id: &str, prompt: &str| ThreadTurn {
            turn_id: turn_id.to_owned(),
            status: HistoryTurnStatus::Completed,
            items_view: HistoryItemDetail::Full,
            items: vec![
                ThreadHistoryItem::UserMessage {
                    item_id: format!("{turn_id}-user"),
                    text: prompt.to_owned(),
                },
                ThreadHistoryItem::Command {
                    item_id: item_id.to_owned(),
                    command: "cargo check".to_owned(),
                    output: "Finished dev profile".to_owned(),
                    status: CommandExecutionStatus::Completed,
                },
            ],
            started_at: Some(1_000),
            completed_at: Some(2_000),
            duration_ms: Some(1_000),
            error: None,
        };
        let history = ThreadHistory {
            thread: ThreadSummary {
                thread_id: "resume-thread".to_owned(),
                title: "Resume disclosure".to_owned(),
                preview: String::new(),
                cwd: PathBuf::from("/tmp/project"),
                project_id: None,
                section: None,
                created_at: 1,
                updated_at: 2,
                recency_at: Some(2),
                activity: ThreadActivity::Idle,
            },
            turns: vec![
                command_turn("historical-turn", "historical-command", "first"),
                command_turn("current-turn", "current-command", "second"),
            ],
            next_turn_cursor: None,
            backwards_turn_cursor: None,
        };

        window.update(|home, _, cx| {
            home.composer_entity()
                .update(cx, |composer, cx| composer.hydrate_history(history, cx));
        });
        window.draw();
        window.read(|home, _| {
            assert!(
                home.tool_group_disclosure_transitions
                    .contains_key("historical-command")
            );
            assert!(
                home.command_scroll_handles
                    .contains_key("historical-command")
            );
        });

        window.update(|home, _, cx| {
            home.expanded_tool_groups
                .insert("historical-command".to_owned());
            cx.notify();
        });
        window.draw();
        window.read(|home, _| {
            assert!(home.expanded_tool_groups.contains("historical-command"));
            assert_eq!(
                home.tool_group_disclosure_transitions
                    .get("historical-command")
                    .expect("historical transition")
                    .target,
                1.0
            );
        });
    }

    #[test]
    fn dense_resumed_tool_group_preserves_row_height_and_scrolls_instead_of_overlapping() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| HomeView::new(ThemeMode::Dark, cx),
        );
        let mut dense_items = vec![ThreadHistoryItem::UserMessage {
            item_id: "dense-user".to_owned(),
            text: "inspect the native material".to_owned(),
        }];
        dense_items.extend((0..20).map(|index| ThreadHistoryItem::Command {
            item_id: format!("dense-command-{index}"),
            command: if index == 4 {
                "python3 - <<'PY'\nfrom PIL import Image\nprint('done')\nPY".to_owned()
            } else {
                format!("cargo check --package fixture-{index}")
            },
            output: "Finished dev profile".to_owned(),
            status: CommandExecutionStatus::Completed,
        }));
        let history = ThreadHistory {
            thread: ThreadSummary {
                thread_id: "dense-resume-thread".to_owned(),
                title: "Dense resume disclosure".to_owned(),
                preview: String::new(),
                cwd: PathBuf::from("/tmp/project"),
                project_id: None,
                section: None,
                created_at: 1,
                updated_at: 2,
                recency_at: Some(2),
                activity: ThreadActivity::Idle,
            },
            turns: vec![
                ThreadTurn {
                    turn_id: "dense-historical-turn".to_owned(),
                    status: HistoryTurnStatus::Completed,
                    items_view: HistoryItemDetail::Full,
                    items: dense_items,
                    started_at: Some(1_000),
                    completed_at: Some(2_000),
                    duration_ms: Some(1_000),
                    error: None,
                },
                ThreadTurn {
                    turn_id: "current-turn".to_owned(),
                    status: HistoryTurnStatus::Completed,
                    items_view: HistoryItemDetail::Full,
                    items: vec![ThreadHistoryItem::UserMessage {
                        item_id: "current-user".to_owned(),
                        text: "continue".to_owned(),
                    }],
                    started_at: Some(3_000),
                    completed_at: Some(4_000),
                    duration_ms: Some(1_000),
                    error: None,
                },
            ],
            next_turn_cursor: None,
            backwards_turn_cursor: None,
        };

        window.update(|home, _, cx| {
            home.composer_entity()
                .update(cx, |composer, cx| composer.hydrate_history(history, cx));
            home.expanded_tool_groups
                .insert("dense-command-0".to_owned());
        });
        window.draw();
        window.read(|home, _| {
            let scroll = home
                .tool_group_scroll_handles
                .get("dense-command-0")
                .expect("dense historical tool group scroll handle");
            // 20 fixed 21px rows, 19 four-pixel gaps, and the four-pixel top
            // inset produce 500px of content in the captured 224px viewport.
            assert_eq!(f32::from(scroll.max_offset().y), 276.0);
        });
    }

    #[test]
    fn suggestion_press_uses_the_reference_scale_and_interruptible_transition() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| HomeView::new(ThemeMode::Dark, cx),
        );

        window.update(|home, window, cx| {
            home.set_suggestion_pressed(0, true, window, cx);
        });
        simulate_next_frame(&mut app, &window, 75);
        let pressed_midpoint = window.read(|home, _| home.suggestion_scale[0]);
        assert!(pressed_midpoint > SUGGESTION_PRESSED_SCALE && pressed_midpoint < 1.0);

        // Releasing halfway through must reverse from the rendered value,
        // rather than jumping to either endpoint.
        window.update(|home, window, cx| {
            home.set_suggestion_pressed(0, false, window, cx);
        });
        assert_eq!(
            window.read(|home, _| home.suggestion_scale[0]),
            pressed_midpoint
        );
        simulate_next_frame(&mut app, &window, 150);
        assert_eq!(window.read(|home, _| home.suggestion_scale[0]), 1.0);
        assert!(!window.read(|home, _| home.suggestion_animation_running));
    }

    #[test]
    fn starting_a_prompt_does_not_render_a_synthetic_status_message() {
        assert_eq!(conversation_status(ConversationPhase::Starting), None);
        assert_eq!(
            conversation_status(ConversationPhase::Thinking),
            Some("正在思考")
        );
    }

    #[test]
    fn event_notices_match_the_live_cdp_geometry() {
        assert_eq!(NOTICE_RADIUS, 20.0);
        assert_eq!(NOTICE_TEXT_SIZE, 13.0);
        assert_eq!(NOTICE_LINE_HEIGHT, 20.0);
        assert_eq!(NOTICE_ICON_SIZE, 18.0);
        assert_eq!(NOTICE_ERROR_GAP, 12.0);
        assert_eq!(NOTICE_WARNING_GAP, 16.0);
        assert_eq!(NOTICE_ERROR_CONTENT_GAP, 6.0);
        assert_eq!(NOTICE_WARNING_CONTENT_GAP, 8.0);
        assert_eq!(NOTICE_BUTTON_HEIGHT, 24.0);
    }

    #[test]
    fn thinking_shimmer_matches_the_cdp_animation_geometry() {
        assert_eq!(THINKING_SHIMMER_DURATION, Duration::from_secs(1));
        assert_eq!(thinking_shimmer_progress(Duration::ZERO), 0.0);
        assert_eq!(thinking_shimmer_progress(Duration::from_millis(500)), 0.5);
        assert_eq!(thinking_shimmer_progress(Duration::from_secs(1)), 1.0);
        assert_eq!(thinking_shimmer_step(0.02), 0.0);
        assert_eq!(thinking_shimmer_step(1.0 / 48.0), 1.0 / 48.0);
        assert_eq!(
            thinking_shimmer_band_left(0.0, THINKING_SHIMMER_WIDTH),
            -28.0
        );
        assert_eq!(
            thinking_shimmer_band_left(1.0, THINKING_SHIMMER_WIDTH),
            70.0
        );
        assert_eq!(thinking_shimmer_alpha(0.0), 0.0);
        assert_eq!(thinking_shimmer_alpha(0.4), 0.75);
        assert_eq!(thinking_shimmer_alpha(0.6), 0.75);
        assert_eq!(thinking_shimmer_alpha(1.0), 0.0);
    }

    #[test]
    fn reasoning_item_matches_the_desktop_geometry_and_localized_labels() {
        assert_eq!(REASONING_HEADER_HEIGHT, 21.0);
        assert_eq!(REASONING_TEXT_SIZE, 14.0);
        assert_eq!(REASONING_LINE_HEIGHT, 21.0);
        assert_eq!(REASONING_CHEVRON_SIZE, 14.0);
        assert_eq!(REASONING_BODY_MAX_HEIGHT, 140.0);
        assert_eq!(REASONING_TRANSITION_DURATION, Duration::from_millis(300));
        assert_eq!(reasoning_transition_ease(0.0), 0.0);
        assert_eq!(reasoning_transition_ease(1.0), 1.0);
        assert!(reasoning_transition_ease(0.5) > 0.9);
        assert_eq!(format_reasoning_elapsed(1), "1s");
        assert_eq!(format_reasoning_elapsed(29_000), "29s");
        assert_eq!(format_reasoning_elapsed(82_000), "1m 22s");
        assert_eq!(format_reasoning_elapsed(3_520_000), "58m 40s");

        let active = ReasoningActivityPresentation {
            item_id: "reasoning_1".into(),
            summary: vec![],
            content: vec![],
            started_at_ms: 1_000,
            completed_at_ms: None,
        };
        assert_eq!(reasoning_header_label(&active), "正在思考");

        let complete = ReasoningActivityPresentation {
            completed_at_ms: Some(30_000),
            ..active.clone()
        };
        assert_eq!(reasoning_header_label(&complete), "思考了 29s");
        let missing_elapsed = ReasoningActivityPresentation {
            completed_at_ms: Some(1_000),
            ..active
        };
        assert_eq!(reasoning_header_label(&missing_elapsed), "完成思考");
    }

    #[test]
    fn active_reasoning_hides_the_streamed_summary_title_like_chatgpt() {
        assert_eq!(active_reasoning_body("  普通正文"), "普通正文");
        assert_eq!(
            active_reasoning_body("**检查实现**\n\n正在阅读协议"),
            "正在阅读协议"
        );
        assert_eq!(active_reasoning_body("**尚未闭合"), "");
    }

    #[test]
    fn completed_reasoning_renders_the_summary_title_without_markdown_delimiters() {
        assert_eq!(
            completed_reasoning_body("**检查实现**\n\n正在阅读协议"),
            (Some("检查实现".into()), "正在阅读协议".into())
        );
        assert_eq!(
            completed_reasoning_body("普通正文"),
            (None, "普通正文".into())
        );
        assert_eq!(
            completed_reasoning_body("**尚未闭合"),
            (None, "**尚未闭合".into())
        );
    }

    #[test]
    fn reasoning_disclosure_uses_one_toggle_path_for_pointer_and_keyboard_activation() {
        let mut app = TestApp::new();
        let home = app.new_entity(|cx| HomeView::new(ThemeMode::Dark, cx));
        let scroll_handle = gpui::ScrollHandle::new();

        app.update(|cx| {
            toggle_reasoning_item(&home, "reasoning_1", &scroll_handle, cx);
        });
        assert!(app.read_entity(&home, |home, _| {
            home.expanded_reasoning.contains("reasoning_1")
        }));

        app.update(|cx| {
            toggle_reasoning_item(&home, "reasoning_1", &scroll_handle, cx);
        });
        assert!(!app.read_entity(&home, |home, _| {
            home.expanded_reasoning.contains("reasoning_1")
        }));
    }

    #[test]
    fn completed_standalone_reasoning_exposes_no_disclosure_target() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| HomeView::new(ThemeMode::Dark, cx),
        );

        window.update(|home, _, cx| home.set_reasoning_for_capture("completed-content", false, cx));
        window.draw();
        // This was the old standalone completed-reasoning hitbox. It must no
        // longer mount an interactive disclosure row.
        window.simulate_click(point(px(100.0), px(176.0)), MouseButton::Left);
        window.simulate_keystrokes("space");
        assert!(
            !window.read(|home, _| { home.expanded_reasoning.contains("reasoning-ui-capture") })
        );
    }

    #[test]
    fn reasoning_disclosure_starts_settled_then_runs_the_reference_transition() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| HomeView::new(ThemeMode::Dark, cx),
        );

        window.update(|home, _, cx| home.set_reasoning_for_capture("active", false, cx));
        window.draw();
        window.read(|home, _| {
            let transition = home
                .reasoning_disclosure_transitions
                .get("reasoning-ui-capture")
                .unwrap();
            assert_eq!(transition.progress, 0.0);
            assert!(transition.started_at.is_none());
        });

        window.update(|home, _, cx| home.set_reasoning_for_capture("active-content", false, cx));
        window.draw();
        simulate_next_frame(&mut app, &window, 150);
        let midpoint = window.read(|home, _| {
            home.reasoning_disclosure_transitions
                .get("reasoning-ui-capture")
                .unwrap()
                .progress
        });
        assert!(midpoint > 0.9 && midpoint < 1.0);

        simulate_next_frame(&mut app, &window, 150);
        window.read(|home, _| {
            let transition = home
                .reasoning_disclosure_transitions
                .get("reasoning-ui-capture")
                .unwrap();
            assert_eq!(transition.progress, 1.0);
            assert!(transition.started_at.is_none());
            assert!(!home.reasoning_transition_running);
        });
    }

    #[test]
    fn tool_group_disclosure_responds_to_real_pointer_and_keyboard_events() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| HomeView::new(ThemeMode::Dark, cx),
        );

        window.update(|home, _, cx| home.set_tool_group_for_capture(false, false, cx));
        window.draw();
        // The preamble starts at y=166 and is 22px tall. The stream's 16px
        // gap puts the 21px grouped-activity button at y=204..225.
        window.simulate_click(point(px(100.0), px(214.0)), MouseButton::Left);
        assert!(
            window.read(|home, _| { home.expanded_tool_groups.contains("tool-group-ui-capture") })
        );

        window.simulate_keystrokes("space");
        assert!(
            !window.read(|home, _| { home.expanded_tool_groups.contains("tool-group-ui-capture") })
        );
    }

    #[test]
    fn image_view_disclosure_responds_to_real_pointer_and_keyboard_events() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| HomeView::new(ThemeMode::Dark, cx),
        );

        window.update(|home, _, cx| {
            home.set_image_view_for_capture(PathBuf::from("/tmp/image-view.png"), cx)
        });
        let preview_path = Arc::new(Mutex::new(None));
        let observed_preview_path = preview_path.clone();
        let home = window.root();
        let _observer = app.new_entity(|cx| {
            cx.subscribe(&home, move |_: &mut (), _, event: &OpenImagePreview, _| {
                *observed_preview_path.lock().unwrap() = Some(event.0.clone());
            })
            .detach();
        });
        window.draw();
        // The entire captured 21px disclosure row at y=204..225 is clickable.
        window.simulate_click(point(px(100.0), px(214.0)), MouseButton::Left);
        assert!(
            window.read(|home, _| { home.expanded_tool_groups.contains("image-view-ui-capture") })
        );

        window.simulate_keystrokes("space");
        assert!(
            window.read(|home, _| { !home.expanded_tool_groups.contains("image-view-ui-capture") })
        );

        window.simulate_keystrokes("enter");
        window.draw();
        window.simulate_click(point(px(100.0), px(250.0)), MouseButton::Left);
        assert_eq!(
            *preview_path.lock().unwrap(),
            Some(PathBuf::from("/tmp/image-view.png"))
        );
    }

    #[test]
    fn tool_group_disclosure_starts_settled_then_runs_the_reference_transition() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| HomeView::new(ThemeMode::Dark, cx),
        );

        window.update(|home, _, cx| home.set_tool_group_for_capture(false, false, cx));
        window.draw();
        window.read(|home, _| {
            let transition = home
                .tool_group_disclosure_transitions
                .get("tool-group-ui-capture")
                .unwrap();
            assert_eq!(transition.progress, 0.0);
            assert!(transition.started_at.is_none());
        });

        window.update(|home, _, cx| {
            home.expanded_tool_groups
                .insert("tool-group-ui-capture".to_owned());
            cx.notify();
        });
        window.draw();
        simulate_next_frame(&mut app, &window, 150);
        let midpoint = window.read(|home, _| {
            home.tool_group_disclosure_transitions
                .get("tool-group-ui-capture")
                .unwrap()
                .progress
        });
        assert!(midpoint > 0.9 && midpoint < 1.0);
        let chevron_midpoint = window.read(|home, _| {
            home.tool_group_disclosure_transitions
                .get("tool-group-ui-capture")
                .unwrap()
                .chevron_progress
        });
        assert!(chevron_midpoint > 0.7 && chevron_midpoint < 0.9);

        simulate_next_frame(&mut app, &window, 150);
        window.read(|home, _| {
            let transition = home
                .tool_group_disclosure_transitions
                .get("tool-group-ui-capture")
                .unwrap();
            assert_eq!(transition.progress, 1.0);
            assert_eq!(transition.chevron_progress, 1.0);
            assert!(transition.started_at.is_none());
            assert!(!home.tool_group_transition_running);
        });
    }

    #[test]
    fn thinking_shimmer_timer_advances_and_loops_the_rendered_phase() {
        let mut app = TestApp::new();
        let home = app.new_entity(|cx| HomeView::new(ThemeMode::Dark, cx));

        app.update_entity(&home, |home, cx| home.start_thinking_shimmer(cx));
        assert_eq!(
            app.read_entity(&home, |home, _| home.thinking_shimmer_progress),
            0.0
        );

        app.advance_clock(THINKING_SHIMMER_FRAME_INTERVAL);
        app.run_until_parked();
        let progress = app.read_entity(&home, |home, _| home.thinking_shimmer_progress);
        assert!((progress - 1.0 / THINKING_SHIMMER_STEPS).abs() < 0.000_001);

        for _ in 1..THINKING_SHIMMER_STEPS as usize {
            app.advance_clock(THINKING_SHIMMER_FRAME_INTERVAL);
            app.run_until_parked();
        }
        assert_eq!(
            app.read_entity(&home, |home, _| home.thinking_shimmer_progress),
            1.0
        );

        app.advance_clock(THINKING_SHIMMER_FRAME_INTERVAL);
        app.run_until_parked();
        let wrapped_progress = app.read_entity(&home, |home, _| home.thinking_shimmer_progress);
        assert!((wrapped_progress - 1.0 / THINKING_SHIMMER_STEPS).abs() < 0.000_001);
        assert!(app.read_entity(&home, |home, _| home.thinking_shimmer_running));

        app.update_entity(&home, |home, cx| {
            home.sync_thinking_shimmer(ConversationPhase::Streaming, cx)
        });
        app.advance_clock(THINKING_SHIMMER_FRAME_INTERVAL);
        app.run_until_parked();
        assert_eq!(
            app.read_entity(&home, |home, _| home.thinking_shimmer_progress),
            0.0
        );
        assert!(!app.read_entity(&home, |home, _| home.thinking_shimmer_running));
    }

    #[test]
    fn user_bubble_uses_the_live_cdp_corner_radius() {
        assert_eq!(USER_MESSAGE_BUBBLE_RADIUS, 22.0);
        assert_eq!(USER_MESSAGE_BUBBLE_SUPERELLIPSE, 1.5);
    }

    #[test]
    fn response_action_icons_use_the_css_resolved_size() {
        assert_eq!(RESPONSE_ACTION_ICON_SIZE, 16.0);
    }

    #[test]
    fn command_card_matches_the_live_cdp_geometry() {
        assert_eq!(COMMAND_ACTIVITY_ICON_SIZE, 16.0);
        assert_eq!(COMMAND_ACTIVITY_CONTENT_GAP, 6.0);
        assert_eq!(COMMAND_ACTIVITY_CHEVRON_SIZE, 14.0);
        assert_eq!(COMMAND_CARD_RADIUS, 12.5);
        assert_eq!(COMMAND_CARD_HEADER_SIZE, 13.0);
        assert_eq!(COMMAND_CARD_HEADER_LINE_HEIGHT, 18.5714);
        assert_eq!(COMMAND_CARD_TEXT_SIZE, 13.0);
        assert_eq!(COMMAND_CARD_LINE_HEIGHT, 19.5);
        assert_eq!(COMMAND_CARD_COMMAND_MAX_HEIGHT, 39.0);
        assert_eq!(COMMAND_CARD_OUTPUT_MAX_HEIGHT, 144.0);
        assert_eq!(COMMAND_CARD_STATUS_HEIGHT, 28.0);
    }

    #[test]
    fn command_card_omits_one_terminal_line_ending() {
        assert_eq!(strip_terminal_line_ending("one\n"), "one");
        assert_eq!(strip_terminal_line_ending("one\r\n"), "one");
        assert_eq!(strip_terminal_line_ending("one\n\n"), "one\n");
        assert_eq!(strip_terminal_line_ending("one"), "one");
    }

    #[test]
    fn assistant_footer_matches_the_live_cdp_geometry() {
        assert_eq!(RESPONSE_ACTION_FOOTER_OFFSET, 6.0);
        assert_eq!(RESPONSE_ACTION_FOOTER_HEIGHT, 20.0);
        assert_eq!(RESPONSE_ACTION_FOOTER_ELECTRON_SHIFT, -4.0);
        assert_eq!(RESPONSE_ACTION_GAP, 2.0);
        assert_eq!(RESPONSE_TIME_MARGIN, 6.0);
        assert_eq!(RESPONSE_ACTION_GAP + RESPONSE_TIME_MARGIN, 8.0);
        assert_eq!(RESPONSE_TIME_SIZE, 12.0);
        assert_eq!(RESPONSE_TIME_LINE_HEIGHT, 16.0);
    }

    #[test]
    fn user_message_footer_matches_the_live_cdp_geometry() {
        assert_eq!(USER_MESSAGE_FOOTER_OFFSET, 3.0);
        assert_eq!(USER_MESSAGE_FOOTER_HEIGHT, 26.0);
        assert_eq!(USER_MESSAGE_FOOTER_SIDE_MARGIN, 4.0);
        assert_eq!(USER_MESSAGE_FOOTER_GAP, 8.0);
        assert_eq!(USER_MESSAGE_TIME_SIZE, 12.0);
        assert_eq!(USER_MESSAGE_TIME_LINE_HEIGHT, 16.0);
    }

    #[test]
    fn user_message_copy_button_copies_the_submitted_prompt() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| HomeView::new(ThemeMode::Dark, cx),
        );

        window.update(|home, _, cx| home.submit_prompt_for_capture("clipboard prompt", cx));
        window.draw();

        // The 736px conversation column is centered in this 900px test
        // window. ChatGPT insets the trailing 26px action by 4px, so it
        // occupies x=788..814 and y=123..149.
        window.simulate_mouse_move(point(px(801.0), px(136.0)));
        window.simulate_click(point(px(801.0), px(136.0)), MouseButton::Left);

        assert_eq!(
            app.read_from_clipboard().and_then(|item| item.text()),
            Some("clipboard prompt".to_owned())
        );
    }

    #[test]
    fn approval_surface_owns_focus_and_drives_real_keyboard_events() {
        use crate::components::{
            approval::{ApprovalKeyboardFocus, ApprovalMenuItem, ApprovalVisualState},
            composer::ConversationActivity,
        };

        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| HomeView::new(ThemeMode::Dark, cx),
        );

        window.update(|home, _, cx| home.set_approval_for_capture("command", "default", cx));
        window.draw();
        window.update(|home, window, cx| {
            assert!(home.approval_focus.is_focused(window));
            assert!(home.composer.read(cx).conversation_render_snapshot().5.iter().any(
                |activity| matches!(activity, ConversationActivity::Approval(model) if model.should_render())
            ));
        });

        window.simulate_keystrokes("shift-tab enter tab");
        window.read(|home, cx| {
            let activities = home.composer.read(cx).conversation_render_snapshot().5;
            let model = activities
                .iter()
                .find_map(|activity| {
                    let ConversationActivity::Approval(model) = activity else {
                        return None;
                    };
                    Some(model)
                })
                .unwrap();
            assert_eq!(
                model.keyboard_focus,
                Some(ApprovalKeyboardFocus::MenuAllowOnce)
            );
            assert_eq!(
                model.visual_state,
                ApprovalVisualState::SplitMenu {
                    focused: Some(ApprovalMenuItem::AllowOnce)
                }
            );
        });

        window.simulate_keystrokes("shift-tab escape");
        window.read(|home, cx| {
            let activities = home.composer.read(cx).conversation_render_snapshot().5;
            let model = activities
                .iter()
                .find_map(|activity| {
                    let ConversationActivity::Approval(model) = activity else {
                        return None;
                    };
                    Some(model)
                })
                .unwrap();
            assert_eq!(
                model.keyboard_focus,
                Some(ApprovalKeyboardFocus::MenuToggle)
            );
            assert_eq!(model.visual_state, ApprovalVisualState::Default);
            assert!(model.should_render());
        });

        window.simulate_keystrokes("enter tab enter");
        window.read(|home, cx| {
            assert!(home.composer.read(cx).conversation_render_snapshot().5.iter().any(
                |activity| matches!(activity, ConversationActivity::Approval(model) if !model.should_render())
            ));
        });
        window.draw();
        window.update(|home, window, _| {
            assert!(!home.approval_focus.is_focused(window));
        });
    }

    #[test]
    fn command_approval_replaces_the_bottom_composer_and_rejects_by_mouse() {
        use crate::components::composer::ConversationActivity;

        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| HomeView::new(ThemeMode::Dark, cx),
        );

        window.update(|home, _, cx| home.set_approval_for_capture("command", "default", cx));
        window.draw();

        // The 736 px card is centered and pinned 16 px above the bottom.
        // Its reject button occupies the actions row around y=654 here. This
        // point was part of the Composer before the approval overlay moved.
        window.simulate_click(point(px(643.0), px(654.0)), MouseButton::Left);

        window.read(|home, cx| {
            assert!(home.composer.read(cx).conversation_render_snapshot().5.iter().any(
                |activity| matches!(activity, ConversationActivity::Approval(model) if !model.should_render())
            ));
        });
    }

    #[test]
    fn file_approval_surface_drives_focus_enter_and_escape() {
        use crate::components::{
            composer::ConversationActivity,
            file_change::{
                FileApprovalKeyboardFocus, FileApprovalMenuItem, FileApprovalVisualState,
            },
        };

        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| HomeView::new(ThemeMode::Dark, cx),
        );

        window.update(|home, _, cx| home.set_file_approval_for_capture("default", cx));
        window.draw();
        window.update(|home, window, _| assert!(home.approval_focus.is_focused(window)));

        // Shift-Tab focuses the split toggle, Enter opens it, and Tab moves
        // focus into the first evidence-backed menu row.
        window.simulate_keystrokes("shift-tab enter tab");
        window.read(|home, cx| {
            let activities = home.composer.read(cx).conversation_render_snapshot().5;
            let model = activities
                .iter()
                .find_map(|activity| {
                    let ConversationActivity::FileApproval(model) = activity else {
                        return None;
                    };
                    Some(model)
                })
                .unwrap();
            assert_eq!(
                model.keyboard_focus,
                Some(FileApprovalKeyboardFocus::MenuAllowOnce)
            );
            assert_eq!(
                model.visual_state,
                FileApprovalVisualState::SplitMenu {
                    focused: Some(FileApprovalMenuItem::AllowOnce)
                }
            );
        });

        window.simulate_keystroke("escape");
        window.read(|home, cx| {
            let activities = home.composer.read(cx).conversation_render_snapshot().5;
            let model = activities
                .iter()
                .find_map(|activity| {
                    let ConversationActivity::FileApproval(model) = activity else {
                        return None;
                    };
                    Some(model)
                })
                .unwrap();
            assert_eq!(
                model.keyboard_focus,
                Some(FileApprovalKeyboardFocus::MenuToggle)
            );
            assert_eq!(model.visual_state, FileApprovalVisualState::Default);
            assert!(model.should_render());
        });

        window.simulate_keystrokes("enter tab enter");
        window.read(|home, cx| {
            assert!(home.composer.read(cx).conversation_render_snapshot().5.iter().any(
                |activity| matches!(activity, ConversationActivity::FileApproval(model) if !model.should_render())
            ));
        });
        window.draw();
        window.update(|home, window, _| assert!(!home.approval_focus.is_focused(window)));

        window.update(|home, _, cx| home.set_file_approval_for_capture("default", cx));
        window.draw();
        window.simulate_keystroke("escape");
        window.read(|home, cx| {
            assert!(home.composer.read(cx).conversation_render_snapshot().5.iter().any(
                |activity| matches!(activity, ConversationActivity::FileApproval(model) if !model.should_render())
            ));
        });
    }

    #[test]
    fn permissions_approval_surface_drives_focus_menu_and_terminal_unmount() {
        use crate::components::{
            composer::ConversationActivity,
            permissions_approval::{
                PermissionApprovalKeyboardFocus, PermissionApprovalMenuItem,
                PermissionApprovalVisualState,
            },
        };

        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| HomeView::new(ThemeMode::Dark, cx),
        );

        window.update(|home, _, cx| {
            home.set_permissions_approval_for_capture("network", "default", cx)
        });
        window.draw();
        window.update(|home, window, _| assert!(home.approval_focus.is_focused(window)));

        // The pending permission card participates in the same native focus
        // loop as command and file approvals: Shift-Tab reaches the split
        // toggle, Enter opens it, and Tab advances into the first menu row.
        window.simulate_keystrokes("shift-tab enter tab");
        window.read(|home, cx| {
            let activities = home.composer.read(cx).conversation_render_snapshot().5;
            let model = activities
                .iter()
                .find_map(|activity| {
                    let ConversationActivity::PermissionsApproval(model) = activity else {
                        return None;
                    };
                    Some(model)
                })
                .expect("pending permissions approval");
            assert_eq!(
                model.keyboard_focus,
                Some(PermissionApprovalKeyboardFocus::MenuAllowOnce)
            );
            assert_eq!(
                model.visual_state,
                PermissionApprovalVisualState::Menu {
                    focused: Some(PermissionApprovalMenuItem::AllowOnce)
                }
            );
        });

        window.simulate_keystroke("escape");
        window.read(|home, cx| {
            let activities = home.composer.read(cx).conversation_render_snapshot().5;
            let model = activities
                .iter()
                .find_map(|activity| {
                    let ConversationActivity::PermissionsApproval(model) = activity else {
                        return None;
                    };
                    Some(model)
                })
                .expect("pending permissions approval");
            assert_eq!(
                model.keyboard_focus,
                Some(PermissionApprovalKeyboardFocus::MenuToggle)
            );
            assert_eq!(model.visual_state, PermissionApprovalVisualState::Default);
            assert!(model.should_render());
        });

        window.simulate_keystrokes("enter tab enter");
        window.read(|home, cx| {
            assert!(home.composer.read(cx).conversation_render_snapshot().5.iter().any(
                |activity| matches!(activity, ConversationActivity::PermissionsApproval(model) if !model.should_render())
            ));
        });
        window.draw();
        window.update(|home, window, _| assert!(!home.approval_focus.is_focused(window)));

        window.update(|home, _, cx| {
            home.set_permissions_approval_for_capture("network", "default", cx)
        });
        window.draw();
        window.simulate_keystroke("escape");
        window.read(|home, cx| {
            assert!(home.composer.read(cx).conversation_render_snapshot().5.iter().any(
                |activity| matches!(activity, ConversationActivity::PermissionsApproval(model) if !model.should_render())
            ));
        });
    }

    #[test]
    fn file_change_disclosure_event_toggles_the_inline_diff() {
        use crate::components::file_change::FileChangeActivityEvent;

        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| HomeView::new(ThemeMode::Dark, cx),
        );

        window.update(|home, _, cx| home.set_file_change_for_capture("completed", cx));
        let item_id = window.read(|home, cx| {
            home.composer
                .read(cx)
                .conversation_render_snapshot()
                .5
                .iter()
                .find_map(|activity| {
                    let ConversationActivity::FileChange(model) = activity else {
                        return None;
                    };
                    Some(model.item_id.clone())
                })
                .expect("completed fileChange activity")
        });

        window.update(|home, _, cx| {
            home.handle_file_change_activity_event(
                FileChangeActivityEvent::ToggleDetails {
                    item_id: item_id.clone(),
                },
                cx,
            )
        });
        assert!(window.read(|home, _| home.expanded_commands.contains(&item_id)));
        window.update(|home, _, cx| {
            home.handle_file_change_activity_event(
                FileChangeActivityEvent::ToggleDetails {
                    item_id: item_id.clone(),
                },
                cx,
            )
        });
        assert!(!window.read(|home, _| home.expanded_commands.contains(&item_id)));
    }

    #[test]
    fn user_input_other_is_a_native_editor_and_tab_returns_to_form_navigation() {
        let mut app = TestApp::new();
        app.update(|cx| {
            cx.bind_keys([KeyBinding::new("enter", Submit, Some("PromptInput"))]);
        });
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| HomeView::new(ThemeMode::Dark, cx),
        );

        window.update(|home, window, cx| {
            home.set_user_input_for_capture("other-focus", cx);
            let focus = home.composer.read(cx).user_input_other_focus_handle(cx);
            window.focus(&focus, cx);
        });
        window.draw();
        window.simulate_input("我想喝茶。");
        window.read(|home, cx| {
            let composer = home.composer.read(cx);
            assert_eq!(composer.user_input_other_entity().read(cx).text(), "我想喝茶。");
            assert!(composer.conversation_render_snapshot().5.iter().any(|activity| {
                matches!(activity, ConversationActivity::UserInput(model) if model.other_answer == "我想喝茶。")
            }));
        });

        // Enter is handled by the PromptInput EntityInputHandler/action, not
        // by the card's top-level KeyDown character concatenation.
        window.simulate_keystroke("enter");
        window.read(|home, cx| {
            assert!(home.composer.read(cx).conversation_render_snapshot().5.iter().any(
                |activity| matches!(activity, ConversationActivity::UserInput(model) if model.status == UserInputRequestStatus::Submitting)
            ));
        });

        window.update(|home, window, cx| {
            home.set_user_input_for_capture("other-focus", cx);
            let focus = home.composer.read(cx).user_input_other_focus_handle(cx);
            window.focus(&focus, cx);
        });
        window.draw();
        window.simulate_keystroke("tab");
        window.update(|home, window, cx| {
            assert!(home.approval_focus.is_focused(window));
            assert!(home.composer.read(cx).conversation_render_snapshot().5.iter().any(
                |activity| matches!(activity, ConversationActivity::UserInput(model) if model.keyboard_focus == Some(UserInputKeyboardFocus::Skip))
            ));
        });
    }
}
