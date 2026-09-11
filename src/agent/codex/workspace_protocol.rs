//! Codex workspace request encoding and response/history decoding.
//! Connection ownership and failure handling belong to the manager.

use std::{
    hash::{Hash, Hasher},
    path::PathBuf,
};

use anyhow::{Context as _, Result, bail};
use serde_json::{Value, json};

use super::{parse_collaboration, parse_image_generation, parse_mcp_tool_call};
use crate::agent::{
    AgentFileChange, AgentFileChangeEntry, AgentFileChangeKind, AgentFileChangeStatus,
    AgentImageView, AgentThreadActiveFlag, FilterValue, HistoryItemDetail, HistoryTurnStatus,
    Page, Project, SortDirection, ThreadActivity, ThreadHistoryItem, ThreadListRequest,
    ThreadSection, ThreadSectionAppearance, ThreadSummary, ThreadTurn, UserMessageAttachment,
    normalize_user_message_for_display,
};

pub(super) fn object_field<'a>(value: &'a Value, field: &str, context: &str) -> Result<&'a Value> {
    value
        .as_object()
        .and_then(|object| object.get(field))
        .with_context(|| format!("{context} 缺少字段 `{field}`"))
}

pub(super) fn string_field(value: &Value, field: &str, context: &str) -> Result<String> {
    object_field(value, field, context)?
        .as_str()
        .map(str::to_owned)
        .with_context(|| format!("{context}.{field} 必须是字符串"))
}

fn integer_field(value: &Value, field: &str, context: &str) -> Result<i64> {
    object_field(value, field, context)?
        .as_i64()
        .with_context(|| format!("{context}.{field} 必须是整数"))
}

fn optional_nullable_integer_field(
    value: &Value,
    field: &str,
    context: &str,
) -> Result<Option<i64>> {
    match value.as_object().and_then(|object| object.get(field)) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => number
            .as_i64()
            .map(Some)
            .with_context(|| format!("{context}.{field} 必须是 int64 或 null")),
        Some(_) => bail!("{context}.{field} 必须是 int64 或 null"),
    }
}

fn nullable_string_field(value: &Value, field: &str, context: &str) -> Result<Option<String>> {
    match object_field(value, field, context)? {
        Value::Null => Ok(None),
        Value::String(value) => Ok(Some(value.clone())),
        _ => bail!("{context}.{field} 必须是字符串或 null"),
    }
}

fn optional_nullable_string_field(
    value: &Value,
    field: &str,
    context: &str,
) -> Result<Option<String>> {
    match value.as_object().and_then(|object| object.get(field)) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => bail!("{context}.{field} 必须是字符串或 null"),
    }
}

pub(super) fn parse_project(value: &Value) -> Result<Project> {
    let roots = object_field(value, "roots", "project")?
        .as_array()
        .context("project.roots 必须是数组")?
        .iter()
        .map(|root| string_field(root, "path", "project root").map(PathBuf::from))
        .collect::<Result<Vec<_>>>()?;
    Ok(Project {
        project_id: string_field(value, "id", "project")?,
        name: string_field(value, "name", "project")?,
        roots,
        created_at: integer_field(value, "createdAt", "project")?,
        updated_at: integer_field(value, "updatedAt", "project")?,
        recency_at: optional_nullable_integer_field(value, "recencyAt", "project")?,
        position: integer_field(value, "position", "project")?,
    })
}

pub(super) fn parse_thread_section(value: &Value) -> Result<ThreadSection> {
    let appearance = match value
        .as_object()
        .and_then(|object| object.get("appearance"))
    {
        None | Some(Value::Null) => None,
        Some(appearance @ Value::Object(_)) => Some(ThreadSectionAppearance {
            icon: optional_nullable_string_field(appearance, "icon", "thread section appearance")?,
            color: optional_nullable_string_field(
                appearance,
                "color",
                "thread section appearance",
            )?,
        }),
        Some(_) => bail!("thread section.appearance 必须是对象或 null"),
    };
    Ok(ThreadSection {
        section_id: string_field(value, "id", "thread section")?,
        name: string_field(value, "name", "thread section")?,
        appearance,
    })
}

