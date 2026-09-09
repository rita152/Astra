//! Interactive request decoding, dispatch, and resolution notifications.

use std::{collections::HashSet, io::Write, path::Path, sync::Arc};

use anyhow::{Context as _, Result, anyhow, bail};
use async_channel::Sender;
use serde_json::{Value, json};

use super::{
    approvals::{parse_command_approval_request, parse_file_approval_request},
    registry::ServerRequestResolution,
    session::CodexTurnSession,
};
use crate::agent::{
    AgentAdditionalFileSystemPermissions, AgentAdditionalNetworkPermissions, AgentApprovalControl,
    AgentApprovalHandle, AgentCommandApprovalRequest, AgentEvent, AgentFileApprovalControl,
    AgentFileApprovalHandle, AgentFileApprovalRequest, AgentFileSystemAccess, AgentFileSystemPath,
    AgentFileSystemPermissionEntry, AgentFileSystemSpecialPath, AgentOptionalField,
    AgentPermissionRequestProfile, AgentPermissionsApprovalControl, AgentPermissionsApprovalHandle,
    AgentPermissionsApprovalRequest, AgentServerRequestId, AgentServerRequestKind,
    AgentServerRequestMetadata, AgentUserInputControl, AgentUserInputHandle, AgentUserInputOption,
    AgentUserInputQuestion, AgentUserInputRequest,
};

pub(super) fn request_id_from_value(value: &Value) -> Result<AgentServerRequestId> {
    match value {
        Value::String(id) => Ok(AgentServerRequestId::String(id.clone())),
        Value::Number(id) => id
            .as_i64()
            .map(AgentServerRequestId::Number)
            .context("Codex JSON-RPC request id 数字超出 int64 范围"),
        _ => bail!("Codex JSON-RPC request id 必须是字符串或 int64 数字"),
    }
}

pub(super) fn request_id_value(request_id: &AgentServerRequestId) -> Value {
    match request_id {
        AgentServerRequestId::Number(id) => json!(id),
        AgentServerRequestId::String(id) => json!(id),
    }
}

pub(super) fn request_metadata_for_command(
    request: &AgentCommandApprovalRequest,
) -> AgentServerRequestMetadata {
    AgentServerRequestMetadata {
        request_id: request.request_id.clone(),
        thread_id: request.thread_id.clone(),
        turn_id: request.turn_id.clone(),
        item_id: request.item_id.clone(),
        kind: AgentServerRequestKind::CommandApproval,
    }
}

pub(super) fn request_metadata_for_file(
    request: &AgentFileApprovalRequest,
) -> AgentServerRequestMetadata {
    AgentServerRequestMetadata {
        request_id: request.request_id.clone(),
        thread_id: request.thread_id.clone(),
        turn_id: request.turn_id.clone(),
        item_id: request.item_id.clone(),
        kind: AgentServerRequestKind::FileApproval,
    }
}

pub(super) fn request_metadata_for_user_input(
    request: &AgentUserInputRequest,
) -> AgentServerRequestMetadata {
    AgentServerRequestMetadata {
        request_id: request.request_id.clone(),
        thread_id: request.thread_id.clone(),
        turn_id: request.turn_id.clone(),
        item_id: request.item_id.clone(),
        kind: AgentServerRequestKind::UserInput,
    }
}

pub(super) fn request_metadata_for_permissions(
    request: &AgentPermissionsApprovalRequest,
) -> AgentServerRequestMetadata {
    AgentServerRequestMetadata {
        request_id: request.request_id.clone(),
        thread_id: request.thread_id.clone(),
        turn_id: request.turn_id.clone(),
        item_id: request.item_id.clone(),
        kind: AgentServerRequestKind::PermissionsApproval,
    }
}

