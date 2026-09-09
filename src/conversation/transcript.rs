//! Restored transcript normalization and current-turn snapshots.

use std::path::PathBuf;

use chrono::{Datelike, Local};

use super::{
    activity::{
        ConversationActivity, ReasoningActivityPresentation, upsert_collaboration_activity,
    },
    state::ConversationState,
};
use crate::{
    agent::{
        AgentImageGenerationStatus, CommandExecution, HistoryTurnStatus, ProjectId, ThreadHistory,
        ThreadHistoryItem, normalize_user_message_for_display,
    },
    components::file_change::FileChangeActivityPresentation,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ConversationPhase {
    #[default]
    Empty,
    Starting,
    Thinking,
    Streaming,
    Stopping,
    Complete,
    Stopped,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConversationTranscriptTurn {
    pub phase: ConversationPhase,
    pub user_message: String,
    pub user_images: Vec<crate::agent::UserMessageAttachment>,
    pub user_message_time: Option<String>,
    pub assistant_message: String,
    pub assistant_message_time: Option<String>,
    pub activities: Vec<ConversationActivity>,
    pub resumed: Option<ResumedTurnPresentation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResumedTurnPresentation {
    pub id: String,
    pub duration_ms: Option<i64>,
    pub final_message_ids: Vec<String>,
}

pub(crate) fn resumed_question_replies(text: &str) -> Option<Vec<(String, String)>> {
    let json = text
        .trim()
        .strip_prefix("<send_user_message_question_reply>")?
        .trim()
        .strip_suffix("</send_user_message_question_reply>")?
        .trim();
    let values: Vec<serde_json::Value> = serde_json::from_str(json).ok()?;
    values
        .iter()
        .map(|v| {
            Some((
                v.get("question")?.as_str()?.to_owned(),
                v.get("answer")?.as_str()?.to_owned(),
            ))
        })
        .collect()
}

pub(crate) fn resumed_final_message_ids(items: &[ThreadHistoryItem]) -> Vec<String> {
    // A clarification can itself be marked final_answer within a continued
    // persisted turn. The last final answer terminates the work disclosure;
    // using the first one exposes all later tool activity by default.
    let explicit: Vec<_> = items
        .iter()
        .rev()
        .filter_map(|item| match item {
            ThreadHistoryItem::AssistantMessage {
                item_id,
                phase: Some(phase),
                ..
            } if phase == "final_answer" => Some(item_id.clone()),
            _ => None,
        })
        .take(1)
        .collect();
    if !explicit.is_empty() {
        return explicit;
    }
    // Older rollouts predate `phase`. Only their last assistant item can be
    // treated as a final answer; earlier commentary remains in the disclosure.
    items
        .iter()
        .rev()
        .find_map(|item| match item {
            ThreadHistoryItem::AssistantMessage {
                item_id,
                phase: None,
                ..
            } => Some(vec![item_id.clone()]),
            _ => None,
        })
        .unwrap_or_default()
}

pub(crate) fn current_local_time_label() -> String {
    Local::now().format("%H:%M").to_string()
}

pub(crate) fn history_time_label(timestamp: Option<i64>) -> Option<String> {
    let timestamp = timestamp?;
    let date = chrono::DateTime::from_timestamp(timestamp, 0)?.with_timezone(&Local);
    let time = date.format("%H:%M");
    if date.date_naive() == Local::now().date_naive() {
        Some(time.to_string())
    } else {
        let weekday = [
            "星期一",
            "星期二",
            "星期三",
            "星期四",
            "星期五",
            "星期六",
            "星期日",
        ][date.weekday().num_days_from_monday() as usize];
        Some(format!("{weekday}{time}"))
    }
}

impl ConversationState {
    #[cfg(test)]
    pub(crate) fn conversation_snapshot(
        &self,
    ) -> (
        ConversationPhase,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
    ) {
        (
            self.phase,
            self.user_message.clone(),
            self.user_message_time.clone(),
            self.assistant_message.clone(),
            self.assistant_message_time.clone(),
        )
    }
    #[cfg(test)]
    pub(crate) fn activity_snapshot(&self) -> Vec<ConversationActivity> {
        self.activities.clone()
    }
    pub(crate) fn phase(&self) -> ConversationPhase {
        self.phase
    }
    pub(crate) fn has_active_context_compaction(&self) -> bool {
        self.activities.iter().any(|activity| {
            matches!(
                activity,
                ConversationActivity::ContextCompaction(compaction) if !compaction.completed
            )
        })
    }
    pub(crate) fn has_active_image_generation(&self) -> bool {
        self.activities.iter().any(|activity| {
            matches!(
                activity,
                ConversationActivity::ImageGeneration(image)
                    if image.status == AgentImageGenerationStatus::InProgress
            )
        })
    }
    pub(crate) fn transcript_render_snapshot(&self) -> Vec<ConversationTranscriptTurn> {
        self.transcript.clone()
    }
    pub(crate) fn thread_id(&self) -> Option<&str> {
        self.thread_id.as_deref()
    }
    pub(crate) fn history_needs_retry(&self) -> bool {
        self.history_error.is_some()
    }
    #[cfg(feature = "screenshot")]
    pub(crate) fn history_loading(&self) -> bool {
        self.history_loading
    }
    #[cfg(feature = "screenshot")]
    pub(crate) fn history_error(&self) -> Option<&str> {
        self.history_error.as_deref()
    }
    pub(crate) fn set_workspace_context(
        &mut self,
        cwd: PathBuf,
        project_id: Option<ProjectId>,
        thread_id: Option<String>,
    ) {
        self.cwd = cwd;
        self.project_id = project_id;
        self.thread_id = thread_id;
    }
    pub(crate) fn set_history_loading(&mut self, loading: bool) {
        self.history_loading = loading;
        self.history_error = None;
        if loading && self.user_message.is_none() && self.transcript.is_empty() {
            self.phase = ConversationPhase::Starting;
        }
    }
    pub(crate) fn set_history_error(&mut self, error: String) {
        self.history_loading = false;
        self.history_error = Some(error.clone());
        self.user_message = Some("无法加载聊天历史".to_owned());
        self.user_message_time = None;
        self.assistant_message = error.clone();
        self.assistant_message_time = None;
        self.activities = vec![ConversationActivity::Error { message: error }];
        self.phase = ConversationPhase::Failed;
    }
    pub(crate) fn hydrate_history(&mut self, history: ThreadHistory) {
        self.reconcile_history_submissions(&history);
        if self.active_turn.is_none() {
            self.turn_identity = None;
        }
        self.thread_id = Some(history.thread.thread_id.clone());
        self.cwd = history.thread.cwd.clone();
        self.project_id = history.thread.project_id.clone();
        self.history_loading = false;
        self.history_error = None;
        self.transcript = history
            .turns
            .iter()
            .map(|turn| {
                let mut seen_users = std::collections::HashSet::new();
                let mut seen_clients = std::collections::HashSet::new();
                let mut user_messages = Vec::new();
                let mut user_images = Vec::new();
                let mut assistant_messages = Vec::new();
                let mut activities = Vec::new();
                for item in &turn.items {
                    match item {
                        ThreadHistoryItem::UserMessage {
                            item_id,
                            client_message_id,
                            text,
                            images,
                        } => {
                            if !seen_users.insert(item_id.clone()) || client_message_id.as_ref().is_some_and(|id| !seen_clients.insert(id.clone())) { continue; }
                            if let Some(replies) = resumed_question_replies(text) {
                                activities.extend(replies.into_iter().map(|(question, answer)| {
                                    ConversationActivity::QuestionReply {
                                        item_id: item_id.clone(),
                                        question,
                                        answer,
                                    }
                                }));
                            } else if user_messages.is_empty() {
                                user_messages.push(normalize_user_message_for_display(text));
                                user_images.extend(images.iter().cloned());
                            } else if !activities.iter().any(|activity| matches!(activity, ConversationActivity::UserMessage { item_id: id, .. } if id == item_id)) {
                                activities.push(ConversationActivity::UserMessage {
                                    item_id: item_id.clone(), text: normalize_user_message_for_display(text), images: images.clone(),
                                });
                            }
                        }
                        ThreadHistoryItem::AssistantMessage { item_id, text, .. } => {
                            assistant_messages.push(text.clone());
                            activities.push(ConversationActivity::AssistantMessage {
                                item_id: item_id.clone(),
                                text: text.clone(),
                            });
                        }
                        ThreadHistoryItem::Reasoning {
                            item_id,
                            summary,
                            content,
                        } => activities.push(ConversationActivity::Reasoning(
                            ReasoningActivityPresentation {
                                item_id: item_id.clone(),
                                summary: summary.clone(),
                                content: content.clone(),
                                started_at_ms: turn.started_at.unwrap_or_default(),
                                completed_at_ms: Some(
                                    turn.completed_at
                                        .unwrap_or_else(|| turn.started_at.unwrap_or_default()),
                                ),
                            },
                        )),
                        ThreadHistoryItem::Command {
                            item_id,
                            command,
                            output,
                            status,
                            actions,
                            cwd,
                            exit_code,
                        } => activities.push(ConversationActivity::Command(CommandExecution {
                            id: item_id.clone(),
                            command: command.clone(),
                            actions: actions.clone(),
                            cwd: cwd
                                .clone()
                                .unwrap_or_else(|| history.thread.cwd.display().to_string()),
                            output: output.clone(),
                            terminal_process_id: None,
                            status: *status,
                            exit_code: *exit_code,
                        })),
                        ThreadHistoryItem::FileChange(change) => {
                            activities.push(ConversationActivity::FileChange(
                                FileChangeActivityPresentation::from_agent_change(
                                    change,
                                    "上一轮",
                                    Some(&history.thread.cwd),
                                ),
                            ));
                        }
                        ThreadHistoryItem::ImageView(image) => {
                            activities.push(ConversationActivity::ImageView(image.clone()));
                        }
                        ThreadHistoryItem::ImageGeneration(image) => {
                            if image.status != AgentImageGenerationStatus::InProgress
                                || turn.status == HistoryTurnStatus::InProgress
                            {
                                activities
                                    .push(ConversationActivity::ImageGeneration(image.clone()));
                            }
                        }
                        ThreadHistoryItem::ContextCompaction(compaction) => {
                            activities
                                .push(ConversationActivity::ContextCompaction(compaction.clone()));
                        }
                        ThreadHistoryItem::Collaboration(collaboration) => {
                            upsert_collaboration_activity(&mut activities, collaboration.clone());
                        }
                        ThreadHistoryItem::McpToolCall(tool_call) => {
                            activities.push(ConversationActivity::McpToolCall(tool_call.clone()));
                        }
                        ThreadHistoryItem::WebSearch {
                            item_id,
                            query,
                            results,
                            ..
                        } => {
                            activities.push(ConversationActivity::WebSearch {
                                item_id: item_id.clone(),
                                query: query.clone(),
                                results: results.clone(),
                            });
                        }
                        ThreadHistoryItem::Unsupported { kind, .. } => {
                            activities.push(ConversationActivity::Warning {
                                message: format!("历史包含当前 UI 尚未呈现的 {kind} 项"),
                            });
                        }
                    }
                }
                if let Some(error) = &turn.error {
                    activities.push(ConversationActivity::Error {
                        message: error.clone(),
                    });
                }
                let final_message_ids = resumed_final_message_ids(&turn.items);
                let final_messages: Vec<_> = turn
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        ThreadHistoryItem::AssistantMessage { item_id, text, .. }
                            if final_message_ids.contains(item_id) =>
                        {
                            Some(text.as_str())
                        }
                        _ => None,
                    })
                    .collect();
                ConversationTranscriptTurn {
                    phase: match turn.status {
                        HistoryTurnStatus::InProgress => ConversationPhase::Streaming,
                        HistoryTurnStatus::Completed => ConversationPhase::Complete,
                        HistoryTurnStatus::Interrupted => ConversationPhase::Stopped,
                        HistoryTurnStatus::Failed => ConversationPhase::Failed,
                    },
                    user_message: user_messages.join("\n\n"),
                    user_images,
                    user_message_time: history_time_label(turn.started_at),
                    assistant_message: if final_messages.is_empty() {
                        assistant_messages.join("\n\n")
                    } else {
                        final_messages.join("\n\n")
                    },
                    assistant_message_time: history_time_label(turn.completed_at),
                    activities,
                    resumed: Some(ResumedTurnPresentation {
                        id: turn.turn_id.clone(),
                        duration_ms: turn.duration_ms,
                        final_message_ids,
                    }),
                }
            })
            .collect();
        if let Some(last) = self.transcript.pop() {
            self.phase = last.phase;
            self.user_message = Some(last.user_message);
            self.user_images = last.user_images;
            self.user_message_time = last.user_message_time;
            self.assistant_message = last.assistant_message;
            self.assistant_message_time = last.assistant_message_time;
            self.activities = last.activities;
            self.resumed_turn = last.resumed;
        } else {
            self.phase = ConversationPhase::Empty;
            self.user_message = None;
            self.user_images.clear();
            self.user_message_time = None;
            self.assistant_message.clear();
            self.assistant_message_time = None;
            self.activities.clear();
            self.resumed_turn = None;
        }
    }
    pub(crate) fn commit_current_turn(&mut self) {
        let Some(user_message) = self.user_message.take() else {
            return;
        };
        self.transcript.push(ConversationTranscriptTurn {
            phase: self.phase,
            user_message,
            user_images: std::mem::take(&mut self.user_images),
            user_message_time: self.user_message_time.take(),
            assistant_message: std::mem::take(&mut self.assistant_message),
            assistant_message_time: self.assistant_message_time.take(),
            activities: std::mem::take(&mut self.activities),
            resumed: self.resumed_turn.take(),
        });
    }
    pub(crate) fn user_images(&self) -> Vec<crate::agent::UserMessageAttachment> {
        self.user_images.clone()
    }
    pub(crate) fn resumed_turn(&self) -> Option<ResumedTurnPresentation> {
        self.resumed_turn.clone()
    }
    pub(crate) fn conversation_render_snapshot(
        &self,
    ) -> (
        ConversationPhase,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        Vec<ConversationActivity>,
    ) {
        // While activities are present they are the render source of truth.
        // Avoid cloning the same growing assistant response a second time on
        // every streaming frame; the aggregate is only needed for fallback
        // rendering and the completed response actions.
        let assistant_message = if self.activities.is_empty()
            || matches!(
                self.phase,
                ConversationPhase::Complete
                    | ConversationPhase::Stopped
                    | ConversationPhase::Failed
            ) {
            self.assistant_message.clone()
        } else {
            String::new()
        };

        (
            self.phase,
            self.user_message.clone(),
            self.user_message_time.clone(),
            assistant_message,
            self.assistant_message_time.clone(),
            self.activities.clone(),
        )
    }
}

#[cfg(test)]
mod tests;
