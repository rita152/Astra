//! Turn-local lifecycle, interruption, and server request response ownership.

use std::{
    io::Write,
    sync::{Arc, Mutex},
};

use anyhow::{Context as _, Result, anyhow, bail};
use async_channel::Sender;
use serde_json::{Value, json};

use super::{
    methods::TURN_SCOPED_SERVER_METHODS,
    notifications::thread_started_id,
    registry::{
        PendingCommandApproval, PendingPermissionsApprovalRequest, PendingServerRequestPayload,
        PendingUserInputRequest, ServerRequestRegistry, ServerRequestResolution,
    },
    requests::{
        request_id_value, request_metadata_for_permissions, request_metadata_for_user_input,
    },
    transport::{AppServerProcess, send},
};
use crate::agent::{
    AgentApprovalControl, AgentCommandApprovalChoice, AgentEvent, AgentInterruptControl,
    AgentInterruptOutcome, AgentPermissionsApprovalChoice, AgentPermissionsApprovalControl,
    AgentPermissionsApprovalRequest, AgentServerRequestFailureKind, AgentServerRequestId,
    AgentServerRequestMetadata, AgentUserInputControl, AgentUserInputRequest,
    AgentUserInputResponse,
};

pub(super) const TURN_INTERRUPT_ID: u64 = 4;

#[derive(Default)]
pub(super) struct TurnSessionState {
    pub(super) thread_id: Option<String>,
    pub(super) turn_id: Option<String>,
    pub(super) interrupt_requested: bool,
    pub(super) interrupt_sent: bool,
    pub(super) terminal: bool,
}

pub(super) struct CodexTurnSession<W> {
    pub(super) writer: Mutex<Option<W>>,
    pub(super) state: Mutex<TurnSessionState>,
    pub(super) server_requests: Mutex<ServerRequestRegistry>,
    pub(super) process: Option<Arc<AppServerProcess>>,
}