pub(super) fn respond_to_server_request_on_session<W: Write + Send + 'static>(
    session: &Arc<CodexTurnSession<W>>,
    message: &Value,
    events: &Sender<AgentEvent>,
) -> Result<()> {
    let Some(id) = message.get("id") else {
        return Ok(());
    };
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        return Ok(());
    };
    if method == "item/commandExecution/requestApproval" {
        let (request_id, request, params, available_decisions) =
            match parse_command_approval_request(message) {
                Ok(parsed) => parsed,
                Err(error) => {
                    return reject_server_request(
                        session,
                        message,
                        -32602,
                        "Invalid item/commandExecution/requestApproval params",
                        error,
                    );
                }
            };
        session.register_command_approval(
            request_metadata_for_command(&request),
            params,
            available_decisions,
        )?;
        let control: Arc<dyn AgentApprovalControl> = session.clone();
        let responder = AgentApprovalHandle::new(request_id, control);
        if events
            .send_blocking(AgentEvent::CommandApprovalRequested { request, responder })
            .is_err()
        {
            let error = match session.drain_pending_server_requests() {
                Ok(_) => anyhow!("Composer command approval 事件通道已经关闭"),
                Err(cleanup_error) => anyhow!(
                    "Composer command approval 事件通道已经关闭，且 pending request 清理失败：{cleanup_error:#}"
                ),
            };
            return reject_server_request(
                session,
                message,
                -32603,
                "Unable to present server request",
                error,
            );
        }
        return Ok(());
    }
    if method == "item/fileChange/requestApproval" {
        let request = match parse_file_approval_request(message) {
            Ok(request) => request,
            Err(error) => {
                return reject_server_request(
                    session,
                    message,
                    -32602,
                    "Invalid item/fileChange/requestApproval params",
                    error,
                );
            }
        };
        // A duplicate id already identifies another response. Never send an
        // error under that same id and accidentally answer the original prompt.
        session.register_file_approval(&request)?;
        let control: Arc<dyn AgentFileApprovalControl> = session.clone();
        let responder = AgentFileApprovalHandle::new(request.request_id.clone(), control);
        if events
            .send_blocking(AgentEvent::FileApprovalRequested { request, responder })
            .is_err()
        {
            session.drain_pending_server_requests()?;
            return reject_server_request(
                session,
                message,
                -32603,
                "Unable to present server request",
                anyhow!("Composer file approval 事件通道已经关闭"),
            );
        }
        return Ok(());
    }
    if matches!(
        method,
        "item/tool/requestUserInput" | "tool/requestUserInput"
    ) {
        let (request_id, request) = match parse_user_input_request(message) {
            Ok(parsed) => parsed,
            Err(error) => {
                return reject_server_request(
                    session,
                    message,
                    -32602,
                    "Invalid item/tool/requestUserInput params",
                    error,
                );
            }
        };
        if let Err(error) = session.register_user_input(&request) {
            return reject_server_request(
                session,
                message,
                -32600,
                "Duplicate or invalid server request",
                error,
            );
        }
        let control: Arc<dyn AgentUserInputControl> = session.clone();
        let responder = AgentUserInputHandle::new(request_id, control);
        if events
            .send_blocking(AgentEvent::UserInputRequested { request, responder })
            .is_err()
        {
            let error = match session.drain_pending_server_requests() {
                Ok(_) => anyhow!("Composer user input 事件通道已经关闭"),
                Err(cleanup_error) => anyhow!(
                    "Composer user input 事件通道已经关闭，且 pending request 清理失败：{cleanup_error:#}"
                ),
            };
            return reject_server_request(
                session,
                message,
                -32603,
                "Unable to present server request",
                error,
            );
        }
        return Ok(());
    }
    if method == "item/permissions/requestApproval" {
        let (request_id, request) = match parse_permissions_approval_request(message) {
            Ok(parsed) => parsed,
            Err(error) => {
                return reject_server_request(
                    session,
                    message,
                    -32602,
                    "Invalid item/permissions/requestApproval params",
                    error,
                );
            }
        };
        if let Err(error) = session.register_permissions_approval(&request) {
            return reject_server_request(
                session,
                message,
                -32600,
                "Duplicate or invalid server request",
                error,
            );
        }
        let control: Arc<dyn AgentPermissionsApprovalControl> = session.clone();
        let responder = AgentPermissionsApprovalHandle::new(request_id, control);
        if events
            .send_blocking(AgentEvent::PermissionsApprovalRequested { request, responder })
            .is_err()
        {
            let error = match session.drain_pending_server_requests() {
                Ok(_) => anyhow!("Composer permissions approval 事件通道已经关闭"),
                Err(cleanup_error) => anyhow!(
                    "Composer permissions approval 事件通道已经关闭，且 pending request 清理失败：{cleanup_error:#}"
                ),
            };
            return reject_server_request(
                session,
                message,
                -32603,
                "Unable to present server request",
                error,
            );
        }
        return Ok(());
    }
    session.send(json!({
        "id": id,
        "error": {
            "code": -32601,
            "message": "This minimal client does not implement server-initiated requests"
        }
    }))
}

