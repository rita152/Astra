//! Typed conversation item decoding and attachment materialization.

use std::{collections::BTreeMap, fs, path::PathBuf};

use anyhow::{Context as _, Result, anyhow, bail};
use base64::Engine as _;
use serde_json::Value;

use super::methods::summarize_json;
use crate::{
    agent::{
        AgentCollaboration, AgentCollaborationStatus, AgentCollaborationTool,
        AgentCollaboratorState, AgentCollaboratorStatus, AgentFileChange, AgentFileChangeEntry,
        AgentFileChangeKind, AgentFileChangeStatus, AgentImageGeneration,
        AgentImageGenerationFailure, AgentImageGenerationStatus, AgentImageView, AgentMcpToolCall,
        AgentMcpToolCallStatus, AgentReasoning, CommandExecution, CommandExecutionAction,
        CommandExecutionStatus, LegacySubAgentActivityKind,
    },
    media::read_image_dimensions,
};

pub(super) fn required_turn_item(message: &Value) -> Result<&serde_json::Map<String, Value>> {
    message
        .pointer("/params/item")
        .and_then(Value::as_object)
        .ok_or_else(|| turn_item_protocol_error(message, "params.item 必须是对象"))
}

pub(super) fn required_turn_item_type<'a>(
    message: &Value,
    item: &'a serde_json::Map<String, Value>,
) -> Result<&'a str> {
    item.get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| turn_item_protocol_error(message, "params.item.type 必须是字符串"))
}

pub(super) fn turn_item_field_context(value: Option<&Value>) -> String {
    match value {
        None => "缺少".to_owned(),
        Some(Value::String(value)) => format!("`{value}`"),
        Some(value) => format!("非字符串({})", summarize_json(value)),
    }
}

pub(super) fn turn_item_protocol_error(
    message: &Value,
    detail: impl std::fmt::Display,
) -> anyhow::Error {
    let method = turn_item_field_context(message.get("method"));
    let item = message.pointer("/params/item");
    let item_type = turn_item_field_context(item.and_then(|item| item.get("type")));
    let item_id = turn_item_field_context(item.and_then(|item| item.get("id")));
    let thread_id = turn_item_field_context(message.pointer("/params/threadId"));
    let turn_id = turn_item_field_context(message.pointer("/params/turnId"));
    let summary = match item {
        Some(item) => format!("item={}", summarize_json(item)),
        None => format!(
            "params={}",
            message
                .get("params")
                .map(summarize_json)
                .unwrap_or_else(|| "null".to_owned())
        ),
    };
    anyhow!(
        "{detail}；JSON-RPC method={method}；item.type={item_type}；item.id/itemId={item_id}；threadId={thread_id}；turnId={turn_id}；{summary}"
    )
}

pub(super) fn required_item_string(
    item: &serde_json::Map<String, Value>,
    item_kind: &str,
    field: &str,
) -> Result<String> {
    item.get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("{item_kind} item.{field} 必须是字符串"))
}

pub(super) fn validate_user_message(item: &serde_json::Map<String, Value>) -> Result<()> {
    let item_type = required_item_string(item, "userMessage", "type")?;
    if item_type != "userMessage" {
        bail!("userMessage item.type 必须是 `userMessage`，实际为 `{item_type}`");
    }
    let _id = required_item_string(item, "userMessage", "id")?;
    match item.get("clientId") {
        None | Some(Value::Null | Value::String(_)) => {}
        Some(_) => bail!("userMessage item.clientId 必须是字符串或 null"),
    }
    let content = item
        .get("content")
        .and_then(Value::as_array)
        .context("userMessage item.content 必须是数组")?;
    for (index, input) in content.iter().enumerate() {
        let input = input
            .as_object()
            .with_context(|| format!("userMessage item.content[{index}] 必须是对象"))?;
        let input_kind = format!("userMessage content[{index}]");
        let input_type = required_item_string(input, &input_kind, "type")?;
        match input_type.as_str() {
            "text" => {
                let _text = required_item_string(input, &input_kind, "text")?;
                match input.get("text_elements") {
                    None | Some(Value::Array(_)) => {}
                    Some(_) => bail!("userMessage item.content[{index}].text_elements 必须是数组"),
                }
            }
            "image" => {
                let _url = required_item_string(input, &input_kind, "url")?;
                validate_image_detail(input, index)?;
            }
            "localImage" => {
                let _path = required_item_string(input, &input_kind, "path")?;
                validate_image_detail(input, index)?;
            }
            "audio" => {
                let _url = required_item_string(input, &input_kind, "url")?;
            }
            "localAudio" => {
                let _path = required_item_string(input, &input_kind, "path")?;
            }
            "skill" | "mention" => {
                let _name = required_item_string(input, &input_kind, "name")?;
                let _path = required_item_string(input, &input_kind, "path")?;
            }
            unsupported => bail!(
                "userMessage item.content[{index}].type `{unsupported}` 不在当前协议 schema 中"
            ),
        }
    }
    Ok(())
}

