//! Turn event dispatch and terminal outcome handling.

use std::{io::Write, sync::Arc};

use anyhow::{Context as _, Result, anyhow, bail};
use async_channel::Sender;
use serde_json::Value;

use super::{
    items::{
        parse_agent_message, parse_collaboration, parse_command_execution,
        parse_context_compaction, parse_file_change, parse_file_change_entries,
        parse_image_generation, parse_image_view, parse_mcp_tool_call, parse_reasoning,
        required_turn_item, required_turn_item_type, turn_item_protocol_error,
        validate_user_message,
    },
    methods::{
        ensure_server_method_is_defined, is_integrated_server_request_method,
        undefined_server_method_error,
    },
    notifications::{
        forward_agent_notification, required_notification_i64, required_notification_index,
        required_notification_string, turn_failure_message,
    },
    requests::{
        handle_server_request_resolved, reject_server_request, respond_to_server_request_on_session,
    },
    session::{CodexTurnSession, TurnOutcome, ensure_session_message_matches},
};
use crate::agent::AgentEvent;

pub(super) fn process_turn_message<W: Write + Send + 'static>(
    session: &Arc<CodexTurnSession<W>>,
    message: &Value,
    expected_thread_id: &str,
    expected_turn_id: &str,
    events: &Sender<AgentEvent>,
    streamed_text: &mut bool,
) -> Result<Option<TurnOutcome>> {
    if let Err(error) =
        ensure_session_message_matches(message, expected_thread_id, expected_turn_id)
    {
        if message.get("id").is_some()
            && message
                .get("method")
                .and_then(Value::as_str)
                .is_some_and(is_integrated_server_request_method)
        {
            return reject_server_request(
                session,
                message,
                -32602,
                "Server request does not match the active thread and turn",
                error,
            )
            .map(|()| None);
        }
        if matches!(
            message.get("method").and_then(Value::as_str),
            Some("item/started" | "item/completed")
        ) {
            return Err(turn_item_protocol_error(message, error));
        }
        return Err(error);
    }
    respond_to_server_request_on_session(session, message, events)?;
    handle_server_request_resolved(session, message, events)?;
    forward_agent_notification(message, events)?;
    ensure_server_method_is_defined(message)?;

    match message.get("method").and_then(Value::as_str) {
        Some("item/started") => {
            let item = required_turn_item(message)?;
            let item_type = required_turn_item_type(message, item)?;
            match item_type {
                "hookPrompt" => {
                    let prompt = super::runtime::parse_hook_prompt(
                        &Value::Object(item.clone()),
                        Some(false),
                    )
                    .map_err(|error| turn_item_protocol_error(message, error))?;
                    send_turn_event(
                        events,
                        AgentEvent::HookPromptUpdated(prompt),
                        "item/started hookPrompt",
                    )?;
                }
                "userMessage" => {
                    forward_user_message(session, item, events)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                }
                "agentMessage" => {
                    let (item_id, _text) = parse_agent_message(item)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    let mut messages = session
                        .agent_messages
                        .lock()
                        .map_err(|_| anyhow!("助手消息注册表锁不可用"))?;
                    let progress = messages.entry(item_id.clone()).or_default();
                    if !progress.started && !progress.completed {
                        progress.started = true;
                        *streamed_text = false;
                        send_turn_event(
                            events,
                            AgentEvent::AssistantMessageStarted { item_id },
                            "item/started agentMessage",
                        )
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    }
                }
                "reasoning" => {
                    let reasoning = parse_reasoning(item)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    let started_at_ms = required_notification_i64(message, "startedAtMs")
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    send_turn_event(
                        events,
                        AgentEvent::ReasoningStarted {
                            reasoning,
                            started_at_ms,
                        },
                        "item/started reasoning",
                    )
                    .map_err(|error| turn_item_protocol_error(message, error))?;
                }
                "commandExecution" => {
                    let command = parse_command_execution(item)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    send_turn_event(
                        events,
                        AgentEvent::CommandStarted(command),
                        "item/started commandExecution",
                    )
                    .map_err(|error| turn_item_protocol_error(message, error))?;
                }
                "fileChange" => {
                    let file_change = parse_file_change(item)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    send_turn_event(
                        events,
                        AgentEvent::FileChangeUpdated(file_change),
                        "item/started fileChange",
                    )
                    .map_err(|error| turn_item_protocol_error(message, error))?;
                }
                "imageView" => {
                    let image = parse_image_view(item)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    send_turn_event(
                        events,
                        AgentEvent::ImageViewed(image),
                        "item/started imageView",
                    )
                    .map_err(|error| turn_item_protocol_error(message, error))?;
                }
                "imageGeneration" => {
                    let image = parse_image_generation(item, false)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    send_turn_event(
                        events,
                        AgentEvent::ImageGenerationUpdated(image),
                        "item/started imageGeneration",
                    )
                    .map_err(|error| turn_item_protocol_error(message, error))?;
                }
                "contextCompaction" => {
                    let compaction = parse_context_compaction(item, false)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    send_turn_event(
                        events,
                        AgentEvent::ContextCompactionUpdated(compaction),
                        "item/started contextCompaction",
                    )
                    .map_err(|error| turn_item_protocol_error(message, error))?;
                }
                "collabToolCall" | "collabAgentToolCall" | "subAgentActivity" => {
                    let collaboration = parse_collaboration(item)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    send_turn_event(
                        events,
                        AgentEvent::CollaborationUpdated(collaboration),
                        "item/started collaboration",
                    )
                    .map_err(|error| turn_item_protocol_error(message, error))?;
                }
                "plan" | "webSearch" | "sleep" => {
                    required_notification_i64(message, "startedAtMs")
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    let event = super::progress::parse_progress_event(item, false)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    send_turn_event(events, event, "progress item")?;
                }
                "mcpToolCall" => {
                    let tool_call = parse_mcp_tool_call(item)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    send_turn_event(
                        events,
                        AgentEvent::McpToolCallUpdated(tool_call),
                        "item/started mcpToolCall",
                    )
                    .map_err(|error| turn_item_protocol_error(message, error))?;
                }
                unsupported => {
                    return Err(turn_item_protocol_error(
                        message,
                        format!("未接入的 item.type `{unsupported}`"),
                    ));
                }
            }
        }
        Some("item/plan/delta") => {
            send_turn_event(
                events,
                AgentEvent::PlanDelta {
                    item_id: required_notification_string(message, "itemId")?,
                    delta: required_notification_string(message, "delta")?,
                },
                "item/plan/delta",
            )?;
        }
        Some("turn/plan/updated") => {
            send_turn_event(
                events,
                AgentEvent::TurnPlanUpdated(super::progress::parse_turn_plan(message)?),
                "turn/plan/updated",
            )?;
        }
        Some("item/agentMessage/delta") => {
            let item_id = required_notification_string(message, "itemId")?;
            let delta = required_notification_string(message, "delta")?;
            let mut messages = session
                .agent_messages
                .lock()
                .map_err(|_| anyhow!("助手消息注册表锁不可用"))?;
            let progress = messages.entry(item_id).or_default();
            if progress.completed {
                return Ok(None);
            }
            progress.has_output = true;
            drop(messages);
            send_turn_event(
                events,
                AgentEvent::TextDelta(delta),
                "item/agentMessage/delta",
            )?;
            *streamed_text = true;
        }
        Some("item/commandExecution/outputDelta") => {
            let item_id = required_notification_string(message, "itemId")?;
            let delta = required_notification_string(message, "delta")?;
            send_turn_event(
                events,
                AgentEvent::CommandOutputDelta { item_id, delta },
                "item/commandExecution/outputDelta",
            )?;
        }
        Some("item/commandExecution/terminalInteraction") => {
            let item_id = required_notification_string(message, "itemId")?;
            let process_id = required_notification_string(message, "processId")?;
            let stdin = required_notification_string(message, "stdin")?;
            send_turn_event(
                events,
                AgentEvent::CommandTerminalInteraction {
                    item_id,
                    process_id,
                    wrote_stdin: !stdin.is_empty(),
                },
                "item/commandExecution/terminalInteraction",
            )?;
        }
        Some("item/fileChange/outputDelta") => {
            let _item_id = required_notification_string(message, "itemId")?;
            let _delta = required_notification_string(message, "delta")?;
        }
        Some("item/fileChange/patchUpdated") => {
            let item_id = required_notification_string(message, "itemId")?;
            let changes = message
                .pointer("/params/changes")
                .context("item/fileChange/patchUpdated 通知缺少 params.changes")?;
            let changes =
                parse_file_change_entries(changes, "item/fileChange/patchUpdated params")?;
            send_turn_event(
                events,
                AgentEvent::FileChangePatchUpdated { item_id, changes },
                "item/fileChange/patchUpdated",
            )?;
        }
        Some("item/reasoning/summaryPartAdded") => {
            let item_id = required_notification_string(message, "itemId")?;
            let summary_index = required_notification_index(message, "summaryIndex")?;
            send_turn_event(
                events,
                AgentEvent::ReasoningSummaryPartAdded {
                    item_id,
                    summary_index,
                },
                "item/reasoning/summaryPartAdded",
            )?;
        }
        Some("item/reasoning/summaryTextDelta") => {
            let item_id = required_notification_string(message, "itemId")?;
            let summary_index = required_notification_index(message, "summaryIndex")?;
            let delta = required_notification_string(message, "delta")?;
            send_turn_event(
                events,
                AgentEvent::ReasoningSummaryTextDelta {
                    item_id,
                    summary_index,
                    delta,
                },
                "item/reasoning/summaryTextDelta",
            )?;
        }
        Some("item/reasoning/textDelta") => {
            let item_id = required_notification_string(message, "itemId")?;
            let content_index = required_notification_index(message, "contentIndex")?;
            let delta = required_notification_string(message, "delta")?;
            send_turn_event(
                events,
                AgentEvent::ReasoningTextDelta {
                    item_id,
                    content_index,
                    delta,
                },
                "item/reasoning/textDelta",
            )?;
        }
        Some("item/mcpToolCall/progress") => {
            let item_id = required_notification_string(message, "itemId")?;
            let progress_message = required_notification_string(message, "message")?;
            send_turn_event(
                events,
                AgentEvent::McpToolCallProgress {
                    item_id,
                    message: progress_message,
                },
                "item/mcpToolCall/progress",
            )?;
        }
        Some("turn/diff/updated") => {
            let diff = required_notification_string(message, "diff")?;
            send_turn_event(
                events,
                AgentEvent::TurnDiffUpdated { diff },
                "turn/diff/updated",
            )?;
        }
        Some("item/completed") => {
            let item = required_turn_item(message)?;
            let item_type = required_turn_item_type(message, item)?;
            match item_type {
                "hookPrompt" => {
                    let prompt =
                        super::runtime::parse_hook_prompt(&Value::Object(item.clone()), Some(true))
                            .map_err(|error| turn_item_protocol_error(message, error))?;
                    send_turn_event(
                        events,
                        AgentEvent::HookPromptUpdated(prompt),
                        "item/completed hookPrompt",
                    )?;
                }
                "userMessage" => {
                    forward_user_message(session, item, events)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                }
                "agentMessage" => {
                    let (item_id, text) = parse_agent_message(item)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    let mut messages = session
                        .agent_messages
                        .lock()
                        .map_err(|_| anyhow!("助手消息注册表锁不可用"))?;
                    let progress = messages.entry(item_id).or_default();
                    if !progress.has_output && !progress.completed {
                        send_turn_event(
                            events,
                            AgentEvent::TextDelta(text),
                            "item/completed agentMessage",
                        )
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    }
                    progress.has_output = true;
                    progress.completed = true;
                    *streamed_text = true;
                }
                "reasoning" => {
                    let reasoning = parse_reasoning(item)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    let completed_at_ms = required_notification_i64(message, "completedAtMs")
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    send_turn_event(
                        events,
                        AgentEvent::ReasoningCompleted {
                            reasoning,
                            completed_at_ms,
                        },
                        "item/completed reasoning",
                    )
                    .map_err(|error| turn_item_protocol_error(message, error))?;
                }
                "commandExecution" => {
                    let command = parse_command_execution(item)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    send_turn_event(
                        events,
                        AgentEvent::CommandCompleted(command),
                        "item/completed commandExecution",
                    )
                    .map_err(|error| turn_item_protocol_error(message, error))?;
                }
                "fileChange" => {
                    let file_change = parse_file_change(item)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    send_turn_event(
                        events,
                        AgentEvent::FileChangeUpdated(file_change),
                        "item/completed fileChange",
                    )
                    .map_err(|error| turn_item_protocol_error(message, error))?;
                }
                "imageView" => {
                    let image = parse_image_view(item)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    send_turn_event(
                        events,
                        AgentEvent::ImageViewed(image),
                        "item/completed imageView",
                    )
                    .map_err(|error| turn_item_protocol_error(message, error))?;
                }
                "imageGeneration" => {
                    let image = parse_image_generation(item, false)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    send_turn_event(
                        events,
                        AgentEvent::ImageGenerationUpdated(image),
                        "item/completed imageGeneration",
                    )
                    .map_err(|error| turn_item_protocol_error(message, error))?;
                }
                "contextCompaction" => {
                    let compaction = parse_context_compaction(item, true)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    send_turn_event(
                        events,
                        AgentEvent::ContextCompactionUpdated(compaction),
                        "item/completed contextCompaction",
                    )
                    .map_err(|error| turn_item_protocol_error(message, error))?;
                }
                "collabToolCall" | "collabAgentToolCall" | "subAgentActivity" => {
                    let collaboration = parse_collaboration(item)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    send_turn_event(
                        events,
                        AgentEvent::CollaborationUpdated(collaboration),
                        "item/completed collaboration",
                    )
                    .map_err(|error| turn_item_protocol_error(message, error))?;
                }
                "plan" | "webSearch" | "sleep" => {
                    required_notification_i64(message, "completedAtMs")
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    let event = super::progress::parse_progress_event(item, true)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    send_turn_event(events, event, "progress item")?;
                }
                "mcpToolCall" => {
                    let tool_call = parse_mcp_tool_call(item)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    send_turn_event(
                        events,
                        AgentEvent::McpToolCallUpdated(tool_call),
                        "item/completed mcpToolCall",
                    )
                    .map_err(|error| turn_item_protocol_error(message, error))?;
                }
                unsupported => {
                    return Err(turn_item_protocol_error(
                        message,
                        format!("未接入的 item.type `{unsupported}`"),
                    ));
                }
            }
        }
        Some("turn/completed") => {
            let status = message
                .pointer("/params/turn/status")
                .and_then(Value::as_str)
                .context("turn/completed 通知缺少 params.turn.status")?;
            return Ok(Some(match status {
                "completed" => TurnOutcome::Completed,
                "interrupted" => TurnOutcome::Interrupted,
                "failed" => TurnOutcome::Failed(turn_failure_message(message)?),
                _ => bail!("Codex turn 结束，状态为未知值 `{status}`"),
            }));
        }
        Some(
            "item/autoApprovalReview/started"
            | "item/autoApprovalReview/completed"
            | "autoApprovalReview/strictReviewRequired"
            | "guardianWarning"
            | "item/commandExecution/requestApproval"
            | "item/fileChange/requestApproval"
            | "item/permissions/requestApproval"
            | "item/tool/requestUserInput"
            | "tool/requestUserInput"
            | "serverRequest/resolved"
            | "remoteControl/status/changed"
            | "mcpServer/startupStatus/updated"
            | "thread/status/changed"
            | "thread/archived"
            | "thread/unarchived"
            | "thread/deleted"
            | "thread/name/updated"
            | "thread/closed"
            | "thread/project/updated"
            | "project/changed"
            | "thread/tokenUsage/updated"
            | "account/rateLimits/updated"
            | "thread/started"
            | "turn/started"
            | "error"
            | "thread/settings/updated"
            | "warning"
            | "configWarning"
            | "model/rerouted"
            | "model/verification"
            | "model/safetyBuffering/updated",
        ) => return Ok(None),
        Some(method) => return Err(undefined_server_method_error(method, message)),
        None => return Ok(None),
    }
    Ok(None)
}

