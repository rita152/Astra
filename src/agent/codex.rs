use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Context as _, Result, anyhow, bail};
use async_channel::{Receiver, Sender};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    AgentActivePermissionProfile, AgentAdditionalFileSystemPermissions,
    AgentAdditionalNetworkPermissions, AgentApprovalControl, AgentApprovalHandle, AgentBackend,
    AgentCommandApprovalChoice, AgentCommandApprovalRequest, AgentConfigWarning,
    AgentEffectivePermissions, AgentEvent, AgentFileSystemAccess, AgentFileSystemPath,
    AgentFileSystemPermissionEntry, AgentFileSystemSpecialPath, AgentInterruptControl,
    AgentInterruptHandle, AgentInterruptOutcome, AgentMcpServerStartupFailureReason,
    AgentMcpServerStartupState, AgentMcpServerStartupStatus, AgentModel, AgentModelCatalog,
    AgentOptionalField, AgentPermissionMode, AgentPermissionProfile, AgentPermissionRequestProfile,
    AgentPermissionsApprovalChoice, AgentPermissionsApprovalControl,
    AgentPermissionsApprovalHandle, AgentPermissionsApprovalRequest, AgentReasoningEffort,
    AgentRequest, AgentRun, AgentServerRequestFailureKind, AgentServerRequestId,
    AgentServerRequestKind, AgentServerRequestMetadata, AgentServiceTier, AgentThreadActiveFlag,
    AgentThreadSettings, AgentThreadStatus, AgentThreadStatusState, AgentUserInputControl,
    AgentUserInputHandle, AgentUserInputOption, AgentUserInputQuestion, AgentUserInputRequest,
    AgentUserInputResponse, CommandExecution, CommandExecutionStatus,
};

const INITIALIZE_ID: u64 = 1;
const THREAD_REQUEST_ID: u64 = 2;
const TURN_START_ID: u64 = 3;
const TURN_INTERRUPT_ID: u64 = 4;
const THREAD_SETTINGS_UPDATE_ID: u64 = 2;
#[allow(dead_code)]
const PERMISSION_PROFILE_LIST_ID: u64 = 2;
const MODEL_LIST_FIRST_ID: u64 = 2;
const MODEL_LIST_PAGE_SIZE: u32 = 50;
const UNDEFINED_METHOD_PARAMS_LIMIT: usize = 2_000;

#[derive(Default)]
struct ThreadStartedCorrelation {
    expected_thread_id: Option<String>,
    observed_thread_id: Option<String>,
}