pub(super) fn validate_image_detail(
    input: &serde_json::Map<String, Value>,
    index: usize,
) -> Result<()> {
    match input.get("detail") {
        None | Some(Value::Null) => Ok(()),
        Some(Value::String(detail))
            if matches!(detail.as_str(), "auto" | "low" | "high" | "original") =>
        {
            Ok(())
        }
        Some(_) => bail!(
            "userMessage item.content[{index}].detail 必须是 auto、low、high、original 或 null"
        ),
    }
}

pub(super) fn parse_agent_message(
    item: &serde_json::Map<String, Value>,
) -> Result<(String, String)> {
    let item_type = required_item_string(item, "agentMessage", "type")?;
    if item_type != "agentMessage" {
        bail!("agentMessage item.type 必须是 `agentMessage`，实际为 `{item_type}`");
    }
    Ok((
        required_item_string(item, "agentMessage", "id")?,
        required_item_string(item, "agentMessage", "text")?,
    ))
}

pub(super) fn parse_image_view(item: &serde_json::Map<String, Value>) -> Result<AgentImageView> {
    let item_type = required_item_string(item, "imageView", "type")?;
    if item_type != "imageView" {
        bail!("imageView item.type 必须是 `imageView`，实际为 `{item_type}`");
    }
    Ok(AgentImageView {
        id: required_item_string(item, "imageView", "id")?,
        path: PathBuf::from(required_item_string(item, "imageView", "path")?),
    })
}

pub(super) fn optional_image_generation_string(
    item: &serde_json::Map<String, Value>,
    field: &str,
    legacy_field: &str,
    allow_legacy: bool,
) -> Result<Option<String>> {
    let value = item
        .get(field)
        .or_else(|| allow_legacy.then(|| item.get(legacy_field)).flatten());
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => bail!("imageGeneration item.{field} 必须是字符串或 null"),
    }
}

pub(super) fn optional_image_generation_bool(
    item: &serde_json::Map<String, Value>,
    field: &str,
    legacy_field: &str,
    allow_legacy: bool,
) -> Result<Option<bool>> {
    let value = item
        .get(field)
        .or_else(|| allow_legacy.then(|| item.get(legacy_field)).flatten());
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => bail!("imageGeneration item.{field} 必须是布尔值或 null"),
    }
}

pub(super) fn optional_image_generation_i64(
    value: &serde_json::Map<String, Value>,
    field: &str,
    legacy_field: &str,
    allow_legacy: bool,
    context: &str,
) -> Result<Option<i64>> {
    let value = value
        .get(field)
        .or_else(|| allow_legacy.then(|| value.get(legacy_field)).flatten());
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(value)) => value
            .as_i64()
            .map(Some)
            .with_context(|| format!("{context}.{field} 必须是 int64 或 null")),
        Some(_) => bail!("{context}.{field} 必须是 int64 或 null"),
    }
}

pub(super) fn parse_image_generation_failure(
    item: &serde_json::Map<String, Value>,
    allow_legacy: bool,
) -> Result<Option<AgentImageGenerationFailure>> {
    let Some(failure) = item.get("failure") else {
        return Ok(None);
    };
    let Value::Object(failure) = failure else {
        if failure.is_null() {
            return Ok(None);
        }
        bail!("imageGeneration item.failure 必须是对象或 null");
    };
    let failure_type = failure
        .get("type")
        .and_then(Value::as_str)
        .context("imageGeneration item.failure.type 必须是字符串")?;
    match failure_type {
        "usageLimitExceeded" | "usage_limit_exceeded" if allow_legacy => {
            let limit_id = failure
                .get("limitId")
                .or_else(|| failure.get("limit_id"))
                .and_then(Value::as_str)
                .context("imageGeneration item.failure.limitId 必须是字符串")?
                .to_owned();
            Ok(Some(AgentImageGenerationFailure::UsageLimitExceeded {
                limit_id,
                resets_at: optional_image_generation_i64(
                    failure,
                    "resetsAt",
                    "resets_at",
                    true,
                    "imageGeneration item.failure",
                )?,
            }))
        }
        "usageLimitExceeded" => {
            let limit_id = failure
                .get("limitId")
                .and_then(Value::as_str)
                .context("imageGeneration item.failure.limitId 必须是字符串")?
                .to_owned();
            Ok(Some(AgentImageGenerationFailure::UsageLimitExceeded {
                limit_id,
                resets_at: optional_image_generation_i64(
                    failure,
                    "resetsAt",
                    "resetsAt",
                    false,
                    "imageGeneration item.failure",
                )?,
            }))
        }
        other => bail!("imageGeneration item.failure.type 包含未知值 `{other}`"),
    }
}