pub(super) fn reject_server_request<W: Write + Send>(
    session: &CodexTurnSession<W>,
    message: &Value,
    code: i64,
    response_message: &str,
    error: anyhow::Error,
) -> Result<()> {
    let id = message
        .get("id")
        .filter(|id| matches!(id, Value::String(_) | Value::Number(_)))
        .cloned()
        .unwrap_or(Value::Null);
    let response = session.send(json!({
        "id": id,
        "error": {
            "code": code,
            "message": response_message
        }
    }));
    match response {
        Ok(()) => Err(error),
        Err(response_error) => Err(anyhow!(
            "{error:#}; 同时无法写入 JSON-RPC error response：{response_error:#}"
        )),
    }
}

pub(super) fn required_request_string(
    params: &serde_json::Map<String, Value>,
    method: &str,
    field: &str,
) -> Result<String> {
    params
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("{method} params.{field} 必须是字符串"))
}

pub(super) fn optional_request_string(
    params: &serde_json::Map<String, Value>,
    method: &str,
    field: &str,
) -> Result<Option<String>> {
    match params.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => bail!("{method} params.{field} 必须是字符串或 null"),
    }
}

pub(super) fn parse_user_input_request(
    message: &Value,
) -> Result<(AgentServerRequestId, AgentUserInputRequest)> {
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .filter(|method| {
            matches!(
                *method,
                "item/tool/requestUserInput" | "tool/requestUserInput"
            )
        })
        .context("user input request method 未接入")?;
    let request_id = request_id_from_value(
        message
            .get("id")
            .context("user input request 缺少 JSON-RPC id")?,
    )?;
    let params = message
        .get("params")
        .and_then(Value::as_object)
        .with_context(|| format!("{method} 缺少对象 params"))?;
    let thread_id = required_request_string(params, method, "threadId")?;
    let turn_id = required_request_string(params, method, "turnId")?;
    let item_id = required_request_string(params, method, "itemId")?;
    let is_blocking = params
        .get("isBlocking")
        .and_then(Value::as_bool)
        .with_context(|| format!("{method} params.isBlocking 必须是布尔值"))?;
    let auto_resolution_ms =
        match params.get("autoResolutionMs") {
            None | Some(Value::Null) => None,
            Some(value) => Some(value.as_u64().with_context(|| {
                format!("{method} params.autoResolutionMs 必须是 uint64 或 null")
            })?),
        };
    let questions = params
        .get("questions")
        .and_then(Value::as_array)
        .with_context(|| format!("{method} params.questions 必须是数组"))?;
    let mut question_ids = HashSet::new();
    let mut parsed_questions = Vec::with_capacity(questions.len());
    for (index, question) in questions.iter().enumerate() {
        let question = question
            .as_object()
            .with_context(|| format!("{method} params.questions[{index}] 必须是对象"))?;
        let id = question
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .with_context(|| format!("{method} params.questions[{index}].id 必须是字符串"))?;
        if !question_ids.insert(id.clone()) {
            bail!("{method} 包含重复 question id `{id}`");
        }
        let header = question
            .get("header")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .with_context(|| format!("{method} params.questions[{index}].header 必须是字符串"))?;
        let question_text = question
            .get("question")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .with_context(|| format!("{method} params.questions[{index}].question 必须是字符串"))?;
        let allows_other = match question.get("isOther") {
            None => false,
            Some(value) => value.as_bool().with_context(|| {
                format!("{method} params.questions[{index}].isOther 必须是布尔值")
            })?,
        };
        let is_secret = match question.get("isSecret") {
            None => false,
            Some(value) => value.as_bool().with_context(|| {
                format!("{method} params.questions[{index}].isSecret 必须是布尔值")
            })?,
        };
        let options = match question.get("options") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(options)) => {
                let mut parsed = Vec::with_capacity(options.len());
                for (option_index, option) in options.iter().enumerate() {
                    let option = option.as_object().with_context(|| {
                        format!(
                            "{method} params.questions[{index}].options[{option_index}] 必须是对象"
                        )
                    })?;
                    let label = option
                        .get("label")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .with_context(|| {
                            format!(
                                "{method} params.questions[{index}].options[{option_index}].label 必须是字符串"
                            )
                        })?;
                    let description = option
                        .get("description")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .with_context(|| {
                            format!(
                                "{method} params.questions[{index}].options[{option_index}].description 必须是字符串"
                            )
                        })?;
                    parsed.push(AgentUserInputOption { label, description });
                }
                parsed
            }
            Some(_) => bail!("{method} params.questions[{index}].options 必须是数组或 null"),
        };
        parsed_questions.push(AgentUserInputQuestion {
            id,
            header,
            question: question_text,
            options,
            allows_other,
            is_secret,
        });
    }
    Ok((
        request_id.clone(),
        AgentUserInputRequest {
            request_id,
            thread_id,
            turn_id,
            item_id,
            questions: parsed_questions,
            is_blocking,
            auto_resolution_ms,
        },
    ))
}