impl ThreadStartedCorrelation {
    fn expect(&mut self, thread_id: &str) -> Result<()> {
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

    fn observe(&mut self, message: &Value) -> Result<()> {
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

const TURN_SCOPED_SERVER_METHODS: &[&str] = &[
    "item/commandExecution/requestApproval",
    "item/permissions/requestApproval",
    "item/tool/requestUserInput",
    "item/started",
    "item/agentMessage/delta",
    "item/commandExecution/outputDelta",
    "item/completed",
    "turn/started",
    "turn/completed",
    "error",
    "model/rerouted",
    "model/verification",
    "model/safetyBuffering/updated",
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelListResponse {
    data: Vec<ModelListEntry>,
    #[serde(default)]
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct PermissionProfileListResponse {
    data: Vec<PermissionProfileListEntry>,
    #[serde(default)]
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct PermissionProfileListEntry {
    id: String,
    allowed: bool,
    #[serde(default)]
    extends: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelListEntry {
    id: String,
    model: String,
    display_name: String,
    description: String,
    hidden: bool,
    supported_reasoning_efforts: Vec<ModelReasoningEffort>,
    default_reasoning_effort: String,
    #[serde(default)]
    service_tiers: Vec<ModelServiceTier>,
    #[serde(default)]
    default_service_tier: Option<String>,
    is_default: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelReasoningEffort {
    reasoning_effort: String,
    description: String,
}

#[derive(Debug, Deserialize)]
struct ModelServiceTier {
    id: String,
    name: String,
    description: String,
}

#[derive(Default)]
struct TurnSessionState {
    thread_id: Option<String>,
    turn_id: Option<String>,
    interrupt_requested: bool,
    interrupt_sent: bool,
    terminal: bool,
}

#[derive(Clone, Debug)]
struct PendingCommandApproval {
    #[allow(dead_code)] // Retained verbatim for protocol auditing; exercised by registry tests.
    params: Value,
    available_decisions: Vec<Value>,
}

#[derive(Clone, Debug)]
struct PendingUserInputRequest {
    question_ids: Vec<String>,
}

#[derive(Clone, Debug)]
struct PendingPermissionsApprovalRequest {
    permissions: AgentPermissionRequestProfile,
}

#[derive(Clone, Debug)]
enum PendingServerRequestPayload {
    CommandApproval(PendingCommandApproval),
    UserInput(PendingUserInputRequest),
    PermissionsApproval(PendingPermissionsApprovalRequest),
}

#[derive(Clone, Debug)]
struct PendingServerRequest {
    metadata: AgentServerRequestMetadata,
    payload: PendingServerRequestPayload,
    responded: bool,
}

#[derive(Default)]
struct ServerRequestRegistry {
    pending: HashMap<AgentServerRequestId, PendingServerRequest>,
    completed: HashMap<AgentServerRequestId, AgentServerRequestMetadata>,
}

enum ServerRequestResolution {
    Resolved(AgentServerRequestMetadata),
    AlreadyResolved,
}

struct AppServerProcess {
    child: Mutex<Option<Child>>,
    reaped: AtomicBool,
}

impl AppServerProcess {
    fn new(child: Child) -> Self {
        Self {
            child: Mutex::new(Some(child)),
            reaped: AtomicBool::new(false),
        }
    }

    fn kill(&self) {
        let Ok(mut child) = self.child.lock() else {
            return;
        };
        if let Some(child) = child.as_mut()
            && child.try_wait().ok().flatten().is_none()
        {
            let _ = child.kill();
        }
    }

    fn terminate_and_wait(&self) -> Result<()> {
        let mut child = self
            .child
            .lock()
            .map_err(|_| anyhow!("Codex app-server 子进程锁已损坏"))?
            .take();
        let Some(mut child) = child.take() else {
            return Ok(());
        };

        if child.try_wait()?.is_none()
            && let Err(kill_error) = child.kill()
            && child.try_wait()?.is_none()
        {
            self.child
                .lock()
                .map_err(|_| anyhow!("Codex app-server 子进程锁已损坏"))?
                .replace(child);
            return Err(kill_error).context("无法终止 Codex app-server 子进程");
        }
        child
            .wait()
            .context("等待 Codex app-server 子进程退出失败")?;
        self.reaped.store(true, Ordering::Release);
        Ok(())
    }

    #[cfg(test)]
    fn is_reaped(&self) -> bool {
        self.reaped.load(Ordering::Acquire)
    }
}

impl Drop for AppServerProcess {
    fn drop(&mut self) {
        let _ = self.terminate_and_wait();
    }
}

struct CodexTurnSession<W> {
    writer: Mutex<Option<W>>,
    state: Mutex<TurnSessionState>,
    server_requests: Mutex<ServerRequestRegistry>,
    process: Option<Arc<AppServerProcess>>,
}

impl<W: Write + Send> CodexTurnSession<W> {
    fn new(writer: W, process: Option<Arc<AppServerProcess>>) -> Self {
        Self {
            writer: Mutex::new(Some(writer)),
            state: Mutex::new(TurnSessionState::default()),
            server_requests: Mutex::new(ServerRequestRegistry::default()),
            process,
        }
    }

    fn register_server_request(
        &self,
        metadata: AgentServerRequestMetadata,
        payload: PendingServerRequestPayload,
    ) -> Result<()> {
        let mut registry = self
            .server_requests
            .lock()
            .map_err(|_| anyhow!("Codex server request registry 锁已损坏"))?;
        if registry.pending.contains_key(&metadata.request_id) {
            bail!(
                "收到重复的 Codex server request id {:?}",
                metadata.request_id
            );
        }
        registry.completed.remove(&metadata.request_id);
        registry.pending.insert(
            metadata.request_id.clone(),
            PendingServerRequest {
                metadata,
                payload,
                responded: false,
            },
        );
        Ok(())
    }

    fn register_command_approval(
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

    fn register_user_input(&self, request: &AgentUserInputRequest) -> Result<()> {
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

    fn register_permissions_approval(
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

    fn resolve_server_request(
        &self,
        request_id: &AgentServerRequestId,
        thread_id: &str,
    ) -> Result<ServerRequestResolution> {
        let mut registry = self
            .server_requests
            .lock()
            .map_err(|_| anyhow!("Codex server request registry 锁已损坏"))?;
        if let Some(request) = registry.pending.get(request_id) {
            if request.metadata.thread_id != thread_id {
                bail!(
                    "serverRequest/resolved threadId `{thread_id}` 与 pending request {:?} 的 threadId `{}` 不一致",
                    request_id,
                    request.metadata.thread_id
                );
            }
            let metadata = request.metadata.clone();
            registry.pending.remove(request_id);
            registry
                .completed
                .insert(request_id.clone(), metadata.clone());
            return Ok(ServerRequestResolution::Resolved(metadata));
        }
        if let Some(metadata) = registry.completed.get(request_id) {
            if metadata.thread_id != thread_id {
                bail!(
                    "重复 serverRequest/resolved 的 threadId `{thread_id}` 与 request {:?} 的 threadId `{}` 不一致",
                    request_id,
                    metadata.thread_id
                );
            }
            return Ok(ServerRequestResolution::AlreadyResolved);
        }
        bail!(
            "serverRequest/resolved 引用了未知 request {:?}（threadId=`{thread_id}`）",
            request_id
        )
    }

    fn respond_to_command_approval(
        &self,
        request_id: &AgentServerRequestId,
        choice: AgentCommandApprovalChoice,
    ) -> Result<()> {
        self.ensure_server_request_responses_open(request_id)?;
        let decision = {
            let mut registry = self
                .server_requests
                .lock()
                .map_err(|_| anyhow!("Codex server request registry 锁已损坏"))?;
            let request = registry.pending.get_mut(request_id).with_context(|| {
                format!("command approval {request_id:?} 已经 resolved 或不存在")
            })?;
            if request.responded {
                bail!("command approval {request_id:?} 已经回复，拒绝重复 decision");
            }
            let PendingServerRequestPayload::CommandApproval(command) = &request.payload else {
                bail!("request {request_id:?} 不是 command approval，拒绝错误类型的 responder")
            };
            let decision = match choice {
                AgentCommandApprovalChoice::Accept => command
                    .available_decisions
                    .iter()
                    .find(|decision| decision.as_str() == Some("accept"))
                    .cloned(),
                // ChatGPT Desktop treats the visible Reject action as
                // `decline` even when app-server advertises only `cancel`.
                // The two values are not synonyms: `decline` rejects the item
                // and lets the turn continue, while `cancel` interrupts it.
                AgentCommandApprovalChoice::Decline => command
                    .available_decisions
                    .iter()
                    .any(|decision| matches!(decision.as_str(), Some("decline" | "cancel")))
                    .then(|| Value::String("decline".to_owned())),
                AgentCommandApprovalChoice::AcceptWithExecpolicyAmendment => command
                    .available_decisions
                    .iter()
                    .find(|decision| decision.get("acceptWithExecpolicyAmendment").is_some())
                    .cloned(),
            }
            .with_context(|| {
                format!("command approval {request_id:?} 未提供所选 decision，拒绝越权回复")
            })?;
            request.responded = true;
            decision
        };
        self.send(json!({ "id": request_id_value(request_id), "result": { "decision": decision } }))
    }

    fn respond_to_user_input(
        &self,
        request_id: &AgentServerRequestId,
        response: AgentUserInputResponse,
    ) -> Result<()> {
        self.ensure_server_request_responses_open(request_id)?;
        let answers = {
            let mut registry = self
                .server_requests
                .lock()
                .map_err(|_| anyhow!("Codex server request registry 锁已损坏"))?;
            let request = registry.pending.get_mut(request_id).with_context(|| {
                format!("user input request {request_id:?} 已经 resolved 或不存在")
            })?;
            if request.responded {
                bail!("user input request {request_id:?} 已经回复，拒绝重复 answers");
            }
            let PendingServerRequestPayload::UserInput(pending) = &request.payload else {
                bail!("request {request_id:?} 不是 user input，拒绝错误类型的 responder")
            };
            let question_ids = pending.question_ids.iter().collect::<HashSet<_>>();
            let mut seen = HashSet::new();
            let mut answers = serde_json::Map::new();
            for answer in response.answers {
                if !seen.insert(answer.question_id.clone()) {
                    bail!(
                        "user input request {request_id:?} 对 question id `{}` 提供了重复答案",
                        answer.question_id
                    );
                }
                if !question_ids.contains(&answer.question_id) {
                    bail!(
                        "user input request {request_id:?} 不包含 question id `{}`",
                        answer.question_id
                    );
                }
                answers.insert(answer.question_id, json!({ "answers": answer.answers }));
            }
            request.responded = true;
            answers
        };
        self.send(json!({
            "id": request_id_value(request_id),
            "result": { "answers": answers }
        }))
        .with_context(|| {
            format!("写入 user input request {request_id:?} 的 JSON-RPC response 失败")
        })
    }

    fn respond_to_permissions_approval(
        &self,
        request_id: &AgentServerRequestId,
        choice: AgentPermissionsApprovalChoice,
    ) -> Result<()> {
        self.ensure_server_request_responses_open(request_id)?;
        let (permissions, scope) = {
            let mut registry = self
                .server_requests
                .lock()
                .map_err(|_| anyhow!("Codex server request registry 锁已损坏"))?;
            let request = registry.pending.get_mut(request_id).with_context(|| {
                format!("permissions approval {request_id:?} 已经 resolved 或不存在")
            })?;
            if request.responded {
                bail!("permissions approval {request_id:?} 已经回复，拒绝重复 decision");
            }
            let PendingServerRequestPayload::PermissionsApproval(pending) = &request.payload else {
                bail!("request {request_id:?} 不是 permissions approval，拒绝错误类型的 responder")
            };
            let response = match choice {
                AgentPermissionsApprovalChoice::AllowOnce => {
                    (permission_profile_value(&pending.permissions), "turn")
                }
                AgentPermissionsApprovalChoice::AllowForSession => {
                    (permission_profile_value(&pending.permissions), "session")
                }
                AgentPermissionsApprovalChoice::Decline => {
                    (Value::Object(Default::default()), "turn")
                }
            };
            request.responded = true;
            response
        };
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

    fn ensure_server_request_responses_open(
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

    fn drain_pending_server_requests(&self) -> Result<Vec<AgentServerRequestMetadata>> {
        let mut registry = self
            .server_requests
            .lock()
            .map_err(|_| anyhow!("Codex server request registry 锁已损坏"))?;
        let pending = std::mem::take(&mut registry.pending);
        let mut metadata = Vec::with_capacity(pending.len());
        for (request_id, request) in pending {
            registry
                .completed
                .insert(request_id, request.metadata.clone());
            metadata.push(request.metadata);
        }
        Ok(metadata)
    }

    fn send(&self, message: Value) -> Result<()> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| anyhow!("Codex app-server stdin 锁已损坏"))?;
        let writer = writer.as_mut().context("Codex app-server 连接已经关闭")?;
        send(writer, message)
    }

    fn activate_turn(&self, thread_id: String, turn_id: String) -> Result<()> {
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

    fn request_interrupt_inner(&self) -> Result<AgentInterruptOutcome> {
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

    fn mark_terminal(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.terminal = true;
        }
    }

    fn close_writer(&self) {
        if let Ok(mut writer) = self.writer.lock() {
            writer.take();
        }
    }

    fn finish(&self) -> Result<()> {
        self.mark_terminal();
        self.close_writer();
        if let Some(process) = &self.process {
            process.terminate_and_wait()?;
        }
        Ok(())
    }

    fn abandon_inner(&self) {
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
    fn snapshot(&self) -> (Option<String>, Option<String>, bool, bool, bool) {
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
    fn pending_server_request_snapshot(
        &self,
    ) -> Vec<(AgentServerRequestMetadata, Option<Value>, bool)> {
        self.server_requests
            .lock()
            .unwrap()
            .pending
            .iter()
            .map(|(_, request)| {
                let params = match &request.payload {
                    PendingServerRequestPayload::CommandApproval(approval) => {
                        Some(approval.params.clone())
                    }
                    PendingServerRequestPayload::UserInput(_)
                    | PendingServerRequestPayload::PermissionsApproval(_) => None,
                };
                (request.metadata.clone(), params, request.responded)
            })
            .collect()
    }

    #[cfg(test)]
    fn pending_approval_snapshot(&self) -> Vec<(AgentServerRequestId, Value, bool)> {
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

fn request_id_from_value(value: &Value) -> Result<AgentServerRequestId> {
    match value {
        Value::String(id) => Ok(AgentServerRequestId::String(id.clone())),
        Value::Number(id) => id
            .as_i64()
            .map(AgentServerRequestId::Number)
            .context("Codex JSON-RPC request id 数字超出 int64 范围"),
        _ => bail!("Codex JSON-RPC request id 必须是字符串或 int64 数字"),
    }
}

fn request_id_value(request_id: &AgentServerRequestId) -> Value {
    match request_id {
        AgentServerRequestId::Number(id) => json!(id),
        AgentServerRequestId::String(id) => json!(id),
    }
}

fn request_metadata_for_command(
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

fn request_metadata_for_user_input(request: &AgentUserInputRequest) -> AgentServerRequestMetadata {
    AgentServerRequestMetadata {
        request_id: request.request_id.clone(),
        thread_id: request.thread_id.clone(),
        turn_id: request.turn_id.clone(),
        item_id: request.item_id.clone(),
        kind: AgentServerRequestKind::UserInput,
    }
}

fn request_metadata_for_permissions(
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

fn insert_optional_field<T>(
    object: &mut serde_json::Map<String, Value>,
    field: &str,
    value: &AgentOptionalField<T>,
    serialize: impl FnOnce(&T) -> Value,
) {
    match value {
        AgentOptionalField::Unspecified => {}
        AgentOptionalField::Null => {
            object.insert(field.to_owned(), Value::Null);
        }
        AgentOptionalField::Value(value) => {
            object.insert(field.to_owned(), serialize(value));
        }
    }
}

fn permission_profile_value(profile: &AgentPermissionRequestProfile) -> Value {
    let mut object = serde_json::Map::new();
    insert_optional_field(
        &mut object,
        "fileSystem",
        &profile.file_system,
        file_system_permissions_value,
    );
    insert_optional_field(
        &mut object,
        "network",
        &profile.network,
        network_permissions_value,
    );
    Value::Object(object)
}

fn file_system_permissions_value(permissions: &AgentAdditionalFileSystemPermissions) -> Value {
    let mut object = serde_json::Map::new();
    insert_optional_field(&mut object, "read", &permissions.read, |paths| json!(paths));
    insert_optional_field(&mut object, "write", &permissions.write, |paths| {
        json!(paths)
    });
    insert_optional_field(
        &mut object,
        "globScanMaxDepth",
        &permissions.glob_scan_max_depth,
        |depth| json!(depth),
    );
    insert_optional_field(&mut object, "entries", &permissions.entries, |entries| {
        Value::Array(entries.iter().map(file_system_entry_value).collect())
    });
    Value::Object(object)
}

fn network_permissions_value(permissions: &AgentAdditionalNetworkPermissions) -> Value {
    let mut object = serde_json::Map::new();
    insert_optional_field(&mut object, "enabled", &permissions.enabled, |enabled| {
        json!(enabled)
    });
    Value::Object(object)
}

fn file_system_entry_value(entry: &AgentFileSystemPermissionEntry) -> Value {
    let access = match entry.access {
        AgentFileSystemAccess::Read => "read",
        AgentFileSystemAccess::Write => "write",
        AgentFileSystemAccess::Deny => "deny",
    };
    json!({
        "path": file_system_path_value(&entry.path),
        "access": access
    })
}

fn file_system_path_value(path: &AgentFileSystemPath) -> Value {
    match path {
        AgentFileSystemPath::Path(path) => json!({ "type": "path", "path": path }),
        AgentFileSystemPath::GlobPattern(pattern) => {
            json!({ "type": "glob_pattern", "pattern": pattern })
        }
        AgentFileSystemPath::Special(value) => {
            json!({ "type": "special", "value": file_system_special_path_value(value) })
        }
    }
}

fn file_system_special_path_value(path: &AgentFileSystemSpecialPath) -> Value {
    match path {
        AgentFileSystemSpecialPath::Root => json!({ "kind": "root" }),
        AgentFileSystemSpecialPath::Minimal => json!({ "kind": "minimal" }),
        AgentFileSystemSpecialPath::ProjectRoots { subpath } => {
            let mut value = serde_json::Map::new();
            value.insert("kind".to_owned(), json!("project_roots"));
            insert_optional_field(&mut value, "subpath", subpath, |subpath| json!(subpath));
            Value::Object(value)
        }
        AgentFileSystemSpecialPath::Tmpdir => json!({ "kind": "tmpdir" }),
        AgentFileSystemSpecialPath::SlashTmp => json!({ "kind": "slash_tmp" }),
        AgentFileSystemSpecialPath::Unknown { path, subpath } => {
            let mut value = serde_json::Map::new();
            value.insert("kind".to_owned(), json!("unknown"));
            value.insert("path".to_owned(), json!(path));
            insert_optional_field(&mut value, "subpath", subpath, |subpath| json!(subpath));
            Value::Object(value)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TurnOutcome {
    Completed,
    Interrupted,
    Failed(String),
}

impl TurnOutcome {
    fn into_event(self) -> AgentEvent {
        match self {
            Self::Completed => AgentEvent::Completed,
            Self::Interrupted => AgentEvent::Interrupted,
            Self::Failed(message) => AgentEvent::Failed(message),
        }
    }
}

fn finish_prompt_session<W: Write + Send>(
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

fn cleanup_pending_server_requests<W: Write + Send>(
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

fn turn_interrupt_request(thread_id: &str, turn_id: &str) -> Value {
    json!({
        "method": "turn/interrupt",
        "id": TURN_INTERRUPT_ID,
        "params": {
            "threadId": thread_id,
            "turnId": turn_id
        }
    })
}

impl From<ModelListEntry> for AgentModel {
    fn from(entry: ModelListEntry) -> Self {
        Self {
            id: entry.id,
            model: entry.model,
            display_name: entry.display_name,
            description: entry.description,
            supported_reasoning_efforts: entry
                .supported_reasoning_efforts
                .into_iter()
                .map(|effort| AgentReasoningEffort {
                    id: effort.reasoning_effort,
                    description: effort.description,
                })
                .collect(),
            default_reasoning_effort: entry.default_reasoning_effort,
            service_tiers: entry
                .service_tiers
                .into_iter()
                .map(|tier| AgentServiceTier {
                    id: tier.id,
                    name: tier.name,
                    description: tier.description,
                })
                .collect(),
            default_service_tier: entry.default_service_tier,
            is_default: entry.is_default,
        }
    }
}

/// Codex CLI adapter. JSON-RPC details intentionally stay inside this module.
#[derive(Default)]
pub struct CodexAppServerBackend;

impl CodexAppServerBackend {
    pub fn new() -> Self {
        Self
    }
}

impl AgentBackend for CodexAppServerBackend {
    fn load_model_catalog(&self) -> Receiver<Result<AgentModelCatalog, String>> {
        let (catalog_tx, catalog_rx) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let result = run_model_catalog_process().map_err(|error| format!("{error:#}"));
            let _ = catalog_tx.send_blocking(result);
        });
        catalog_rx
    }

    fn load_permission_profiles(
        &self,
        cwd: PathBuf,
    ) -> Receiver<Result<Vec<AgentPermissionProfile>, String>> {
        let (tx, rx) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let result = run_permission_profile_process(&cwd).map_err(|error| format!("{error:#}"));
            let _ = tx.send_blocking(result);
        });
        rx
    }

    fn update_thread_permissions(
        &self,
        thread_id: String,
        cwd: PathBuf,
        mode: AgentPermissionMode,
    ) -> Receiver<Result<AgentThreadSettings, String>> {
        let (tx, rx) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let result = run_thread_settings_update_process(&thread_id, &cwd, mode)
                .map_err(|error| format!("{error:#}"));
            let _ = tx.send_blocking(result);
        });
        rx
    }

    fn run_prompt(&self, request: AgentRequest) -> AgentRun {
        let (events_tx, events_rx) = async_channel::unbounded();
        let interrupt = match spawn_prompt_session() {
            Ok((mut reader, session)) => {
                let control: Arc<dyn AgentInterruptControl> = session.clone();
                let interrupt = AgentInterruptHandle::new(control);
                std::thread::spawn(move || {
                    let result = drive_session(&mut reader, &session, &request, &events_tx);
                    let cleanup = cleanup_pending_server_requests(&session, &result, &events_tx);
                    let result = match (result, cleanup) {
                        (result, Ok(())) => result,
                        (Ok(_), Err(error)) => Err(error),
                        (Err(error), Err(cleanup_error)) => Err(anyhow!(
                            "{error:#}\n清理 pending server request 同时失败：{cleanup_error:#}"
                        )),
                    };
                    let event = finish_prompt_session(&session, result);
                    let _ = events_tx.send_blocking(event);
                });
                Some(interrupt)
            }
            Err(error) => {
                let _ = events_tx.send_blocking(AgentEvent::Failed(format!("{error:#}")));
                None
            }
        };
        AgentRun::new(events_rx, interrupt)
    }
}

fn with_app_server<T>(
    drive: impl FnOnce(&mut BufReader<ChildStdout>, &mut ChildStdin) -> Result<T>,
) -> Result<T> {
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
    let result = drive(&mut reader, &mut stdin);
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    result
}

#[allow(dead_code)]
fn run_permission_profile_process(cwd: &Path) -> Result<Vec<AgentPermissionProfile>> {
    with_app_server(|reader, writer| drive_permission_profiles(reader, writer, cwd))
}

fn run_thread_settings_update_process(
    thread_id: &str,
    cwd: &Path,
    mode: AgentPermissionMode,
) -> Result<AgentThreadSettings> {
    with_app_server(|reader, writer| {
        drive_thread_settings_update(reader, writer, thread_id, cwd, mode)
    })
}

#[cfg_attr(test, allow(dead_code))]
fn run_model_catalog_process() -> Result<AgentModelCatalog> {
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

fn spawn_prompt_session() -> Result<(BufReader<ChildStdout>, Arc<CodexTurnSession<ChildStdin>>)> {
    let mut child = Command::new("codex")
        .args(["app-server", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("无法启动 `codex app-server --stdio`；请确认 Codex CLI 已安装并完成登录")?;

    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        bail!("无法读取 Codex app-server stdout");
    };
    let Some(stdin) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        bail!("无法写入 Codex app-server stdin");
    };
    let process = Arc::new(AppServerProcess::new(child));
    let session = Arc::new(CodexTurnSession::new(stdin, Some(process)));
    Ok((BufReader::new(stdout), session))
}

fn initialize_connection<R: BufRead, W: Write>(
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
                    "requestAttestation": false
                }
            }
        }),
    )?;
    wait_for_response(reader, writer, INITIALIZE_ID, events)?;
    send(writer, json!({ "method": "initialized", "params": {} }))
}

fn initialize_turn_connection<R: BufRead, W: Write + Send + 'static>(
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
                "requestAttestation": false
            }
        }
    }))?;
    wait_for_session_response(reader, session, INITIALIZE_ID, events, None, None)?;
    session.send(json!({ "method": "initialized", "params": {} }))
}

fn drive_model_catalog<R: BufRead, W: Write>(
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

#[allow(dead_code)]
fn drive_permission_profiles<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    cwd: &Path,
) -> Result<Vec<AgentPermissionProfile>> {
    initialize_connection(reader, writer, None)?;
    send(
        writer,
        json!({
            "method": "permissionProfile/list",
            "id": PERMISSION_PROFILE_LIST_ID,
            "params": { "cursor": null, "limit": 100, "cwd": cwd }
        }),
    )?;
    let response = wait_for_response(reader, writer, PERMISSION_PROFILE_LIST_ID, None)?;
    let page: PermissionProfileListResponse = serde_json::from_value(
        response
            .get("result")
            .cloned()
            .context("permissionProfile/list 响应缺少 result")?,
    )
    .context("无法解析 permissionProfile/list 响应")?;
    if page.next_cursor.is_some() {
        bail!("permissionProfile/list 返回了超出 100 项的 profile；当前客户端不应静默截断");
    }
    Ok(page
        .data
        .into_iter()
        .map(|profile| AgentPermissionProfile {
            id: profile.id,
            allowed: profile.allowed,
            extends: profile.extends,
        })
        .collect())
}

fn workspace_roots(cwd: &Path, thread_id: &str) -> Vec<String> {
    let mut roots = vec![cwd.to_string_lossy().into_owned()];
    if let Some(home) = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex")))
    {
        let dated = chrono::Local::now().format("%Y/%m/%d").to_string();
        roots.push(
            home.join("visualizations")
                .join(dated)
                .join(thread_id)
                .to_string_lossy()
                .into_owned(),
        );
    }
    roots
}

fn workspace_write_policy(roots: &[String], network_access: bool) -> Value {
    json!({
        "type": "workspaceWrite",
        "writableRoots": roots,
        "networkAccess": network_access,
        "excludeTmpdirEnvVar": false,
        "excludeSlashTmp": false
    })
}

fn custom_sandbox_policy(cwd: &Path, roots: &[String]) -> Result<Value> {
    let config_paths = [
        std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex")))
            .map(|path| path.join("config.toml")),
        Some(cwd.join(".codex/config.toml")),
    ];
    let mut merged = toml::Value::Table(toml::map::Map::new());
    for path in config_paths
        .into_iter()
        .flatten()
        .filter(|path| path.is_file())
    {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("无法读取 {}", path.display()))?;
        let parsed: toml::Value =
            toml::from_str(&text).with_context(|| format!("无法解析 {}", path.display()))?;
        if let (Some(target), Some(source)) = (merged.as_table_mut(), parsed.as_table()) {
            target.extend(source.clone());
        }
    }
    let mode = merged
        .get("sandbox_mode")
        .and_then(toml::Value::as_str)
        .unwrap_or("workspace-write");
    match mode {
        "danger-full-access" => Ok(json!({ "type": "dangerFullAccess" })),
        "read-only" => Ok(json!({ "type": "readOnly" })),
        "workspace-write" => {
            let network = merged
                .get("sandbox_workspace_write")
                .and_then(|value| value.get("network_access"))
                .and_then(toml::Value::as_bool)
                .unwrap_or(false);
            Ok(workspace_write_policy(roots, network))
        }
        other => bail!("config.toml 中的 sandbox_mode `{other}` 不受支持"),
    }
}

fn permission_fields(
    mode: AgentPermissionMode,
    cwd: &Path,
    thread_id: &str,
    existing_thread_update: bool,
) -> Result<(
    String,
    String,
    Option<Value>,
    Option<String>,
    Option<Vec<String>>,
)> {
    let roots = workspace_roots(cwd, thread_id);
    let fields = match mode {
        AgentPermissionMode::Request => (
            "on-request".into(),
            "user".into(),
            Some(workspace_write_policy(&roots, false)),
            existing_thread_update.then(|| ":workspace".into()),
            None,
        ),
        AgentPermissionMode::Assist => (
            "on-request".into(),
            if existing_thread_update {
                "guardian_subagent"
            } else {
                "auto_review"
            }
            .into(),
            Some(workspace_write_policy(&roots, false)),
            existing_thread_update.then(|| ":workspace".into()),
            None,
        ),
        AgentPermissionMode::Full => (
            "never".into(),
            "user".into(),
            None,
            Some(":danger-full-access".into()),
            (!existing_thread_update).then_some(roots),
        ),
        AgentPermissionMode::Custom => (
            "on-request".into(),
            "user".into(),
            Some(custom_sandbox_policy(cwd, &roots)?),
            None,
            (!existing_thread_update).then_some(roots),
        ),
    };
    Ok(fields)
}

fn thread_settings_update_request(
    id: u64,
    thread_id: &str,
    cwd: &Path,
    mode: AgentPermissionMode,
) -> Result<Value> {
    let (approval_policy, approvals_reviewer, sandbox_policy, permissions, _) =
        permission_fields(mode, cwd, thread_id, true)?;
    let mut params = serde_json::Map::new();
    params.insert("threadId".into(), json!(thread_id));
    params.insert("approvalPolicy".into(), json!(approval_policy));
    params.insert("approvalsReviewer".into(), json!(approvals_reviewer));
    if mode == AgentPermissionMode::Custom {
        params.insert(
            "sandboxPolicy".into(),
            sandbox_policy.context("Custom 缺少 sandboxPolicy")?,
        );
    } else {
        params.insert(
            "permissions".into(),
            json!(permissions.context("权限模式缺少 profile")?),
        );
    }
    Ok(json!({ "method": "thread/settings/update", "id": id, "params": params }))
}

fn drive_thread_settings_update<R: BufRead, W: Write>(
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
        if response_ok && effective.is_some() {
            return Ok(effective.expect("checked above"));
        }
    }
}

fn drive_session<R: BufRead, W: Write + Send + 'static>(
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
                "params": { "threadId": expected_thread_id }
            }))?;
            let thread_response = wait_for_session_response(
                reader,
                session,
                THREAD_REQUEST_ID,
                events,
                Some(&mut thread_started_correlation),
                Some(&mut deferred_turn_notifications),
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
        let (approval_policy, approvals_reviewer, sandbox_policy, permissions, runtime_roots) =
            permission_fields(request.permission_mode, &request.cwd, &thread_id, false)?;
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

fn process_turn_message<W: Write + Send + 'static>(
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
                "userMessage" => {
                    validate_user_message(item)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                }
                "agentMessage" => {
                    let (item_id, _text) = parse_agent_message(item)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    send_turn_event(
                        events,
                        AgentEvent::AssistantMessageStarted { item_id },
                        "item/started agentMessage",
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
                unsupported => {
                    return Err(turn_item_protocol_error(
                        message,
                        format!("未接入的 item.type `{unsupported}`"),
                    ));
                }
            }
        }
        Some("item/agentMessage/delta") => {
            let _item_id = required_notification_string(message, "itemId")?;
            let delta = required_notification_string(message, "delta")?;
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
        Some("item/completed") => {
            let item = required_turn_item(message)?;
            let item_type = required_turn_item_type(message, item)?;
            match item_type {
                "agentMessage" => {
                    let (_item_id, text) = parse_agent_message(item)
                        .map_err(|error| turn_item_protocol_error(message, error))?;
                    match *streamed_text {
                        true => Ok(()),
                        false => send_turn_event(
                            events,
                            AgentEvent::TextDelta(text),
                            "item/completed agentMessage",
                        )
                        .map_err(|error| turn_item_protocol_error(message, error)),
                    }?;
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
            "item/commandExecution/requestApproval"
            | "item/permissions/requestApproval"
            | "item/tool/requestUserInput"
            | "serverRequest/resolved"
            | "remoteControl/status/changed"
            | "mcpServer/startupStatus/updated"
            | "thread/status/changed"
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

fn is_defined_server_method(method: &str) -> bool {
    matches!(
        method,
        "item/commandExecution/requestApproval"
            | "item/permissions/requestApproval"
            | "item/tool/requestUserInput"
            | "serverRequest/resolved"
            | "item/started"
            | "item/agentMessage/delta"
            | "item/commandExecution/outputDelta"
            | "item/completed"
            | "thread/started"
            | "turn/started"
            | "turn/completed"
            | "error"
            | "thread/settings/updated"
            | "warning"
            | "configWarning"
            | "model/rerouted"
            | "model/verification"
            | "model/safetyBuffering/updated"
    )
}

fn validate_remote_control_status_changed(message: &Value) -> Result<()> {
    let status = required_notification_string(message, "status")?;
    if !matches!(
        status.as_str(),
        "disabled" | "connecting" | "connected" | "errored"
    ) {
        bail!("remoteControl/status/changed 通知字段 params.status 为未知状态 `{status}`");
    }
    let _server_name = required_notification_string(message, "serverName")?;
    let _installation_id = required_notification_string(message, "installationId")?;
    let _environment_id = required_nullable_notification_string(message, "environmentId")?;
    Ok(())
}

fn parse_mcp_server_startup_status_updated(message: &Value) -> Result<AgentMcpServerStartupStatus> {
    let thread_id = optional_string_at(message, "/params/threadId", "params.threadId")?;
    let name = required_notification_string(message, "name")?;
    let raw_state = required_notification_string(message, "status")?;
    let state = match raw_state.as_str() {
        "starting" => AgentMcpServerStartupState::Starting,
        "ready" => AgentMcpServerStartupState::Ready,
        "failed" => AgentMcpServerStartupState::Failed,
        "cancelled" => AgentMcpServerStartupState::Cancelled,
        _ => {
            bail!("mcpServer/startupStatus/updated 通知字段 params.status 为未知状态 `{raw_state}`")
        }
    };
    let error = optional_string_at(message, "/params/error", "params.error")?;
    let failure_reason = match optional_string_at(
        message,
        "/params/failureReason",
        "params.failureReason",
    )? {
        Some(reason) if reason == "reauthenticationRequired" => {
            Some(AgentMcpServerStartupFailureReason::ReauthenticationRequired)
        }
        Some(reason) => bail!(
            "mcpServer/startupStatus/updated 通知字段 params.failureReason 为未知原因 `{reason}`"
        ),
        None => None,
    };
    Ok(AgentMcpServerStartupStatus {
        thread_id,
        name,
        state,
        error,
        failure_reason,
    })
}

fn parse_thread_status_changed(message: &Value) -> Result<AgentThreadStatus> {
    let thread_id = required_notification_string(message, "threadId")?;
    let status = message
        .pointer("/params/status")
        .and_then(Value::as_object)
        .context("thread/status/changed 通知缺少对象字段 params.status")?;
    let raw_state = status
        .get("type")
        .and_then(Value::as_str)
        .context("thread/status/changed 通知缺少字符串字段 params.status.type")?;
    let state = match raw_state {
        "notLoaded" => AgentThreadStatusState::NotLoaded,
        "idle" => AgentThreadStatusState::Idle,
        "systemError" => AgentThreadStatusState::SystemError,
        "active" => {
            let raw_flags = status
                .get("activeFlags")
                .and_then(Value::as_array)
                .context(
                    "thread/status/changed active 状态缺少数组字段 params.status.activeFlags",
                )?;
            let active_flags = raw_flags
                .iter()
                .enumerate()
                .map(|(index, flag)| match flag.as_str() {
                    Some("waitingOnApproval") => Ok(AgentThreadActiveFlag::WaitingOnApproval),
                    Some("waitingOnUserInput") => Ok(AgentThreadActiveFlag::WaitingOnUserInput),
                    Some(flag) => bail!(
                        "thread/status/changed 通知字段 params.status.activeFlags[{index}] 为未知 flag `{flag}`"
                    ),
                    None => bail!(
                        "thread/status/changed 通知字段 params.status.activeFlags[{index}] 必须是字符串"
                    ),
                })
                .collect::<Result<Vec<_>>>()?;
            AgentThreadStatusState::Active { active_flags }
        }
        _ => bail!("thread/status/changed 通知字段 params.status.type 为未知状态 `{raw_state}`"),
    };
    Ok(AgentThreadStatus { thread_id, state })
}

fn required_notification_string(message: &Value, field: &str) -> Result<String> {
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("未知方法");
    match message.pointer(&format!("/params/{field}")) {
        None => bail!("{method} 通知缺少字符串字段 params.{field}"),
        Some(Value::String(value)) => Ok(value.clone()),
        Some(value) => bail!(
            "{method} 通知字段 params.{field} 必须是字符串，实际为 {}",
            summarize_json(value)
        ),
    }
}

fn required_notification_strings(message: &Value, field: &str) -> Result<Vec<String>> {
    message
        .pointer(&format!("/params/{field}"))
        .and_then(Value::as_array)
        .with_context(|| {
            let method = message
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("未知方法");
            format!("{method} 通知缺少数组字段 params.{field}")
        })?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .with_context(|| format!("通知字段 params.{field} 必须是字符串数组"))
        })
        .collect()
}

fn required_nullable_notification_string(message: &Value, field: &str) -> Result<Option<String>> {
    match message.pointer(&format!("/params/{field}")) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => {
            let method = message
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("未知方法");
            bail!("{method} 通知缺少字符串或 null 字段 params.{field}")
        }
    }
}

fn required_string_at(message: &Value, pointer: &str, field: &str) -> Result<String> {
    message
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| {
            let method = message
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("未知方法");
            format!("{method} 通知缺少字符串字段 {field}")
        })
}

fn thread_started_id(message: &Value) -> Result<String> {
    required_string_at(message, "/params/thread/id", "params.thread.id")
}

fn optional_string_at(message: &Value, pointer: &str, field: &str) -> Result<Option<String>> {
    match message.pointer(pointer) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => {
            let method = message
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("未知方法");
            bail!("{method} 通知字段 {field} 必须是字符串或 null")
        }
    }
}

fn parse_agent_notification(message: &Value) -> Result<Option<AgentEvent>> {
    let event = match message.get("method").and_then(Value::as_str) {
        Some("turn/started") => {
            let _ = required_notification_string(message, "threadId")?;
            let _ = required_string_at(message, "/params/turn/id", "params.turn.id")?;
            let status = required_string_at(message, "/params/turn/status", "params.turn.status")?;
            if status != "inProgress" {
                bail!(
                    "turn/started 通知的 params.turn.status 必须是 `inProgress`，实际为 `{status}`"
                );
            }
            AgentEvent::Started
        }
        Some("error") => {
            let _ = required_notification_string(message, "threadId")?;
            let _ = required_notification_string(message, "turnId")?;
            AgentEvent::Error {
                message: required_string_at(
                    message,
                    "/params/error/message",
                    "params.error.message",
                )?,
                details: optional_string_at(
                    message,
                    "/params/error/additionalDetails",
                    "params.error.additionalDetails",
                )?,
                will_retry: message
                    .pointer("/params/willRetry")
                    .and_then(Value::as_bool)
                    .context("error 通知缺少布尔字段 params.willRetry")?,
            }
        }
        Some("thread/settings/updated") => {
            let _ = required_notification_string(message, "threadId")?;
            let permissions = match message.pointer("/params/threadSettings/approvalPolicy") {
                None => None,
                Some(_) => {
                    let active_permission_profile =
                        match message.pointer("/params/threadSettings/activePermissionProfile") {
                            None | Some(Value::Null) => None,
                            Some(profile) => Some(AgentActivePermissionProfile {
                                id: profile
                                    .get("id")
                                    .and_then(Value::as_str)
                                    .context("activePermissionProfile.id 必须是字符串")?
                                    .to_owned(),
                                extends: profile
                                    .get("extends")
                                    .and_then(Value::as_str)
                                    .map(str::to_owned),
                            }),
                        };
                    Some(AgentEffectivePermissions {
                        approval_policy: required_string_at(
                            message,
                            "/params/threadSettings/approvalPolicy",
                            "params.threadSettings.approvalPolicy",
                        )?,
                        approvals_reviewer: required_string_at(
                            message,
                            "/params/threadSettings/approvalsReviewer",
                            "params.threadSettings.approvalsReviewer",
                        )?,
                        sandbox_policy: message
                            .pointer("/params/threadSettings/sandboxPolicy")
                            .filter(|value| !value.is_null())
                            .cloned(),
                        active_permission_profile,
                    })
                }
            };
            AgentEvent::ThreadSettingsUpdated(AgentThreadSettings {
                model: required_string_at(
                    message,
                    "/params/threadSettings/model",
                    "params.threadSettings.model",
                )?,
                effort: optional_string_at(
                    message,
                    "/params/threadSettings/effort",
                    "params.threadSettings.effort",
                )?,
                service_tier: optional_string_at(
                    message,
                    "/params/threadSettings/serviceTier",
                    "params.threadSettings.serviceTier",
                )?,
                cwd: required_string_at(
                    message,
                    "/params/threadSettings/cwd",
                    "params.threadSettings.cwd",
                )?,
                permissions,
            })
        }
        Some("mcpServer/startupStatus/updated") => AgentEvent::McpServerStartupStatusUpdated(
            parse_mcp_server_startup_status_updated(message)?,
        ),
        Some("thread/status/changed") => {
            AgentEvent::ThreadStatusChanged(parse_thread_status_changed(message)?)
        }
        Some("warning") => {
            let _ = optional_string_at(message, "/params/threadId", "params.threadId")?;
            AgentEvent::Warning {
                message: required_notification_string(message, "message")?,
            }
        }
        Some("configWarning") => {
            let range = match message.pointer("/params/range") {
                None | Some(Value::Null) => None,
                Some(Value::Object(_)) => Some((
                    message
                        .pointer("/params/range/start/line")
                        .and_then(Value::as_u64)
                        .context("configWarning 通知缺少无符号整数字段 params.range.start.line")?,
                    message
                        .pointer("/params/range/start/column")
                        .and_then(Value::as_u64)
                        .context(
                            "configWarning 通知缺少无符号整数字段 params.range.start.column",
                        )?,
                )),
                Some(_) => bail!("configWarning 通知字段 params.range 必须是对象或 null"),
            };
            AgentEvent::ConfigWarning(AgentConfigWarning {
                summary: required_notification_string(message, "summary")?,
                details: optional_string_at(message, "/params/details", "params.details")?,
                path: optional_string_at(message, "/params/path", "params.path")?,
                line: range.map(|(line, _)| line),
                column: range.map(|(_, column)| column),
            })
        }
        Some("model/rerouted") => {
            let _ = required_notification_string(message, "threadId")?;
            let _ = required_notification_string(message, "turnId")?;
            AgentEvent::ModelRerouted {
                from_model: required_notification_string(message, "fromModel")?,
                to_model: required_notification_string(message, "toModel")?,
                reason: required_notification_string(message, "reason")?,
            }
        }
        Some("model/verification") => {
            let _ = required_notification_string(message, "threadId")?;
            let _ = required_notification_string(message, "turnId")?;
            AgentEvent::ModelVerificationRequired {
                verifications: required_notification_strings(message, "verifications")?,
            }
        }
        Some("model/safetyBuffering/updated") => {
            let _ = required_notification_string(message, "threadId")?;
            let _ = required_notification_string(message, "turnId")?;
            AgentEvent::ModelSafetyBufferingUpdated {
                model: required_notification_string(message, "model")?,
                use_cases: required_notification_strings(message, "useCases")?,
                reasons: required_notification_strings(message, "reasons")?,
                show_buffering_ui: message
                    .pointer("/params/showBufferingUi")
                    .and_then(Value::as_bool)
                    .context(
                        "model/safetyBuffering/updated 通知缺少布尔字段 params.showBufferingUi",
                    )?,
                faster_model: required_nullable_notification_string(message, "fasterModel")?,
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(event))
}

fn forward_agent_notification(message: &Value, events: &Sender<AgentEvent>) -> Result<()> {
    if let Some(event) = parse_agent_notification(message)? {
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .context("已解析的 AgentEvent 缺少字符串 JSON-RPC method")?;
        events
            .send_blocking(event)
            .map_err(|_| anyhow!("Composer `{method}` 事件通道已经关闭"))?;
    }
    Ok(())
}

fn turn_failure_message(message: &Value) -> Result<String> {
    let message_text = optional_string_at(
        message,
        "/params/turn/error/message",
        "params.turn.error.message",
    )?
    .filter(|message| !message.trim().is_empty())
    .unwrap_or_else(|| "Codex turn 失败".to_owned());
    let details = optional_string_at(
        message,
        "/params/turn/error/additionalDetails",
        "params.turn.error.additionalDetails",
    )?
    .filter(|details| !details.trim().is_empty());
    Ok(match details {
        Some(details) if !message_text.contains(&details) => format!("{message_text}\n{details}"),
        _ => message_text,
    })
}

fn ensure_server_method_is_defined(message: &Value) -> Result<()> {
    let Some(method) = message.get("method") else {
        return Ok(());
    };
    let Some(method) = method.as_str() else {
        bail!(
            "Codex JSON-RPC 消息的 `method` 必须是字符串：{}",
            summarize_json(method)
        );
    };
    match method {
        // The request response remains the canonical source of the thread id.
        // This lifecycle notification is still schema-checked and correlated
        // with that response by the active prompt session.
        "thread/started" => thread_started_id(message).map(|_| ()),
        // app-server emits this connection-level status during initialization,
        // including on short-lived model catalog connections. It has no
        // Composer UI, but its protocol payload must remain schema-checked so
        // future shape changes still fail loudly.
        "remoteControl/status/changed" => validate_remote_control_status_changed(message),
        "mcpServer/startupStatus/updated" => {
            parse_mcp_server_startup_status_updated(message).map(|_| ())
        }
        "thread/status/changed" => parse_thread_status_changed(message).map(|_| ()),
        method if is_defined_server_method(method) => Ok(()),
        method => Err(undefined_server_method_error(method, message)),
    }
}

fn undefined_server_method_error(method: &str, message: &Value) -> anyhow::Error {
    let kind = if message.get("id").is_some() {
        "请求"
    } else {
        "通知"
    };
    let params = message
        .get("params")
        .map(summarize_json)
        .unwrap_or_else(|| "null".to_owned());
    anyhow!("遇到未定义的 Codex JSON-RPC {kind}方法 `{method}`；params={params}")
}

fn summarize_json(value: &Value) -> String {
    let rendered = value.to_string();
    let mut characters = rendered.chars();
    let mut summary: String = characters
        .by_ref()
        .take(UNDEFINED_METHOD_PARAMS_LIMIT)
        .collect();
    if characters.next().is_some() {
        summary.push('…');
    }
    summary
}

fn send_turn_event(events: &Sender<AgentEvent>, event: AgentEvent, source: &str) -> Result<()> {
    events
        .send_blocking(event)
        .map_err(|_| anyhow!("Composer `{source}` 事件通道已经关闭"))
}

fn required_turn_item(message: &Value) -> Result<&serde_json::Map<String, Value>> {
    message
        .pointer("/params/item")
        .and_then(Value::as_object)
        .ok_or_else(|| turn_item_protocol_error(message, "params.item 必须是对象"))
}

fn required_turn_item_type<'a>(
    message: &Value,
    item: &'a serde_json::Map<String, Value>,
) -> Result<&'a str> {
    item.get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| turn_item_protocol_error(message, "params.item.type 必须是字符串"))
}

fn turn_item_field_context(value: Option<&Value>) -> String {
    match value {
        None => "缺少".to_owned(),
        Some(Value::String(value)) => format!("`{value}`"),
        Some(value) => format!("非字符串({})", summarize_json(value)),
    }
}

fn turn_item_protocol_error(message: &Value, detail: impl std::fmt::Display) -> anyhow::Error {
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

fn required_item_string(
    item: &serde_json::Map<String, Value>,
    item_kind: &str,
    field: &str,
) -> Result<String> {
    item.get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("{item_kind} item.{field} 必须是字符串"))
}

fn validate_user_message(item: &serde_json::Map<String, Value>) -> Result<()> {
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
            unsupported => bail!("userMessage item.content[{index}].type `{unsupported}` 尚未接入"),
        }
    }
    Ok(())
}