pub(super) fn sanitized_image_generation_id(item_id: &str) -> String {
    let sanitized = item_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .take(160)
        .collect::<String>();
    if sanitized.is_empty() {
        "image".to_owned()
    } else {
        sanitized
    }
}

pub(super) fn image_file_extension(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "jpg"
    } else if bytes.starts_with(b"GIF8") {
        "gif"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "webp"
    } else {
        "img"
    }
}

pub(super) fn materialize_image_generation_result(item_id: &str, encoded: &str) -> Result<PathBuf> {
    let encoded = encoded
        .split_once(',')
        .filter(|(prefix, _)| prefix.starts_with("data:image/"))
        .map_or(encoded, |(_, bytes)| bytes);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .context("imageGeneration item.result 不是有效 base64")?;
    if bytes.is_empty() {
        bail!("imageGeneration item.result 解码为空");
    }
    let directory = std::env::temp_dir()
        .join("gpui-chat-clone")
        .join("generated-images");
    fs::create_dir_all(&directory)
        .with_context(|| format!("无法创建 imageGeneration 缓存目录 {}", directory.display()))?;
    let path = directory.join(format!(
        "{}.{}",
        sanitized_image_generation_id(item_id),
        image_file_extension(&bytes)
    ));
    fs::write(&path, bytes)
        .with_context(|| format!("无法写入 imageGeneration 缓存 {}", path.display()))?;
    Ok(path)
}

pub(super) fn parse_image_generation(
    item: &serde_json::Map<String, Value>,
    allow_legacy: bool,
) -> Result<AgentImageGeneration> {
    let item_type = item
        .get("type")
        .and_then(Value::as_str)
        .context("imageGeneration item.type 必须是字符串")?;
    if item_type != "imageGeneration" && !(allow_legacy && item_type == "image_generation") {
        bail!("imageGeneration item.type 包含未知值 `{item_type}`");
    }
    let id = required_item_string(item, "imageGeneration", "id")?;
    let status = match required_item_string(item, "imageGeneration", "status")?.as_str() {
        "in_progress" => AgentImageGenerationStatus::InProgress,
        "inProgress" if allow_legacy => AgentImageGenerationStatus::InProgress,
        "completed" => AgentImageGenerationStatus::Completed,
        "failed" => AgentImageGenerationStatus::Failed,
        other => bail!("imageGeneration item.status 包含未知值 `{other}`"),
    };
    let revised_prompt =
        optional_image_generation_string(item, "revisedPrompt", "revised_prompt", allow_legacy)?;
    let transparent_background = optional_image_generation_bool(
        item,
        "transparentBackground",
        "transparent_background",
        allow_legacy,
    )?;
    let failure = parse_image_generation_failure(item, allow_legacy)?;
    let saved_path =
        optional_image_generation_string(item, "savedPath", "saved_path", allow_legacy)?
            .map(PathBuf::from);
    let result = match item.get("result") {
        Some(Value::String(value)) => value.as_str(),
        None if allow_legacy => "",
        Some(_) => bail!("imageGeneration item.result 必须是字符串"),
        None => bail!("imageGeneration item.result 必须是字符串"),
    };

    let (path, dimensions, load_error) = if status == AgentImageGenerationStatus::Completed {
        let resolved_path = match saved_path {
            Some(path) if path.is_file() => Ok(Some(path)),
            _missing_or_unreadable if !result.is_empty() => {
                materialize_image_generation_result(&id, result).map(Some)
            }
            Some(path) => Err(anyhow!("生成的图像文件不存在：{}", path.display())),
            None => Err(anyhow!("生成结果缺少 savedPath，且 result 为空")),
        };
        match resolved_path {
            Ok(Some(path)) => match read_image_dimensions(&path) {
                Ok(dimensions) => (Some(path), dimensions, None),
                Err(error) => (None, None, Some(format!("{error:#}"))),
            },
            Ok(None) => (None, None, Some("生成结果没有可显示的图像".to_owned())),
            Err(error) => (None, None, Some(format!("{error:#}"))),
        }
    } else {
        (None, None, None)
    };

    Ok(AgentImageGeneration {
        id,
        status,
        revised_prompt,
        path,
        dimensions,
        transparent_background,
        failure,
        load_error,
    })
}