fn parse_thread_activity(value: &Value) -> Result<ThreadActivity> {
    let kind = string_field(value, "type", "thread status")?;
    Ok(match kind.as_str() {
        "notLoaded" => ThreadActivity::NotLoaded,
        "idle" => ThreadActivity::Idle,
        "systemError" => ThreadActivity::SystemError,
        "active" => {
            let flags = object_field(value, "activeFlags", "thread status")?
                .as_array()
                .context("thread status.activeFlags 必须是数组")?
                .iter()
                .map(|flag| {
                    Ok(
                        match flag
                            .as_str()
                            .context("thread status.activeFlags 项必须是字符串")?
                        {
                            "waitingOnApproval" => AgentThreadActiveFlag::WaitingOnApproval,
                            "waitingOnUserInput" => AgentThreadActiveFlag::WaitingOnUserInput,
                            flag => bail!("thread status.activeFlags 包含未知值 `{flag}`"),
                        },
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            ThreadActivity::Active { flags }
        }
        _ => bail!("thread status.type 包含未知值 `{kind}`"),
    })
}

pub(super) fn parse_thread_summary(value: &Value) -> Result<ThreadSummary> {
    let preview = string_field(value, "preview", "thread")?;
    let name = optional_nullable_string_field(value, "name", "thread")?;
    let title = name
        .filter(|name| !name.trim().is_empty())
        .or_else(|| {
            preview
                .lines()
                .find(|line| !line.trim().is_empty())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "新对话".to_owned());
    let section = match value.as_object().and_then(|object| object.get("section")) {
        None | Some(Value::Null) => None,
        Some(section @ Value::Object(_)) => Some(parse_thread_section(section)?),
        Some(_) => bail!("thread.section 必须是对象或 null"),
    };
    Ok(ThreadSummary {
        thread_id: string_field(value, "id", "thread")?,
        title,
        preview,
        cwd: PathBuf::from(string_field(value, "cwd", "thread")?),
        project_id: nullable_string_field(value, "projectId", "thread")?,
        section,
        created_at: integer_field(value, "createdAt", "thread")?,
        updated_at: integer_field(value, "updatedAt", "thread")?,
        recency_at: optional_nullable_integer_field(value, "recencyAt", "thread")?,
        activity: parse_thread_activity(object_field(value, "status", "thread")?)?,
    })
}

fn parse_file_change_status(value: &str) -> Result<AgentFileChangeStatus> {
    match value {
        "inProgress" => Ok(AgentFileChangeStatus::InProgress),
        "completed" => Ok(AgentFileChangeStatus::Completed),
        "failed" => Ok(AgentFileChangeStatus::Failed),
        "declined" => Ok(AgentFileChangeStatus::Declined),
        other => bail!("fileChange.status 包含未知值 `{other}`"),
    }
}

fn parse_history_file_change(value: &Value, item_id: String) -> Result<AgentFileChange> {
    let changes = object_field(value, "changes", "fileChange item")?
        .as_array()
        .context("fileChange item.changes 必须是数组")?
        .iter()
        .enumerate()
        .map(|(index, change)| {
            let context = format!("fileChange item.changes[{index}]");
            let kind_value = object_field(change, "kind", &context)?;
            let kind_type = string_field(kind_value, "type", &format!("{context}.kind"))?;
            let kind = match kind_type.as_str() {
                "add" => AgentFileChangeKind::Add,
                "delete" => AgentFileChangeKind::Delete,
                "update" => AgentFileChangeKind::Update {
                    move_path: optional_nullable_string_field(
                        kind_value,
                        "move_path",
                        &format!("{context}.kind"),
                    )?,
                },
                other => bail!("{context}.kind.type 包含未知值 `{other}`"),
            };
            Ok(AgentFileChangeEntry {
                path: string_field(change, "path", &context)?,
                diff: string_field(change, "diff", &context)?,
                kind,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(AgentFileChange {
        id: item_id,
        changes,
        status: parse_file_change_status(&string_field(value, "status", "fileChange item")?)?,
    })
}

fn string_array_field(value: &Value, field: &str, context: &str) -> Result<Vec<String>> {
    match value.as_object().and_then(|object| object.get(field)) {
        None => Ok(Vec::new()),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .with_context(|| format!("{context}.{field} 项必须是字符串"))
            })
            .collect(),
        Some(_) => bail!("{context}.{field} 必须是数组"),
    }
}

pub(super) fn parse_history_item(value: &Value) -> Result<ThreadHistoryItem> {
    let kind = string_field(value, "type", "thread item")?;
    let item_id = string_field(value, "id", "thread item")?;
    match kind.as_str() {
        "userMessage" => {
            let content = object_field(value, "content", "userMessage item")?
                .as_array()
                .context("userMessage item.content 必须是数组")?;
            let text = content
                .iter()
                .filter_map(|part| {
                    (part.get("type").and_then(Value::as_str) == Some("text"))
                        .then(|| part.get("text").and_then(Value::as_str))
                        .flatten()
                })
                .collect::<Vec<_>>()
                .join("\n");
            let images = content
                .iter()
                .enumerate()
                .filter_map(
                    |(index, part)| match part.get("type").and_then(Value::as_str) {
                        Some("localImage") => Some(
                            string_field(part, "path", "localImage")
                                .map(|path| UserMessageAttachment::Local(path.into())),
                        ),
                        Some("image") => Some(string_field(part, "url", "image").map(|url| {
                            if url.starts_with("data:image/") {
                                let mut hash = std::collections::hash_map::DefaultHasher::new();
                                url.hash(&mut hash);
                                match super::materialize_image_generation_result(
                                    &format!("user-{index}-{:016x}", hash.finish()),
                                    &url,
                                ) {
                                    Ok(path) => UserMessageAttachment::Local(path),
                                    Err(_) => UserMessageAttachment::Unavailable(
                                        "无法读取图片附件".into(),
                                    ),
                                }
                            } else {
                                UserMessageAttachment::Remote(url)
                            }
                        })),
                        _ => None,
                    },
                )
                .collect::<Result<Vec<_>>>()?;
            let ordered = super::input::restore_attachments(&text, images);
            Ok(ThreadHistoryItem::UserMessage {
                client_message_id: optional_nullable_string_field(
                    value,
                    "clientId",
                    "userMessage item",
                )?,
                images: ordered,
                item_id,
                // The answer envelope contains JSON escaping, not Markdown.
                // Preserve it for the resumed question/answer presentation.
                text: if text
                    .trim_start()
                    .starts_with("<send_user_message_question_reply>")
                {
                    text.trim().to_owned()
                } else {
                    normalize_user_message_for_display(&text)
                },
            })
        }
        "hookPrompt" => Ok(ThreadHistoryItem::HookPrompt(
            super::runtime::parse_hook_prompt(value, None)?,
        )),
        "agentMessage" => Ok(ThreadHistoryItem::AssistantMessage {
            item_id,
            text: string_field(value, "text", "agentMessage item")?,
            phase: optional_nullable_string_field(value, "phase", "agentMessage item")?,
        }),
        "reasoning" => Ok(ThreadHistoryItem::Reasoning {
            item_id,
            summary: string_array_field(value, "summary", "reasoning item")?,
            content: string_array_field(value, "content", "reasoning item")?,
        }),
        "commandExecution" => {
            // Older persisted items can omit cwd/actions. When present, retain
            // the same semantic action labels and shell status as live items.
            let mut item = value
                .as_object()
                .context("commandExecution item 必须是对象")?
                .clone();
            item.entry("commandActions").or_insert_with(|| json!([]));
            item.entry("cwd").or_insert_with(|| json!(""));
            let execution = super::parse_command_execution(&item)?;
            Ok(ThreadHistoryItem::Command {
                item_id,
                command: execution.command,
                output: execution.output,
                status: execution.status,
                actions: execution.actions,
                cwd: (!execution.cwd.is_empty()).then_some(execution.cwd),
                exit_code: execution.exit_code,
            })
        }
        "fileChange" => Ok(ThreadHistoryItem::FileChange(parse_history_file_change(
            value, item_id,
        )?)),
        "imageView" => Ok(ThreadHistoryItem::ImageView(AgentImageView {
            id: item_id,
            path: PathBuf::from(string_field(value, "path", "imageView item")?),
        })),
        "imageGeneration" | "image_generation" => {
            Ok(ThreadHistoryItem::ImageGeneration(parse_image_generation(
                value
                    .as_object()
                    .context("imageGeneration history item 必须是对象")?,
                true,
            )?))
        }
        "contextCompaction" => Ok(ThreadHistoryItem::ContextCompaction(
            crate::agent::AgentContextCompaction {
                id: item_id,
                completed: true,
            },
        )),
        "collabToolCall" | "collabAgentToolCall" | "subAgentActivity" => {
            Ok(ThreadHistoryItem::Collaboration(parse_collaboration(
                value
                    .as_object()
                    .context("collaboration history item 必须是对象")?,
            )?))
        }
        "mcpToolCall" => Ok(ThreadHistoryItem::McpToolCall(Box::from(
            parse_mcp_tool_call(value.as_object().context("mcpToolCall item 必须是对象")?)?,
        ))),
        "plan" | "webSearch" | "sleep" => super::progress::parse_progress_history(value),
        _ => Ok(ThreadHistoryItem::Unsupported { item_id, kind }),
    }
}

pub(super) fn parse_history_turn(value: &Value) -> Result<ThreadTurn> {
    let status = match string_field(value, "status", "turn")?.as_str() {
        "inProgress" => HistoryTurnStatus::InProgress,
        "completed" => HistoryTurnStatus::Completed,
        "interrupted" => HistoryTurnStatus::Interrupted,
        "failed" => HistoryTurnStatus::Failed,
        value => bail!("turn.status 包含未知值 `{value}`"),
    };
    let items_view = match optional_nullable_string_field(value, "itemsView", "turn")?
        .as_deref()
        .unwrap_or("full")
    {
        "notLoaded" => HistoryItemDetail::NotLoaded,
        "summary" => HistoryItemDetail::Summary,
        "full" => HistoryItemDetail::Full,
        value => bail!("turn.itemsView 包含未知值 `{value}`"),
    };
    let items = object_field(value, "items", "turn")?
        .as_array()
        .context("turn.items 必须是数组")?
        .iter()
        .map(parse_history_item)
        .collect::<Result<Vec<_>>>()?;
    let error = match value.as_object().and_then(|object| object.get("error")) {
        None | Some(Value::Null) => None,
        Some(Value::Object(error)) => error
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_owned),
        Some(_) => bail!("turn.error 必须是对象或 null"),
    };
    Ok(ThreadTurn {
        turn_id: string_field(value, "id", "turn")?,
        status,
        items_view,
        items,
        started_at: optional_nullable_integer_field(value, "startedAt", "turn")?,
        completed_at: optional_nullable_integer_field(value, "completedAt", "turn")?,
        duration_ms: optional_nullable_integer_field(value, "durationMs", "turn")?,
        error,
    })
}

pub(super) fn response_result<'a>(response: &'a Value, method: &str) -> Result<&'a Value> {
    response
        .get("result")
        .with_context(|| format!("{method} 响应缺少 result"))
}

/// Decode the shared workspace page envelope without owning connection policy.
/// Entries are decoded before cursors so the first error stays unchanged.
pub(super) fn parse_page<T>(
    response: &Value,
    method: &str,
    parse_entry: impl FnMut(&Value) -> Result<T>,
) -> Result<Page<T>> {
    let result = response_result(response, method)?;
    let context = format!("{method} result");
    let data = object_field(result, "data", &context)?
        .as_array()
        .with_context(|| format!("{context}.data 必须是数组"))?
        .iter()
        .map(parse_entry)
        .collect::<Result<Vec<_>>>()?;
    let (next_cursor, backwards_cursor) = page_cursors(result, &context)?;
    Ok(Page {
        data,
        next_cursor,
        backwards_cursor,
    })
}

pub(super) fn page_cursors(
    result: &Value,
    method: &str,
) -> Result<(Option<String>, Option<String>)> {
    let next = optional_nullable_string_field(result, "nextCursor", method)?;
    let backwards = optional_nullable_string_field(result, "backwardsCursor", method)?;
    Ok((next, backwards))
}

pub(super) fn sort_direction(direction: SortDirection) -> &'static str {
    match direction {
        SortDirection::Ascending => "asc",
        SortDirection::Descending => "desc",
    }
}

pub(super) fn thread_sort_key(sort_key: crate::agent::ThreadSortKey) -> &'static str {
    match sort_key {
        crate::agent::ThreadSortKey::CreatedAt => "created_at",
        crate::agent::ThreadSortKey::UpdatedAt => "updated_at",
        crate::agent::ThreadSortKey::RecencyAt => "recency_at",
        crate::agent::ThreadSortKey::SectionPosition => "section_position",
    }
}

fn insert_filter_value(
    params: &mut serde_json::Map<String, Value>,
    field: &str,
    filter: &FilterValue<String>,
) {
    match filter {
        FilterValue::Any => {}
        FilterValue::None => {
            params.insert(field.to_owned(), Value::Null);
        }
        FilterValue::Value(value) => {
            params.insert(field.to_owned(), Value::String(value.clone()));
        }
    }
}

pub(super) fn thread_list_params(request: &ThreadListRequest) -> Value {
    let mut params = serde_json::Map::new();
    params.insert("cursor".into(), json!(request.page.cursor));
    params.insert("limit".into(), json!(request.page.limit));
    params.insert("archived".into(), json!(request.archived));
    params.insert(
        "sortKey".into(),
        Value::String(thread_sort_key(request.sort_key).to_owned()),
    );
    params.insert(
        "sortDirection".into(),
        Value::String(sort_direction(request.sort_direction).to_owned()),
    );
    if let Some(search_term) = request
        .search_term
        .as_deref()
        .filter(|search_term| !search_term.trim().is_empty())
    {
        params.insert("searchTerm".into(), Value::String(search_term.to_owned()));
    }
    insert_filter_value(&mut params, "projectId", &request.project);
    insert_filter_value(&mut params, "sectionId", &request.section);
    Value::Object(params)
}

#[cfg(test)]
mod page_tests;

#[cfg(test)]
mod resumed_rendering_metadata_tests {
    use super::*;

    #[test]
    fn question_reply_json_escaping_survives_history_normalization() {
        let text = "<send_user_message_question_reply>\n[{\"questionItemId\":\"[\\\"request_user_input_async\\\",\\\"call_1\\\",0]\",\"question\":\"哪里？\",\"answer\":\"左侧栏\"}]\n</send_user_message_question_reply>";
        let item = parse_history_item(
            &json!({"type":"userMessage","id":"reply","content":[{"type":"text","text":text}]}),
        )
        .unwrap();
        let ThreadHistoryItem::UserMessage { text: actual, .. } = item else {
            panic!("user reply")
        };
        assert_eq!(actual, text);
    }

    #[test]
    fn resumed_web_search_preserves_query_actions_and_results() {
        let value = json!({"type":"webSearch","id":"search","query":"字体","action":{"type":"search","queries":["字体"]},"results":[{"url":"https://example.test/","title":"字体"}]});
        let ThreadHistoryItem::WebSearch(crate::agent::AgentWebSearch {
            query,
            action,
            results,
            ..
        }) = parse_history_item(&value).unwrap()
        else {
            panic!("search must not become an unsupported warning")
        };
        assert_eq!(query, "字体");
        assert_eq!(action, value["action"]);
        assert_eq!(results, value["results"]);
    }
}