fn parse_agent_message(item: &serde_json::Map<String, Value>) -> Result<(String, String)> {
    let item_type = required_item_string(item, "agentMessage", "type")?;
    if item_type != "agentMessage" {
        bail!("agentMessage item.type 必须是 `agentMessage`，实际为 `{item_type}`");
    }
    Ok((
        required_item_string(item, "agentMessage", "id")?,
        required_item_string(item, "agentMessage", "text")?,
    ))
}

fn validate_nullable_command_action_string(
    action: &serde_json::Map<String, Value>,
    index: usize,
    field: &str,
) -> Result<()> {
    match action.get(field) {
        None | Some(Value::Null | Value::String(_)) => Ok(()),
        Some(_) => {
            bail!("commandExecution item.commandActions[{index}].{field} 必须是字符串或 null")
        }
    }
}

fn parse_command_execution(item: &serde_json::Map<String, Value>) -> Result<CommandExecution> {
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
    for (index, action) in actions.iter().enumerate() {
        let action = action
            .as_object()
            .with_context(|| format!("commandExecution item.commandActions[{index}] 必须是对象"))?;
        let action_type = required_item_string(
            action,
            &format!("commandExecution item.commandActions[{index}]"),
            "type",
        )?;
        let action_command = required_item_string(
            action,
            &format!("commandExecution item.commandActions[{index}]"),
            "command",
        )?;
        match action_type.as_str() {
            "read" => {
                let _name = required_item_string(
                    action,
                    &format!("commandExecution item.commandActions[{index}]"),
                    "name",
                )?;
                let _path = required_item_string(
                    action,
                    &format!("commandExecution item.commandActions[{index}]"),
                    "path",
                )?;
                Ok(())
            }
            "listFiles" => {
                validate_nullable_command_action_string(action, index, "path")?;
                Ok(())
            }
            "search" => {
                validate_nullable_command_action_string(action, index, "path")?;
                validate_nullable_command_action_string(action, index, "query")?;
                Ok(())
            }
            "unknown" => Ok(()),
            other => Err(anyhow!(
                "commandExecution item.commandActions[{index}].type 包含未知值 `{other}`"
            )),
        }?;
        if index == 0 {
            first_action_command = Some(action_command);
        }
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
        cwd,
        output,
        status,
        exit_code,
    })
}