pub(super) fn parse_context_compaction(
    item: &serde_json::Map<String, Value>,
    completed: bool,
) -> Result<crate::agent::AgentContextCompaction> {
    let item_type = required_item_string(item, "contextCompaction", "type")?;
    if item_type != "contextCompaction" {
        bail!("contextCompaction item.type 必须是 `contextCompaction`，实际为 `{item_type}`");
    }
    Ok(crate::agent::AgentContextCompaction {
        id: required_item_string(item, "contextCompaction", "id")?,
        completed,
    })
}

pub(super) fn parse_collaboration_tool(value: &str) -> Result<AgentCollaborationTool> {
    Ok(match value {
        "spawnAgent" => AgentCollaborationTool::SpawnAgent,
        "sendInput" => AgentCollaborationTool::SendInput,
        "resumeAgent" => AgentCollaborationTool::ResumeAgent,
        "wait" => AgentCollaborationTool::Wait,
        "closeAgent" => AgentCollaborationTool::CloseAgent,
        "sendMessage" => AgentCollaborationTool::SendMessage,
        "followupTask" => AgentCollaborationTool::FollowupTask,
        "interruptAgent" => AgentCollaborationTool::InterruptAgent,
        "listAgents" => AgentCollaborationTool::ListAgents,
        unsupported => bail!("collabAgentToolCall item.tool 包含未知值 `{unsupported}`"),
    })
}

pub(super) fn parse_collaboration_status(value: &str) -> Result<AgentCollaborationStatus> {
    Ok(match value {
        "inProgress" => AgentCollaborationStatus::InProgress,
        "completed" => AgentCollaborationStatus::Completed,
        "failed" => AgentCollaborationStatus::Failed,
        "interrupted" => AgentCollaborationStatus::Interrupted,
        unsupported => bail!("collabAgentToolCall item.status 包含未知值 `{unsupported}`"),
    })
}

pub(super) fn parse_collaborator_status(value: &str) -> Result<AgentCollaboratorStatus> {
    Ok(match value {
        "pendingInit" => AgentCollaboratorStatus::PendingInit,
        "running" => AgentCollaboratorStatus::Running,
        "interrupted" => AgentCollaboratorStatus::Interrupted,
        "completed" => AgentCollaboratorStatus::Completed,
        "errored" => AgentCollaboratorStatus::Errored,
        "shutdown" => AgentCollaboratorStatus::Shutdown,
        "notFound" => AgentCollaboratorStatus::NotFound,
        unsupported => bail!("collabAgentToolCall agent.status 包含未知值 `{unsupported}`"),
    })
}

pub(super) fn optional_nullable_item_string(
    item: &serde_json::Map<String, Value>,
    item_kind: &str,
    field: &str,
) -> Result<Option<String>> {
    match item.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => bail!("{item_kind} item.{field} 必须是字符串或 null"),
    }
}

pub(super) fn required_item_strings(
    item: &serde_json::Map<String, Value>,
    item_kind: &str,
    field: &str,
) -> Result<Vec<String>> {
    item.get(field)
        .and_then(Value::as_array)
        .with_context(|| format!("{item_kind} item.{field} 必须是字符串数组"))?
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value
                .as_str()
                .map(str::to_owned)
                .with_context(|| format!("{item_kind} item.{field}[{index}] 必须是字符串"))
        })
        .collect()
}

pub(super) fn parse_legacy_sub_agent_kind(value: &str) -> Result<LegacySubAgentActivityKind> {
    Ok(match value {
        "started" => LegacySubAgentActivityKind::Started,
        "interacted" => LegacySubAgentActivityKind::Interacted,
        "interrupted" => LegacySubAgentActivityKind::Interrupted,
        "completed" => LegacySubAgentActivityKind::Completed,
        unsupported => bail!("subAgentActivity item.kind 包含未知值 `{unsupported}`"),
    })
}

pub(super) fn default_collaborator_status(
    status: AgentCollaborationStatus,
) -> AgentCollaboratorStatus {
    match status {
        AgentCollaborationStatus::InProgress => AgentCollaboratorStatus::Running,
        AgentCollaborationStatus::Completed => AgentCollaboratorStatus::Completed,
        AgentCollaborationStatus::Failed => AgentCollaboratorStatus::Errored,
        AgentCollaborationStatus::Interrupted => AgentCollaboratorStatus::Interrupted,
    }
}

