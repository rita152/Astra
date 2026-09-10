//! Scripted protocol drivers used only by adapter regression tests.

use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Command, Stdio},
    sync::Arc,
};

use anyhow::{Context as _, Result, anyhow, bail};
use async_channel::Sender;
use serde_json::{Value, json};

use super::{
    catalog::{MODEL_LIST_PAGE_SIZE, ModelListResponse},
    dispatch::process_turn_message,
    methods::{
        TURN_SCOPED_SERVER_METHODS, ensure_server_method_is_defined,
        is_integrated_server_request_method,
    },
    notifications::{
        forward_agent_notification, parse_agent_notification, thread_started_id,
        validate_resume_goal_cleared,
    },
    permissions::{PermissionFields, permission_fields, thread_settings_update_request},
    requests::{
        handle_server_request_resolved, reject_server_request, respond_to_server_request_on_session,
    },
    session::{CodexTurnSession, TurnOutcome, ensure_session_message_matches},
    transport::send,
};
use crate::agent::{
    AgentEvent, AgentModel, AgentModelCatalog, AgentPermissionMode, AgentPermissionProfile,
    AgentRequest, AgentThreadSettings,
};

#[cfg(test)]
pub(super) const INITIALIZE_ID: u64 = 1;

#[cfg(test)]
pub(super) const THREAD_REQUEST_ID: u64 = 2;

#[cfg(test)]
pub(super) const TURN_START_ID: u64 = 3;

#[cfg(test)]
pub(super) const THREAD_SETTINGS_UPDATE_ID: u64 = 2;

#[cfg(test)]
pub(super) const PERMISSION_PROFILE_LIST_ID: u64 = 2;

#[cfg(test)]
pub(super) const MODEL_LIST_FIRST_ID: u64 = 2;

#[cfg(test)]
#[derive(Default)]
pub(super) struct ThreadStartedCorrelation {
    pub(super) expected_thread_id: Option<String>,
    pub(super) observed_thread_id: Option<String>,
}

#[cfg(test)]
impl ThreadStartedCorrelation {
    pub(super) fn expect(&mut self, thread_id: &str) -> Result<()> {
        if let Some(expected_thread_id) = &self.expected_thread_id
            && expected_thread_id != thread_id
        {
            bail!(
                "当前会话 thread id `{expected_thread_id}` 与新的 canonical thread id `{thread_id}` 不一致"
            );
        }
        if let Some(observed_thread_id) = &self.observed_thread_id
            && observed_thread_id != thread_id
        {
            bail!(
                "thread/started 通知的 thread id `{observed_thread_id}` 与 canonical thread id `{thread_id}` 不一致"
            );
        }
        self.expected_thread_id = Some(thread_id.to_owned());
        Ok(())
    }

    pub(super) fn observe(&mut self, message: &Value) -> Result<()> {
        let thread_id = thread_started_id(message)?;
        if let Some(observed_thread_id) = &self.observed_thread_id
            && observed_thread_id != &thread_id
        {
            bail!(
                "连续 thread/started 通知的 thread id 不一致：先收到 `{observed_thread_id}`，随后收到 `{thread_id}`"
            );
        }
        if let Some(expected_thread_id) = &self.expected_thread_id
            && expected_thread_id != &thread_id
        {
            bail!(
                "thread/started 通知的 thread id `{thread_id}` 与当前会话的 canonical thread id `{expected_thread_id}` 不一致"
            );
        }
        self.observed_thread_id = Some(thread_id);
        Ok(())
    }
}

