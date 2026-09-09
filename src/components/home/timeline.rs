//! Timeline presentation and interaction for the conversation view.

#[cfg(feature = "screenshot")]
use super::mcp::mcp_tool_call_label;

use std::collections::HashSet;

use super::{
    context::CurrentTurnRows,
    mcp::{computer_use_surface_label, is_computer_use_call},
};
use crate::{
    agent::{
        AgentFileChangeStatus, AgentMcpToolCallStatus, CommandExecution, CommandExecutionAction,
        CommandExecutionStatus,
    },
    components::file_change::DiffReviewPresentation,
    conversation::{
        ConversationActivity, ConversationPhase, ConversationTranscriptTurn,
        ReasoningActivityPresentation, ResumedTurnPresentation,
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ToolActivityGroupPresentation {
    pub(super) id: String,
    pub(super) reasoning: Vec<ReasoningActivityPresentation>,
    pub(super) activities: Vec<ConversationActivity>,
    pub(super) commands: Vec<CommandExecution>,
    pub(super) file_changes: Vec<crate::components::file_change::FileChangeActivityPresentation>,
}

impl ToolActivityGroupPresentation {
    pub(super) fn is_active(&self) -> bool {
        self.activities.iter().any(|activity| matches!(activity,
            ConversationActivity::AutoApprovalReview(review) if review.status() == crate::agent::AgentAutoApprovalReviewStatus::InProgress))
            || self.activities.iter().any(|activity| matches!(activity, ConversationActivity::WebSearch(call) if call.status == crate::agent::AgentActivityStatus::InProgress))
            || self.activities.iter().any(|activity| matches!(activity,
            ConversationActivity::McpToolCall(call) if call.status == AgentMcpToolCallStatus::InProgress))
            || self.reasoning
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
pub(super) enum ActivityStreamUnit {
    Standalone(ConversationActivity),
    ToolGroup(ToolActivityGroupPresentation),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ConversationListRow {
    FileSummary(DiffReviewPresentation),
    ResumedWork {
        id: String,
        label: String,
        expanded: bool,
    },
    HistoricalUser {
        turn_index: usize,
        message: String,
        images: Vec<crate::agent::UserMessageImage>,
        time: Option<String>,
    },
    CurrentUser {
        message: String,
        images: Vec<crate::agent::UserMessageImage>,
        time: String,
    },
    AssistantMarkdown {
        id: String,
        text: String,
    },
    Activity {
        unit: ActivityStreamUnit,
        show_thinking_tail: bool,
    },
    Thinking,
    CurrentResponseFooter {
        id: String,
        message: String,
        completed_at: Option<String>,
    },
}

#[derive(Default)]
pub(super) struct PendingToolActivityGroup {
    pub(super) id: Option<String>,
    pub(super) reasoning: Vec<ReasoningActivityPresentation>,
    pub(super) activities: Vec<ConversationActivity>,
    pub(super) commands: Vec<CommandExecution>,
    pub(super) file_changes: Vec<crate::components::file_change::FileChangeActivityPresentation>,
}

pub(super) fn flush_pending_tool_activity_group(
    pending: &mut PendingToolActivityGroup,
    units: &mut Vec<ActivityStreamUnit>,
) {
    if pending.activities.is_empty() {
        // ChatGPT does not render completed reasoning as an independent
        // "思考了 …" row. It is presentation context for an adjacent tool
        // block and remains invisible when no command belongs to the group.
        pending.reasoning.clear();
        pending.id = None;
        return;
    }

    if let [ConversationActivity::WebSearch(search)] = pending.activities.as_slice() {
        units.push(ActivityStreamUnit::Standalone(
            ConversationActivity::WebSearch(search.clone()),
        ));
        pending.activities.clear();
        pending.reasoning.clear();
        pending.id = None;
        return;
    }
    let id = pending.id.take().expect("a populated tool group has an id");
    units.push(ActivityStreamUnit::ToolGroup(
        ToolActivityGroupPresentation {
            id,
            reasoning: std::mem::take(&mut pending.reasoning),
            activities: std::mem::take(&mut pending.activities),
            commands: std::mem::take(&mut pending.commands),
            file_changes: std::mem::take(&mut pending.file_changes),
        },
    ));
}

pub(super) fn activity_stream_units(
    activities: &[ConversationActivity],
) -> Vec<ActivityStreamUnit> {
    let target_id = |a: &ConversationActivity| match a {
        ConversationActivity::Command(c) => Some(c.id.clone()),
        ConversationActivity::FileChange(c) => Some(c.item_id.clone()),
        ConversationActivity::McpToolCall(c) => Some(c.id.clone()),
        _ => None,
    };
    let mut attached = std::collections::HashMap::<String, Vec<ConversationActivity>>::new();
    let mut attached_keys = HashSet::new();
    for activity in activities {
        let ConversationActivity::AutoApprovalReview(review) = activity else {
            continue;
        };
        if review.status() == crate::agent::AgentAutoApprovalReviewStatus::Approved {
            continue;
        }
        let Some(target) = review.review.target_item_id.as_ref() else {
            continue;
        };
        if activities.iter().any(|a| {
            target_id(a).as_ref() == Some(target)
                && !(matches!(a, ConversationActivity::McpToolCall(_))
                    && review.status() == crate::agent::AgentAutoApprovalReviewStatus::Denied)
        }) {
            let mut model = review.clone();
            model.attached_to_item = true;
            attached
                .entry(target.clone())
                .or_default()
                .push(ConversationActivity::AutoApprovalReview(model));
            attached_keys.insert(review.review.key.clone());
        }
    }
    let activities = activities
        .iter()
        .flat_map(|activity| {
            if let ConversationActivity::AutoApprovalReview(review) = activity
                && (review.status() == crate::agent::AgentAutoApprovalReviewStatus::Approved
                    || attached_keys.contains(&review.review.key))
            {
                return Vec::new();
            }
            let mut result = vec![activity.clone()];
            if let Some(id) = target_id(activity) {
                result.extend(attached.remove(&id).unwrap_or_default());
            }
            result
        })
        .collect::<Vec<_>>();
    let mut units = Vec::new();
    let mut pending = PendingToolActivityGroup::default();
    let mut active_reasoning = Vec::new();
    let mut proposed_plan = None;

    for activity in &activities {
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
            ConversationActivity::TurnPlan(_) => {}
            ConversationActivity::Plan(plan) => {
                proposed_plan = Some(plan.clone());
            }
            ConversationActivity::Reasoning(reasoning) => {
                pending.id.get_or_insert_with(|| reasoning.item_id.clone());
                pending.reasoning.push(reasoning.clone());
            }
            ConversationActivity::Command(command) => {
                pending.id.get_or_insert_with(|| command.id.clone());
                pending.commands.push(command.clone());
                pending.activities.push(activity.clone());
            }
            ConversationActivity::FileChange(change) => {
                pending.id.get_or_insert_with(|| change.item_id.clone());
                pending.file_changes.push(change.clone());
                pending.activities.push(activity.clone());
            }
            ConversationActivity::WebSearch(crate::agent::AgentWebSearch {
                id: item_id, ..
            }) => {
                pending.id.get_or_insert_with(|| item_id.clone());
                pending.activities.push(activity.clone());
            }
            ConversationActivity::McpToolCall(call) if is_computer_use_call(call) => {
                pending.id.get_or_insert_with(|| call.id.clone());
                pending.activities.push(activity.clone());
            }
            ConversationActivity::AutoApprovalReview(review)
                if review.attached_to_item && !pending.activities.is_empty() =>
            {
                pending.activities.push(activity.clone());
            }
            standalone => {
                flush_pending_tool_activity_group(&mut pending, &mut units);
                units.push(ActivityStreamUnit::Standalone(standalone.clone()));
            }
        }
    }
    flush_pending_tool_activity_group(&mut pending, &mut units);
    if let Some(plan) = proposed_plan {
        units.push(ActivityStreamUnit::Standalone(ConversationActivity::Plan(
            plan,
        )));
    }
    let mut merged = Vec::new();
    for unit in units {
        if let ActivityStreamUnit::Standalone(ConversationActivity::ImageView(image)) = unit {
            match merged.last_mut() {
                Some(ActivityStreamUnit::Standalone(ConversationActivity::ImageView(first))) => {
                    let images = vec![first.clone(), image];
                    *merged.last_mut().unwrap() =
                        ActivityStreamUnit::Standalone(ConversationActivity::ImageViews(images));
                }
                Some(ActivityStreamUnit::Standalone(ConversationActivity::ImageViews(images))) => {
                    images.push(image)
                }
                _ => merged.push(ActivityStreamUnit::Standalone(
                    ConversationActivity::ImageView(image),
                )),
            }
        } else {
            merged.push(unit);
        }
    }
    let mut units = merged;
    units.extend(active_reasoning.into_iter().map(|reasoning| {
        ActivityStreamUnit::Standalone(ConversationActivity::Reasoning(reasoning))
    }));
    units
}

pub(super) fn reasoning_activity_title(
    reasoning: &ReasoningActivityPresentation,
) -> Option<String> {
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

pub(super) fn tool_group_reasoning_title(group: &ToolActivityGroupPresentation) -> Option<String> {
    group
        .reasoning
        .iter()
        .rev()
        .find_map(reasoning_activity_title)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CommandActivitySummary {
    pub(super) icon: &'static str,
    pub(super) text: String,
    pub(super) reads_files: bool,
    pub(super) runs_command: bool,
}

pub(super) fn command_activity_summary(command: &CommandExecution) -> CommandActivitySummary {
    command_activity_summaries(command)
        .into_iter()
        .next()
        .expect("every command execution has at least one presentation row")
}

pub(super) fn command_activity_summaries(
    command: &CommandExecution,
) -> Vec<CommandActivitySummary> {
    if command.actions.is_empty() {
        return vec![generic_command_activity_summary(command, &command.command)];
    }
    command
        .actions
        .iter()
        .map(|action| command_action_summary(command, action))
        .collect()
}

pub(super) fn command_action_summary(
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

pub(super) fn generic_command_activity_summary(
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
        CommandExecutionStatus::Failed => format!("已运行 {display_command}"),
    };
    CommandActivitySummary {
        icon: "panel-terminal",
        text,
        reads_files: false,
        runs_command: true,
    }
}

pub(super) fn completed_tool_group_summary(
    group: &ToolActivityGroupPresentation,
) -> CommandActivitySummary {
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
    let mut surfaces = group
        .activities
        .iter()
        .filter_map(|activity| match activity {
            ConversationActivity::McpToolCall(call) => computer_use_surface_label(call),
            _ => None,
        })
        .collect::<Vec<_>>();
    surfaces.sort_by_key(|name| name.to_lowercase());
    surfaces.dedup();
    let uses_computer = !surfaces.is_empty();
    let searches_web = group
        .activities
        .iter()
        .any(|a| matches!(a, ConversationActivity::WebSearch(_)));
    let text = if uses_computer {
        let suffix = if surfaces.iter().any(|s| s == "浏览器") {
            ""
        } else {
            " 集成"
        };
        let operations = if text == "已工作" {
            String::new()
        } else {
            text.strip_prefix("已")
                .unwrap_or(text)
                .replace("编辑了文件", "编辑了多个文件")
        };
        format!("已使用 {}{suffix}{operations}", surfaces.join("和"))
    } else {
        text.to_owned()
    };
    CommandActivitySummary {
        icon: if uses_computer {
            if surfaces.iter().any(|s| s == "浏览器") {
                "activity-computer-use"
            } else {
                "activity-native-app"
            }
        } else if edits_files {
            "message-edit"
        } else if reads_files {
            "activity-read"
        } else {
            "panel-terminal"
        },
        text: if searches_web {
            format!("{}已搜索网页", if text == "已工作" { "" } else { &text })
        } else {
            text
        },
        reads_files,
        runs_command,
    }
}

pub(super) fn command_activity_row_count(command: &CommandExecution) -> usize {
    command.actions.len().max(1)
}

pub(super) fn tool_group_row_count(group: &ToolActivityGroupPresentation) -> usize {
    group
        .commands
        .iter()
        .map(command_activity_row_count)
        .sum::<usize>()
        + group
            .file_changes
            .iter()
            .map(|change| change.review.files.len())
            .sum::<usize>()
        + group
            .activities
            .iter()
            .filter(|a| {
                matches!(
                    a,
                    ConversationActivity::McpToolCall(_) | ConversationActivity::WebSearch(_)
                )
            })
            .count()
}

pub(super) fn strip_terminal_line_ending(output: &str) -> &str {
    output
        .strip_suffix("\r\n")
        .or_else(|| output.strip_suffix('\n'))
        .unwrap_or(output)
}

pub(super) fn conversation_list_rows(
    transcript: Vec<ConversationTranscriptTurn>,
    current: CurrentTurnRows<'_>,
    expanded_resumed_turns: &HashSet<String>,
) -> Vec<ConversationListRow> {
    let CurrentTurnRows {
        phase,
        user_message,
        user_images,
        user_message_time,
        assistant_message,
        assistant_message_time,
        conversation_activity,
        resumed_turn,
    } = current;
    let has_active_reasoning = conversation_activity.iter().any(|activity| {
        matches!(activity, ConversationActivity::Reasoning(reasoning) if reasoning.is_active())
    });
    let show_thinking_tail = conversation_status(phase).is_some() && !has_active_reasoning;
    let mut rows = Vec::new();
    for (turn_index, turn) in transcript.into_iter().enumerate() {
        if !turn.user_message.is_empty() || !turn.user_images.is_empty() {
            rows.push(ConversationListRow::HistoricalUser {
                turn_index,
                message: turn.user_message,
                images: turn.user_images,
                time: turn.user_message_time,
            });
        }
        if turn.activities.is_empty() {
            if !turn.assistant_message.is_empty() {
                rows.push(ConversationListRow::AssistantMarkdown {
                    id: format!("historical-assistant-{turn_index}"),
                    text: turn.assistant_message.clone(),
                });
            }
        } else {
            let turn_show_thinking = conversation_status(turn.phase).is_some();
            append_turn_activity_rows(
                &mut rows,
                &turn.activities,
                turn_show_thinking,
                turn.phase,
                turn.resumed.as_ref(),
                expanded_resumed_turns,
            );
        }
        if turn.resumed.is_some()
            && matches!(
                turn.phase,
                ConversationPhase::Complete | ConversationPhase::Failed
            )
            && !turn.assistant_message.is_empty()
        {
            if let Some(review) = resumed_file_summary(&turn.activities, turn.resumed.as_ref()) {
                rows.push(ConversationListRow::FileSummary(review));
            } else {
                rows.push(ConversationListRow::CurrentResponseFooter {
                    id: turn
                        .resumed
                        .as_ref()
                        .map(|t| t.id.clone())
                        .unwrap_or_else(|| format!("historical-{turn_index}")),
                    message: turn.assistant_message,
                    completed_at: turn.assistant_message_time,
                });
            }
        }
    }
    if !user_message.is_empty() || !user_images.is_empty() || phase != ConversationPhase::Empty {
        rows.push(ConversationListRow::CurrentUser {
            message: user_message,
            images: user_images,
            time: user_message_time,
        });
    }
    if conversation_activity.is_empty() {
        if !assistant_message.is_empty() {
            rows.push(ConversationListRow::AssistantMarkdown {
                id: "current-assistant".to_owned(),
                text: assistant_message.clone(),
            });
        }
    } else {
        append_turn_activity_rows(
            &mut rows,
            conversation_activity,
            show_thinking_tail,
            phase,
            resumed_turn.as_ref(),
            expanded_resumed_turns,
        );
    }
    if show_thinking_tail {
        rows.push(ConversationListRow::Thinking);
    }
    if matches!(
        phase,
        ConversationPhase::Complete | ConversationPhase::Failed
    ) && !assistant_message.is_empty()
    {
        if let Some(review) = resumed_file_summary(conversation_activity, resumed_turn.as_ref()) {
            rows.push(ConversationListRow::FileSummary(review));
        } else {
            rows.push(ConversationListRow::CurrentResponseFooter {
                id: resumed_turn
                    .as_ref()
                    .map(|t| t.id.clone())
                    .unwrap_or_else(|| "current".to_owned()),
                message: assistant_message,
                completed_at: assistant_message_time,
            });
        }
    }
    rows
}

// Rollout file edits retain their original patch order. Combine repeated paths
// for the turn summary while retaining every patch's lines for review.
pub(super) fn resumed_file_summary(
    activities: &[ConversationActivity],
    turn: Option<&ResumedTurnPresentation>,
) -> Option<DiffReviewPresentation> {
    let turn = turn?;
    let mut files: Vec<crate::components::file_change::DiffFilePresentation> = Vec::new();
    for activity in activities {
        if let ConversationActivity::FileChange(change) = activity
            && change.status == AgentFileChangeStatus::Completed
        {
            for file in &change.review.files {
                if let Some(existing) = files.iter_mut().find(|entry| entry.path == file.path) {
                    existing.additions += file.additions;
                    existing.deletions += file.deletions;
                    existing.lines.extend(file.lines.clone());
                } else {
                    files.push(file.clone());
                }
            }
        }
    }
    (!files.is_empty()).then(|| {
        let mut review =
            DiffReviewPresentation::new(format!("resumed-summary-{}", turn.id), "本轮更改", files);
        let raw = activities
            .iter()
            .filter_map(|a| {
                if let ConversationActivity::FileChange(c) = a {
                    c.review.raw_diff.as_deref()
                } else {
                    None
                }
            })
            .collect::<String>();
        review.raw_diff = (!raw.is_empty()).then_some(raw);
        review
    })
}

pub(super) fn resumed_work_label(duration_ms: Option<i64>) -> String {
    match duration_ms.filter(|duration| *duration >= 0) {
        Some(ms) => {
            let seconds = ms / 1000;
            if seconds >= 3600 {
                format!(
                    "用时 {}小时 {}分钟 {}秒",
                    seconds / 3600,
                    seconds / 60 % 60,
                    seconds % 60
                )
            } else if seconds >= 60 {
                format!("用时 {}分钟 {}秒", seconds / 60, seconds % 60)
            } else {
                format!("用时 {seconds}秒")
            }
        }
        None => "工作过程".to_owned(),
    }
}

pub(super) fn append_turn_activity_rows(
    rows: &mut Vec<ConversationListRow>,
    activities: &[ConversationActivity],
    show_thinking_tail: bool,
    phase: ConversationPhase,
    resumed: Option<&ResumedTurnPresentation>,
    expanded_turns: &HashSet<String>,
) {
    let units = activity_stream_units(activities);
    let final_start = resumed.and_then(|turn| units.iter().position(|unit| {
        matches!(unit, ActivityStreamUnit::Standalone(ConversationActivity::AssistantMessage { item_id, .. })
            if turn.final_message_ids.contains(item_id))
    }));
    // Keep errors, interrupted turns, approvals and unfinished work visible.
    // Only a completed prefix preceding an identified answer is collapsible.
    if let (ConversationPhase::Complete, Some(turn), Some(final_start)) =
        (phase, resumed, final_start)
        && final_start > 0
    {
        let expanded = expanded_turns.contains(&turn.id);
        rows.push(ConversationListRow::ResumedWork {
            id: turn.id.clone(),
            label: resumed_work_label(turn.duration_ms),
            expanded,
        });
        rows.extend(units.into_iter().enumerate().filter_map(|(index, unit)| {
            (expanded
                || index >= final_start
                || matches!(
                    &unit,
                    ActivityStreamUnit::Standalone(ConversationActivity::QuestionReply { .. })
                ))
            .then_some(ConversationListRow::Activity {
                unit,
                show_thinking_tail,
            })
        }));
    } else {
        rows.extend(units.into_iter().map(|unit| ConversationListRow::Activity {
            unit,
            show_thinking_tail,
        }));
    }
}

pub(super) fn conversation_status(phase: ConversationPhase) -> Option<&'static str> {
    match phase {
        ConversationPhase::Thinking => Some("正在思考"),
        _ => None,
    }
}

/// Semantic companion to resumed-thread screenshots. Uses the same grouping
/// function as the live view; no rollout-file access or alternate renderer.
#[cfg(feature = "screenshot")]
pub(crate) fn resumed_activity_audit(activities: &[ConversationActivity]) -> serde_json::Value {
    fn item(activity: &ConversationActivity) -> serde_json::Value {
        use serde_json::json;
        match activity {
            ConversationActivity::Command(c) => {
                json!({"type":"command","id":c.id,"labels":command_activity_summaries(c).iter().map(|s|s.text.clone()).collect::<Vec<_>>()})
            }
            ConversationActivity::McpToolCall(c) => {
                json!({"type":"mcp","id":c.id,"label":mcp_tool_call_label(c)})
            }
            ConversationActivity::FileChange(c) => {
                json!({"type":"fileChange","id":c.item_id,"files":c.review.files.iter().map(|f|&f.path).collect::<Vec<_>>()})
            }
            ConversationActivity::ImageView(i) => json!({"type":"imageView","id":i.id}),
            ConversationActivity::ImageViews(images) => {
                json!({"type":"imageViews","ids":images.iter().map(|i|&i.id).collect::<Vec<_>>()})
            }
            ConversationActivity::AssistantMessage { item_id, text } => {
                json!({"type":"assistant","id":item_id,"text":text})
            }
            ConversationActivity::QuestionReply {
                item_id,
                question,
                answer,
            } => json!({"type":"questionReply","id":item_id,"question":question,"answer":answer}),
            ConversationActivity::WebSearch(crate::agent::AgentWebSearch {
                id: item_id,
                query,
                ..
            }) => {
                json!({"type":"webSearch","id":item_id,"query":query})
            }
            ConversationActivity::ContextCompaction(c) => {
                json!({"type":"contextCompaction","id":c.id})
            }
            other => json!({"type":"other","description":format!("{other:?}")}),
        }
    }
    serde_json::Value::Array(activity_stream_units(activities).iter().map(|unit| match unit {
        ActivityStreamUnit::ToolGroup(g) => serde_json::json!({"type":"toolGroup","label":completed_tool_group_summary(g).text,"items":g.activities.iter().map(item).collect::<Vec<_>>()}),
        ActivityStreamUnit::Standalone(a) => item(a),
    }).collect())
}