pub(super) fn parse_optional_field<T>(
    object: &serde_json::Map<String, Value>,
    field: &str,
    parse: impl FnOnce(&Value) -> Result<T>,
) -> Result<AgentOptionalField<T>> {
    match object.get(field) {
        None => Ok(AgentOptionalField::Unspecified),
        Some(Value::Null) => Ok(AgentOptionalField::Null),
        Some(value) => parse(value).map(AgentOptionalField::Value),
    }
}

pub(super) fn parse_permissions_approval_request(
    message: &Value,
) -> Result<(AgentServerRequestId, AgentPermissionsApprovalRequest)> {
    const METHOD: &str = "item/permissions/requestApproval";
    let request_id = request_id_from_value(
        message
            .get("id")
            .context("permissions approval request 缺少 JSON-RPC id")?,
    )?;
    let params = message
        .get("params")
        .and_then(Value::as_object)
        .context("item/permissions/requestApproval 缺少对象 params")?;
    let thread_id = required_request_string(params, METHOD, "threadId")?;
    let turn_id = required_request_string(params, METHOD, "turnId")?;
    let item_id = required_request_string(params, METHOD, "itemId")?;
    let cwd = required_request_string(params, METHOD, "cwd")?;
    let cwd_path = Path::new(&cwd);
    if !cwd_path.is_absolute()
        || cwd_path.components().any(|component| {
            matches!(
                component,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
    {
        bail!("item/permissions/requestApproval params.cwd 必须是规范化绝对路径");
    }
    let started_at_ms = params
        .get("startedAtMs")
        .and_then(Value::as_i64)
        .context("item/permissions/requestApproval params.startedAtMs 必须是 int64")?;
    let environment_id = optional_request_string(params, METHOD, "environmentId")?;
    let reason = optional_request_string(params, METHOD, "reason")?;
    let permissions = parse_permission_request_profile(
        params
            .get("permissions")
            .context("params.permissions 缺失")?,
    )?;
    Ok((
        request_id.clone(),
        AgentPermissionsApprovalRequest {
            request_id,
            thread_id,
            turn_id,
            item_id,
            environment_id,
            started_at_ms,
            cwd,
            reason,
            permissions,
        },
    ))
}

pub(super) fn parse_permission_request_profile(
    value: &Value,
) -> Result<AgentPermissionRequestProfile> {
    let permissions = value.as_object().context("params.permissions 必须是对象")?;
    if let Some(field) = permissions
        .keys()
        .find(|field| !matches!(field.as_str(), "fileSystem" | "network"))
    {
        bail!(
            "item/permissions/requestApproval params.permissions 包含 schema 未定义字段 `{field}`"
        );
    }
    let file_system = parse_optional_field(permissions, "fileSystem", |value| {
        parse_additional_file_system_permissions(value)
    })?;
    let network = parse_optional_field(permissions, "network", |value| {
        parse_additional_network_permissions(value)
    })?;
    Ok(AgentPermissionRequestProfile {
        file_system,
        network,
    })
}

pub(super) fn parse_additional_network_permissions(
    value: &Value,
) -> Result<AgentAdditionalNetworkPermissions> {
    let object = value
        .as_object()
        .context("item/permissions/requestApproval params.permissions.network 必须是对象或 null")?;
    Ok(AgentAdditionalNetworkPermissions {
        enabled: parse_optional_field(object, "enabled", |value| {
            value.as_bool().context(
                "item/permissions/requestApproval params.permissions.network.enabled 必须是布尔值或 null",
            )
        })?,
    })
}

pub(super) fn parse_additional_file_system_permissions(
    value: &Value,
) -> Result<AgentAdditionalFileSystemPermissions> {
    let object = value.as_object().context(
        "item/permissions/requestApproval params.permissions.fileSystem 必须是对象或 null",
    )?;
    let parse_paths = |value: &Value, field: &str| -> Result<Vec<String>> {
        value
            .as_array()
            .with_context(|| {
                format!(
                    "item/permissions/requestApproval params.permissions.fileSystem.{field} 必须是字符串数组或 null"
                )
            })?
            .iter()
            .enumerate()
            .map(|(index, value)| {
                value.as_str().map(str::to_owned).with_context(|| {
                    format!(
                        "item/permissions/requestApproval params.permissions.fileSystem.{field}[{index}] 必须是字符串"
                    )
                })
            })
            .collect()
    };
    let read = parse_optional_field(object, "read", |value| parse_paths(value, "read"))?;
    let write = parse_optional_field(object, "write", |value| parse_paths(value, "write"))?;
    let glob_scan_max_depth = parse_optional_field(object, "globScanMaxDepth", |value| {
        let depth = value.as_u64().context(
            "item/permissions/requestApproval params.permissions.fileSystem.globScanMaxDepth 必须是正整数或 null",
        )?;
        if depth == 0 {
            bail!(
                "item/permissions/requestApproval params.permissions.fileSystem.globScanMaxDepth 必须至少为 1"
            );
        }
        Ok(depth)
    })?;
    let entries = parse_optional_field(object, "entries", |value| {
        value
            .as_array()
            .context(
                "item/permissions/requestApproval params.permissions.fileSystem.entries 必须是数组或 null",
            )?
            .iter()
            .enumerate()
            .map(|(index, value)| parse_file_system_permission_entry(value, index))
            .collect()
    })?;
    Ok(AgentAdditionalFileSystemPermissions {
        read,
        write,
        glob_scan_max_depth,
        entries,
    })
}

pub(super) fn parse_file_system_permission_entry(
    value: &Value,
    index: usize,
) -> Result<AgentFileSystemPermissionEntry> {
    let object = value.as_object().with_context(|| {
        format!(
            "item/permissions/requestApproval params.permissions.fileSystem.entries[{index}] 必须是对象"
        )
    })?;
    let access = match object.get("access").and_then(Value::as_str) {
        Some("read") => AgentFileSystemAccess::Read,
        Some("write") => AgentFileSystemAccess::Write,
        Some("deny") => AgentFileSystemAccess::Deny,
        Some(other) => bail!(
            "item/permissions/requestApproval params.permissions.fileSystem.entries[{index}].access 包含未知值 `{other}`"
        ),
        None => bail!(
            "item/permissions/requestApproval params.permissions.fileSystem.entries[{index}].access 必须是字符串"
        ),
    };
    let path = parse_file_system_path(
        object.get("path").with_context(|| {
            format!(
                "item/permissions/requestApproval params.permissions.fileSystem.entries[{index}] 缺少 path"
            )
        })?,
        index,
    )?;
    Ok(AgentFileSystemPermissionEntry { path, access })
}

pub(super) fn parse_file_system_path(
    value: &Value,
    entry_index: usize,
) -> Result<AgentFileSystemPath> {
    let object = value.as_object().with_context(|| {
        format!(
            "item/permissions/requestApproval params.permissions.fileSystem.entries[{entry_index}].path 必须是对象"
        )
    })?;
    match object.get("type").and_then(Value::as_str) {
        Some("path") => Ok(AgentFileSystemPath::Path(
            object
                .get("path")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .with_context(|| {
                    format!(
                        "item/permissions/requestApproval params.permissions.fileSystem.entries[{entry_index}].path.path 必须是字符串"
                    )
                })?,
        )),
        Some("glob_pattern") => Ok(AgentFileSystemPath::GlobPattern(
            object
                .get("pattern")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .with_context(|| {
                    format!(
                        "item/permissions/requestApproval params.permissions.fileSystem.entries[{entry_index}].path.pattern 必须是字符串"
                    )
                })?,
        )),
        Some("special") => Ok(AgentFileSystemPath::Special(parse_file_system_special_path(
            object.get("value").with_context(|| {
                format!(
                    "item/permissions/requestApproval params.permissions.fileSystem.entries[{entry_index}].path 缺少 value"
                )
            })?,
            entry_index,
        )?)),
        Some(other) => bail!(
            "item/permissions/requestApproval params.permissions.fileSystem.entries[{entry_index}].path.type 包含未知值 `{other}`"
        ),
        None => bail!(
            "item/permissions/requestApproval params.permissions.fileSystem.entries[{entry_index}].path.type 必须是字符串"
        ),
    }
}

pub(super) fn parse_file_system_special_path(
    value: &Value,
    entry_index: usize,
) -> Result<AgentFileSystemSpecialPath> {
    let object = value.as_object().with_context(|| {
        format!(
            "item/permissions/requestApproval params.permissions.fileSystem.entries[{entry_index}].path.value 必须是对象"
        )
    })?;
    let optional_subpath = || {
        parse_optional_field(object, "subpath", |value| {
            value.as_str().map(str::to_owned).with_context(|| {
                format!(
                    "item/permissions/requestApproval params.permissions.fileSystem.entries[{entry_index}].path.value.subpath 必须是字符串或 null"
                )
            })
        })
    };
    match object.get("kind").and_then(Value::as_str) {
        Some("root") => Ok(AgentFileSystemSpecialPath::Root),
        Some("minimal") => Ok(AgentFileSystemSpecialPath::Minimal),
        Some("project_roots") => Ok(AgentFileSystemSpecialPath::ProjectRoots {
            subpath: optional_subpath()?,
        }),
        Some("tmpdir") => Ok(AgentFileSystemSpecialPath::Tmpdir),
        Some("slash_tmp") => Ok(AgentFileSystemSpecialPath::SlashTmp),
        Some("unknown") => Ok(AgentFileSystemSpecialPath::Unknown {
            path: object
                .get("path")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .with_context(|| {
                    format!(
                        "item/permissions/requestApproval params.permissions.fileSystem.entries[{entry_index}].path.value.path 必须是字符串"
                    )
                })?,
            subpath: optional_subpath()?,
        }),
        Some(other) => bail!(
            "item/permissions/requestApproval params.permissions.fileSystem.entries[{entry_index}].path.value.kind 包含未知值 `{other}`"
        ),
        None => bail!(
            "item/permissions/requestApproval params.permissions.fileSystem.entries[{entry_index}].path.value.kind 必须是字符串"
        ),
    }
}

pub(super) fn handle_server_request_resolved<W: Write + Send>(
    session: &CodexTurnSession<W>,
    message: &Value,
    events: &Sender<AgentEvent>,
) -> Result<()> {
    if message.get("method").and_then(Value::as_str) != Some("serverRequest/resolved") {
        return Ok(());
    }
    let thread_id = message
        .pointer("/params/threadId")
        .and_then(Value::as_str)
        .context("serverRequest/resolved 缺少字符串 params.threadId")?;
    let request_id = request_id_from_value(
        message
            .pointer("/params/requestId")
            .context("serverRequest/resolved 缺少 params.requestId")?,
    )?;
    match session.resolve_server_request(&request_id, thread_id)? {
        ServerRequestResolution::AlreadyResolved => Ok(()),
        ServerRequestResolution::Resolved(request) => events
            .send_blocking(AgentEvent::ServerRequestResolved { request })
            .map_err(|_| anyhow!("Composer server request resolved 事件通道已经关闭")),
    }
}