impl<W: Write + Send> CodexTurnSession<W> {
    fn lock_server_requests(&self) -> Result<std::sync::MutexGuard<'_, ServerRequestRegistry>> {
        self.server_requests
            .lock()
            .map_err(|_| anyhow!("Codex server request registry 锁已损坏"))
    }

    pub(super) fn new(writer: W, process: Option<Arc<AppServerProcess>>) -> Self {
        Self {
            writer: Mutex::new(Some(writer)),
            state: Mutex::new(TurnSessionState::default()),
            server_requests: Mutex::new(ServerRequestRegistry::default()),
            process,
        }
    }

    pub(super) fn register_server_request(
        &self,
        metadata: AgentServerRequestMetadata,
        payload: PendingServerRequestPayload,
    ) -> Result<()> {
        self.lock_server_requests()?
            .register_server_request(metadata, payload)
    }

    pub(super) fn register_command_approval(
        &self,
        metadata: AgentServerRequestMetadata,
        params: Value,
        available_decisions: Vec<Value>,
    ) -> Result<()> {
        self.register_server_request(
            metadata,
            PendingServerRequestPayload::CommandApproval(PendingCommandApproval {
                params,
                available_decisions,
            }),
        )
    }

    pub(super) fn register_user_input(&self, request: &AgentUserInputRequest) -> Result<()> {
        self.register_server_request(
            request_metadata_for_user_input(request),
            PendingServerRequestPayload::UserInput(PendingUserInputRequest {
                question_ids: request
                    .questions
                    .iter()
                    .map(|question| question.id.clone())
                    .collect(),
            }),
        )
    }

    pub(super) fn register_permissions_approval(
        &self,
        request: &AgentPermissionsApprovalRequest,
    ) -> Result<()> {
        self.register_server_request(
            request_metadata_for_permissions(request),
            PendingServerRequestPayload::PermissionsApproval(PendingPermissionsApprovalRequest {
                permissions: request.permissions.clone(),
            }),
        )
    }

    pub(super) fn resolve_server_request(
        &self,
        request_id: &AgentServerRequestId,
        thread_id: &str,
    ) -> Result<ServerRequestResolution> {
        self.lock_server_requests()?
            .resolve_server_request(request_id, thread_id)
    }

    pub(super) fn respond_to_command_approval(
        &self,
        request_id: &AgentServerRequestId,
        choice: AgentCommandApprovalChoice,
    ) -> Result<()> {
        self.ensure_server_request_responses_open(request_id)?;
        let decision = self
            .lock_server_requests()?
            .command_decision(request_id, choice)?;
        self.send(json!({ "id": request_id_value(request_id), "result": { "decision": decision } }))
    }

    pub(super) fn respond_to_user_input(
        &self,
        request_id: &AgentServerRequestId,
        response: AgentUserInputResponse,
    ) -> Result<()> {
        self.ensure_server_request_responses_open(request_id)?;
        let answers = self
            .lock_server_requests()?
            .user_input_answers(request_id, response)?;
        self.send(json!({
            "id": request_id_value(request_id),
            "result": { "answers": answers }
        }))
        .with_context(|| {
            format!("写入 user input request {request_id:?} 的 JSON-RPC response 失败")
        })
    }

    pub(super) fn respond_to_permissions_approval(
        &self,
        request_id: &AgentServerRequestId,
        choice: AgentPermissionsApprovalChoice,
    ) -> Result<()> {
        self.ensure_server_request_responses_open(request_id)?;
        let (permissions, scope) = self
            .lock_server_requests()?
            .permissions_decision(request_id, choice)?;
        self.send(json!({
            "id": request_id_value(request_id),
            "result": {
                "permissions": permissions,
                "scope": scope
            }
        }))
        .with_context(|| {
            format!("写入 permissions approval {request_id:?} 的 JSON-RPC response 失败")
        })
    }

    pub(super) fn ensure_server_request_responses_open(
        &self,
        request_id: &AgentServerRequestId,
    ) -> Result<()> {
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow!("Codex turn 会话状态锁已损坏"))?;
        if state.terminal {
            bail!("Codex turn 已结束，request {request_id:?} 不再可回复");
        }
        Ok(())
    }

    pub(super) fn drain_pending_server_requests(&self) -> Result<Vec<AgentServerRequestMetadata>> {
        self.lock_server_requests()?.drain_pending_server_requests()
    }

    pub(super) fn send(&self, message: Value) -> Result<()> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| anyhow!("Codex app-server stdin 锁已损坏"))?;
        let writer = writer.as_mut().context("Codex app-server 连接已经关闭")?;
        send(writer, message)
    }

    pub(super) fn activate_turn(&self, thread_id: String, turn_id: String) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("Codex turn 会话状态锁已损坏"))?;
        state.thread_id = Some(thread_id.clone());
        state.turn_id = Some(turn_id.clone());
        if state.interrupt_requested && !state.interrupt_sent && !state.terminal {
            self.send(turn_interrupt_request(&thread_id, &turn_id))?;
            state.interrupt_sent = true;
        }
        Ok(())
    }

    pub(super) fn request_interrupt_inner(&self) -> Result<AgentInterruptOutcome> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("Codex turn 会话状态锁已损坏"))?;
        if state.terminal {
            return Ok(AgentInterruptOutcome::AlreadyFinished);
        }
        if state.interrupt_requested {
            return Ok(AgentInterruptOutcome::AlreadyRequested);
        }

        state.interrupt_requested = true;
        let Some((thread_id, turn_id)) = state.thread_id.clone().zip(state.turn_id.clone()) else {
            return Ok(AgentInterruptOutcome::Requested);
        };
        if let Err(error) = self.send(turn_interrupt_request(&thread_id, &turn_id)) {
            state.terminal = true;
            drop(state);
            self.close_writer();
            if let Some(process) = &self.process {
                process.kill();
            }
            return Err(error);
        }
        state.interrupt_sent = true;
        Ok(AgentInterruptOutcome::Requested)
    }

    pub(super) fn mark_terminal(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.terminal = true;
        }
    }

    pub(super) fn close_writer(&self) {
        if let Ok(mut writer) = self.writer.lock() {
            writer.take();
        }
    }

    #[cfg(test)]
    pub(super) fn finish(&self) -> Result<()> {
        self.mark_terminal();
        self.close_writer();
        if let Some(process) = &self.process {
            process.terminate_and_wait()?;
        }
        Ok(())
    }

    pub(super) fn abandon_inner(&self) {
        let should_kill = self
            .state
            .lock()
            .map(|mut state| {
                if state.terminal {
                    false
                } else {
                    state.terminal = true;
                    true
                }
            })
            .unwrap_or(true);
        if should_kill {
            self.close_writer();
            if let Some(process) = &self.process {
                process.kill();
            }
        }
    }

    #[cfg(test)]
    pub(super) fn snapshot(&self) -> (Option<String>, Option<String>, bool, bool, bool) {
        let state = self.state.lock().unwrap();
        (
            state.thread_id.clone(),
            state.turn_id.clone(),
            state.interrupt_requested,
            state.interrupt_sent,
            state.terminal,
        )
    }

    #[cfg(test)]
    pub(super) fn pending_server_request_snapshot(
        &self,
    ) -> Vec<(AgentServerRequestMetadata, Option<Value>, bool)> {
        self.server_requests
            .lock()
            .unwrap()
            .pending_server_request_snapshot()
    }

    #[cfg(test)]
    pub(super) fn pending_approval_snapshot(&self) -> Vec<(AgentServerRequestId, Value, bool)> {
        self.pending_server_request_snapshot()
            .into_iter()
            .filter_map(|(metadata, params, responded)| {
                params.map(|params| (metadata.request_id, params, responded))
            })
            .collect()
    }
}