pub(super) fn parse_collaborator_state(
    value: &Value,
    context: &str,
) -> Result<AgentCollaboratorState> {
    match value {
        Value::String(status) => Ok(AgentCollaboratorState {
            status: parse_collaborator_status(status)?,
            message: None,
            name: None,
        }),
        Value::Object(state) => Ok(AgentCollaboratorState {
            status: parse_collaborator_status(&required_item_string(state, context, "status")?)?,
            message: optional_nullable_item_string(state, context, "message")?,
            name: None,
        }),
        _ => bail!("{context} 必须是状态字符串或对象"),
    }
}

pub(super) fn humanize_agent_path(path: &str) -> Option<String> {
    let leaf = path.rsplit('/').find(|part| !part.is_empty())?;
    let words = leaf
        .split(['_', '-'])
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let joined = words.join(" ");
    let mut characters = joined.chars();
    let leading = characters.next()?.to_uppercase().collect::<String>();
    Some(format!("{leading}{}", characters.as_str()))
}

pub(super) fn parse_collaboration(
    item: &serde_json::Map<String, Value>,
) -> Result<AgentCollaboration> {
    let item_type = required_item_string(item, "collaboration", "type")?;
    match item_type.as_str() {
        "collabToolCall" => {
            let context = "collabToolCall";
            let status =
                parse_collaboration_status(&required_item_string(item, context, "status")?)?;
            let default_status = default_collaborator_status(status);
            let mut receiver_thread_ids = Vec::new();
            for field in ["receiverThreadId", "newThreadId"] {
                if let Some(thread_id) = optional_nullable_item_string(item, context, field)?
                    && !receiver_thread_ids.contains(&thread_id)
                {
                    receiver_thread_ids.push(thread_id);
                }
            }
            let mut agents_states = receiver_thread_ids
                .iter()
                .cloned()
                .map(|thread_id| {
                    (
                        thread_id,
                        AgentCollaboratorState {
                            status: default_status,
                            message: None,
                            name: None,
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>();

            if let Some(agent_status) = item.get("agentStatus") {
                let parsed =
                    parse_collaborator_state(agent_status, "collabToolCall item.agentStatus")?;
                let thread_id = receiver_thread_ids.first().cloned().context(
                    "collabToolCall item.agentStatus 存在时必须提供 receiverThreadId 或 newThreadId",
                )?;
                agents_states.insert(thread_id, parsed);
            }

            let agent_path = optional_nullable_item_string(item, context, "agentPath")?;
            let name = optional_nullable_item_string(item, context, "agentName")?
                .or(optional_nullable_item_string(
                    item,
                    context,
                    "newAgentNickname",
                )?)
                .or_else(|| agent_path.as_deref().and_then(humanize_agent_path));
            if let (Some(thread_id), Some(name)) = (receiver_thread_ids.first(), name)
                && let Some(state) = agents_states.get_mut(thread_id)
            {
                state.name = Some(name);
            }

            Ok(AgentCollaboration {
                id: required_item_string(item, context, "id")?,
                tool: parse_collaboration_tool(&required_item_string(item, context, "tool")?)?,
                status,
                sender_thread_id: required_item_string(item, context, "senderThreadId")?,
                receiver_thread_ids,
                agents_states,
                prompt: optional_nullable_item_string(item, context, "prompt")?,
                model: None,
                reasoning_effort: None,
                legacy_agent_path: agent_path,
                legacy_kind: None,
            })
        }
        "collabAgentToolCall" => {
            let mut agents_states = BTreeMap::new();
            let states = item
                .get("agentsStates")
                .and_then(Value::as_object)
                .context("collabAgentToolCall item.agentsStates 必须是对象")?;
            for (thread_id, state) in states {
                let state = state.as_object().with_context(|| {
                    format!("collabAgentToolCall item.agentsStates.{thread_id} 必须是对象")
                })?;
                let status = parse_collaborator_status(&required_item_string(
                    state,
                    "collabAgentToolCall agent state",
                    "status",
                )?)?;
                let message = optional_nullable_item_string(
                    state,
                    "collabAgentToolCall agent state",
                    "message",
                )?;
                agents_states.insert(
                    thread_id.clone(),
                    AgentCollaboratorState {
                        status,
                        message,
                        name: None,
                    },
                );
            }

            let reasoning_effort =
                optional_nullable_item_string(item, "collabAgentToolCall", "reasoningEffort")?;
            if reasoning_effort.as_deref().is_some_and(str::is_empty) {
                bail!("collabAgentToolCall item.reasoningEffort 不能为空字符串");
            }
            Ok(AgentCollaboration {
                id: required_item_string(item, "collabAgentToolCall", "id")?,
                tool: parse_collaboration_tool(&required_item_string(
                    item,
                    "collabAgentToolCall",
                    "tool",
                )?)?,
                status: parse_collaboration_status(&required_item_string(
                    item,
                    "collabAgentToolCall",
                    "status",
                )?)?,
                sender_thread_id: required_item_string(
                    item,
                    "collabAgentToolCall",
                    "senderThreadId",
                )?,
                receiver_thread_ids: required_item_strings(
                    item,
                    "collabAgentToolCall",
                    "receiverThreadIds",
                )?,
                agents_states,
                prompt: optional_nullable_item_string(item, "collabAgentToolCall", "prompt")?,
                model: optional_nullable_item_string(item, "collabAgentToolCall", "model")?,
                reasoning_effort,
                legacy_agent_path: None,
                legacy_kind: None,
            })
        }
        "subAgentActivity" => {
            let kind = parse_legacy_sub_agent_kind(&required_item_string(
                item,
                "subAgentActivity",
                "kind",
            )?)?;
            let agent_thread_id = required_item_string(item, "subAgentActivity", "agentThreadId")?;
            let (status, agent_status) = match kind {
                LegacySubAgentActivityKind::Started | LegacySubAgentActivityKind::Interacted => (
                    AgentCollaborationStatus::InProgress,
                    AgentCollaboratorStatus::Running,
                ),
                LegacySubAgentActivityKind::Interrupted => (
                    AgentCollaborationStatus::Interrupted,
                    AgentCollaboratorStatus::Interrupted,
                ),
                LegacySubAgentActivityKind::Completed => (
                    AgentCollaborationStatus::Completed,
                    AgentCollaboratorStatus::Completed,
                ),
            };
            Ok(AgentCollaboration {
                id: required_item_string(item, "subAgentActivity", "id")?,
                tool: AgentCollaborationTool::LegacyActivity,
                status,
                sender_thread_id: String::new(),
                receiver_thread_ids: vec![agent_thread_id.clone()],
                agents_states: BTreeMap::from([(
                    agent_thread_id,
                    AgentCollaboratorState {
                        status: agent_status,
                        message: None,
                        name: None,
                    },
                )]),
                prompt: None,
                model: None,
                reasoning_effort: None,
                legacy_agent_path: Some(required_item_string(
                    item,
                    "subAgentActivity",
                    "agentPath",
                )?),
                legacy_kind: Some(kind),
            })
        }
        unsupported => bail!("collaboration item.type 包含未知值 `{unsupported}`"),
    }
}

pub(super) fn optional_mcp_string(
    item: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<Option<String>> {
    match item.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => bail!("mcpToolCall item.{field} 必须是字符串或 null"),
    }
}

pub(super) fn parse_mcp_tool_call(
    item: &serde_json::Map<String, Value>,
) -> Result<AgentMcpToolCall> {
    let item_type = required_item_string(item, "mcpToolCall", "type")?;
    if item_type != "mcpToolCall" {
        bail!("mcpToolCall item.type 必须是 `mcpToolCall`，实际为 `{item_type}`");
    }
    let status = match required_item_string(item, "mcpToolCall", "status")?.as_str() {
        "inProgress" => AgentMcpToolCallStatus::InProgress,
        "completed" => AgentMcpToolCallStatus::Completed,
        "failed" => AgentMcpToolCallStatus::Failed,
        other => bail!("mcpToolCall item.status 包含未知值 `{other}`"),
    };
    let arguments = item
        .get("arguments")
        .cloned()
        .context("mcpToolCall item.arguments 缺失")?;
    let app_context = match item.get("appContext") {
        None | Some(Value::Null) => None,
        Some(Value::Object(context)) => {
            context
                .get("connectorId")
                .and_then(Value::as_str)
                .context("mcpToolCall item.appContext.connectorId 必须是字符串")?;
            Some(Value::Object(context.clone()))
        }
        Some(_) => bail!("mcpToolCall item.appContext 必须是对象或 null"),
    };
    let result = match item.get("result") {
        None | Some(Value::Null) => None,
        Some(Value::Object(result)) => {
            result
                .get("content")
                .and_then(Value::as_array)
                .context("mcpToolCall item.result.content 必须是数组")?;
            Some(Value::Object(result.clone()))
        }
        Some(_) => bail!("mcpToolCall item.result 必须是对象或 null"),
    };
    let error = match item.get("error") {
        None | Some(Value::Null) => None,
        Some(Value::Object(error)) => Some(
            error
                .get("message")
                .and_then(Value::as_str)
                .context("mcpToolCall item.error.message 必须是字符串")?
                .to_owned(),
        ),
        // Some older persisted histories encoded the same message directly.
        Some(Value::String(error)) => Some(error.clone()),
        Some(_) => bail!("mcpToolCall item.error 必须是对象、字符串或 null"),
    };
    let read_only_hint = match item.get("readOnlyHint") {
        None | Some(Value::Null) => None,
        Some(Value::Bool(value)) => Some(*value),
        Some(_) => bail!("mcpToolCall item.readOnlyHint 必须是布尔值或 null"),
    };
    let duration_ms = match item.get("durationMs") {
        None | Some(Value::Null) => None,
        Some(Value::Number(value)) => Some(
            value
                .as_i64()
                .context("mcpToolCall item.durationMs 必须是 int64 或 null")?,
        ),
        Some(_) => bail!("mcpToolCall item.durationMs 必须是 int64 或 null"),
    };

    Ok(AgentMcpToolCall {
        id: required_item_string(item, "mcpToolCall", "id")?,
        server: required_item_string(item, "mcpToolCall", "server")?,
        tool: required_item_string(item, "mcpToolCall", "tool")?,
        status,
        arguments,
        app_context,
        plugin_id: optional_mcp_string(item, "pluginId")?,
        result,
        error,
        legacy_resource_uri: optional_mcp_string(item, "mcpAppResourceUri")?,
        read_only_hint,
        duration_ms,
        progress: Vec::new(),
    })
}

pub(super) fn optional_item_strings(
    item: &serde_json::Map<String, Value>,
    item_kind: &str,
    field: &str,
) -> Result<Vec<String>> {
    let Some(value) = item.get(field) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .with_context(|| format!("{item_kind} item.{field} 必须是字符串数组"))?;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value
                .as_str()
                .map(str::to_owned)
                .with_context(|| format!("{item_kind} item.{field}[{index}] 必须是字符串"))
        })
        .collect()
}

pub(super) fn parse_reasoning(item: &serde_json::Map<String, Value>) -> Result<AgentReasoning> {
    let item_type = required_item_string(item, "reasoning", "type")?;
    if item_type != "reasoning" {
        bail!("reasoning item.type 必须是 `reasoning`，实际为 `{item_type}`");
    }
    Ok(AgentReasoning {
        id: required_item_string(item, "reasoning", "id")?,
        summary: optional_item_strings(item, "reasoning", "summary")?,
        content: optional_item_strings(item, "reasoning", "content")?,
    })
}

pub(super) fn parse_file_change_status(
    value: &str,
    context: &str,
) -> Result<AgentFileChangeStatus> {
    match value {
        "inProgress" => Ok(AgentFileChangeStatus::InProgress),
        "completed" => Ok(AgentFileChangeStatus::Completed),
        "failed" => Ok(AgentFileChangeStatus::Failed),
        "declined" => Ok(AgentFileChangeStatus::Declined),
        other => bail!("{context}.status 包含未知值 `{other}`"),
    }
}

pub(super) fn parse_file_change_entries(
    value: &Value,
    context: &str,
) -> Result<Vec<AgentFileChangeEntry>> {
    let changes = value
        .as_array()
        .with_context(|| format!("{context}.changes 必须是数组"))?;
    changes
        .iter()
        .enumerate()
        .map(|(index, change)| {
            let change = change
                .as_object()
                .with_context(|| format!("{context}.changes[{index}] 必须是对象"))?;
            let change_context = format!("{context}.changes[{index}]");
            let kind = change
                .get("kind")
                .and_then(Value::as_object)
                .with_context(|| format!("{change_context}.kind 必须是对象"))?;
            let kind_type = required_item_string(kind, &format!("{change_context}.kind"), "type")?;
            let kind = match kind_type.as_str() {
                "add" => AgentFileChangeKind::Add,
                "delete" => AgentFileChangeKind::Delete,
                "update" => {
                    let move_path = match kind.get("move_path") {
                        None | Some(Value::Null) => None,
                        Some(Value::String(path)) => Some(path.clone()),
                        Some(_) => bail!("{change_context}.kind.move_path 必须是字符串或 null"),
                    };
                    AgentFileChangeKind::Update { move_path }
                }
                other => bail!("{change_context}.kind.type 包含未知值 `{other}`"),
            };
            Ok(AgentFileChangeEntry {
                path: required_item_string(change, &change_context, "path")?,
                diff: required_item_string(change, &change_context, "diff")?,
                kind,
            })
        })
        .collect()
}

pub(super) fn parse_file_change(item: &serde_json::Map<String, Value>) -> Result<AgentFileChange> {
    let item_type = required_item_string(item, "fileChange", "type")?;
    if item_type != "fileChange" {
        bail!("fileChange item.type 必须是 `fileChange`，实际为 `{item_type}`");
    }
    Ok(AgentFileChange {
        id: required_item_string(item, "fileChange", "id")?,
        changes: parse_file_change_entries(
            item.get("changes")
                .context("fileChange item.changes 缺失")?,
            "fileChange item",
        )?,
        status: parse_file_change_status(
            &required_item_string(item, "fileChange", "status")?,
            "fileChange item",
        )?,
    })
}

pub(super) fn optional_command_action_string(
    action: &serde_json::Map<String, Value>,
    index: usize,
    field: &str,
) -> Result<Option<String>> {
    match action.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => {
            bail!("commandExecution item.commandActions[{index}].{field} 必须是字符串或 null")
        }
    }
}

pub(super) fn parse_command_execution(
    item: &serde_json::Map<String, Value>,
) -> Result<CommandExecution> {
    let item_type = required_item_string(item, "commandExecution", "type")?;
    if item_type != "commandExecution" {
        bail!("commandExecution item.type 必须是 `commandExecution`，实际为 `{item_type}`");
    }
    let id = required_item_string(item, "commandExecution", "id")?;
    let raw_command = required_item_string(item, "commandExecution", "command")?;
    let cwd = required_item_string(item, "commandExecution", "cwd")?;
    let actions = item
        .get("commandActions")
        .and_then(Value::as_array)
        .context("commandExecution item.commandActions 必须是数组")?;
    let mut first_action_command = None;
    let mut parsed_actions = Vec::with_capacity(actions.len());
    for (index, action) in actions.iter().enumerate() {
        let action = action
            .as_object()
            .with_context(|| format!("commandExecution item.commandActions[{index}] 必须是对象"))?;
        let action_kind = format!("commandExecution item.commandActions[{index}]");
        let action_type = required_item_string(action, &action_kind, "type")?;
        let action_command = required_item_string(action, &action_kind, "command")?;
        let parsed_action = match action_type.as_str() {
            "read" => CommandExecutionAction::Read {
                command: action_command.clone(),
                name: required_item_string(action, &action_kind, "name")?,
                path: required_item_string(action, &action_kind, "path")?,
            },
            "listFiles" => CommandExecutionAction::ListFiles {
                command: action_command.clone(),
                path: optional_command_action_string(action, index, "path")?,
            },
            "search" => CommandExecutionAction::Search {
                command: action_command.clone(),
                path: optional_command_action_string(action, index, "path")?,
                query: optional_command_action_string(action, index, "query")?,
            },
            "unknown" => CommandExecutionAction::Unknown {
                command: action_command.clone(),
            },
            other => {
                bail!("commandExecution item.commandActions[{index}].type 包含未知值 `{other}`")
            }
        };
        if index == 0 {
            first_action_command = Some(action_command);
        }
        parsed_actions.push(parsed_action);
    }
    let output = match item.get("aggregatedOutput") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(output)) => output.clone(),
        Some(_) => bail!("commandExecution item.aggregatedOutput 必须是字符串或 null"),
    };
    let exit_code = match item.get("exitCode") {
        None | Some(Value::Null) => None,
        Some(Value::Number(exit_code)) => {
            let exit_code = exit_code
                .as_i64()
                .context("commandExecution item.exitCode 必须是 int32 或 null")?;
            i32::try_from(exit_code).context("commandExecution item.exitCode 超出 int32 范围")?;
            Some(exit_code)
        }
        Some(_) => bail!("commandExecution item.exitCode 必须是 int32 或 null"),
    };
    let raw_status = required_item_string(item, "commandExecution", "status")?;
    let status = match raw_status.as_str() {
        "inProgress" => CommandExecutionStatus::InProgress,
        "completed" if exit_code.is_some_and(|exit_code| exit_code != 0) => {
            CommandExecutionStatus::Failed
        }
        "completed" => CommandExecutionStatus::Completed,
        "failed" | "declined" => CommandExecutionStatus::Failed,
        other => bail!("commandExecution item.status 包含未知值 `{other}`"),
    };
    Ok(CommandExecution {
        id,
        command: first_action_command.unwrap_or(raw_command),
        actions: parsed_actions,
        cwd,
        output,
        terminal_process_id: None,
        status,
        exit_code,
    })
}