#[cfg(test)]
pub(super) fn finish_prompt_session<W: Write + Send>(
    session: &CodexTurnSession<W>,
    result: Result<TurnOutcome>,
) -> AgentEvent {
    session.mark_terminal();
    let cleanup = session.finish();
    match (result, cleanup) {
        (Ok(outcome), Ok(())) => outcome.into_event(),
        (Err(error), Ok(())) => AgentEvent::Failed(format!("{error:#}")),
        (Ok(TurnOutcome::Failed(message)), Err(error)) => AgentEvent::Failed(format!(
            "{message}\nCodex turn 已失败，且 app-server 资源回收失败：{error:#}"
        )),
        (Ok(_), Err(error)) => AgentEvent::Failed(format!(
            "Codex turn 已结束，但 app-server 资源回收失败：{error:#}"
        )),
        (Err(error), Err(cleanup_error)) => AgentEvent::Failed(format!(
            "{error:#}\nCodex app-server 资源回收同时失败：{cleanup_error:#}"
        )),
    }
}

#[cfg(test)]
pub(super) fn run_model_catalog_process() -> Result<AgentModelCatalog> {
    let mut child = Command::new("codex")
        .args(["app-server", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("无法启动 `codex app-server --stdio`；请确认 Codex CLI 已安装并完成登录")?;

    let stdout = child
        .stdout
        .take()
        .context("无法读取 Codex app-server stdout")?;
    let mut stdin = child
        .stdin
        .take()
        .context("无法写入 Codex app-server stdin")?;
    let mut reader = BufReader::new(stdout);
    let result = drive_model_catalog(&mut reader, &mut stdin);

    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    result
}

#[cfg(test)]
pub(super) fn initialize_connection<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    events: Option<&Sender<AgentEvent>>,
) -> Result<()> {
    send(
        writer,
        json!({
            "method": "initialize",
            "id": INITIALIZE_ID,
            "params": {
                "clientInfo": {
                    "name": "gpui_chat_clone",
                    "title": "GPUI Chat Clone",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": {
                    "experimentalApi": true,
                    "requestAttestation": false,
                    "optOutNotificationMethods": super::runtime::OPT_OUT_NOTIFICATION_METHODS
                }
            }
        }),
    )?;
    wait_for_response(reader, writer, INITIALIZE_ID, events)?;
    send(writer, json!({ "method": "initialized", "params": {} }))
}

#[cfg(test)]
pub(super) fn initialize_turn_connection<R: BufRead, W: Write + Send + 'static>(
    reader: &mut R,
    session: &Arc<CodexTurnSession<W>>,
    events: &Sender<AgentEvent>,
) -> Result<()> {
    session.send(json!({
        "method": "initialize",
        "id": INITIALIZE_ID,
        "params": {
            "clientInfo": {
                "name": "gpui_chat_clone",
                "title": "GPUI Chat Clone",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "experimentalApi": true,
                "requestAttestation": false,
                    "optOutNotificationMethods": super::runtime::OPT_OUT_NOTIFICATION_METHODS
            }
        }
    }))?;
    wait_for_session_response(reader, session, INITIALIZE_ID, events, None, None, None)?;
    session.send(json!({ "method": "initialized", "params": {} }))
}

#[cfg(test)]
pub(super) fn drive_model_catalog<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
) -> Result<AgentModelCatalog> {
    initialize_connection(reader, writer, None)?;

    let mut models = Vec::new();
    let mut cursor: Option<String> = None;
    let mut request_id = MODEL_LIST_FIRST_ID;
    let mut seen_cursors = std::collections::HashSet::new();
    loop {
        send(
            writer,
            json!({
                "method": "model/list",
                "id": request_id,
                "params": {
                    "cursor": cursor,
                    "limit": MODEL_LIST_PAGE_SIZE,
                    "includeHidden": false
                }
            }),
        )?;
        let response = wait_for_response(reader, writer, request_id, None)?;
        let result = response
            .get("result")
            .cloned()
            .context("model/list 响应缺少 result")?;
        let page: ModelListResponse = serde_json::from_value(result)
            .context("无法解析 model/list 响应；本机 Codex CLI schema 可能已变化")?;
        models.extend(
            page.data
                .into_iter()
                .filter(|entry| !entry.hidden)
                .map(AgentModel::from),
        );

        let Some(next_cursor) = page.next_cursor else {
            break;
        };
        if !seen_cursors.insert(next_cursor.clone()) {
            bail!("model/list 返回了重复分页 cursor `{next_cursor}`");
        }
        cursor = Some(next_cursor);
        request_id = request_id
            .checked_add(1)
            .context("model/list 分页请求 id 溢出")?;
    }

    if models.is_empty() {
        bail!("model/list 未返回可显示的模型");
    }
    Ok(AgentModelCatalog { models })
}