fn send(writer: &mut impl Write, message: Value) -> Result<()> {
    serde_json::to_writer(&mut *writer, &message).context("序列化 Codex JSON-RPC 消息失败")?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn read_message(reader: &mut impl BufRead) -> Result<Value> {
    let mut line = String::new();
    let bytes = reader.read_line(&mut line)?;
    if bytes == 0 {
        bail!("Codex app-server 在 turn 完成前关闭了输出流");
    }
    serde_json::from_str(&line).with_context(|| format!("无法解析 Codex JSON-RPC 消息：{line}"))
}

fn wait_for_response(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
    expected_id: u64,
    events: Option<&Sender<AgentEvent>>,
) -> Result<Value> {
    loop {
        let message = read_message(reader)?;
        respond_to_server_request(writer, &message)?;
        // App-scoped MCP startup status can arrive on short-lived connections
        // that have no Composer event stream. It is schema-checked below and
        // only the thread prompt connection forwards it into GPUI state.
        if let Some(events) = events {
            forward_agent_notification(&message, events)?;
        } else if parse_agent_notification(&message)?.is_some()
            && message.get("method").and_then(Value::as_str)
                != Some("mcpServer/startupStatus/updated")
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

fn wait_for_session_response<R: BufRead, W: Write + Send + 'static>(
    reader: &mut R,
    session: &Arc<CodexTurnSession<W>>,
    expected_id: u64,
    events: &Sender<AgentEvent>,
    mut thread_started_correlation: Option<&mut ThreadStartedCorrelation>,
    mut deferred_turn_notifications: Option<&mut Vec<Value>>,
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

fn ensure_deferred_session_messages_match<W: Write + Send>(
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

fn is_integrated_server_request_method(method: &str) -> bool {
    matches!(
        method,
        "item/commandExecution/requestApproval"
            | "item/tool/requestUserInput"
            | "item/permissions/requestApproval"
    )
}

fn ensure_session_message_matches(
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

fn respond_to_server_request(writer: &mut impl Write, message: &Value) -> Result<()> {
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

fn respond_to_server_request_on_session<W: Write + Send + 'static>(
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
        if let Err(error) = session.register_command_approval(
            request_metadata_for_command(&request),
            params,
            available_decisions,
        ) {
            return reject_server_request(
                session,
                message,
                -32600,
                "Duplicate or invalid server request",
                error,
            );
        }
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
    if method == "item/tool/requestUserInput" {
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

fn reject_server_request<W: Write + Send>(
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

fn parse_command_approval_request(
    message: &Value,
) -> Result<(
    AgentServerRequestId,
    AgentCommandApprovalRequest,
    Value,
    Vec<Value>,
)> {
    let request_id = request_id_from_value(
        message
            .get("id")
            .context("command approval request 缺少 JSON-RPC id")?,
    )?;
    let params = message
        .get("params")
        .and_then(Value::as_object)
        .context("command approval request 缺少对象 params")?;
    for field in ["kind", "threadId", "turnId", "itemId"] {
        params
            .get(field)
            .and_then(Value::as_str)
            .with_context(|| format!("command approval params.{field} 必须是字符串"))?;
    }
    params
        .get("startedAtMs")
        .and_then(Value::as_i64)
        .context("command approval params.startedAtMs 必须是 int64")?;
    match params.get("environmentId") {
        Some(Value::String(_) | Value::Null) => {}
        _ => bail!("command approval params.environmentId 必须是字符串或 null"),
    }

    let available_decisions = match params.get("availableDecisions") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(decisions)) => decisions.clone(),
        Some(_) => bail!("command approval params.availableDecisions 必须是数组或 null"),
    };
    let allow_once = available_decisions
        .iter()
        .any(|decision| decision.as_str() == Some("accept"));
    let decline = available_decisions
        .iter()
        .any(|decision| decision.as_str() == Some("decline"));
    let cancel = available_decisions
        .iter()
        .any(|decision| decision.as_str() == Some("cancel"));
    let accept_with_execpolicy_amendment = available_decisions
        .iter()
        .find(|decision| decision.get("acceptWithExecpolicyAmendment").is_some())
        .cloned();

    let optional_string = |field: &str| -> Result<Option<String>> {
        match params.get(field) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(value)) => Ok(Some(value.clone())),
            Some(_) => bail!("command approval params.{field} 必须是字符串或 null"),
        }
    };
    let command = optional_string("command")?
        .or_else(|| {
            params
                .get("commandActions")
                .and_then(Value::as_array)
                .and_then(|actions| actions.first())
                .and_then(|action| action.get("command"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_default();
    let network_host = match params.get("networkApprovalContext") {
        None | Some(Value::Null) => None,
        Some(Value::Object(context)) => Some(
            context
                .get("host")
                .and_then(Value::as_str)
                .context("command approval params.networkApprovalContext.host 必须是字符串")?
                .to_owned(),
        ),
        Some(_) => bail!("command approval params.networkApprovalContext 必须是对象或 null"),
    };
    let request = AgentCommandApprovalRequest {
        request_id: request_id.clone(),
        thread_id: params["threadId"]
            .as_str()
            .expect("validated above")
            .to_owned(),
        turn_id: params["turnId"]
            .as_str()
            .expect("validated above")
            .to_owned(),
        item_id: params["itemId"]
            .as_str()
            .expect("validated above")
            .to_owned(),
        command,
        reason: optional_string("reason")?,
        network_host,
        allow_once,
        decline,
        cancel,
        can_accept_with_execpolicy_amendment: accept_with_execpolicy_amendment.is_some(),
    };
    Ok((
        request_id,
        request,
        Value::Object(params.clone()),
        available_decisions,
    ))
}

fn required_request_string(
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

fn optional_request_string(
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

fn parse_user_input_request(
    message: &Value,
) -> Result<(AgentServerRequestId, AgentUserInputRequest)> {
    const METHOD: &str = "item/tool/requestUserInput";
    let request_id = request_id_from_value(
        message
            .get("id")
            .context("user input request 缺少 JSON-RPC id")?,
    )?;
    let params = message
        .get("params")
        .and_then(Value::as_object)
        .context("item/tool/requestUserInput 缺少对象 params")?;
    let thread_id = required_request_string(params, METHOD, "threadId")?;
    let turn_id = required_request_string(params, METHOD, "turnId")?;
    let item_id = required_request_string(params, METHOD, "itemId")?;
    let is_blocking = params
        .get("isBlocking")
        .and_then(Value::as_bool)
        .context("item/tool/requestUserInput params.isBlocking 必须是布尔值")?;
    let auto_resolution_ms =
        match params.get("autoResolutionMs") {
            None | Some(Value::Null) => None,
            Some(value) => Some(value.as_u64().context(
                "item/tool/requestUserInput params.autoResolutionMs 必须是 uint64 或 null",
            )?),
        };
    let questions = params
        .get("questions")
        .and_then(Value::as_array)
        .context("item/tool/requestUserInput params.questions 必须是数组")?;
    let mut question_ids = HashSet::new();
    let mut parsed_questions = Vec::with_capacity(questions.len());
    for (index, question) in questions.iter().enumerate() {
        let question = question.as_object().with_context(|| {
            format!("item/tool/requestUserInput params.questions[{index}] 必须是对象")
        })?;
        let id = question
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .with_context(|| {
                format!("item/tool/requestUserInput params.questions[{index}].id 必须是字符串")
            })?;
        if !question_ids.insert(id.clone()) {
            bail!("item/tool/requestUserInput 包含重复 question id `{id}`");
        }
        let header = question
            .get("header")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .with_context(|| {
                format!("item/tool/requestUserInput params.questions[{index}].header 必须是字符串")
            })?;
        let question_text = question
            .get("question")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .with_context(|| {
                format!(
                    "item/tool/requestUserInput params.questions[{index}].question 必须是字符串"
                )
            })?;
        let allows_other = match question.get("isOther") {
            None => false,
            Some(value) => value.as_bool().with_context(|| {
                format!("item/tool/requestUserInput params.questions[{index}].isOther 必须是布尔值")
            })?,
        };
        let is_secret = match question.get("isSecret") {
            None => false,
            Some(value) => value.as_bool().with_context(|| {
                format!(
                    "item/tool/requestUserInput params.questions[{index}].isSecret 必须是布尔值"
                )
            })?,
        };
        let options = match question.get("options") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(options)) => {
                let mut parsed = Vec::with_capacity(options.len());
                for (option_index, option) in options.iter().enumerate() {
                    let option = option.as_object().with_context(|| {
                        format!(
                            "item/tool/requestUserInput params.questions[{index}].options[{option_index}] 必须是对象"
                        )
                    })?;
                    let label = option
                        .get("label")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .with_context(|| {
                            format!(
                                "item/tool/requestUserInput params.questions[{index}].options[{option_index}].label 必须是字符串"
                            )
                        })?;
                    let description = option
                        .get("description")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .with_context(|| {
                            format!(
                                "item/tool/requestUserInput params.questions[{index}].options[{option_index}].description 必须是字符串"
                            )
                        })?;
                    parsed.push(AgentUserInputOption { label, description });
                }
                parsed
            }
            Some(_) => bail!(
                "item/tool/requestUserInput params.questions[{index}].options 必须是数组或 null"
            ),
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

fn parse_optional_field<T>(
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

fn parse_permissions_approval_request(
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
    let permissions = params
        .get("permissions")
        .and_then(Value::as_object)
        .context("item/permissions/requestApproval params.permissions 必须是对象")?;
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
            permissions: AgentPermissionRequestProfile {
                file_system,
                network,
            },
        },
    ))
}

fn parse_additional_network_permissions(
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

fn parse_additional_file_system_permissions(
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

fn parse_file_system_permission_entry(
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

fn parse_file_system_path(value: &Value, entry_index: usize) -> Result<AgentFileSystemPath> {
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

fn parse_file_system_special_path(
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

fn handle_server_request_resolved<W: Write + Send>(
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

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        io::{Cursor, Error as IoError, ErrorKind, Write},
        path::{Path, PathBuf},
        process::Command,
        sync::Arc,
        time::{Duration, Instant},
    };

    use serde_json::{Value, json};

    use super::{
        AgentBackend, AgentCommandApprovalChoice, AgentConfigWarning, AgentEvent,
        AgentInterruptControl, AgentInterruptHandle, AgentInterruptOutcome,
        AgentMcpServerStartupFailureReason, AgentMcpServerStartupState,
        AgentMcpServerStartupStatus, AgentOptionalField, AgentPermissionMode,
        AgentPermissionsApprovalChoice, AgentRequest, AgentServerRequestFailureKind,
        AgentServerRequestId, AgentServerRequestKind, AgentServerRequestMetadata,
        AgentThreadActiveFlag, AgentThreadSettings, AgentThreadStatus, AgentThreadStatusState,
        AgentUserInputResponse, AppServerProcess, CodexAppServerBackend, CodexTurnSession,
        INITIALIZE_ID, MODEL_LIST_PAGE_SIZE, TurnOutcome, UNDEFINED_METHOD_PARAMS_LIMIT,
        cleanup_pending_server_requests, drive_model_catalog, drive_permission_profiles,
        drive_session, drive_thread_settings_update, ensure_server_method_is_defined,
        finish_prompt_session, handle_server_request_resolved, parse_agent_notification,
        respond_to_server_request_on_session, run_model_catalog_process,
        thread_settings_update_request, wait_for_response,
    };
    use crate::agent::AgentUserInputAnswer;

    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
            Err(IoError::new(
                ErrorKind::BrokenPipe,
                "fixture JSON-RPC write failure",
            ))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn command_approval_request(id: Value) -> Value {
        command_approval_request_for(id, "thr_1", "turn_1")
    }

    fn current_command_approval_request(id: Value) -> Value {
        let mut request = command_approval_request(id);
        request["params"]["availableDecisions"][2] = json!("cancel");
        request
    }

    fn command_approval_request_for(id: Value, thread_id: &str, turn_id: &str) -> Value {
        json!({
            "id": id,
            "method": "item/commandExecution/requestApproval",
            "params": {
                "kind": "command",
                "threadId": thread_id,
                "turnId": turn_id,
                "itemId": "item_1",
                "startedAtMs": 1_777_777_777_000_i64,
                "environmentId": null,
                "reason": "需要读取版本",
                "command": "git --version",
                "cwd": "/tmp",
                "commandActions": [{"type":"unknown","command":"git --version"}],
                "proposedExecpolicyAmendment": ["git", "--version"],
                "availableDecisions": [
                    "accept",
                    {"acceptWithExecpolicyAmendment":{"execpolicy_amendment":["git","--version"]}},
                    "decline"
                ]
            }
        })
    }

    fn user_input_request(id: Value) -> Value {
        json!({
            "id": id,
            "method": "item/tool/requestUserInput",
            "params": {
                "threadId": "thr_1",
                "turnId": "turn_1",
                "itemId": "tool_1",
                "questions": [
                    {
                        "id": "color",
                        "header": "Color",
                        "question": "Choose colors",
                        "isOther": true,
                        "isSecret": false,
                        "options": [
                            {"label": "red", "description": "Warm"},
                            {"label": "blue", "description": "Cool"}
                        ]
                    },
                    {
                        "id": "token",
                        "header": "Token",
                        "question": "Enter the token",
                        "isOther": true,
                        "isSecret": true,
                        "options": null
                    }
                ],
                "isBlocking": true,
                "autoResolutionMs": 1500
            }
        })
    }

    fn permissions_approval_request(id: Value) -> Value {
        json!({
            "id": id,
            "method": "item/permissions/requestApproval",
            "params": {
                "threadId": "thr_1",
                "turnId": "turn_1",
                "itemId": "permissions_1",
                "environmentId": "env_1",
                "startedAtMs": 1_777_777_777_000_i64,
                "cwd": "/workspace/project",
                "reason": "Read fixtures and contact the network",
                "permissions": {
                    "fileSystem": {
                        "read": ["/legacy/read"],
                        "write": null,
                        "globScanMaxDepth": 4,
                        "entries": [
                            {"access":"read","path":{"type":"path","path":"/workspace/input"}},
                            {"access":"write","path":{"type":"glob_pattern","pattern":"/workspace/out/**"}},
                            {"access":"deny","path":{"type":"special","value":{"kind":"project_roots","subpath":"private"}}},
                            {"access":"read","path":{"type":"special","value":{"kind":"root"}}},
                            {"access":"read","path":{"type":"special","value":{"kind":"minimal"}}},
                            {"access":"read","path":{"type":"special","value":{"kind":"tmpdir"}}},
                            {"access":"read","path":{"type":"special","value":{"kind":"slash_tmp"}}},
                            {"access":"read","path":{"type":"special","value":{"kind":"unknown","path":"/unknown","subpath":"child"}}}
                        ]
                    },
                    "network": {"enabled": true}
                }
            }
        })
    }

    fn take_session_output(session: &CodexTurnSession<Vec<u8>>) -> Vec<u8> {
        let mut writer = session.writer.lock().unwrap();
        std::mem::take(writer.as_mut().unwrap())
    }

    fn turn_item_message(method: &str, item: Value) -> Value {
        json!({
            "method": method,
            "params": {
                "threadId": "thr_1",
                "turnId": "turn_1",
                "item": item
            }
        })
    }

    fn assert_turn_message_fails(message: &Value, expected: &[&str]) -> String {
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        let mut streamed_text = false;
        let error = super::process_turn_message(
            &session,
            message,
            "thr_1",
            "turn_1",
            &tx,
            &mut streamed_text,
        )
        .unwrap_err()
        .to_string();
        assert!(
            rx.try_recv().is_err(),
            "unexpected event for {message}: {error}"
        );
        for fragment in expected {
            assert!(
                error.contains(fragment),
                "error for {message} did not contain `{fragment}`: {error}"
            );
        }
        error
    }

    fn command_execution_item(status: &str) -> Value {
        json!({
            "type": "commandExecution",
            "id": "exec_1",
            "command": "/bin/zsh -lc pwd",
            "commandActions": [{"type": "unknown", "command": "pwd"}],
            "cwd": "/tmp/project",
            "status": status,
            "aggregatedOutput": null,
            "exitCode": null
        })
    }

    fn turn_start_for_mode(mode: AgentPermissionMode, cwd: PathBuf) -> Value {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_permissions\"}}}\n",
            "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_permissions\"}}}\n",
            "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thr_permissions\",\"turn\":{\"id\":\"turn_permissions\",\"status\":\"completed\"}}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, _rx) = async_channel::unbounded();
        drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "permission probe".into(),
                cwd,
                thread_id: None,
                model: "gpt-test".into(),
                effort: "medium".into(),
                service_tier: None,
                permission_mode: mode,
            },
            &tx,
        )
        .unwrap();
        String::from_utf8(take_session_output(&session))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|message| message.get("method").and_then(Value::as_str) == Some("turn/start"))
            .unwrap()
    }

    #[test]
    fn permission_mode_requests_match_the_four_protocol_shapes() {
        let cwd = PathBuf::from("/tmp/project");
        let request =
            thread_settings_update_request(7, "thr_1", &cwd, AgentPermissionMode::Request).unwrap();
        assert_eq!(
            request.pointer("/params/approvalPolicy"),
            Some(&json!("on-request"))
        );
        assert_eq!(
            request.pointer("/params/approvalsReviewer"),
            Some(&json!("user"))
        );
        assert_eq!(
            request.pointer("/params/permissions"),
            Some(&json!(":workspace"))
        );
        assert!(request.pointer("/params/sandboxPolicy").is_none());

        let assist =
            thread_settings_update_request(7, "thr_1", &cwd, AgentPermissionMode::Assist).unwrap();
        assert_eq!(
            assist.pointer("/params/approvalsReviewer"),
            Some(&json!("guardian_subagent"))
        );
        assert_eq!(
            assist.pointer("/params/permissions"),
            Some(&json!(":workspace"))
        );

        let full =
            thread_settings_update_request(7, "thr_1", &cwd, AgentPermissionMode::Full).unwrap();
        assert_eq!(
            full.pointer("/params/approvalPolicy"),
            Some(&json!("never"))
        );
        assert_eq!(
            full.pointer("/params/approvalsReviewer"),
            Some(&json!("user"))
        );
        assert_eq!(
            full.pointer("/params/permissions"),
            Some(&json!(":danger-full-access"))
        );

        let temp =
            std::env::temp_dir().join(format!("gpui-permission-custom-{}", std::process::id()));
        std::fs::create_dir_all(temp.join(".codex")).unwrap();
        std::fs::write(
            temp.join(".codex/config.toml"),
            "sandbox_mode = \"danger-full-access\"\n",
        )
        .unwrap();
        let custom =
            thread_settings_update_request(7, "thr_1", &temp, AgentPermissionMode::Custom).unwrap();
        assert_eq!(
            custom.pointer("/params/approvalPolicy"),
            Some(&json!("on-request"))
        );
        assert_eq!(
            custom.pointer("/params/approvalsReviewer"),
            Some(&json!("user"))
        );
        assert_eq!(
            custom.pointer("/params/sandboxPolicy/type"),
            Some(&json!("dangerFullAccess"))
        );
        assert!(custom.pointer("/params/permissions").is_none());
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn first_turn_carries_each_permission_mode_and_assist_uses_effective_reviewer() {
        let request =
            turn_start_for_mode(AgentPermissionMode::Request, PathBuf::from("/tmp/project"));
        assert_eq!(
            request.pointer("/params/approvalPolicy"),
            Some(&json!("on-request"))
        );
        assert_eq!(
            request.pointer("/params/approvalsReviewer"),
            Some(&json!("user"))
        );
        assert_eq!(
            request.pointer("/params/sandboxPolicy/type"),
            Some(&json!("workspaceWrite"))
        );
        assert_eq!(request.pointer("/params/permissions"), Some(&Value::Null));
        assert_eq!(
            request.pointer("/params/runtimeWorkspaceRoots"),
            Some(&Value::Null)
        );

        let assist =
            turn_start_for_mode(AgentPermissionMode::Assist, PathBuf::from("/tmp/project"));
        assert_eq!(
            assist.pointer("/params/approvalsReviewer"),
            Some(&json!("auto_review"))
        );

        let full = turn_start_for_mode(AgentPermissionMode::Full, PathBuf::from("/tmp/project"));
        assert_eq!(
            full.pointer("/params/permissions"),
            Some(&json!(":danger-full-access"))
        );
        assert_eq!(full.pointer("/params/sandboxPolicy"), Some(&Value::Null));
        assert!(
            full.pointer("/params/runtimeWorkspaceRoots")
                .is_some_and(Value::is_array)
        );

        let temp = std::env::temp_dir().join(format!(
            "gpui-permission-turn-custom-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(temp.join(".codex")).unwrap();
        std::fs::write(
            temp.join(".codex/config.toml"),
            "sandbox_mode = \"danger-full-access\"\n",
        )
        .unwrap();
        let custom = turn_start_for_mode(AgentPermissionMode::Custom, temp.clone());
        assert_eq!(
            custom.pointer("/params/sandboxPolicy/type"),
            Some(&json!("dangerFullAccess"))
        );
        assert_eq!(custom.pointer("/params/permissions"), Some(&Value::Null));
        assert!(
            custom
                .pointer("/params/runtimeWorkspaceRoots")
                .is_some_and(Value::is_array)
        );
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn effective_permission_notification_preserves_auto_review_and_profile() {
        let message = json!({
            "method": "thread/settings/updated",
            "params": { "threadId": "thr_1", "threadSettings": {
                "model": "gpt-test", "effort": "medium", "serviceTier": null, "cwd": "/tmp/project",
                "approvalPolicy": "on-request", "approvalsReviewer": "auto_review",
                "sandboxPolicy": { "type": "workspaceWrite", "writableRoots": ["/tmp/project"] },
                "activePermissionProfile": { "id": ":workspace", "extends": null }
            }}
        });
        let Some(AgentEvent::ThreadSettingsUpdated(settings)) =
            parse_agent_notification(&message).unwrap()
        else {
            panic!("expected settings event");
        };
        let permissions = settings.permissions.unwrap();
        assert_eq!(permissions.approvals_reviewer, "auto_review");
        assert_eq!(permissions.approval_policy, "on-request");
        assert_eq!(
            permissions.active_permission_profile.unwrap().id,
            ":workspace"
        );
        assert_eq!(
            permissions.sandbox_policy.unwrap()["type"],
            "workspaceWrite"
        );
    }

    #[test]
    fn settings_rpc_failure_returns_error_without_an_effective_update() {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"id\":2,\"error\":{\"code\":-32602,\"message\":\"invalid permissions\"}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let mut writer = Vec::new();
        let error = drive_thread_settings_update(
            &mut reader,
            &mut writer,
            "thr_1",
            Path::new("/tmp/project"),
            AgentPermissionMode::Assist,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("invalid permissions"));
    }

    #[test]
    fn existing_thread_switch_waits_for_and_returns_effective_settings() {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"id\":2,\"result\":{}}\n",
            "{\"method\":\"thread/settings/updated\",\"params\":{\"threadId\":\"thr_1\",\"threadSettings\":{\"model\":\"gpt-test\",\"effort\":\"medium\",\"serviceTier\":null,\"cwd\":\"/tmp/project\",\"approvalPolicy\":\"on-request\",\"approvalsReviewer\":\"auto_review\",\"sandboxPolicy\":{\"type\":\"workspaceWrite\"},\"activePermissionProfile\":{\"id\":\":workspace\",\"extends\":null}}}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let mut writer = Vec::new();
        let settings = drive_thread_settings_update(
            &mut reader,
            &mut writer,
            "thr_1",
            Path::new("/tmp/project"),
            AgentPermissionMode::Assist,
        )
        .unwrap();
        let effective = settings.permissions.unwrap();
        assert_eq!(effective.approvals_reviewer, "auto_review");
        let sent = String::from_utf8(writer).unwrap();
        let update: Value = sent
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .find(|message: &Value| {
                message.get("method").and_then(Value::as_str) == Some("thread/settings/update")
            })
            .unwrap();
        assert_eq!(
            update.pointer("/params/approvalsReviewer"),
            Some(&json!("guardian_subagent"))
        );
    }

    #[test]
    fn permission_profile_list_maps_available_profiles() {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"id\":2,\"result\":{\"data\":[{\"id\":\":workspace\",\"allowed\":true,\"extends\":null}],\"nextCursor\":null}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let mut writer = Vec::new();
        let profiles =
            drive_permission_profiles(&mut reader, &mut writer, Path::new("/tmp/project")).unwrap();
        assert_eq!(
            profiles,
            vec![super::AgentPermissionProfile {
                id: ":workspace".into(),
                allowed: true,
                extends: None
            }]
        );
        let sent = String::from_utf8(writer).unwrap();
        assert!(sent.contains("\"method\":\"permissionProfile/list\""));
    }

    #[test]
    fn drives_one_complete_prompt_and_normalizes_stream_events() {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"method\":\"thread/status/changed\",\"params\":{\"threadId\":\"thr_1\",\"status\":{\"type\":\"active\",\"activeFlags\":[]}}}\n",
            "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_1\"}}}\n",
            "{\"method\":\"thread/started\",\"params\":{\"thread\":{\"id\":\"thr_1\",\"sessionId\":\"thr_1\",\"ephemeral\":false,\"turns\":[]}}}\n",
            "{\"method\":\"mcpServer/startupStatus/updated\",\"params\":{\"threadId\":\"thr_1\",\"name\":\"codex_apps\",\"status\":\"starting\",\"error\":null,\"failureReason\":null}}\n",
            "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_1\",\"turn\":{\"id\":\"turn_1\",\"items\":[],\"status\":\"inProgress\"}}}\n",
            "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_1\"}}}\n",
            "{\"method\":\"item/started\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"item\":{\"type\":\"agentMessage\",\"id\":\"msg_1\",\"text\":\"\"}}}\n",
            "{\"method\":\"item/agentMessage/delta\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"itemId\":\"msg_1\",\"delta\":\"你好\"}}\n",
            "{\"method\":\"item/agentMessage/delta\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"itemId\":\"msg_1\",\"delta\":\"！\"}}\n",
            "{\"method\":\"item/started\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"item\":{\"type\":\"commandExecution\",\"id\":\"exec_1\",\"command\":\"/bin/zsh -lc pwd\",\"cwd\":\"/tmp/project\",\"status\":\"inProgress\",\"commandActions\":[{\"type\":\"unknown\",\"command\":\"pwd\"}],\"aggregatedOutput\":null,\"exitCode\":null}}}\n",
            "{\"method\":\"item/commandExecution/outputDelta\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"itemId\":\"exec_1\",\"delta\":\"/tmp/project\\n\"}}\n",
            "{\"method\":\"item/completed\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"item\":{\"type\":\"commandExecution\",\"id\":\"exec_1\",\"command\":\"/bin/zsh -lc pwd\",\"cwd\":\"/tmp/project\",\"status\":\"completed\",\"commandActions\":[{\"type\":\"unknown\",\"command\":\"pwd\"}],\"aggregatedOutput\":\"/tmp/project\\n\",\"exitCode\":0}}}\n",
            "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thr_1\",\"turn\":{\"id\":\"turn_1\",\"status\":\"completed\"}}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        let outcome = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "打个招呼".into(),
                cwd: PathBuf::from("/tmp/project"),
                thread_id: None,
                model: "gpt-test".into(),
                effort: "high".into(),
                service_tier: Some("priority".into()),
                permission_mode: AgentPermissionMode::Full,
            },
            &tx,
        )
        .unwrap();
        assert_eq!(outcome, TurnOutcome::Completed);
        tx.send_blocking(outcome.into_event()).unwrap();
        drop(tx);

        let mut received = Vec::new();
        while let Ok(event) = rx.try_recv() {
            received.push(event);
        }
        assert_eq!(
            received,
            vec![
                AgentEvent::ThreadStatusChanged(AgentThreadStatus {
                    thread_id: "thr_1".into(),
                    state: AgentThreadStatusState::Active {
                        active_flags: Vec::new(),
                    },
                }),
                AgentEvent::ThreadCreated {
                    thread_id: "thr_1".into()
                },
                AgentEvent::McpServerStartupStatusUpdated(AgentMcpServerStartupStatus {
                    thread_id: Some("thr_1".into()),
                    name: "codex_apps".into(),
                    state: AgentMcpServerStartupState::Starting,
                    error: None,
                    failure_reason: None,
                }),
                AgentEvent::Started,
                AgentEvent::AssistantMessageStarted {
                    item_id: "msg_1".into(),
                },
                AgentEvent::TextDelta("你好".into()),
                AgentEvent::TextDelta("！".into()),
                AgentEvent::CommandStarted(super::CommandExecution {
                    id: "exec_1".into(),
                    command: "pwd".into(),
                    cwd: "/tmp/project".into(),
                    output: String::new(),
                    status: super::CommandExecutionStatus::InProgress,
                    exit_code: None,
                }),
                AgentEvent::CommandOutputDelta {
                    item_id: "exec_1".into(),
                    delta: "/tmp/project\n".into(),
                },
                AgentEvent::CommandCompleted(super::CommandExecution {
                    id: "exec_1".into(),
                    command: "pwd".into(),
                    cwd: "/tmp/project".into(),
                    output: "/tmp/project\n".into(),
                    status: super::CommandExecutionStatus::Completed,
                    exit_code: Some(0),
                }),
                AgentEvent::Completed,
            ]
        );

        let sent = String::from_utf8(take_session_output(&session)).unwrap();
        assert!(sent.contains("\"method\":\"initialize\""));
        assert!(sent.contains("\"method\":\"initialized\""));
        assert!(sent.contains("\"method\":\"thread/start\""));
        assert!(sent.contains("\"method\":\"turn/start\""));
        assert!(sent.contains("\"threadId\":\"thr_1\""));

        let sent_messages: Vec<serde_json::Value> = sent
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let sent_methods: Vec<_> = sent_messages
            .iter()
            .filter_map(|message| message.get("method").and_then(Value::as_str))
            .collect();
        assert_eq!(
            sent_methods,
            vec!["initialize", "initialized", "thread/start", "turn/start"]
        );
        let initialize = sent_messages
            .iter()
            .find(|message| {
                message.get("method").and_then(|value| value.as_str()) == Some("initialize")
            })
            .unwrap();
        assert_eq!(
            initialize.pointer("/params/capabilities/experimentalApi"),
            Some(&json!(true))
        );
        assert_eq!(
            initialize.pointer("/params/capabilities/requestAttestation"),
            Some(&json!(false))
        );
        let thread_start = sent_messages
            .iter()
            .find(|message| {
                message.get("method").and_then(|value| value.as_str()) == Some("thread/start")
            })
            .unwrap();
        assert_eq!(
            thread_start
                .pointer("/params/model")
                .and_then(|value| value.as_str()),
            Some("gpt-test")
        );
        assert_eq!(
            thread_start
                .pointer("/params/serviceTier")
                .and_then(|value| value.as_str()),
            Some("priority")
        );
        assert!(thread_start.pointer("/params/approvalPolicy").is_none());
        assert!(thread_start.pointer("/params/sandbox").is_none());
        let turn_start = sent_messages
            .iter()
            .find(|message| {
                message.get("method").and_then(|value| value.as_str()) == Some("turn/start")
            })
            .unwrap();
        assert_eq!(
            turn_start
                .pointer("/params/model")
                .and_then(|value| value.as_str()),
            Some("gpt-test")
        );
        assert_eq!(
            turn_start
                .pointer("/params/effort")
                .and_then(|value| value.as_str()),
            Some("high")
        );
        assert_eq!(
            turn_start
                .pointer("/params/serviceTier")
                .and_then(|value| value.as_str()),
            Some("priority")
        );
        assert_eq!(
            turn_start.pointer("/params/approvalPolicy"),
            Some(&json!("never"))
        );
        assert_eq!(
            turn_start.pointer("/params/approvalsReviewer"),
            Some(&json!("user"))
        );
        assert_eq!(
            turn_start.pointer("/params/sandboxPolicy"),
            Some(&Value::Null)
        );
        assert_eq!(
            turn_start.pointer("/params/permissions"),
            Some(&json!(":danger-full-access"))
        );
        assert!(
            turn_start
                .pointer("/params/runtimeWorkspaceRoots")
                .is_some_and(Value::is_array)
        );
    }

    #[test]
    fn existing_thread_resumes_before_turn_start() {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"method\":\"thread/started\",\"params\":{\"thread\":{\"id\":\"thr_existing\",\"turns\":[]}}}\n",
            "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_existing\",\"turns\":[]}}}\n",
            "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_existing\",\"turn\":{\"id\":\"turn_next\",\"items\":[],\"status\":\"inProgress\"}}}\n",
            "{\"method\":\"item/agentMessage/delta\",\"params\":{\"threadId\":\"thr_existing\",\"turnId\":\"turn_next\",\"itemId\":\"msg_next\",\"delta\":\"继续\"}}\n",
            "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_next\"}}}\n",
            "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thr_existing\",\"turn\":{\"id\":\"turn_next\",\"status\":\"completed\"}}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();

        let outcome = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "继续对话".into(),
                cwd: PathBuf::from("/tmp/project"),
                thread_id: Some("thr_existing".into()),
                model: "gpt-test".into(),
                effort: "high".into(),
                service_tier: Some("priority".into()),
                permission_mode: AgentPermissionMode::Request,
            },
            &tx,
        )
        .unwrap();
        assert_eq!(outcome, TurnOutcome::Completed);
        tx.send_blocking(outcome.into_event()).unwrap();
        drop(tx);

        let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert_eq!(
            events,
            vec![
                AgentEvent::Started,
                AgentEvent::TextDelta("继续".into()),
                AgentEvent::Completed,
            ]
        );

        let sent_messages: Vec<Value> = String::from_utf8(take_session_output(&session))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let sent_methods: Vec<_> = sent_messages
            .iter()
            .filter_map(|message| message.get("method").and_then(Value::as_str))
            .collect();
        assert_eq!(
            sent_methods,
            vec!["initialize", "initialized", "thread/resume", "turn/start"]
        );
        let resume = sent_messages
            .iter()
            .find(|message| message.get("method").and_then(Value::as_str) == Some("thread/resume"))
            .unwrap();
        assert_eq!(resume.get("id"), Some(&json!(2)));
        assert_eq!(
            resume.get("params"),
            Some(&json!({"threadId":"thr_existing"}))
        );

        let turn_start = sent_messages
            .iter()
            .find(|message| message.get("method").and_then(Value::as_str) == Some("turn/start"))
            .unwrap();
        assert_eq!(
            turn_start.pointer("/params/threadId"),
            Some(&json!("thr_existing"))
        );
        assert_eq!(
            turn_start.pointer("/params/model"),
            Some(&json!("gpt-test"))
        );
        assert_eq!(turn_start.pointer("/params/effort"), Some(&json!("high")));
        assert_eq!(
            turn_start.pointer("/params/serviceTier"),
            Some(&json!("priority"))
        );
        for field in [
            "approvalPolicy",
            "approvalsReviewer",
            "sandboxPolicy",
            "permissions",
            "runtimeWorkspaceRoots",
        ] {
            assert!(turn_start.pointer(&format!("/params/{field}")).is_none());
        }
    }

    #[test]
    fn thread_started_must_match_the_canonical_thread_id_in_either_order() {
        let cases = [
            (
                concat!(
                    "{\"id\":1,\"result\":{}}\n",
                    "{\"method\":\"thread/started\",\"params\":{\"thread\":{\"id\":\"thr_wrong\"}}}\n",
                    "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_expected\"}}}\n"
                ),
                "thread/start 通知与响应不一致",
            ),
            (
                concat!(
                    "{\"id\":1,\"result\":{}}\n",
                    "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_expected\"}}}\n",
                    "{\"method\":\"thread/started\",\"params\":{\"thread\":{\"id\":\"thr_wrong\"}}}\n"
                ),
                "turn/start 失败",
            ),
        ];

        for (input, expected_context) in cases {
            let mut reader = Cursor::new(input.as_bytes());
            let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
            let (tx, _rx) = async_channel::unbounded();
            let error = drive_session(
                &mut reader,
                &session,
                &AgentRequest {
                    prompt: "检查生命周期关联".into(),
                    cwd: PathBuf::from("/tmp/project"),
                    thread_id: None,
                    model: "gpt-test".into(),
                    effort: "medium".into(),
                    service_tier: None,
                    permission_mode: AgentPermissionMode::Full,
                },
                &tx,
            )
            .unwrap_err();
            let message = format!("{error:#}");
            assert!(message.contains(expected_context), "{message}");
            assert!(message.contains("thr_wrong"), "{message}");
            assert!(message.contains("thr_expected"), "{message}");
        }
    }

    #[test]
    fn deferred_turn_is_not_forwarded_when_turn_start_fails() {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_existing\",\"turns\":[]}}}\n",
            "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_existing\",\"turn\":{\"id\":\"turn_auto\",\"items\":[],\"status\":\"inProgress\"}}}\n",
            "{\"id\":3,\"error\":{\"code\":-32600,\"message\":\"thread already has an active turn\"}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();

        let result = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "继续对话".into(),
                cwd: PathBuf::from("/tmp/project"),
                thread_id: Some("thr_existing".into()),
                model: "gpt-test".into(),
                effort: "medium".into(),
                service_tier: None,
                permission_mode: AgentPermissionMode::Full,
            },
            &tx,
        );
        let terminal = finish_prompt_session(&session, result);

        let AgentEvent::Failed(message) = terminal else {
            panic!("expected active-turn failure");
        };
        assert!(message.contains("turn/start 失败"));
        assert!(message.contains("thread already has an active turn"));
        assert!(rx.try_recv().is_err());
        assert!(session.writer.lock().unwrap().is_none());
    }

    #[test]
    fn deferred_batch_is_atomic_when_a_later_notification_mismatches() {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_existing\",\"turns\":[]}}}\n",
            "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_existing\",\"turn\":{\"id\":\"turn_user\",\"items\":[],\"status\":\"inProgress\"}}}\n",
            "{\"method\":\"error\",\"params\":{\"threadId\":\"thr_existing\",\"turnId\":\"turn_auto\",\"error\":{\"message\":\"old turn\",\"additionalDetails\":null},\"willRetry\":false}}\n",
            "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_user\"}}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();

        let result = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "继续对话".into(),
                cwd: PathBuf::from("/tmp/project"),
                thread_id: Some("thr_existing".into()),
                model: "gpt-test".into(),
                effort: "medium".into(),
                service_tier: None,
                permission_mode: AgentPermissionMode::Full,
            },
            &tx,
        );
        let terminal = finish_prompt_session(&session, result);

        let AgentEvent::Failed(message) = terminal else {
            panic!("expected the deferred batch to fail atomically");
        };
        assert!(message.contains("属于其他 turn"));
        assert!(message.contains("turn_auto"));
        assert!(message.contains("turn_user"));
        assert!(rx.try_recv().is_err());
        assert!(session.writer.lock().unwrap().is_none());
    }

    #[test]
    fn active_goal_approval_is_discarded_when_turn_start_fails() {
        let approval = command_approval_request_for(json!(77), "thr_existing", "turn_auto");
        let input = format!(
            "{{\"id\":1,\"result\":{{}}}}\n\
             {{\"id\":2,\"result\":{{\"thread\":{{\"id\":\"thr_existing\"}}}}}}\n\
             {approval}\n\
             {{\"id\":3,\"error\":{{\"code\":-32600,\"message\":\"thread already has an active turn\"}}}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();

        let result = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "继续对话".into(),
                cwd: PathBuf::from("/tmp/project"),
                thread_id: Some("thr_existing".into()),
                model: "gpt-test".into(),
                effort: "medium".into(),
                service_tier: None,
                permission_mode: AgentPermissionMode::Full,
            },
            &tx,
        );
        let sent: Vec<Value> = String::from_utf8(take_session_output(&session))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let terminal = finish_prompt_session(&session, result);

        let AgentEvent::Failed(message) = terminal else {
            panic!("expected turn/start failure");
        };
        assert!(message.contains("thread already has an active turn"));
        assert!(rx.try_recv().is_err());
        assert!(session.pending_approval_snapshot().is_empty());
        assert!(
            !sent
                .iter()
                .any(|message| message.get("id") == Some(&json!(77)))
        );
    }

    #[test]
    fn deferred_started_approval_and_resolution_keep_wire_order() {
        let approval = command_approval_request_for(json!(77), "thr_existing", "turn_user");
        let input = format!(
            "{{\"id\":1,\"result\":{{}}}}\n\
             {{\"id\":2,\"result\":{{\"thread\":{{\"id\":\"thr_existing\"}}}}}}\n\
             {{\"method\":\"turn/started\",\"params\":{{\"threadId\":\"thr_existing\",\"turn\":{{\"id\":\"turn_user\",\"items\":[],\"status\":\"inProgress\"}}}}}}\n\
             {approval}\n\
             {{\"method\":\"serverRequest/resolved\",\"params\":{{\"threadId\":\"thr_existing\",\"requestId\":77}}}}\n\
             {{\"id\":3,\"result\":{{\"turn\":{{\"id\":\"turn_user\"}}}}}}\n\
             {{\"method\":\"turn/completed\",\"params\":{{\"threadId\":\"thr_existing\",\"turn\":{{\"id\":\"turn_user\",\"status\":\"completed\"}}}}}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();

        let outcome = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "继续对话".into(),
                cwd: PathBuf::from("/tmp/project"),
                thread_id: Some("thr_existing".into()),
                model: "gpt-test".into(),
                effort: "medium".into(),
                service_tier: None,
                permission_mode: AgentPermissionMode::Full,
            },
            &tx,
        )
        .unwrap();
        tx.send_blocking(outcome.into_event()).unwrap();
        drop(tx);

        assert_eq!(rx.try_recv().unwrap(), AgentEvent::Started);
        let AgentEvent::CommandApprovalRequested { request, .. } = rx.try_recv().unwrap() else {
            panic!("expected deferred approval after Started");
        };
        assert_eq!(request.request_id, AgentServerRequestId::Number(77));
        assert_eq!(
            rx.try_recv().unwrap(),
            AgentEvent::ServerRequestResolved {
                request: AgentServerRequestMetadata {
                    request_id: AgentServerRequestId::Number(77),
                    thread_id: "thr_existing".into(),
                    turn_id: "turn_user".into(),
                    item_id: "item_1".into(),
                    kind: AgentServerRequestKind::CommandApproval,
                }
            }
        );
        assert_eq!(rx.try_recv().unwrap(), AgentEvent::Completed);
        assert!(rx.try_recv().is_err());
        assert!(session.pending_approval_snapshot().is_empty());
    }

    #[test]
    fn live_item_event_must_match_the_active_turn() {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_existing\"}}}\n",
            "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_user\"}}}\n",
            "{\"method\":\"item/agentMessage/delta\",\"params\":{\"threadId\":\"thr_existing\",\"turnId\":\"turn_old\",\"itemId\":\"msg_old\",\"delta\":\"旧内容\"}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();

        let result = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "继续对话".into(),
                cwd: PathBuf::from("/tmp/project"),
                thread_id: Some("thr_existing".into()),
                model: "gpt-test".into(),
                effort: "medium".into(),
                service_tier: None,
                permission_mode: AgentPermissionMode::Full,
            },
            &tx,
        );
        let terminal = finish_prompt_session(&session, result);

        let AgentEvent::Failed(message) = terminal else {
            panic!("expected mismatched item event to fail");
        };
        assert!(message.contains("item/agentMessage/delta"));
        assert!(message.contains("turn_old"));
        assert!(message.contains("turn_user"));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn completed_turn_can_finish_before_turn_start_response() {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_fast\"}}}\n",
            "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_fast\",\"turn\":{\"id\":\"turn_fast\",\"items\":[],\"status\":\"inProgress\"}}}\n",
            "{\"method\":\"item/agentMessage/delta\",\"params\":{\"threadId\":\"thr_fast\",\"turnId\":\"turn_fast\",\"itemId\":\"msg_fast\",\"delta\":\"完成\"}}\n",
            "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thr_fast\",\"turn\":{\"id\":\"turn_fast\",\"status\":\"completed\"}}}\n",
            "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_fast\"}}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();

        let outcome = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "快速完成".into(),
                cwd: PathBuf::from("/tmp/project"),
                thread_id: None,
                model: "gpt-test".into(),
                effort: "medium".into(),
                service_tier: None,
                permission_mode: AgentPermissionMode::Full,
            },
            &tx,
        )
        .unwrap();
        assert_eq!(outcome, TurnOutcome::Completed);
        tx.send_blocking(outcome.into_event()).unwrap();
        drop(tx);

        assert_eq!(
            std::iter::from_fn(|| rx.try_recv().ok()).collect::<Vec<_>>(),
            vec![
                AgentEvent::ThreadCreated {
                    thread_id: "thr_fast".into()
                },
                AgentEvent::Started,
                AgentEvent::TextDelta("完成".into()),
                AgentEvent::Completed,
            ]
        );
    }

    #[test]
    fn resume_rpc_error_fails_closed_without_starting_or_turning() {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"id\":2,\"error\":{\"code\":-32600,\"message\":\"no rollout found for thread id thr_missing\"}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        let result = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "继续对话".into(),
                cwd: PathBuf::from("/tmp/project"),
                thread_id: Some("thr_missing".into()),
                model: "gpt-test".into(),
                effort: "medium".into(),
                service_tier: None,
                permission_mode: AgentPermissionMode::Full,
            },
            &tx,
        );
        let sent_messages: Vec<Value> = String::from_utf8(take_session_output(&session))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let sent_methods: Vec<_> = sent_messages
            .iter()
            .filter_map(|message| message.get("method").and_then(Value::as_str))
            .collect();
        assert_eq!(
            sent_methods,
            vec!["initialize", "initialized", "thread/resume"]
        );

        let terminal = finish_prompt_session(&session, result);
        let AgentEvent::Failed(message) = terminal else {
            panic!("expected resume failure");
        };
        assert!(message.contains("thread/resume `thr_missing` 失败"));
        assert!(message.contains("no rollout found for thread id thr_missing"));
        assert!(session.writer.lock().unwrap().is_none());
        assert_eq!(session.snapshot(), (None, None, false, false, true));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn resume_response_requires_the_requested_thread_id() {
        for (resume_response, expected_error) in [
            (
                json!({"id":2,"result":{"thread":{}}}),
                "thread/resume 响应缺少字符串 result.thread.id",
            ),
            (
                json!({"id":2,"result":{"thread":{"id":"thr_other"}}}),
                "thread/resume 响应的 thread id `thr_other` 与请求的 `thr_existing` 不一致",
            ),
        ] {
            let input = format!("{}\n{}\n", json!({"id":1,"result":{}}), resume_response);
            let mut reader = Cursor::new(input.into_bytes());
            let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
            let (tx, rx) = async_channel::unbounded();
            let result = drive_session(
                &mut reader,
                &session,
                &AgentRequest {
                    prompt: "继续对话".into(),
                    cwd: PathBuf::from("/tmp/project"),
                    thread_id: Some("thr_existing".into()),
                    model: "gpt-test".into(),
                    effort: "medium".into(),
                    service_tier: None,
                    permission_mode: AgentPermissionMode::Full,
                },
                &tx,
            );
            let sent_messages: Vec<Value> = String::from_utf8(take_session_output(&session))
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            assert_eq!(
                sent_messages
                    .iter()
                    .filter_map(|message| message.get("method").and_then(Value::as_str))
                    .collect::<Vec<_>>(),
                vec!["initialize", "initialized", "thread/resume"]
            );

            let terminal = finish_prompt_session(&session, result);
            let AgentEvent::Failed(message) = terminal else {
                panic!("expected malformed resume response to fail");
            };
            assert!(
                message.contains(expected_error),
                "unexpected error: {message}"
            );
            assert!(session.writer.lock().unwrap().is_none());
            assert!(rx.try_recv().is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn resume_failure_cleanup_reaps_the_app_server_process() {
        let process = Arc::new(AppServerProcess::new(
            Command::new("sleep").arg("30").spawn().unwrap(),
        ));
        let session = Arc::new(CodexTurnSession::new(Vec::new(), Some(process.clone())));
        let mut reader = Cursor::new(
            concat!(
                "{\"id\":1,\"result\":{}}\n",
                "{\"id\":2,\"error\":{\"code\":-32600,\"message\":\"resume failed\"}}\n"
            )
            .as_bytes(),
        );
        let (tx, _rx) = async_channel::unbounded();

        let result = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "继续对话".into(),
                cwd: PathBuf::from("/tmp/project"),
                thread_id: Some("thr_existing".into()),
                model: "gpt-test".into(),
                effort: "medium".into(),
                service_tier: None,
                permission_mode: AgentPermissionMode::Full,
            },
            &tx,
        );
        let terminal = finish_prompt_session(&session, result);

        let AgentEvent::Failed(message) = terminal else {
            panic!("expected resume failure");
        };
        assert!(message.contains("resume failed"));
        assert!(process.is_reaped());
        assert!(process.child.lock().unwrap().is_none());
        assert!(session.writer.lock().unwrap().is_none());
    }

    #[test]
    fn pending_interrupt_uses_the_active_thread_and_turn_and_waits_for_terminal_status() {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_interrupt\"}}}\n",
            "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_interrupt\",\"turn\":{\"id\":\"turn_interrupt\",\"items\":[],\"status\":\"inProgress\"}}}\n",
            "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_interrupt\"}}}\n",
            "{\"id\":4,\"result\":{}}\n",
            "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thr_interrupt\",\"turn\":{\"id\":\"turn_interrupt\",\"status\":\"interrupted\"}}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();

        assert_eq!(
            session.request_interrupt_inner().unwrap(),
            AgentInterruptOutcome::Requested
        );
        assert_eq!(session.snapshot(), (None, None, true, false, false));

        let outcome = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "interrupt me".into(),
                cwd: PathBuf::from("/tmp/project"),
                thread_id: None,
                model: "gpt-test".into(),
                effort: "medium".into(),
                service_tier: None,
                permission_mode: AgentPermissionMode::Full,
            },
            &tx,
        )
        .unwrap();
        assert_eq!(outcome, TurnOutcome::Interrupted);
        assert_eq!(
            session.snapshot(),
            (
                Some("thr_interrupt".into()),
                Some("turn_interrupt".into()),
                true,
                true,
                true,
            )
        );

        tx.send_blocking(outcome.into_event()).unwrap();
        drop(tx);
        assert_eq!(
            rx.try_recv().unwrap(),
            AgentEvent::ThreadCreated {
                thread_id: "thr_interrupt".into()
            }
        );
        assert_eq!(rx.try_recv().unwrap(), AgentEvent::Started);
        assert_eq!(rx.try_recv().unwrap(), AgentEvent::Interrupted);
        assert!(rx.try_recv().is_err());

        let sent: Vec<Value> = String::from_utf8(take_session_output(&session))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let interrupts: Vec<_> = sent
            .iter()
            .filter(|message| {
                message.get("method").and_then(Value::as_str) == Some("turn/interrupt")
            })
            .collect();
        assert_eq!(interrupts.len(), 1);
        assert_eq!(interrupts[0].get("id").and_then(Value::as_u64), Some(4));
        assert_eq!(
            interrupts[0]
                .pointer("/params/threadId")
                .and_then(Value::as_str),
            Some("thr_interrupt")
        );
        assert_eq!(
            interrupts[0]
                .pointer("/params/turnId")
                .and_then(Value::as_str),
            Some("turn_interrupt")
        );
    }

    #[test]
    fn duplicate_and_finished_interrupts_do_not_write_again() {
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        session
            .activate_turn("thr_1".into(), "turn_1".into())
            .unwrap();

        assert_eq!(
            session.request_interrupt_inner().unwrap(),
            AgentInterruptOutcome::Requested
        );
        assert_eq!(
            session.request_interrupt_inner().unwrap(),
            AgentInterruptOutcome::AlreadyRequested
        );
        session.mark_terminal();
        assert_eq!(
            session.request_interrupt_inner().unwrap(),
            AgentInterruptOutcome::AlreadyFinished
        );

        let sent = String::from_utf8(take_session_output(&session)).unwrap();
        assert_eq!(sent.matches("\"method\":\"turn/interrupt\"").count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn abandoned_session_kills_and_reaps_its_child_process() {
        let process = Arc::new(AppServerProcess::new(
            Command::new("sleep").arg("30").spawn().unwrap(),
        ));
        let session = Arc::new(CodexTurnSession::new(Vec::new(), Some(process.clone())));
        let control: Arc<dyn AgentInterruptControl> = session.clone();
        let handle = AgentInterruptHandle::new(control);

        drop(handle);
        session.finish().unwrap();

        assert!(process.is_reaped());
        assert!(process.child.lock().unwrap().is_none());
        assert!(session.writer.lock().unwrap().is_none());
    }

    #[test]
    fn model_catalog_accumulates_pages_and_maps_defaults_and_options() {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"method\":\"remoteControl/status/changed\",\"params\":{\"status\":\"disabled\",\"serverName\":\"test-host\",\"installationId\":\"install-1\",\"environmentId\":null}}\n",
            "{\"method\":\"mcpServer/startupStatus/updated\",\"params\":{\"threadId\":null,\"name\":\"codex_apps\",\"status\":\"ready\",\"error\":null,\"failureReason\":null}}\n",
            "{\"id\":2,\"result\":{\"data\":[",
            "{\"id\":\"hidden\",\"model\":\"hidden\",\"displayName\":\"Hidden\",\"description\":\"hidden\",\"hidden\":true,\"supportedReasoningEfforts\":[{\"reasoningEffort\":\"low\",\"description\":\"Low\"}],\"defaultReasoningEffort\":\"low\",\"isDefault\":false},",
            "{\"id\":\"model-a\",\"model\":\"model-a-wire\",\"displayName\":\"Model A\",\"description\":\"First page\",\"hidden\":false,\"supportedReasoningEfforts\":[{\"reasoningEffort\":\"low\",\"description\":\"Low\"}],\"defaultReasoningEffort\":\"low\",\"serviceTiers\":[],\"defaultServiceTier\":null,\"isDefault\":false}],\"nextCursor\":\"page-2\"}}\n",
            "{\"id\":3,\"result\":{\"data\":[{\"id\":\"model-b\",\"model\":\"model-b-wire\",\"displayName\":\"Model B\",\"description\":\"Second page\",\"hidden\":false,\"supportedReasoningEfforts\":[{\"reasoningEffort\":\"medium\",\"description\":\"Balanced\"},{\"reasoningEffort\":\"high\",\"description\":\"Deep\"}],\"defaultReasoningEffort\":\"medium\",\"serviceTiers\":[{\"id\":\"priority\",\"name\":\"Fast\",\"description\":\"Lower latency\"}],\"defaultServiceTier\":\"priority\",\"isDefault\":true}],\"nextCursor\":null}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let mut output = Vec::new();

        let catalog = drive_model_catalog(&mut reader, &mut output).unwrap();
        assert_eq!(catalog.models.len(), 2);
        assert_eq!(catalog.models[0].id, "model-a");
        assert_eq!(catalog.models[0].model, "model-a-wire");
        assert_eq!(catalog.models[1].display_name, "Model B");
        assert!(catalog.models[1].is_default);
        assert_eq!(catalog.models[1].default_reasoning_effort, "medium");
        assert_eq!(catalog.models[1].service_tiers[0].id, "priority");
        assert_eq!(
            catalog.models[1].default_service_tier.as_deref(),
            Some("priority")
        );

        let sent = String::from_utf8(output).unwrap();
        let requests: Vec<serde_json::Value> = sent
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .filter(|message: &serde_json::Value| {
                message.get("method").and_then(|value| value.as_str()) == Some("model/list")
            })
            .collect();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].pointer("/params/cursor"), Some(&json!(null)));
        assert_eq!(
            requests[0]
                .pointer("/params/limit")
                .and_then(|value| value.as_u64()),
            Some(u64::from(MODEL_LIST_PAGE_SIZE))
        );
        assert_eq!(
            requests[1]
                .pointer("/params/cursor")
                .and_then(|value| value.as_str()),
            Some("page-2")
        );
    }

    #[test]
    fn thread_started_is_a_validated_lifecycle_notification() {
        ensure_server_method_is_defined(&json!({
            "method": "thread/started",
            "params": {
                "thread": {
                    "id": "thr_1",
                    "sessionId": "thr_1",
                    "ephemeral": false,
                    "turns": []
                }
            }
        }))
        .unwrap();

        for thread in [json!({}), json!({"id": null}), json!({"id": 7})] {
            let error = ensure_server_method_is_defined(&json!({
                "method": "thread/started",
                "params": {"thread": thread}
            }))
            .unwrap_err()
            .to_string();
            assert!(error.contains("thread/started"), "{error}");
            assert!(error.contains("params.thread.id"), "{error}");
        }
    }

    #[test]
    fn remote_control_status_changed_is_a_validated_connection_notification() {
        ensure_server_method_is_defined(&json!({
            "method": "remoteControl/status/changed",
            "params": {
                "status": "connected",
                "serverName": "test-host",
                "installationId": "install-1",
                "environmentId": "environment-1"
            }
        }))
        .unwrap();

        for params in [
            json!({
                "status": "future-status",
                "serverName": "test-host",
                "installationId": "install-1",
                "environmentId": null
            }),
            json!({
                "status": "disabled",
                "serverName": "test-host",
                "installationId": "install-1"
            }),
        ] {
            let error = ensure_server_method_is_defined(&json!({
                "method": "remoteControl/status/changed",
                "params": params
            }))
            .unwrap_err()
            .to_string();
            assert!(error.contains("remoteControl/status/changed"), "{error}");
        }
    }

    #[test]
    fn mcp_server_startup_status_is_validated_and_normalized() {
        let starting = json!({
            "method": "mcpServer/startupStatus/updated",
            "params": {
                "threadId": "thr_1",
                "name": "codex_apps",
                "status": "starting",
                "error": null,
                "failureReason": null
            }
        });
        ensure_server_method_is_defined(&starting).unwrap();
        assert_eq!(
            parse_agent_notification(&starting).unwrap(),
            Some(AgentEvent::McpServerStartupStatusUpdated(
                AgentMcpServerStartupStatus {
                    thread_id: Some("thr_1".into()),
                    name: "codex_apps".into(),
                    state: AgentMcpServerStartupState::Starting,
                    error: None,
                    failure_reason: None,
                }
            ))
        );

        for (status, state) in [
            ("ready", AgentMcpServerStartupState::Ready),
            ("cancelled", AgentMcpServerStartupState::Cancelled),
        ] {
            assert_eq!(
                parse_agent_notification(&json!({
                    "method": "mcpServer/startupStatus/updated",
                    "params": {"name": "codex_apps", "status": status}
                }))
                .unwrap(),
                Some(AgentEvent::McpServerStartupStatusUpdated(
                    AgentMcpServerStartupStatus {
                        thread_id: None,
                        name: "codex_apps".into(),
                        state,
                        error: None,
                        failure_reason: None,
                    }
                ))
            );
        }

        let failed = json!({
            "method": "mcpServer/startupStatus/updated",
            "params": {
                "threadId": null,
                "name": "remote_tools",
                "status": "failed",
                "error": "OAuth token expired",
                "failureReason": "reauthenticationRequired"
            }
        });
        assert_eq!(
            parse_agent_notification(&failed).unwrap(),
            Some(AgentEvent::McpServerStartupStatusUpdated(
                AgentMcpServerStartupStatus {
                    thread_id: None,
                    name: "remote_tools".into(),
                    state: AgentMcpServerStartupState::Failed,
                    error: Some("OAuth token expired".into()),
                    failure_reason: Some(
                        AgentMcpServerStartupFailureReason::ReauthenticationRequired
                    ),
                }
            ))
        );

        for params in [
            json!({
                "threadId": "thr_1",
                "name": "codex_apps",
                "status": "future-status",
                "error": null,
                "failureReason": null
            }),
            json!({
                "threadId": "thr_1",
                "name": "codex_apps",
                "status": "failed",
                "error": null,
                "failureReason": "future-reason"
            }),
            json!({
                "threadId": 7,
                "name": "codex_apps",
                "status": "ready",
                "error": null,
                "failureReason": null
            }),
        ] {
            let error = ensure_server_method_is_defined(&json!({
                "method": "mcpServer/startupStatus/updated",
                "params": params
            }))
            .unwrap_err()
            .to_string();
            assert!(error.contains("mcpServer/startupStatus/updated"), "{error}");
        }
    }

    #[test]
    fn thread_status_changed_is_validated_and_normalized() {
        let active = json!({
            "method": "thread/status/changed",
            "params": {
                "threadId": "thr_1",
                "status": {
                    "type": "active",
                    "activeFlags": ["waitingOnApproval", "waitingOnUserInput"]
                }
            }
        });
        ensure_server_method_is_defined(&active).unwrap();
        assert_eq!(
            parse_agent_notification(&active).unwrap(),
            Some(AgentEvent::ThreadStatusChanged(AgentThreadStatus {
                thread_id: "thr_1".into(),
                state: AgentThreadStatusState::Active {
                    active_flags: vec![
                        AgentThreadActiveFlag::WaitingOnApproval,
                        AgentThreadActiveFlag::WaitingOnUserInput,
                    ],
                },
            }))
        );

        for (raw_state, state) in [
            ("notLoaded", AgentThreadStatusState::NotLoaded),
            ("idle", AgentThreadStatusState::Idle),
            ("systemError", AgentThreadStatusState::SystemError),
        ] {
            assert_eq!(
                parse_agent_notification(&json!({
                    "method": "thread/status/changed",
                    "params": {
                        "threadId": "thr_1",
                        "status": {"type": raw_state}
                    }
                }))
                .unwrap(),
                Some(AgentEvent::ThreadStatusChanged(AgentThreadStatus {
                    thread_id: "thr_1".into(),
                    state,
                }))
            );
        }

        for params in [
            json!({"threadId": 7, "status": {"type": "idle"}}),
            json!({"threadId": "thr_1", "status": null}),
            json!({"threadId": "thr_1", "status": {}}),
            json!({"threadId": "thr_1", "status": {"type": "future-status"}}),
            json!({"threadId": "thr_1", "status": {"type": "active"}}),
            json!({
                "threadId": "thr_1",
                "status": {"type": "active", "activeFlags": "waitingOnApproval"}
            }),
            json!({
                "threadId": "thr_1",
                "status": {"type": "active", "activeFlags": ["future-flag"]}
            }),
            json!({
                "threadId": "thr_1",
                "status": {"type": "active", "activeFlags": [7]}
            }),
        ] {
            let error = ensure_server_method_is_defined(&json!({
                "method": "thread/status/changed",
                "params": params
            }))
            .unwrap_err()
            .to_string();
            assert!(error.contains("thread/status/changed"), "{error}");
        }
    }

    #[test]
    fn unsupported_formerly_passive_methods_are_all_undefined() {
        for method in [
            "thread/goal/updated",
            "thread/goal/cleared",
            "turn/plan/updated",
            "thread/tokenUsage/updated",
            "account/rateLimits/updated",
        ] {
            let error = ensure_server_method_is_defined(&json!({
                "method": method,
                "params": { "probe": true }
            }))
            .unwrap_err()
            .to_string();
            assert!(error.contains("未定义"), "{method}: {error}");
            assert!(error.contains(method), "{method}: {error}");
        }
    }

    #[test]
    fn user_facing_methods_are_defined() {
        for method in [
            "turn/started",
            "error",
            "turn/completed",
            "thread/settings/updated",
            "warning",
            "configWarning",
        ] {
            ensure_server_method_is_defined(&json!({
                "method": method,
                "params": {}
            }))
            .unwrap();
        }
    }

    #[test]
    fn user_facing_notifications_are_normalized_without_ending_the_turn() {
        let input = concat!(
            "{\"method\":\"configWarning\",\"params\":{\"summary\":\"配置值已弃用\",\"details\":\"请迁移到新键\",\"path\":\"/tmp/project/config.toml\",\"range\":{\"start\":{\"line\":8,\"column\":4},\"end\":{\"line\":8,\"column\":12}}}}\n",
            "{\"id\":1,\"result\":{}}\n",
            "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_notices\"}}}\n",
            "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_notices\",\"turn\":{\"id\":\"turn_notices\",\"items\":[],\"status\":\"inProgress\"}}}\n",
            "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_notices\"}}}\n",
            "{\"method\":\"thread/settings/updated\",\"params\":{\"threadId\":\"thr_notices\",\"threadSettings\":{\"model\":\"model-b\",\"effort\":\"high\",\"serviceTier\":\"priority\",\"cwd\":\"/tmp/project/updated\"}}}\n",
            "{\"method\":\"warning\",\"params\":{\"threadId\":null,\"message\":\"上下文窗口即将用尽\"}}\n",
            "{\"method\":\"error\",\"params\":{\"threadId\":\"thr_notices\",\"turnId\":\"turn_notices\",\"error\":{\"message\":\"连接暂时中断\",\"additionalDetails\":\"2 秒后重试\"},\"willRetry\":true}}\n",
            "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thr_notices\",\"turn\":{\"id\":\"turn_notices\",\"status\":\"completed\"}}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();

        let outcome = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "probe notices".into(),
                cwd: PathBuf::from("/tmp/project"),
                thread_id: None,
                model: "model-a".into(),
                effort: "medium".into(),
                service_tier: None,
                permission_mode: AgentPermissionMode::Full,
            },
            &tx,
        )
        .unwrap();
        assert_eq!(outcome, TurnOutcome::Completed);
        tx.send_blocking(outcome.into_event()).unwrap();
        drop(tx);

        let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert_eq!(
            events,
            vec![
                AgentEvent::ConfigWarning(AgentConfigWarning {
                    summary: "配置值已弃用".into(),
                    details: Some("请迁移到新键".into()),
                    path: Some("/tmp/project/config.toml".into()),
                    line: Some(8),
                    column: Some(4),
                }),
                AgentEvent::ThreadCreated {
                    thread_id: "thr_notices".into()
                },
                AgentEvent::Started,
                AgentEvent::ThreadSettingsUpdated(AgentThreadSettings {
                    model: "model-b".into(),
                    effort: Some("high".into()),
                    service_tier: Some("priority".into()),
                    cwd: "/tmp/project/updated".into(),
                    permissions: None,
                }),
                AgentEvent::Warning {
                    message: "上下文窗口即将用尽".into(),
                },
                AgentEvent::Error {
                    message: "连接暂时中断".into(),
                    details: Some("2 秒后重试".into()),
                    will_retry: true,
                },
                AgentEvent::Completed,
            ]
        );
    }

    #[test]
    fn failed_turn_completion_is_the_terminal_event_and_keeps_error_details() {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_failed\"}}}\n",
            "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_failed\"}}}\n",
            "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_failed\",\"turn\":{\"id\":\"turn_failed\",\"items\":[],\"status\":\"inProgress\"}}}\n",
            "{\"method\":\"error\",\"params\":{\"threadId\":\"thr_failed\",\"turnId\":\"turn_failed\",\"error\":{\"message\":\"模型请求失败\",\"additionalDetails\":\"上游返回 503\"},\"willRetry\":false}}\n",
            "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thr_failed\",\"turn\":{\"id\":\"turn_failed\",\"status\":\"failed\",\"error\":{\"message\":\"模型请求失败\",\"additionalDetails\":\"上游返回 503\"}}}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();

        let outcome = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "fail".into(),
                cwd: PathBuf::from("/tmp/project"),
                thread_id: None,
                model: "model-a".into(),
                effort: "medium".into(),
                service_tier: None,
                permission_mode: AgentPermissionMode::Full,
            },
            &tx,
        )
        .unwrap();
        assert_eq!(
            outcome,
            TurnOutcome::Failed("模型请求失败\n上游返回 503".into())
        );
        tx.send_blocking(outcome.into_event()).unwrap();
        drop(tx);

        let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert_eq!(
            events,
            vec![
                AgentEvent::ThreadCreated {
                    thread_id: "thr_failed".into()
                },
                AgentEvent::Started,
                AgentEvent::Error {
                    message: "模型请求失败".into(),
                    details: Some("上游返回 503".into()),
                    will_retry: false,
                },
                AgentEvent::Failed("模型请求失败\n上游返回 503".into()),
            ]
        );
        assert!(session.snapshot().4);
    }

    #[test]
    fn non_turn_connection_does_not_silently_drop_visible_notifications() {
        let mut reader =
            Cursor::new(b"{\"method\":\"warning\",\"params\":{\"message\":\"visible warning\"}}\n");
        let mut output = Vec::new();

        let error = wait_for_response(&mut reader, &mut output, INITIALIZE_ID, None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("需要可见 UI 承接"));
        assert!(error.contains("warning"));
    }

    #[test]
    fn unknown_method_error_contains_its_kind_name_and_params() {
        let error = ensure_server_method_is_defined(&json!({
            "method": "item/futureTool/progress",
            "params": {
                "itemId": "future_1",
                "progress": 0.5
            }
        }))
        .unwrap_err()
        .to_string();

        assert!(error.contains("未定义"));
        assert!(error.contains("通知"));
        assert!(error.contains("item/futureTool/progress"));
        assert!(error.contains("future_1"));
        assert!(error.contains("progress"));
    }

    #[test]
    fn unknown_method_payload_is_truncated_on_a_utf8_boundary() {
        let error = ensure_server_method_is_defined(&json!({
            "method": "item/future/hugeDelta",
            "params": { "delta": "中".repeat(UNDEFINED_METHOD_PARAMS_LIMIT + 500) }
        }))
        .unwrap_err()
        .to_string();

        assert!(error.contains("item/future/hugeDelta"));
        assert!(error.ends_with('…'));
        assert!(error.chars().count() < UNDEFINED_METHOD_PARAMS_LIMIT + 100);
    }

    #[test]
    fn handshake_wait_rejects_unknown_methods_instead_of_skipping_them() {
        let mut reader = Cursor::new(
            b"{\"method\":\"protocol/futureHandshake\",\"params\":{\"phase\":\"initialize\"}}\n",
        );
        let mut output = Vec::new();

        let error = wait_for_response(&mut reader, &mut output, INITIALIZE_ID, None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("protocol/futureHandshake"));
        assert!(error.contains("initialize"));
    }

    #[test]
    fn unknown_server_request_is_replied_to_and_reported_locally() {
        let mut reader = Cursor::new(
            b"{\"id\":99,\"method\":\"item/futureApproval/request\",\"params\":{\"reason\":\"probe\"}}\n",
        );
        let mut output = Vec::new();

        let error = wait_for_response(&mut reader, &mut output, INITIALIZE_ID, None)
            .unwrap_err()
            .to_string();
        let response = String::from_utf8(output).unwrap();

        assert!(error.contains("请求"));
        assert!(error.contains("item/futureApproval/request"));
        assert!(response.contains("\"id\":99"));
        assert!(response.contains("\"code\":-32601"));
    }

    #[test]
    fn command_approval_enters_live_ui_and_replies_exactly_once() {
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        let message = command_approval_request(json!(77));

        respond_to_server_request_on_session(&session, &message, &tx).unwrap();
        assert!(take_session_output(&session).is_empty());
        let pending = session.pending_approval_snapshot();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0, AgentServerRequestId::Number(77));
        assert_eq!(pending[0].1, message["params"]);
        assert!(!pending[0].2);

        let event = rx.try_recv().unwrap();
        let AgentEvent::CommandApprovalRequested { request, responder } = event else {
            panic!("expected command approval event");
        };
        assert_eq!(request.request_id, AgentServerRequestId::Number(77));
        assert_eq!(request.command, "git --version");
        assert!(request.allow_once);
        assert!(request.decline);
        assert!(!request.cancel);
        assert!(request.can_accept_with_execpolicy_amendment);

        responder
            .respond(AgentCommandApprovalChoice::Accept)
            .unwrap();
        let response: Value = serde_json::from_slice(&take_session_output(&session)).unwrap();
        assert_eq!(response, json!({"id":77,"result":{"decision":"accept"}}));
        let duplicate = responder
            .respond(AgentCommandApprovalChoice::Decline)
            .unwrap_err();
        assert!(duplicate.contains("拒绝重复 decision"));
        assert!(take_session_output(&session).is_empty());
    }

    #[test]
    fn current_cancel_advertisement_uses_chatgpt_style_decline() {
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        let message = current_command_approval_request(json!(78));

        respond_to_server_request_on_session(&session, &message, &tx).unwrap();
        let event = rx.try_recv().unwrap();
        let AgentEvent::CommandApprovalRequested { request, responder } = event else {
            panic!("expected command approval event");
        };
        assert!(request.allow_once);
        assert!(!request.decline);
        assert!(request.cancel);

        responder
            .respond(AgentCommandApprovalChoice::Decline)
            .unwrap();
        let response: Value = serde_json::from_slice(&take_session_output(&session)).unwrap();
        assert_eq!(response, json!({"id":78,"result":{"decision":"decline"}}));
    }

    #[test]
    fn command_approval_preserves_string_ids_and_raw_execpolicy_decision() {
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        respond_to_server_request_on_session(&session, &command_approval_request(json!("77")), &tx)
            .unwrap();
        respond_to_server_request_on_session(&session, &command_approval_request(json!(77)), &tx)
            .unwrap();
        assert_eq!(session.pending_approval_snapshot().len(), 2);

        let first = rx.try_recv().unwrap();
        let AgentEvent::CommandApprovalRequested { request, responder } = first else {
            panic!("expected command approval event");
        };
        assert_eq!(
            request.request_id,
            AgentServerRequestId::String("77".into())
        );
        responder
            .respond(AgentCommandApprovalChoice::AcceptWithExecpolicyAmendment)
            .unwrap();
        let response: Value = serde_json::from_slice(&take_session_output(&session)).unwrap();
        assert_eq!(response["id"], json!("77"));
        assert_eq!(
            response["result"]["decision"],
            json!({"acceptWithExecpolicyAmendment":{"execpolicy_amendment":["git","--version"]}})
        );

        let second = rx.try_recv().unwrap();
        let AgentEvent::CommandApprovalRequested { request, responder } = second else {
            panic!("expected second command approval event");
        };
        assert_eq!(request.request_id, AgentServerRequestId::Number(77));
        responder
            .respond(AgentCommandApprovalChoice::Decline)
            .unwrap();
        let response: Value = serde_json::from_slice(&take_session_output(&session)).unwrap();
        assert_eq!(response, json!({"id":77,"result":{"decision":"decline"}}));
    }

    #[test]
    fn user_input_request_preserves_all_questions_answers_and_original_id_once() {
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        let message = user_input_request(json!("user-input-7"));
        respond_to_server_request_on_session(&session, &message, &tx).unwrap();

        let AgentEvent::UserInputRequested { request, responder } = rx.try_recv().unwrap() else {
            panic!("expected user input event");
        };
        assert_eq!(
            request.request_id,
            AgentServerRequestId::String("user-input-7".into())
        );
        assert_eq!(request.thread_id, "thr_1");
        assert_eq!(request.turn_id, "turn_1");
        assert_eq!(request.item_id, "tool_1");
        assert!(request.is_blocking);
        assert_eq!(request.auto_resolution_ms, Some(1500));
        assert_eq!(request.questions.len(), 2);
        assert_eq!(request.questions[0].header, "Color");
        assert_eq!(request.questions[0].options.len(), 2);
        assert!(request.questions[0].allows_other);
        assert!(request.questions[1].is_secret);

        let response = AgentUserInputResponse {
            answers: vec![
                AgentUserInputAnswer {
                    question_id: "color".into(),
                    answers: vec!["red".into(), "blue".into(), "custom shade".into()],
                },
                AgentUserInputAnswer {
                    question_id: "token".into(),
                    answers: vec!["super-secret-value".into()],
                },
            ],
        };
        let debug = format!("{response:?}");
        assert!(!debug.contains("super-secret-value"));
        assert!(debug.contains("<redacted>"));
        responder.respond(response).unwrap();
        let wire: Value = serde_json::from_slice(&take_session_output(&session)).unwrap();
        assert_eq!(wire["id"], json!("user-input-7"));
        assert_eq!(
            wire["result"],
            json!({
                "answers": {
                    "color": {"answers": ["red", "blue", "custom shade"]},
                    "token": {"answers": ["super-secret-value"]}
                }
            })
        );

        let duplicate = responder
            .respond(AgentUserInputResponse::default())
            .unwrap_err();
        assert!(duplicate.contains("拒绝重复 answers"));
        assert!(!duplicate.contains("super-secret-value"));
        assert!(take_session_output(&session).is_empty());
    }

    #[test]
    fn user_input_invalid_params_receive_minus_32602_without_creating_pending_ui() {
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        let mut message = user_input_request(json!(81));
        message["params"]["questions"][0]["isSecret"] = json!("yes");

        let error = respond_to_server_request_on_session(&session, &message, &tx)
            .unwrap_err()
            .to_string();
        assert!(error.contains("isSecret"));
        assert!(rx.try_recv().is_err());
        assert!(session.pending_server_request_snapshot().is_empty());
        let wire: Value = serde_json::from_slice(&take_session_output(&session)).unwrap();
        assert_eq!(wire["id"], json!(81));
        assert_eq!(wire["error"]["code"], json!(-32602));
    }

    #[test]
    fn permissions_approval_preserves_structured_permissions_and_maps_all_scopes() {
        for (id, choice, expected_scope, expected_permissions) in [
            (
                91,
                AgentPermissionsApprovalChoice::AllowOnce,
                "turn",
                Some(permissions_approval_request(json!(91))["params"]["permissions"].clone()),
            ),
            (
                92,
                AgentPermissionsApprovalChoice::AllowForSession,
                "session",
                Some(permissions_approval_request(json!(92))["params"]["permissions"].clone()),
            ),
            (93, AgentPermissionsApprovalChoice::Decline, "turn", None),
        ] {
            let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
            let (tx, rx) = async_channel::unbounded();
            respond_to_server_request_on_session(
                &session,
                &permissions_approval_request(json!(id)),
                &tx,
            )
            .unwrap();
            let AgentEvent::PermissionsApprovalRequested { request, responder } =
                rx.try_recv().unwrap()
            else {
                panic!("expected permissions approval event");
            };
            assert_eq!(request.cwd, "/workspace/project");
            assert_eq!(
                request.reason.as_deref(),
                Some("Read fixtures and contact the network")
            );
            assert_eq!(request.environment_id.as_deref(), Some("env_1"));
            responder.respond(choice).unwrap();
            let wire: Value = serde_json::from_slice(&take_session_output(&session)).unwrap();
            assert_eq!(wire["id"], json!(id));
            assert_eq!(wire["result"]["scope"], json!(expected_scope));
            assert!(wire["result"].get("strictAutoReview").is_none());
            match expected_permissions {
                Some(permissions) => assert_eq!(wire["result"]["permissions"], permissions),
                None => assert_eq!(wire["result"]["permissions"], json!({})),
            }
            let duplicate = responder.respond(choice).unwrap_err();
            assert!(duplicate.contains("拒绝重复 decision"));
            assert!(take_session_output(&session).is_empty());
        }
    }

    #[test]
    fn permissions_file_network_and_mixed_profiles_are_all_parsed() {
        for (id, file_system, network) in [(94, true, false), (95, false, true), (96, true, true)] {
            let mut message = permissions_approval_request(json!(id));
            if !file_system {
                message["params"]["permissions"]["fileSystem"] = Value::Null;
            }
            if !network {
                message["params"]["permissions"]["network"] = Value::Null;
            }
            let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
            let (tx, rx) = async_channel::unbounded();
            respond_to_server_request_on_session(&session, &message, &tx).unwrap();
            let AgentEvent::PermissionsApprovalRequested { request, .. } = rx.try_recv().unwrap()
            else {
                panic!("expected permissions approval event");
            };
            assert_eq!(
                matches!(
                    request.permissions.file_system,
                    AgentOptionalField::Value(_)
                ),
                file_system
            );
            assert_eq!(
                matches!(request.permissions.network, AgentOptionalField::Value(_)),
                network
            );
        }
    }

    #[test]
    fn permissions_invalid_params_receive_minus_32602() {
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        let mut message = permissions_approval_request(json!(97));
        message["params"]["permissions"]["fileSystem"]["entries"][0]["path"]["type"] =
            json!("future_path");
        let error = respond_to_server_request_on_session(&session, &message, &tx)
            .unwrap_err()
            .to_string();
        assert!(error.contains("future_path"));
        assert!(rx.try_recv().is_err());
        let wire: Value = serde_json::from_slice(&take_session_output(&session)).unwrap();
        assert_eq!(wire["error"]["code"], json!(-32602));
    }

    #[test]
    fn unsupported_file_change_server_request_is_rejected() {
        for (id, method) in [(78_u64, "item/fileChange/requestApproval")] {
            let input = format!(
                "{}\n",
                json!({
                    "id": id,
                    "method": method,
                    "params": {
                        "threadId": "thr_1",
                        "turnId": "turn_1",
                        "itemId": "item_1"
                    }
                })
            );
            let mut reader = Cursor::new(input.into_bytes());
            let mut output = Vec::new();

            let error = wait_for_response(&mut reader, &mut output, INITIALIZE_ID, None)
                .unwrap_err()
                .to_string();
            let response = String::from_utf8(output).unwrap();

            assert!(error.contains(method));
            assert!(response.contains(&format!("\"id\":{id}")));
            assert!(response.contains("\"code\":-32601"));
        }
    }

    #[test]
    fn server_request_resolved_clears_only_the_matching_pending_request() {
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        for id in [json!(77), json!("77")] {
            respond_to_server_request_on_session(&session, &command_approval_request(id), &tx)
                .unwrap();
            rx.try_recv().unwrap();
        }

        let resolved = json!({
            "method":"serverRequest/resolved",
            "params":{"threadId":"thr_1","requestId":77}
        });
        handle_server_request_resolved(&session, &resolved, &tx).unwrap();
        assert_eq!(session.pending_approval_snapshot().len(), 1);
        assert_eq!(
            session.pending_approval_snapshot()[0].0,
            AgentServerRequestId::String("77".into())
        );
        assert_eq!(
            rx.try_recv().unwrap(),
            AgentEvent::ServerRequestResolved {
                request: AgentServerRequestMetadata {
                    request_id: AgentServerRequestId::Number(77),
                    thread_id: "thr_1".into(),
                    turn_id: "turn_1".into(),
                    item_id: "item_1".into(),
                    kind: AgentServerRequestKind::CommandApproval,
                }
            }
        );
        ensure_server_method_is_defined(&resolved).unwrap();
    }

    #[test]
    fn all_three_server_request_kinds_follow_response_then_resolved_and_duplicate_is_idempotent() {
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();

        respond_to_server_request_on_session(&session, &command_approval_request(json!(101)), &tx)
            .unwrap();
        let AgentEvent::CommandApprovalRequested {
            responder: command, ..
        } = rx.try_recv().unwrap()
        else {
            panic!("expected command approval");
        };
        command.respond(AgentCommandApprovalChoice::Accept).unwrap();

        respond_to_server_request_on_session(&session, &user_input_request(json!(102)), &tx)
            .unwrap();
        let AgentEvent::UserInputRequested {
            responder: user_input,
            ..
        } = rx.try_recv().unwrap()
        else {
            panic!("expected user input");
        };
        user_input
            .respond(AgentUserInputResponse {
                answers: vec![AgentUserInputAnswer {
                    question_id: "color".into(),
                    answers: vec!["red".into()],
                }],
            })
            .unwrap();

        respond_to_server_request_on_session(
            &session,
            &permissions_approval_request(json!(103)),
            &tx,
        )
        .unwrap();
        let AgentEvent::PermissionsApprovalRequested {
            responder: permissions,
            ..
        } = rx.try_recv().unwrap()
        else {
            panic!("expected permissions approval");
        };
        permissions
            .respond(AgentPermissionsApprovalChoice::AllowOnce)
            .unwrap();
        take_session_output(&session);

        assert_eq!(session.pending_server_request_snapshot().len(), 3);
        for (request_id, expected_kind) in [
            (101, AgentServerRequestKind::CommandApproval),
            (102, AgentServerRequestKind::UserInput),
            (103, AgentServerRequestKind::PermissionsApproval),
        ] {
            let resolved = json!({
                "method": "serverRequest/resolved",
                "params": {"threadId": "thr_1", "requestId": request_id}
            });
            handle_server_request_resolved(&session, &resolved, &tx).unwrap();
            let AgentEvent::ServerRequestResolved { request } = rx.try_recv().unwrap() else {
                panic!("expected resolved event");
            };
            assert_eq!(request.request_id, AgentServerRequestId::Number(request_id));
            assert_eq!(request.kind, expected_kind);
            assert_eq!(request.thread_id, "thr_1");
            assert_eq!(request.turn_id, "turn_1");

            handle_server_request_resolved(&session, &resolved, &tx).unwrap();
            assert!(
                rx.try_recv().is_err(),
                "duplicate resolved must be idempotent"
            );
        }
        assert!(session.pending_server_request_snapshot().is_empty());
    }

    #[test]
    fn resolved_rejects_wrong_thread_and_unknown_request_without_clearing_pending() {
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        respond_to_server_request_on_session(&session, &user_input_request(json!(111)), &tx)
            .unwrap();
        rx.try_recv().unwrap();

        let wrong_thread = json!({
            "method": "serverRequest/resolved",
            "params": {"threadId": "thr_wrong", "requestId": 111}
        });
        let error = handle_server_request_resolved(&session, &wrong_thread, &tx)
            .unwrap_err()
            .to_string();
        assert!(error.contains("不一致"));
        assert_eq!(session.pending_server_request_snapshot().len(), 1);

        let unknown = json!({
            "method": "serverRequest/resolved",
            "params": {"threadId": "thr_1", "requestId": 999}
        });
        let error = handle_server_request_resolved(&session, &unknown, &tx)
            .unwrap_err()
            .to_string();
        assert!(error.contains("未知 request"));
        assert_eq!(session.pending_server_request_snapshot().len(), 1);
    }

    #[test]
    fn mismatched_server_request_turn_returns_minus_32602_and_no_ui_event() {
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        let mut message = user_input_request(json!(112));
        message["params"]["turnId"] = json!("turn_wrong");
        let mut streamed_text = false;
        let error = super::process_turn_message(
            &session,
            &message,
            "thr_1",
            "turn_1",
            &tx,
            &mut streamed_text,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("属于其他 turn"));
        assert!(rx.try_recv().is_err());
        assert!(session.pending_server_request_snapshot().is_empty());
        let wire: Value = serde_json::from_slice(&take_session_output(&session)).unwrap();
        assert_eq!(wire["id"], json!(112));
        assert_eq!(wire["error"]["code"], json!(-32602));
    }

    #[test]
    fn turn_end_drains_all_pending_responders_and_reports_visible_terminal_states() {
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        respond_to_server_request_on_session(&session, &command_approval_request(json!(121)), &tx)
            .unwrap();
        let AgentEvent::CommandApprovalRequested {
            responder: command, ..
        } = rx.try_recv().unwrap()
        else {
            panic!("expected command approval");
        };
        respond_to_server_request_on_session(&session, &user_input_request(json!(122)), &tx)
            .unwrap();
        let AgentEvent::UserInputRequested {
            responder: user_input,
            ..
        } = rx.try_recv().unwrap()
        else {
            panic!("expected user input");
        };
        respond_to_server_request_on_session(
            &session,
            &permissions_approval_request(json!(123)),
            &tx,
        )
        .unwrap();
        let AgentEvent::PermissionsApprovalRequested {
            responder: permissions,
            ..
        } = rx.try_recv().unwrap()
        else {
            panic!("expected permissions approval");
        };

        cleanup_pending_server_requests(&session, &Ok(TurnOutcome::Interrupted), &tx).unwrap();
        let mut kinds = HashSet::new();
        for _ in 0..3 {
            let AgentEvent::ServerRequestFailed {
                request,
                kind,
                message,
            } = rx.try_recv().unwrap()
            else {
                panic!("expected pending cleanup event");
            };
            assert_eq!(kind, AgentServerRequestFailureKind::Cancelled);
            assert!(message.contains("已取消"));
            kinds.insert(request.kind);
        }
        assert_eq!(kinds.len(), 3);
        assert!(session.pending_server_request_snapshot().is_empty());
        assert!(
            command
                .respond(AgentCommandApprovalChoice::Decline)
                .is_err()
        );
        assert!(
            user_input
                .respond(AgentUserInputResponse::default())
                .is_err()
        );
        assert!(
            permissions
                .respond(AgentPermissionsApprovalChoice::Decline)
                .is_err()
        );
    }

    #[test]
    fn normal_completion_with_unresolved_request_is_a_protocol_consistency_error() {
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        respond_to_server_request_on_session(&session, &user_input_request(json!(131)), &tx)
            .unwrap();
        rx.try_recv().unwrap();
        let error = cleanup_pending_server_requests(&session, &Ok(TurnOutcome::Completed), &tx)
            .unwrap_err()
            .to_string();
        assert!(error.contains("未 resolved"));
        let AgentEvent::ServerRequestFailed { kind, .. } = rx.try_recv().unwrap() else {
            panic!("expected cleanup failure event");
        };
        assert_eq!(kind, AgentServerRequestFailureKind::Failed);
    }

    #[test]
    fn closed_writer_and_failed_write_are_explicit_and_still_one_shot() {
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        respond_to_server_request_on_session(&session, &user_input_request(json!(141)), &tx)
            .unwrap();
        let AgentEvent::UserInputRequested { responder, .. } = rx.try_recv().unwrap() else {
            panic!("expected user input");
        };
        session.close_writer();
        let error = responder
            .respond(AgentUserInputResponse::default())
            .unwrap_err();
        assert!(error.contains("连接已经关闭"));
        let duplicate = responder
            .respond(AgentUserInputResponse::default())
            .unwrap_err();
        assert!(duplicate.contains("拒绝重复 answers"));

        let session = Arc::new(CodexTurnSession::new(FailingWriter, None));
        let (tx, rx) = async_channel::unbounded();
        respond_to_server_request_on_session(&session, &user_input_request(json!(142)), &tx)
            .unwrap();
        let AgentEvent::UserInputRequested { responder, .. } = rx.try_recv().unwrap() else {
            panic!("expected user input");
        };
        let failure = responder
            .respond(AgentUserInputResponse {
                answers: vec![AgentUserInputAnswer {
                    question_id: "token".into(),
                    answers: vec!["never-log-this-secret".into()],
                }],
            })
            .unwrap_err();
        assert!(failure.contains("fixture JSON-RPC write failure"));
        assert!(!failure.contains("never-log-this-secret"));
        assert!(
            responder
                .respond(AgentUserInputResponse::default())
                .unwrap_err()
                .contains("拒绝重复 answers")
        );
    }

    #[test]
    #[ignore = "requires a logged-in local Codex CLI and makes one model request"]
    fn real_cli_safe_network_command_accept_once_round_trip() {
        let catalog = run_model_catalog_process().unwrap();
        let model = catalog
            .models
            .iter()
            .find(|model| model.is_default)
            .unwrap_or(&catalog.models[0]);
        let run = CodexAppServerBackend::new().run_prompt(AgentRequest {
            prompt: "Use the shell to run exactly `curl -I https://example.com` and no other command. Request approval for network access, then wait for my decision.".into(),
            cwd: std::env::current_dir().unwrap(),
            thread_id: None,
            model: model.model.clone(),
            effort: model.default_reasoning_effort.clone(),
            service_tier: model.default_service_tier.clone(),
            permission_mode: AgentPermissionMode::Request,
        });
        let (events, _interrupt) = run.into_parts();
        let deadline = Instant::now() + Duration::from_secs(180);
        let mut approval_id = None;
        let mut resolved = false;
        let mut completed = false;
        while Instant::now() < deadline && !completed {
            match events.try_recv() {
                Ok(AgentEvent::CommandApprovalRequested { request, responder }) => {
                    eprintln!("real command approval: {request:#?}");
                    assert!(request.allow_once);
                    approval_id = Some(request.request_id);
                    responder
                        .respond(AgentCommandApprovalChoice::Accept)
                        .unwrap();
                }
                Ok(AgentEvent::ServerRequestResolved { request }) => {
                    assert_eq!(Some(&request.request_id), approval_id.as_ref());
                    resolved = true;
                }
                Ok(AgentEvent::Completed) => completed = true,
                Ok(AgentEvent::Failed(error)) => panic!("real CLI turn failed: {error}"),
                Ok(_) | Err(async_channel::TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(async_channel::TryRecvError::Closed) => break,
            }
        }
        assert!(
            approval_id.is_some(),
            "real CLI did not request command approval"
        );
        assert!(resolved, "real CLI did not emit serverRequest/resolved");
        assert!(completed, "real CLI turn did not complete after accept");
    }

    #[test]
    fn unsupported_turn_diff_notification_is_undefined() {
        let mut reader = Cursor::new(
            b"{\"method\":\"turn/diff/updated\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"diff\":\"*** Begin Patch\"}}\n",
        );
        let mut output = Vec::new();

        let error = wait_for_response(&mut reader, &mut output, INITIALIZE_ID, None)
            .unwrap_err()
            .to_string();

        assert!(error.contains("turn/diff/updated"));
        assert!(error.contains("通知"));
        assert!(output.is_empty());
    }

    #[test]
    fn item_started_file_change_fails_fast_with_full_context() {
        let message = turn_item_message(
            "item/started",
            json!({
                "type": "fileChange",
                "id": "file_1",
                "status": "inProgress",
                "changes": []
            }),
        );
        assert_turn_message_fails(
            &message,
            &[
                "item/started",
                "fileChange",
                "file_1",
                "itemId",
                "thr_1",
                "turn_1",
                "changes",
            ],
        );
    }

    #[test]
    fn item_completed_file_change_fails_fast_with_full_context() {
        let message = turn_item_message(
            "item/completed",
            json!({
                "type": "fileChange",
                "id": "file_1",
                "status": "completed",
                "changes": []
            }),
        );
        assert_turn_message_fails(
            &message,
            &[
                "item/completed",
                "fileChange",
                "file_1",
                "itemId",
                "thr_1",
                "turn_1",
                "changes",
            ],
        );
    }

    #[test]
    fn future_item_type_fails_fast_for_started_and_completed() {
        for method in ["item/started", "item/completed"] {
            let message = turn_item_message(
                method,
                json!({"type": "futureItem", "id": "future_1", "payload": "probe"}),
            );
            assert_turn_message_fails(
                &message,
                &[method, "futureItem", "future_1", "thr_1", "turn_1", "probe"],
            );
        }
    }

    #[test]
    fn item_started_user_message_is_validated_without_duplicate_ui_event() {
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        let mut streamed_text = false;
        let message = json!({
            "method": "item/started",
            "params": {
                "item": {
                    "type": "userMessage",
                    "id": "user_1",
                    "clientId": null,
                    "content": [{
                        "type": "text",
                        "text": "hello",
                        "text_elements": []
                    }]
                },
                "threadId": "thr_1",
                "turnId": "turn_1",
                "startedAtMs": 1
            },
            "emittedAtMs": 1
        });

        let outcome = super::process_turn_message(
            &session,
            &message,
            "thr_1",
            "turn_1",
            &tx,
            &mut streamed_text,
        )
        .unwrap();

        assert_eq!(outcome, None);
        assert!(!streamed_text);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn item_started_user_message_schema_errors_fail_fast() {
        let cases = [
            (json!({"type":"userMessage","content":[]}), "item.id"),
            (json!({"type":"userMessage","id":"user_1"}), "item.content"),
            (
                json!({"type":"userMessage","id":"user_1","clientId":1,"content":[]}),
                "item.clientId",
            ),
            (
                json!({"type":"userMessage","id":"user_1","content":["hello"]}),
                "content[0]",
            ),
            (
                json!({"type":"userMessage","id":"user_1","content":[{"text":"hello"}]}),
                "content[0] item.type",
            ),
            (
                json!({"type":"userMessage","id":"user_1","content":[{"type":"text"}]}),
                "content[0] item.text",
            ),
            (
                json!({"type":"userMessage","id":"user_1","content":[{"type":"text","text":"hello","text_elements":1}]}),
                "text_elements",
            ),
            (
                json!({"type":"userMessage","id":"user_1","content":[{"type":"image","url":"https://example.com/image.png"}]}),
                "type `image` 尚未接入",
            ),
        ];

        for (item, expected) in cases {
            let message = turn_item_message("item/started", item);
            assert_turn_message_fails(&message, &["item/started", "userMessage", expected]);
        }
    }

    #[test]
    fn item_completed_user_message_remains_unsupported() {
        let message = turn_item_message(
            "item/completed",
            json!({
                "type": "userMessage",
                "id": "user_1",
                "clientId": null,
                "content": [{"type":"text","text":"hello","text_elements":[]}]
            }),
        );
        assert_turn_message_fails(
            &message,
            &["item/completed", "userMessage", "user_1", "未接入"],
        );
    }

    #[test]
    fn every_unsupported_thread_item_type_fails_for_started_and_completed() {
        for item_type in [
            "hookPrompt",
            "functionCallOutput",
            "plan",
            "reasoning",
            "fileChange",
            "mcpToolCall",
            "dynamicToolCall",
            "collabAgentToolCall",
            "subAgentActivity",
            "webSearch",
            "imageView",
            "sleep",
            "imageGeneration",
            "enteredReviewMode",
            "exitedReviewMode",
            "contextCompaction",
        ] {
            for method in ["item/started", "item/completed"] {
                let item_id = format!("{item_type}_1");
                let message = turn_item_message(
                    method,
                    json!({"type": item_type, "id": item_id, "probe": true}),
                );
                assert_turn_message_fails(
                    &message,
                    &[method, item_type, &item_id, "thr_1", "turn_1"],
                );
            }
        }
    }

    #[test]
    fn required_item_and_delta_fields_never_fall_through() {
        let cases = vec![
            (
                "started missing item",
                json!({"method":"item/started","params":{"threadId":"thr_1","turnId":"turn_1"}}),
                "params.item",
            ),
            (
                "completed missing item",
                json!({"method":"item/completed","params":{"threadId":"thr_1","turnId":"turn_1"}}),
                "params.item",
            ),
            (
                "item is not object",
                turn_item_message("item/started", json!("agentMessage")),
                "params.item",
            ),
            (
                "missing type",
                turn_item_message("item/started", json!({"id":"msg_1","text":"hello"})),
                "item.type",
            ),
            (
                "completed missing type",
                turn_item_message("item/completed", json!({"id":"msg_1","text":"hello"})),
                "item.type",
            ),
            (
                "type is not string",
                turn_item_message(
                    "item/completed",
                    json!({"type":1,"id":"msg_1","text":"hello"}),
                ),
                "item.type",
            ),
            (
                "missing id",
                turn_item_message(
                    "item/started",
                    json!({"type":"agentMessage","text":"hello"}),
                ),
                "item.id",
            ),
            (
                "completed missing id",
                turn_item_message(
                    "item/completed",
                    json!({"type":"agentMessage","text":"hello"}),
                ),
                "item.id",
            ),
            (
                "id is not string",
                turn_item_message(
                    "item/completed",
                    json!({"type":"agentMessage","id":1,"text":"hello"}),
                ),
                "item.id",
            ),
            (
                "missing text",
                turn_item_message("item/started", json!({"type":"agentMessage","id":"msg_1"})),
                "item.text",
            ),
            (
                "completed missing text",
                turn_item_message(
                    "item/completed",
                    json!({"type":"agentMessage","id":"msg_1"}),
                ),
                "item.text",
            ),
            (
                "text is not string",
                turn_item_message(
                    "item/completed",
                    json!({"type":"agentMessage","id":"msg_1","text":1}),
                ),
                "item.text",
            ),
            (
                "agent delta missing itemId",
                json!({"method":"item/agentMessage/delta","params":{"threadId":"thr_1","turnId":"turn_1","delta":"hello"}}),
                "params.itemId",
            ),
            (
                "agent delta missing delta",
                json!({"method":"item/agentMessage/delta","params":{"threadId":"thr_1","turnId":"turn_1","itemId":"msg_1"}}),
                "params.delta",
            ),
            (
                "agent delta itemId is not string",
                json!({"method":"item/agentMessage/delta","params":{"threadId":"thr_1","turnId":"turn_1","itemId":1,"delta":"hello"}}),
                "params.itemId",
            ),
            (
                "agent delta is not string",
                json!({"method":"item/agentMessage/delta","params":{"threadId":"thr_1","turnId":"turn_1","itemId":"msg_1","delta":1}}),
                "params.delta",
            ),
            (
                "command delta missing itemId",
                json!({"method":"item/commandExecution/outputDelta","params":{"threadId":"thr_1","turnId":"turn_1","delta":"output"}}),
                "params.itemId",
            ),
            (
                "command delta missing delta",
                json!({"method":"item/commandExecution/outputDelta","params":{"threadId":"thr_1","turnId":"turn_1","itemId":"exec_1"}}),
                "params.delta",
            ),
            (
                "command delta itemId is not string",
                json!({"method":"item/commandExecution/outputDelta","params":{"threadId":"thr_1","turnId":"turn_1","itemId":1,"delta":"output"}}),
                "params.itemId",
            ),
            (
                "command delta is not string",
                json!({"method":"item/commandExecution/outputDelta","params":{"threadId":"thr_1","turnId":"turn_1","itemId":"exec_1","delta":1}}),
                "params.delta",
            ),
        ];
        for (name, message, expected) in cases {
            let error = assert_turn_message_fails(&message, &[expected]);
            assert!(
                error.contains("缺少字符串") || error.contains("必须是"),
                "{name}: {error}"
            );
        }
    }

    #[test]
    fn command_execution_required_fields_and_nullable_fields_are_strict() {
        for field in ["id", "command", "commandActions", "cwd", "status"] {
            let mut item = command_execution_item("inProgress");
            item.as_object_mut().unwrap().remove(field);
            let message = turn_item_message("item/started", item);
            assert_turn_message_fails(&message, &["commandExecution", field]);
        }

        for (field, invalid) in [
            ("id", json!(1)),
            ("command", json!(1)),
            ("commandActions", json!({})),
            ("cwd", json!(1)),
            ("status", json!(1)),
        ] {
            let mut item = command_execution_item("inProgress");
            item[field] = invalid;
            let message = turn_item_message("item/started", item);
            assert_turn_message_fails(&message, &["commandExecution", field]);
        }

        for (field, invalid) in [("aggregatedOutput", json!(7)), ("exitCode", json!("zero"))] {
            let mut item = command_execution_item("completed");
            item[field] = invalid;
            let message = turn_item_message("item/completed", item);
            assert_turn_message_fails(&message, &["commandExecution", field]);
        }
    }

    #[test]
    fn command_execution_unknown_status_fails_fast() {
        let message = turn_item_message(
            "item/completed",
            command_execution_item("pausedByFutureServer"),
        );
        assert_turn_message_fails(
            &message,
            &[
                "item/completed",
                "commandExecution",
                "exec_1",
                "pausedByFutureServer",
                "thr_1",
                "turn_1",
            ],
        );
    }

    #[test]
    fn agent_message_completion_is_explicit_with_and_without_streaming() {
        for streamed in [false, true] {
            let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
            let (tx, rx) = async_channel::unbounded();
            let mut streamed_text = false;
            let started = turn_item_message(
                "item/started",
                json!({"type":"agentMessage","id":"msg_1","text":""}),
            );
            super::process_turn_message(
                &session,
                &started,
                "thr_1",
                "turn_1",
                &tx,
                &mut streamed_text,
            )
            .unwrap();
            if streamed {
                let delta = json!({
                    "method":"item/agentMessage/delta",
                    "params":{"threadId":"thr_1","turnId":"turn_1","itemId":"msg_1","delta":"hello"}
                });
                super::process_turn_message(
                    &session,
                    &delta,
                    "thr_1",
                    "turn_1",
                    &tx,
                    &mut streamed_text,
                )
                .unwrap();
            }
            let completed = turn_item_message(
                "item/completed",
                json!({"type":"agentMessage","id":"msg_1","text":"hello"}),
            );
            super::process_turn_message(
                &session,
                &completed,
                "thr_1",
                "turn_1",
                &tx,
                &mut streamed_text,
            )
            .unwrap();
            drop(tx);

            let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
            assert_eq!(
                events,
                vec![
                    AgentEvent::AssistantMessageStarted {
                        item_id: "msg_1".into()
                    },
                    AgentEvent::TextDelta("hello".into()),
                ],
                "streamed={streamed}"
            );
        }
    }

    #[test]
    fn active_turn_event_delivery_failure_is_fatal() {
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        drop(rx);
        let mut streamed_text = false;
        let message = turn_item_message(
            "item/started",
            json!({"type":"agentMessage","id":"msg_1","text":""}),
        );
        let error = super::process_turn_message(
            &session,
            &message,
            "thr_1",
            "turn_1",
            &tx,
            &mut streamed_text,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("事件通道已经关闭"));
        assert!(error.contains("item/started agentMessage"));
        for fragment in ["agentMessage", "msg_1", "thr_1", "turn_1", "item="] {
            assert!(error.contains(fragment), "missing `{fragment}`: {error}");
        }
    }

    #[test]
    fn forwarded_notification_delivery_failure_is_fatal() {
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        drop(rx);
        let mut streamed_text = false;
        let message = json!({
            "method": "turn/started",
            "params": {
                "threadId": "thr_1",
                "turn": {"id": "turn_1", "items": [], "status": "inProgress"}
            }
        });
        let error = super::process_turn_message(
            &session,
            &message,
            "thr_1",
            "turn_1",
            &tx,
            &mut streamed_text,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("事件通道已经关闭"));
        assert!(error.contains("turn/started"));
    }

    #[test]
    fn thread_created_delivery_failure_stops_the_session() {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_1\"}}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        drop(rx);
        let error = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "probe".into(),
                cwd: PathBuf::from("/tmp/project"),
                thread_id: None,
                model: "gpt-test".into(),
                effort: "medium".into(),
                service_tier: None,
                permission_mode: AgentPermissionMode::Full,
            },
            &tx,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("thread created 事件通道已经关闭"));
    }

    #[test]
    fn active_turn_stops_at_the_first_unknown_method() {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_1\"}}}\n",
            "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_1\"}}}\n",
            "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_1\",\"turn\":{\"id\":\"turn_1\",\"items\":[],\"status\":\"inProgress\"}}}\n",
            "{\"method\":\"item/brandNew/delta\",\"params\":{\"delta\":\"diagnostic payload\"}}\n",
            "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thr_1\",\"turn\":{\"id\":\"turn_1\",\"status\":\"completed\"}}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();

        let error = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "probe".into(),
                cwd: PathBuf::from("/tmp/project"),
                thread_id: None,
                model: "gpt-test".into(),
                effort: "medium".into(),
                service_tier: None,
                permission_mode: AgentPermissionMode::Full,
            },
            &tx,
        )
        .unwrap_err()
        .to_string();
        drop(tx);

        assert!(error.contains("item/brandNew/delta"));
        assert!(error.contains("diagnostic payload"));
        assert_eq!(
            rx.try_recv().unwrap(),
            AgentEvent::ThreadCreated {
                thread_id: "thr_1".into()
            }
        );
        assert_eq!(rx.try_recv().unwrap(), AgentEvent::Started);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn model_notifications_are_normalized_into_agent_events() {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_1\"}}}\n",
            "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_1\"}}}\n",
            "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_1\",\"turn\":{\"id\":\"turn_1\",\"items\":[],\"status\":\"inProgress\"}}}\n",
            "{\"method\":\"model/rerouted\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"fromModel\":\"model-a\",\"toModel\":\"model-b\",\"reason\":\"highRiskCyberActivity\"}}\n",
            "{\"method\":\"model/safetyBuffering/updated\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"model\":\"model-b\",\"useCases\":[\"cyber\"],\"reasons\":[\"review\"],\"showBufferingUi\":true,\"fasterModel\":\"model-c\"}}\n",
            "{\"method\":\"model/safetyBuffering/updated\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"model\":\"model-b\",\"useCases\":[],\"reasons\":[],\"showBufferingUi\":false,\"fasterModel\":null}}\n",
            "{\"method\":\"model/verification\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"verifications\":[\"trustedAccessForCyber\"]}}\n",
            "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thr_1\",\"turn\":{\"id\":\"turn_1\",\"status\":\"completed\"}}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        let outcome = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "probe".into(),
                cwd: PathBuf::from("/tmp/project"),
                thread_id: None,
                model: "model-a".into(),
                effort: "high".into(),
                service_tier: None,
                permission_mode: AgentPermissionMode::Full,
            },
            &tx,
        )
        .unwrap();
        assert_eq!(outcome, TurnOutcome::Completed);
        tx.send_blocking(outcome.into_event()).unwrap();
        drop(tx);

        let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert_eq!(
            events,
            vec![
                AgentEvent::ThreadCreated {
                    thread_id: "thr_1".into()
                },
                AgentEvent::Started,
                AgentEvent::ModelRerouted {
                    from_model: "model-a".into(),
                    to_model: "model-b".into(),
                    reason: "highRiskCyberActivity".into(),
                },
                AgentEvent::ModelSafetyBufferingUpdated {
                    model: "model-b".into(),
                    use_cases: vec!["cyber".into()],
                    reasons: vec!["review".into()],
                    show_buffering_ui: true,
                    faster_model: Some("model-c".into()),
                },
                AgentEvent::ModelSafetyBufferingUpdated {
                    model: "model-b".into(),
                    use_cases: Vec::new(),
                    reasons: Vec::new(),
                    show_buffering_ui: false,
                    faster_model: None,
                },
                AgentEvent::ModelVerificationRequired {
                    verifications: vec!["trustedAccessForCyber".into()],
                },
                AgentEvent::Completed,
            ]
        );
    }
}
