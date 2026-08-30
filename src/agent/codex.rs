use std::{
    io::{BufRead, BufReader, Write},
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
    AgentBackend, AgentConfigWarning, AgentEvent, AgentInterruptControl, AgentInterruptHandle,
    AgentInterruptOutcome, AgentModel, AgentModelCatalog, AgentReasoningEffort, AgentRequest,
    AgentRun, AgentServiceTier, AgentThreadSettings, CommandExecution, CommandExecutionStatus,
};

const INITIALIZE_ID: u64 = 1;
const THREAD_START_ID: u64 = 2;
const TURN_START_ID: u64 = 3;
const TURN_INTERRUPT_ID: u64 = 4;
const MODEL_LIST_FIRST_ID: u64 = 2;
const MODEL_LIST_PAGE_SIZE: u32 = 50;
const UNDEFINED_METHOD_PARAMS_LIMIT: usize = 2_000;

// These protocol methods are intentionally recognized even though this view
// does not render them yet. Keep the list exact: a prefix match or wildcard
// would hide new app-server surface area instead of reporting it. Every method
// with a user-facing event is handled outside this list.
const PASSIVE_SERVER_METHODS: &[&str] = &[
    "remoteControl/status/changed",
    "thread/started",
    "mcpServer/startupStatus/updated",
    "thread/status/changed",
    "turn/plan/updated",
    "thread/tokenUsage/updated",
    "account/rateLimits/updated",
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
    process: Option<Arc<AppServerProcess>>,
}

impl<W: Write + Send> CodexTurnSession<W> {
    fn new(writer: W, process: Option<Arc<AppServerProcess>>) -> Self {
        Self {
            writer: Mutex::new(Some(writer)),
            state: Mutex::new(TurnSessionState::default()),
            process,
        }
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

    fn ensure_current_turn(&self, thread_id: &str, turn_id: &str) -> Result<()> {
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow!("Codex turn 会话状态锁已损坏"))?;
        let current_thread_id = state
            .thread_id
            .as_deref()
            .context("收到 turn/completed 时尚未保存当前 threadId")?;
        if current_thread_id != thread_id {
            bail!(
                "turn/completed 的 threadId `{thread_id}` 与当前 threadId `{current_thread_id}` 不一致"
            );
        }
        let current_turn_id = state
            .turn_id
            .as_deref()
            .context("收到 turn/completed 时尚未保存当前 turnId")?;
        if current_turn_id != turn_id {
            bail!("turn/completed 的 turnId `{turn_id}` 与当前 turnId `{current_turn_id}` 不一致");
        }
        Ok(())
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

    fn run_prompt(&self, request: AgentRequest) -> AgentRun {
        let (events_tx, events_rx) = async_channel::unbounded();
        let interrupt = match spawn_prompt_session() {
            Ok((mut reader, session)) => {
                let control: Arc<dyn AgentInterruptControl> = session.clone();
                let interrupt = AgentInterruptHandle::new(control);
                std::thread::spawn(move || {
                    let result = drive_session(&mut reader, &session, &request, &events_tx);
                    session.mark_terminal();
                    let cleanup = session.finish();
                    let event = match (result, cleanup) {
                        (Ok(outcome), Ok(())) => outcome.into_event(),
                        (Err(error), Ok(())) => AgentEvent::Failed(format!("{error:#}")),
                        (Ok(TurnOutcome::Failed(message)), Err(error)) => {
                            AgentEvent::Failed(format!(
                                "{message}\nCodex turn 已失败，且 app-server 资源回收失败：{error:#}"
                            ))
                        }
                        (Ok(_), Err(error)) => AgentEvent::Failed(format!(
                            "Codex turn 已结束，但 app-server 资源回收失败：{error:#}"
                        )),
                        (Err(error), Err(cleanup_error)) => AgentEvent::Failed(format!(
                            "{error:#}\nCodex app-server 资源回收同时失败：{cleanup_error:#}"
                        )),
                    };
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
                }
            }
        }),
    )?;
    wait_for_response(reader, writer, INITIALIZE_ID, events)?;
    send(writer, json!({ "method": "initialized", "params": {} }))
}

fn initialize_turn_connection<R: BufRead, W: Write + Send>(
    reader: &mut R,
    session: &CodexTurnSession<W>,
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
            }
        }
    }))?;
    wait_for_session_response(reader, session, INITIALIZE_ID, events)?;
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