#[cfg(test)]
pub(super) fn drive_permission_profiles<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    cwd: &Path,
) -> Result<Vec<AgentPermissionProfile>> {
    initialize_connection(reader, writer, None)?;
    let mut id = PERMISSION_PROFILE_LIST_ID;
    super::catalog::permission_profile_pages(cwd, |params| {
        let request_id = id;
        id += 1;
        send(
            writer,
            json!({"method":"permissionProfile/list","id":request_id,"params":params}),
        )?;
        wait_for_response(reader, writer, request_id, None)
    })
}

#[cfg(test)]
pub(super) fn drive_thread_settings_update<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    thread_id: &str,
    cwd: &Path,
    mode: AgentPermissionMode,
) -> Result<AgentThreadSettings> {
    initialize_connection(reader, writer, None)?;
    send(
        writer,
        thread_settings_update_request(THREAD_SETTINGS_UPDATE_ID, thread_id, cwd, mode)?,
    )?;
    let mut response_ok = false;
    let mut effective = None;
    loop {
        let message = read_message(reader)?;
        respond_to_server_request(writer, &message)?;
        ensure_server_method_is_defined(&message)?;
        if message.get("id").and_then(Value::as_u64) == Some(THREAD_SETTINGS_UPDATE_ID) {
            if let Some(error) = message.get("error") {
                bail!("thread/settings/update 失败：{error}");
            }
            response_ok = true;
        }
        if message.get("method").and_then(Value::as_str) == Some("thread/settings/updated") {
            if message.pointer("/params/threadId").and_then(Value::as_str) != Some(thread_id) {
                continue;
            }
            let event = parse_agent_notification(&message)?;
            if let Some(AgentEvent::ThreadSettingsUpdated(settings)) = event
                && settings.permissions.is_some()
            {
                effective = Some(settings);
            }
        }
        if response_ok && let Some(settings) = effective.take() {
            return Ok(settings);
        }
    }
}