impl<W: Write + Send + 'static> AgentInterruptControl for CodexTurnSession<W> {
    fn request_interrupt(&self) -> Result<AgentInterruptOutcome, String> {
        self.request_interrupt_inner()
            .map_err(|error| format!("{error:#}"))
    }

    fn abandon(&self) {
        self.abandon_inner();
    }
}

impl<W: Write + Send + 'static> AgentApprovalControl for CodexTurnSession<W> {
    fn respond(
        &self,
        request_id: &AgentServerRequestId,
        choice: AgentCommandApprovalChoice,
    ) -> Result<(), String> {
        self.respond_to_command_approval(request_id, choice)
            .map_err(|error| format!("{error:#}"))
    }
}

impl<W: Write + Send + 'static> AgentUserInputControl for CodexTurnSession<W> {
    fn respond(
        &self,
        request_id: &AgentServerRequestId,
        response: AgentUserInputResponse,
    ) -> Result<(), String> {
        self.respond_to_user_input(request_id, response)
            .map_err(|error| format!("{error:#}"))
    }
}

impl<W: Write + Send + 'static> AgentPermissionsApprovalControl for CodexTurnSession<W> {
    fn respond(
        &self,
        request_id: &AgentServerRequestId,
        choice: AgentPermissionsApprovalChoice,
    ) -> Result<(), String> {
        self.respond_to_permissions_approval(request_id, choice)
            .map_err(|error| format!("{error:#}"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TurnOutcome {
    Completed,
    Interrupted,
    Failed(String),
}

impl TurnOutcome {
    pub(super) fn into_event(self) -> AgentEvent {
        match self {
            Self::Completed => AgentEvent::Completed,
            Self::Interrupted => AgentEvent::Interrupted,
            Self::Failed(message) => AgentEvent::Failed(message),
        }
    }
}

pub(super) fn cleanup_pending_server_requests<W: Write + Send>(
    session: &CodexTurnSession<W>,
    result: &Result<TurnOutcome>,
    events: &Sender<AgentEvent>,
) -> Result<()> {
    let pending = session.drain_pending_server_requests()?;
    if pending.is_empty() {
        return Ok(());
    }
    let (kind, message) = match result {
        Ok(TurnOutcome::Interrupted) => (
            AgentServerRequestFailureKind::Cancelled,
            "Codex turn 已取消，等待中的请求不再可回复".to_owned(),
        ),
        Ok(TurnOutcome::Completed) => (
            AgentServerRequestFailureKind::Failed,
            "Codex turn 已完成，但请求未收到 serverRequest/resolved".to_owned(),
        ),
        Ok(TurnOutcome::Failed(_)) | Err(_) => (
            AgentServerRequestFailureKind::Failed,
            "Codex 连接或 turn 失败，等待中的请求不再可回复".to_owned(),
        ),
    };
    for request in &pending {
        events
            .send_blocking(AgentEvent::ServerRequestFailed {
                request: request.clone(),
                kind,
                message: message.clone(),
            })
            .map_err(|_| anyhow!("Composer server request 清理事件通道已经关闭"))?;
    }
    if matches!(result, Ok(TurnOutcome::Completed)) {
        let requests = pending
            .iter()
            .map(|request| format!("{:?}:{:?}", request.kind, request.request_id))
            .collect::<Vec<_>>()
            .join(", ");
        bail!("Codex turn 正常完成时仍有未 resolved 的 server request：{requests}");
    }
    Ok(())
}

pub(super) fn turn_interrupt_request(thread_id: &str, turn_id: &str) -> Value {
    json!({
        "method": "turn/interrupt",
        "id": TURN_INTERRUPT_ID,
        "params": {
            "threadId": thread_id,
            "turnId": turn_id
        }
    })
}

pub(super) fn ensure_session_message_matches(
    message: &Value,
    expected_thread_id: &str,
    expected_turn_id: &str,
) -> Result<()> {
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        return Ok(());
    };
    if method == "thread/started" {
        let thread_id = thread_started_id(message)?;
        if thread_id != expected_thread_id {
            bail!(
                "收到属于其他 thread 的 `thread/started`：threadId=`{thread_id}`；当前 threadId=`{expected_thread_id}`"
            );
        }
        return Ok(());
    }
    if method == "serverRequest/resolved" {
        let thread_id = message
            .pointer("/params/threadId")
            .and_then(Value::as_str)
            .context("serverRequest/resolved 缺少字符串 params.threadId")?;
        if thread_id != expected_thread_id {
            bail!(
                "收到属于其他 thread 的 `serverRequest/resolved`：threadId=`{thread_id}`；当前 threadId=`{expected_thread_id}`"
            );
        }
        return Ok(());
    }
    if !TURN_SCOPED_SERVER_METHODS.contains(&method) {
        return Ok(());
    }
    let thread_id = message
        .pointer("/params/threadId")
        .and_then(Value::as_str)
        .with_context(|| format!("{method} 消息缺少字符串 params.threadId"))?;
    let turn_id = if matches!(method, "turn/started" | "turn/completed") {
        message
            .pointer("/params/turn/id")
            .and_then(Value::as_str)
            .with_context(|| format!("{method} 消息缺少字符串 params.turn.id"))?
    } else {
        message
            .pointer("/params/turnId")
            .and_then(Value::as_str)
            .with_context(|| format!("{method} 消息缺少字符串 params.turnId"))?
    };
    if thread_id != expected_thread_id || turn_id != expected_turn_id {
        bail!(
            "收到属于其他 turn 的 `{method}` 消息：threadId=`{thread_id}`，turnId=`{turn_id}`；当前 threadId=`{expected_thread_id}`，turnId=`{expected_turn_id}`"
        );
    }
    Ok(())
}