fn drive_session<R: BufRead, W: Write + Send>(
    reader: &mut R,
    session: &CodexTurnSession<W>,
    request: &AgentRequest,
    events: &Sender<AgentEvent>,
) -> Result<TurnOutcome> {
    initialize_turn_connection(reader, session, events)?;
    session.send(json!({
        "method": "thread/start",
        "id": THREAD_START_ID,
        "params": {
            "cwd": request.cwd,
            "approvalPolicy": "never",
            "sandbox": "read-only",
            "ephemeral": true,
            "serviceName": "gpui-chat-clone",
            "model": request.model,
            "serviceTier": request.service_tier
        }
    }))?;
    let thread_response = wait_for_session_response(reader, session, THREAD_START_ID, events)?;
    let thread_id = thread_response
        .pointer("/result/thread/id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .context("thread/start 响应缺少 result.thread.id")?;

    session.send(json!({
        "method": "turn/start",
        "id": TURN_START_ID,
        "params": {
            "threadId": thread_id,
            "input": [{ "type": "text", "text": request.prompt }],
            "model": request.model,
            "effort": request.effort,
            "serviceTier": request.service_tier
        }
    }))?;
    let turn_response = wait_for_session_response(reader, session, TURN_START_ID, events)?;
    let turn_id = turn_response
        .pointer("/result/turn/id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .context("turn/start 响应缺少 result.turn.id")?;
    session.activate_turn(thread_id, turn_id)?;

    let mut streamed_text = false;
    loop {
        let message = read_message(reader)?;
        respond_to_server_request_on_session(session, &message)?;
        forward_agent_notification(&message, events)?;
        ensure_server_method_is_defined(&message)?;

        match message.get("method").and_then(Value::as_str) {
            Some("item/started") => {
                let item = message.pointer("/params/item");
                match item
                    .and_then(|item| item.get("type"))
                    .and_then(Value::as_str)
                {
                    Some("agentMessage") => {
                        if let Some(item_id) =
                            item.and_then(|item| item.get("id")).and_then(Value::as_str)
                        {
                            let _ = events.send_blocking(AgentEvent::AssistantMessageStarted {
                                item_id: item_id.to_owned(),
                            });
                        }
                    }
                    Some("commandExecution") => {
                        if let Some(command) = item.and_then(parse_command_execution) {
                            let _ = events.send_blocking(AgentEvent::CommandStarted(command));
                        }
                    }
                    _ => {}
                }
            }
            Some("item/agentMessage/delta") => {
                if let Some(delta) = message.pointer("/params/delta").and_then(Value::as_str) {
                    streamed_text = true;
                    let _ = events.send_blocking(AgentEvent::TextDelta(delta.to_owned()));
                }
            }
            Some("item/commandExecution/outputDelta") => {
                let item_id = message.pointer("/params/itemId").and_then(Value::as_str);
                let delta = message.pointer("/params/delta").and_then(Value::as_str);
                if let (Some(item_id), Some(delta)) = (item_id, delta) {
                    let _ = events.send_blocking(AgentEvent::CommandOutputDelta {
                        item_id: item_id.to_owned(),
                        delta: delta.to_owned(),
                    });
                }
            }
            Some("item/completed") => {
                let item = message.pointer("/params/item");
                if let Some(command) = item.and_then(parse_command_execution) {
                    let _ = events.send_blocking(AgentEvent::CommandCompleted(command));
                } else if !streamed_text
                    && item
                        .and_then(|item| item.get("type"))
                        .and_then(Value::as_str)
                        == Some("agentMessage")
                {
                    if let Some(text) = item
                        .and_then(|item| item.get("text"))
                        .and_then(Value::as_str)
                    {
                        let _ = events.send_blocking(AgentEvent::TextDelta(text.to_owned()));
                    }
                }
            }
            Some("turn/completed") => {
                let completed_thread_id = message
                    .pointer("/params/threadId")
                    .and_then(Value::as_str)
                    .context("turn/completed 通知缺少 params.threadId")?;
                let completed_turn_id = message
                    .pointer("/params/turn/id")
                    .and_then(Value::as_str)
                    .context("turn/completed 通知缺少 params.turn.id")?;
                session.ensure_current_turn(completed_thread_id, completed_turn_id)?;
                let status = message
                    .pointer("/params/turn/status")
                    .and_then(Value::as_str)
                    .context("turn/completed 通知缺少 params.turn.status")?;
                let outcome = match status {
                    "completed" => TurnOutcome::Completed,
                    "interrupted" => TurnOutcome::Interrupted,
                    "failed" => TurnOutcome::Failed(turn_failure_message(&message)?),
                    _ => bail!("Codex turn 结束，状态为未知值 `{status}`"),
                };
                session.mark_terminal();
                return Ok(outcome);
            }
            Some(
                "turn/started"
                | "error"
                | "thread/settings/updated"
                | "warning"
                | "configWarning"
                | "model/rerouted"
                | "model/verification"
                | "model/safetyBuffering/updated",
            ) => {}
            Some(method) if PASSIVE_SERVER_METHODS.contains(&method) => {}
            Some(method) => return Err(undefined_server_method_error(method, &message)),
            None => {}
        }
    }
}

fn is_defined_server_method(method: &str) -> bool {
    matches!(
        method,
        "item/started"
            | "item/agentMessage/delta"
            | "item/commandExecution/outputDelta"
            | "item/completed"
            | "turn/started"
            | "turn/completed"
            | "error"
            | "thread/settings/updated"
            | "warning"
            | "configWarning"
            | "model/rerouted"
            | "model/verification"
            | "model/safetyBuffering/updated"
    ) || PASSIVE_SERVER_METHODS.contains(&method)
}

fn required_notification_string(message: &Value, field: &str) -> Result<String> {
    message
        .pointer(&format!("/params/{field}"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| {
            let method = message
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("未知方法");
            format!("{method} 通知缺少字符串字段 params.{field}")
        })
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
            })
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
        let _ = events.send_blocking(event);
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
    if is_defined_server_method(method) {
        Ok(())
    } else {
        Err(undefined_server_method_error(method, message))
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

fn parse_command_execution(item: &Value) -> Option<CommandExecution> {
    if item.get("type").and_then(Value::as_str) != Some("commandExecution") {
        return None;
    }
    let status = match item.get("status").and_then(Value::as_str) {
        Some("completed") if item.get("exitCode").and_then(Value::as_i64).unwrap_or(0) == 0 => {
            CommandExecutionStatus::Completed
        }
        Some("failed" | "declined") => CommandExecutionStatus::Failed,
        Some("completed") => CommandExecutionStatus::Failed,
        _ => CommandExecutionStatus::InProgress,
    };
    let command = item
        .pointer("/commandActions/0/command")
        .or_else(|| item.get("command"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    Some(CommandExecution {
        id: item.get("id").and_then(Value::as_str)?.to_owned(),
        command,
        cwd: item
            .get("cwd")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        output: item
            .get("aggregatedOutput")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        status,
        exit_code: item.get("exitCode").and_then(Value::as_i64),
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
        if let Some(events) = events {
            forward_agent_notification(&message, events)?;
        } else if parse_agent_notification(&message)?.is_some() {
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

fn wait_for_session_response<R: BufRead, W: Write + Send>(
    reader: &mut R,
    session: &CodexTurnSession<W>,
    expected_id: u64,
    events: &Sender<AgentEvent>,
) -> Result<Value> {
    loop {
        let message = read_message(reader)?;
        respond_to_server_request_on_session(session, &message)?;
        forward_agent_notification(&message, events)?;
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

fn respond_to_server_request_on_session<W: Write + Send>(
    session: &CodexTurnSession<W>,
    message: &Value,
) -> Result<()> {
    let Some(id) = message.get("id") else {
        return Ok(());
    };
    if message.get("method").is_none() {
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

#[cfg(test)]
mod tests {
    use std::{io::Cursor, path::PathBuf, process::Command, sync::Arc};

    use serde_json::{Value, json};

    use super::{
        AgentConfigWarning, AgentEvent, AgentInterruptControl, AgentInterruptHandle,
        AgentInterruptOutcome, AgentRequest, AgentThreadSettings, AppServerProcess,
        CodexTurnSession, INITIALIZE_ID, MODEL_LIST_PAGE_SIZE, PASSIVE_SERVER_METHODS, TurnOutcome,
        UNDEFINED_METHOD_PARAMS_LIMIT, drive_model_catalog, drive_session,
        ensure_server_method_is_defined, wait_for_response,
    };

    fn take_session_output(session: &CodexTurnSession<Vec<u8>>) -> Vec<u8> {
        session.writer.lock().unwrap().take().unwrap()
    }

    #[test]
    fn drives_one_complete_prompt_and_normalizes_stream_events() {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_1\"}}}\n",
            "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_1\",\"turn\":{\"id\":\"turn_1\",\"items\":[],\"status\":\"inProgress\"}}}\n",
            "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_1\"}}}\n",
            "{\"method\":\"thread/status/changed\",\"params\":{\"threadId\":\"thr_1\"}}\n",
            "{\"method\":\"item/started\",\"params\":{\"item\":{\"type\":\"agentMessage\",\"id\":\"msg_1\",\"text\":\"\"}}}\n",
            "{\"method\":\"item/agentMessage/delta\",\"params\":{\"delta\":\"你好\"}}\n",
            "{\"method\":\"item/agentMessage/delta\",\"params\":{\"delta\":\"！\"}}\n",
            "{\"method\":\"item/started\",\"params\":{\"item\":{\"type\":\"commandExecution\",\"id\":\"exec_1\",\"command\":\"/bin/zsh -lc pwd\",\"cwd\":\"/tmp/project\",\"status\":\"inProgress\",\"commandActions\":[{\"type\":\"unknown\",\"command\":\"pwd\"}],\"aggregatedOutput\":null,\"exitCode\":null}}}\n",
            "{\"method\":\"item/commandExecution/outputDelta\",\"params\":{\"itemId\":\"exec_1\",\"delta\":\"/tmp/project\\n\"}}\n",
            "{\"method\":\"item/completed\",\"params\":{\"item\":{\"type\":\"commandExecution\",\"id\":\"exec_1\",\"command\":\"/bin/zsh -lc pwd\",\"cwd\":\"/tmp/project\",\"status\":\"completed\",\"commandActions\":[{\"type\":\"unknown\",\"command\":\"pwd\"}],\"aggregatedOutput\":\"/tmp/project\\n\",\"exitCode\":0}}}\n",
            "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thr_1\",\"turn\":{\"id\":\"turn_1\",\"status\":\"completed\"}}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let session = CodexTurnSession::new(Vec::new(), None);
        let (tx, rx) = async_channel::unbounded();
        let outcome = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "打个招呼".into(),
                cwd: PathBuf::from("/tmp/project"),
                model: "gpt-test".into(),
                effort: "high".into(),
                service_tier: Some("priority".into()),
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
    }

    #[test]
    fn pending_interrupt_uses_the_active_thread_and_turn_and_waits_for_terminal_status() {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_interrupt\"}}}\n",
            "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_interrupt\"}}}\n",
            "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_interrupt\",\"turn\":{\"id\":\"turn_interrupt\",\"items\":[],\"status\":\"inProgress\"}}}\n",
            "{\"id\":4,\"result\":{}}\n",
            "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thr_interrupt\",\"turn\":{\"id\":\"turn_interrupt\",\"status\":\"interrupted\"}}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let session = CodexTurnSession::new(Vec::new(), None);
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
                model: "gpt-test".into(),
                effort: "medium".into(),
                service_tier: None,
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
        let session = CodexTurnSession::new(Vec::new(), None);
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
    fn every_captured_passive_method_is_explicitly_defined() {
        for method in PASSIVE_SERVER_METHODS {
            ensure_server_method_is_defined(&json!({
                "method": method,
                "params": { "probe": true }
            }))
            .unwrap();
        }
    }

    #[test]
    fn user_facing_methods_are_defined_and_never_passive() {
        for method in [
            "turn/started",
            "error",
            "turn/completed",
            "thread/settings/updated",
            "warning",
            "configWarning",
        ] {
            assert!(!PASSIVE_SERVER_METHODS.contains(&method));
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
        let session = CodexTurnSession::new(Vec::new(), None);
        let (tx, rx) = async_channel::unbounded();

        let outcome = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "probe notices".into(),
                cwd: PathBuf::from("/tmp/project"),
                model: "model-a".into(),
                effort: "medium".into(),
                service_tier: None,
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
                AgentEvent::Started,
                AgentEvent::ThreadSettingsUpdated(AgentThreadSettings {
                    model: "model-b".into(),
                    effort: Some("high".into()),
                    service_tier: Some("priority".into()),
                    cwd: "/tmp/project/updated".into(),
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
        let session = CodexTurnSession::new(Vec::new(), None);
        let (tx, rx) = async_channel::unbounded();

        let outcome = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "fail".into(),
                cwd: PathBuf::from("/tmp/project"),
                model: "model-a".into(),
                effort: "medium".into(),
                service_tier: None,
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
        let session = CodexTurnSession::new(Vec::new(), None);
        let (tx, rx) = async_channel::unbounded();

        let error = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "probe".into(),
                cwd: PathBuf::from("/tmp/project"),
                model: "gpt-test".into(),
                effort: "medium".into(),
                service_tier: None,
            },
            &tx,
        )
        .unwrap_err()
        .to_string();
        drop(tx);

        assert!(error.contains("item/brandNew/delta"));
        assert!(error.contains("diagnostic payload"));
        assert_eq!(rx.try_recv().unwrap(), AgentEvent::Started);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn captured_method_set_replays_without_false_unknowns() {
        let input = concat!(
            "{\"method\":\"remoteControl/status/changed\",\"params\":{}}\n",
            "{\"method\":\"mcpServer/startupStatus/updated\",\"params\":{}}\n",
            "{\"id\":1,\"result\":{}}\n",
            "{\"method\":\"thread/started\",\"params\":{}}\n",
            "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_1\"}}}\n",
            "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_1\"}}}\n",
            "{\"method\":\"thread/status/changed\",\"params\":{}}\n",
            "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_1\",\"turn\":{\"id\":\"turn_1\",\"items\":[],\"status\":\"inProgress\"}}}\n",
            "{\"method\":\"turn/plan/updated\",\"params\":{}}\n",
            "{\"method\":\"thread/tokenUsage/updated\",\"params\":{}}\n",
            "{\"method\":\"account/rateLimits/updated\",\"params\":{}}\n",
            "{\"method\":\"item/started\",\"params\":{\"item\":{\"type\":\"commandExecution\",\"id\":\"exec_1\",\"command\":\"pwd\",\"cwd\":\"/tmp/project\",\"status\":\"inProgress\"}}}\n",
            "{\"method\":\"item/commandExecution/outputDelta\",\"params\":{\"itemId\":\"exec_1\",\"delta\":\"/tmp/project\\n\"}}\n",
            "{\"method\":\"item/completed\",\"params\":{\"item\":{\"type\":\"commandExecution\",\"id\":\"exec_1\",\"command\":\"pwd\",\"cwd\":\"/tmp/project\",\"status\":\"completed\",\"aggregatedOutput\":\"/tmp/project\\n\",\"exitCode\":0}}}\n",
            "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thr_1\",\"turn\":{\"id\":\"turn_1\",\"status\":\"completed\"}}}\n"
        );

        let mut reader = Cursor::new(input.as_bytes());
        let session = CodexTurnSession::new(Vec::new(), None);
        let (tx, rx) = async_channel::unbounded();
        let outcome = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "深入分析当前项目".into(),
                cwd: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
                model: "gpt-test".into(),
                effort: "medium".into(),
                service_tier: None,
            },
            &tx,
        )
        .unwrap();
        assert_eq!(outcome, TurnOutcome::Completed);
        tx.send_blocking(outcome.into_event()).unwrap();
        drop(tx);

        let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert_eq!(events.first(), Some(&AgentEvent::Started));
        assert_eq!(events.last(), Some(&AgentEvent::Completed));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AgentEvent::CommandOutputDelta { .. }))
        );
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
        let session = CodexTurnSession::new(Vec::new(), None);
        let (tx, rx) = async_channel::unbounded();
        let outcome = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                prompt: "probe".into(),
                cwd: PathBuf::from("/tmp/project"),
                model: "model-a".into(),
                effort: "high".into(),
                service_tier: None,
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