#[cfg(test)]
pub(super) fn drive_session<R: BufRead, W: Write + Send + 'static>(
    reader: &mut R,
    session: &Arc<CodexTurnSession<W>>,
    request: &AgentRequest,
    events: &Sender<AgentEvent>,
) -> Result<TurnOutcome> {
    initialize_turn_connection(reader, session, events)?;
    let is_new_thread = request.thread_id.is_none();
    let mut deferred_turn_notifications = Vec::new();
    let mut thread_started_correlation = ThreadStartedCorrelation::default();
    let thread_id = match &request.thread_id {
        Some(expected_thread_id) => {
            thread_started_correlation
                .expect(expected_thread_id)
                .context("无法建立 thread/resume 生命周期关联")?;
            session.send(json!({
                "method": "thread/resume",
                "id": THREAD_REQUEST_ID,
                "params": {
                    "threadId": expected_thread_id,
                    "excludeTurns": true
                }
            }))?;
            let thread_response = wait_for_session_response(
                reader,
                session,
                THREAD_REQUEST_ID,
                events,
                Some(&mut thread_started_correlation),
                Some(&mut deferred_turn_notifications),
                Some(expected_thread_id),
            )
            .with_context(|| format!("thread/resume `{expected_thread_id}` 失败"))?;
            let resumed_thread_id = thread_response
                .pointer("/result/thread/id")
                .and_then(Value::as_str)
                .context("thread/resume 响应缺少字符串 result.thread.id")?;
            if resumed_thread_id != expected_thread_id {
                bail!(
                    "thread/resume 响应的 thread id `{resumed_thread_id}` 与请求的 `{expected_thread_id}` 不一致"
                );
            }
            thread_started_correlation
                .expect(resumed_thread_id)
                .context("thread/resume 通知与响应不一致")?;
            resumed_thread_id.to_owned()
        }
        None => {
            session.send(json!({
                "method": "thread/start",
                "id": THREAD_REQUEST_ID,
                "params": {
                    "cwd": request.cwd,
                    "ephemeral": false,
                    "serviceName": "gpui-chat-clone",
                    "model": request.model,
                    "serviceTier": request.service_tier
                }
            }))?;
            let thread_response = wait_for_session_response(
                reader,
                session,
                THREAD_REQUEST_ID,
                events,
                Some(&mut thread_started_correlation),
                Some(&mut deferred_turn_notifications),
                None,
            )
            .context("thread/start 失败")?;
            let thread_id = thread_response
                .pointer("/result/thread/id")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .context("thread/start 响应缺少字符串 result.thread.id")?;
            thread_started_correlation
                .expect(&thread_id)
                .context("thread/start 通知与响应不一致")?;
            events
                .send_blocking(AgentEvent::ThreadCreated {
                    thread_id: thread_id.clone(),
                })
                .map_err(|_| anyhow!("Composer thread created 事件通道已经关闭"))?;
            thread_id
        }
    };

    let mut turn_params = serde_json::Map::new();
    turn_params.insert("threadId".into(), json!(thread_id));
    turn_params.insert(
        "input".into(),
        json!([{ "type": "text", "text": request.prompt }]),
    );
    turn_params.insert("model".into(), json!(request.model));
    turn_params.insert("effort".into(), json!(request.effort));
    turn_params.insert("serviceTier".into(), json!(request.service_tier));
    if is_new_thread {
        let PermissionFields {
            approval_policy,
            approvals_reviewer,
            sandbox_policy,
            permissions,
            runtime_workspace_roots: runtime_roots,
        } = permission_fields(
            request.permission_mode.clone(),
            &request.cwd,
            &thread_id,
            false,
        )?;
        turn_params.insert("approvalPolicy".into(), json!(approval_policy));
        turn_params.insert("approvalsReviewer".into(), json!(approvals_reviewer));
        turn_params.insert("sandboxPolicy".into(), json!(sandbox_policy));
        turn_params.insert("permissions".into(), json!(permissions));
        turn_params.insert("runtimeWorkspaceRoots".into(), json!(runtime_roots));
    }
    session.send(json!({ "method": "turn/start", "id": TURN_START_ID, "params": turn_params }))?;
    let turn_response = wait_for_session_response(
        reader,
        session,
        TURN_START_ID,
        events,
        Some(&mut thread_started_correlation),
        Some(&mut deferred_turn_notifications),
        (!is_new_thread).then_some(thread_id.as_str()),
    )
    .context("turn/start 失败")?;
    let turn_id = turn_response
        .pointer("/result/turn/id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .context("turn/start 响应缺少 result.turn.id")?;
    ensure_deferred_session_messages_match(
        session,
        &deferred_turn_notifications,
        &thread_id,
        &turn_id,
    )?;
    let mut streamed_text = false;
    for message in &deferred_turn_notifications {
        if let Some(outcome) = process_turn_message(
            session,
            message,
            &thread_id,
            &turn_id,
            events,
            &mut streamed_text,
        )? {
            session.mark_terminal();
            return Ok(outcome);
        }
    }
    session.activate_turn(thread_id.clone(), turn_id.clone())?;

    loop {
        let message = read_message(reader)?;
        if let Some(outcome) = process_turn_message(
            session,
            &message,
            &thread_id,
            &turn_id,
            events,
            &mut streamed_text,
        )? {
            session.mark_terminal();
            return Ok(outcome);
        }
    }
}

#[cfg(test)]
pub(super) fn read_message(reader: &mut impl BufRead) -> Result<Value> {
    let mut line = String::new();
    let bytes = reader.read_line(&mut line)?;
    if bytes == 0 {
        bail!("Codex app-server 在 turn 完成前关闭了输出流");
    }
    serde_json::from_str(&line).with_context(|| format!("无法解析 Codex JSON-RPC 消息：{line}"))
}

#[cfg(test)]
pub(super) fn wait_for_response(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
    expected_id: u64,
    events: Option<&Sender<AgentEvent>>,
) -> Result<Value> {
    loop {
        let message = read_message(reader)?;
        respond_to_server_request(writer, &message)?;
        // App-scoped state notifications can arrive on short-lived connections
        // that have no Composer event stream. They are schema-checked below and
        // only the thread prompt connection forwards them into GPUI state.
        if let Some(events) = events {
            forward_agent_notification(&message, events)?;
        } else if parse_agent_notification(&message)?.is_some()
            && !matches!(
                message.get("method").and_then(Value::as_str),
                Some("mcpServer/startupStatus/updated" | "account/rateLimits/updated")
            )
        {
            let method = message
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("未知方法");
            bail!(
                "Codex 模型目录连接收到需要可见 UI 承接的通知 `{method}`，但该连接没有 turn 事件流"
            );
        }
        ensure_server_method_is_defined(&message)?;
        if message.get("id").and_then(Value::as_u64) != Some(expected_id) {
            continue;
        }
        if let Some(error) = message.get("error") {
            return Err(anyhow!("Codex JSON-RPC 请求 {expected_id} 失败：{error}"));
        }
        return Ok(message);
    }
}

#[cfg(test)]
pub(super) fn wait_for_session_response<R: BufRead, W: Write + Send + 'static>(
    reader: &mut R,
    session: &Arc<CodexTurnSession<W>>,
    expected_id: u64,
    events: &Sender<AgentEvent>,
    mut thread_started_correlation: Option<&mut ThreadStartedCorrelation>,
    mut deferred_turn_notifications: Option<&mut Vec<Value>>,
    resume_bootstrap_thread_id: Option<&str>,
) -> Result<Value> {
    loop {
        let message = read_message(reader)?;
        if message.get("method").is_none()
            && message.get("id").and_then(Value::as_u64) == Some(expected_id)
        {
            if let Some(error) = message.get("error") {
                return Err(anyhow!("Codex JSON-RPC 请求 {expected_id} 失败：{error}"));
            }
            return Ok(message);
        }

        let method = message.get("method").and_then(Value::as_str);
        if method == Some("thread/goal/cleared")
            && let Some(expected_thread_id) = resume_bootstrap_thread_id
        {
            validate_resume_goal_cleared(&message, expected_thread_id)?;
            continue;
        }
        if method == Some("thread/started")
            && let Some(correlation) = thread_started_correlation.as_deref_mut()
        {
            correlation.observe(&message)?;
        }
        if method.is_some_and(|method| {
            TURN_SCOPED_SERVER_METHODS.contains(&method) || method == "serverRequest/resolved"
        }) && let Some(deferred) = deferred_turn_notifications.as_deref_mut()
        {
            ensure_server_method_is_defined(&message)?;
            deferred.push(message);
        } else {
            respond_to_server_request_on_session(session, &message, events)?;
            handle_server_request_resolved(session, &message, events)?;
            forward_agent_notification(&message, events)?;
            ensure_server_method_is_defined(&message)?;
        }
    }
}

#[cfg(test)]
pub(super) fn ensure_deferred_session_messages_match<W: Write + Send>(
    session: &CodexTurnSession<W>,
    messages: &[Value],
    expected_thread_id: &str,
    expected_turn_id: &str,
) -> Result<()> {
    for message in messages {
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
                );
            }
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn respond_to_server_request(writer: &mut impl Write, message: &Value) -> Result<()> {
    let Some(id) = message.get("id") else {
        return Ok(());
    };
    if message.get("method").is_none() {
        return Ok(());
    }
    send(
        writer,
        json!({
            "id": id,
            "error": {
                "code": -32601,
                "message": "This minimal client does not implement server-initiated requests"
            }
        }),
    )
}