pub(super) fn send_turn_event(
    events: &Sender<AgentEvent>,
    event: AgentEvent,
    source: &str,
) -> Result<()> {
    events
        .send_blocking(event)
        .map_err(|_| anyhow!("Composer `{source}` 事件通道已经关闭"))
}

fn forward_user_message<W: Write + Send + 'static>(
    session: &CodexTurnSession<W>,
    item: &serde_json::Map<String, Value>,
    events: &Sender<AgentEvent>,
) -> Result<()> {
    validate_user_message(item)?;
    let value = Value::Object(item.clone());
    let id = item["id"].as_str().expect("validated item id");
    let mut seen = session
        .user_messages
        .lock()
        .map_err(|_| anyhow!("用户消息注册表锁不可用"))?;
    if seen.get(id) == Some(&value) {
        return Ok(());
    }
    seen.insert(id.to_owned(), value.clone());
    drop(seen);
    if let crate::agent::ThreadHistoryItem::UserMessage {
        item_id,
        client_message_id,
        text,
        images,
    } = super::workspace_protocol::parse_history_item(&value)?
    {
        send_turn_event(
            events,
            AgentEvent::UserMessage {
                item_id,
                text,
                images,
                client_message_id,
            },
            "userMessage",
        )?;
    }
    Ok(())
}
