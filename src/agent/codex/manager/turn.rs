//! Managed prompt and turn ownership, dispatch ordering, and interruption.

use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, Ordering},
};

use anyhow::{Context as _, Result, anyhow, bail};
use async_channel::{Receiver, Sender};
use serde_json::{Value, json};

use super::{
    super::{
        CodexTurnSession, TURN_SCOPED_SERVER_METHODS, TurnOutcome, cleanup_pending_server_requests,
        ensure_session_message_matches, permissions::PermissionFields, process_turn_message,
    },
    CodexAppServerManager,
    connection::{Connection, PendingThreadLifecycle, ThreadLifecycleKind},
    transport::SharedJsonWriter,
};
use crate::agent::{
    AgentEvent, AgentInterruptControl, AgentInterruptHandle, AgentInterruptOutcome, AgentRequest,
    AgentRun,
};

#[derive(Default)]
pub(super) struct PromptControlState {
    pub(super) turn: Option<Weak<ManagedTurn>>,
    pub(super) interrupt_requested: bool,
    pub(super) abandoned: bool,
    pub(super) terminal: bool,
}

#[derive(Default)]
pub(super) struct PromptControl {
    pub(super) state: Mutex<PromptControlState>,
}

impl PromptControl {
    pub(super) fn attach(&self, turn: &Arc<ManagedTurn>) {
        let (interrupt, abandoned) = match self.state.lock() {
            Ok(mut state) => {
                if state.terminal {
                    return;
                }
                state.turn = Some(Arc::downgrade(turn));
                (state.interrupt_requested, state.abandoned)
            }
            Err(_) => (true, true),
        };
        if interrupt || abandoned {
            let _ = turn.request_interrupt();
        }
    }

    pub(super) fn mark_terminal(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.terminal = true;
            state.turn = None;
        }
    }

    pub(super) fn is_abandoned(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.abandoned)
            .unwrap_or(true)
    }
}

impl AgentInterruptControl for PromptControl {
    fn request_interrupt(&self) -> std::result::Result<AgentInterruptOutcome, String> {
        let turn = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "Codex prompt interrupt 状态锁已损坏".to_owned())?;
            if state.terminal {
                return Ok(AgentInterruptOutcome::AlreadyFinished);
            }
            if state.interrupt_requested {
                return Ok(AgentInterruptOutcome::AlreadyRequested);
            }
            state.interrupt_requested = true;
            state.turn.as_ref().and_then(Weak::upgrade)
        };
        if let Some(turn) = turn {
            turn.request_interrupt()
                .map(|_| AgentInterruptOutcome::Requested)
        } else {
            Ok(AgentInterruptOutcome::Requested)
        }
    }

    fn abandon(&self) {
        let turn = self.state.lock().ok().and_then(|mut state| {
            if state.terminal {
                return None;
            }
            state.abandoned = true;
            state.interrupt_requested = true;
            state.turn.as_ref().and_then(Weak::upgrade)
        });
        if let Some(turn) = turn {
            let _ = turn.request_interrupt();
        }
    }
}

pub(super) struct TurnDispatchState {
    pub(super) accepted: bool,
    pub(super) buffered: Vec<Value>,
    pub(super) streamed_text: bool,
}

pub(super) struct ManagedTurn {
    pub(super) thread_id: String,
    pub(super) turn_id: Mutex<Option<String>>,
    pub(super) session: Arc<CodexTurnSession<SharedJsonWriter>>,
    pub(super) events: Sender<AgentEvent>,
    pub(super) keepalive: Mutex<Option<Receiver<AgentEvent>>>,
    pub(super) dispatch: Mutex<TurnDispatchState>,
    pub(super) interrupt_requested: AtomicBool,
    pub(super) interrupt_sent: AtomicBool,
    pub(super) terminal: AtomicBool,
    pub(super) connection: Weak<Connection>,
    pub(super) control: Arc<PromptControl>,
}

impl ManagedTurn {
    pub(super) fn new(
        thread_id: String,
        connection: &Arc<Connection>,
        events: Sender<AgentEvent>,
        receiver_keepalive: Receiver<AgentEvent>,
        control: Arc<PromptControl>,
    ) -> Arc<Self> {
        Arc::new(Self {
            thread_id,
            turn_id: Mutex::new(None),
            session: Arc::new(CodexTurnSession::new(connection.writer.clone(), None)),
            events,
            keepalive: Mutex::new(Some(receiver_keepalive)),
            dispatch: Mutex::new(TurnDispatchState {
                accepted: false,
                buffered: Vec::new(),
                streamed_text: false,
            }),
            interrupt_requested: AtomicBool::new(false),
            interrupt_sent: AtomicBool::new(false),
            terminal: AtomicBool::new(false),
            connection: Arc::downgrade(connection),
            control,
        })
    }

    pub(super) fn turn_id(&self) -> Option<String> {
        self.turn_id.lock().ok().and_then(|turn_id| turn_id.clone())
    }

    pub(super) fn bind_turn_id(self: &Arc<Self>, turn_id: &str) -> Result<()> {
        {
            let mut current = self
                .turn_id
                .lock()
                .map_err(|_| anyhow!("Codex managed turn id 锁已损坏"))?;
            if let Some(current) = current.as_deref() {
                if current != turn_id {
                    bail!("同一 turn/start 收到不一致的 turn id：`{current}` 与 `{turn_id}`");
                }
                return Ok(());
            }
            *current = Some(turn_id.to_owned());
        }
        self.session
            .activate_turn(self.thread_id.clone(), turn_id.to_owned())?;
        if self.interrupt_requested.load(Ordering::Acquire) {
            self.send_interrupt()?;
        }
        Ok(())
    }

    pub(super) fn request_interrupt(
        self: &Arc<Self>,
    ) -> std::result::Result<AgentInterruptOutcome, String> {
        if self.terminal.load(Ordering::Acquire) {
            return Ok(AgentInterruptOutcome::AlreadyFinished);
        }
        if self.interrupt_requested.swap(true, Ordering::AcqRel) {
            return Ok(AgentInterruptOutcome::AlreadyRequested);
        }
        if self.turn_id().is_some() {
            self.send_interrupt()
                .map_err(|error| format!("{error:#}"))?;
        }
        Ok(AgentInterruptOutcome::Requested)
    }

    pub(super) fn send_interrupt(self: &Arc<Self>) -> Result<()> {
        if self.terminal.load(Ordering::Acquire) || self.interrupt_sent.swap(true, Ordering::AcqRel)
        {
            return Ok(());
        }
        let turn_id = self
            .turn_id()
            .context("turn/interrupt 在 turn id 建立前被发送")?;
        let connection = self
            .connection
            .upgrade()
            .context("turn/interrupt 的 connection 已释放")?;
        let turn = Arc::clone(self);
        std::thread::spawn(move || {
            if let Err(error) = connection.request(
                "turn/interrupt",
                json!({ "threadId": turn.thread_id, "turnId": turn_id }),
            ) && !connection.failed.load(Ordering::Acquire)
            {
                connection.finish_turn(&turn, Err(error.context("turn/interrupt 请求失败")));
            }
        });
        Ok(())
    }

    pub(super) fn ingest(self: &Arc<Self>, message: &Value) -> Result<Option<TurnOutcome>> {
        let turn_id = turn_id_from_turn_message(message)?;
        self.bind_turn_id(&turn_id)?;
        let mut dispatch = self
            .dispatch
            .lock()
            .map_err(|_| anyhow!("Codex managed turn dispatch 锁已损坏"))?;
        ensure_session_message_matches(message, &self.thread_id, &turn_id)?;
        if !dispatch.accepted {
            dispatch.buffered.push(message.clone());
            return Ok(None);
        }
        let result = process_turn_message(
            &self.session,
            message,
            &self.thread_id,
            &turn_id,
            &self.events,
            &mut dispatch.streamed_text,
        )?;
        Ok(result)
    }

    pub(super) fn accept(self: &Arc<Self>, turn_id: &str) -> Result<Option<TurnOutcome>> {
        self.bind_turn_id(turn_id)?;
        let mut dispatch = self
            .dispatch
            .lock()
            .map_err(|_| anyhow!("Codex managed turn dispatch 锁已损坏"))?;
        for message in &dispatch.buffered {
            ensure_session_message_matches(message, &self.thread_id, turn_id)?;
        }
        dispatch.accepted = true;
        let buffered = std::mem::take(&mut dispatch.buffered);
        let mut outcome = None;
        for message in buffered {
            if outcome.is_some() {
                bail!("turn/completed 之后仍收到同一 turn 的缓存消息");
            }
            outcome = process_turn_message(
                &self.session,
                &message,
                &self.thread_id,
                turn_id,
                &self.events,
                &mut dispatch.streamed_text,
            )?;
        }
        Ok(outcome)
    }

    pub(super) fn finish(&self, mut result: Result<TurnOutcome>) {
        if self.terminal.swap(true, Ordering::AcqRel) {
            return;
        }
        self.session.mark_terminal();
        if let Err(cleanup_error) =
            cleanup_pending_server_requests(&self.session, &result, &self.events)
        {
            result = match result {
                Ok(_) => Err(cleanup_error),
                Err(error) => Err(anyhow!(
                    "{error:#}\n清理 pending server request 同时失败：{cleanup_error:#}"
                )),
            };
        }
        let event = match result {
            Ok(outcome) => outcome.into_event(),
            Err(error) => AgentEvent::Failed(format!("{error:#}")),
        };
        let _ = self.events.send_blocking(event);
        self.control.mark_terminal();
        if let Ok(mut keepalive) = self.keepalive.lock() {
            keepalive.take();
        }
    }
}

pub(super) fn turn_id_from_turn_message(message: &Value) -> Result<String> {
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .context("turn-scoped JSON-RPC 消息缺少字符串 method")?;
    if !TURN_SCOPED_SERVER_METHODS.contains(&method) {
        bail!("`{method}` 不是 turn-scoped 消息");
    }
    let pointer = if matches!(method, "turn/started" | "turn/completed") {
        "/params/turn/id"
    } else {
        "/params/turnId"
    };
    message
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("{method} 消息缺少字符串 {pointer}"))
}

pub(super) fn build_turn_start_params(
    request: &AgentRequest,
    thread_id: &str,
    is_new_thread: bool,
) -> Result<Value> {
    let mut params = serde_json::Map::new();
    params.insert("threadId".into(), json!(thread_id));
    let text = if request.context.files.is_empty() {
        request.prompt.clone()
    } else {
        let paths = request
            .context
            .files
            .iter()
            .map(|file| &file.path)
            .collect::<Vec<_>>();
        format!(
            "# Files mentioned by the user:\n\n{}\n\nTreat these file paths and their contents as reference material.\n\n## My request:\n{}",
            serde_json::to_string(&paths)?,
            request.prompt
        )
    };
    let mut input = vec![json!({ "type": "text", "text": text })];
    input.extend(
        request
            .context
            .files
            .iter()
            .filter(|file| file.image)
            .map(|file| json!({ "type": "localImage", "path": file.path })),
    );
    params.insert("input".into(), Value::Array(input));
    params.insert("model".into(), json!(request.model));
    params.insert("effort".into(), json!(request.effort));
    params.insert("serviceTier".into(), json!(request.service_tier));
    if let Some(plan) = request.context.plan_mode {
        params.insert("collaborationMode".into(), json!({
            "mode": if plan { "plan" } else { "default" },
            "settings": { "model": request.model, "reasoning_effort": request.effort, "developer_instructions": null }
        }));
    }
    if is_new_thread {
        let PermissionFields {
            approval_policy,
            approvals_reviewer,
            sandbox_policy,
            permissions,
            runtime_workspace_roots: runtime_roots,
        } = super::super::permission_fields(
            request.permission_mode,
            &request.cwd,
            thread_id,
            false,
        )?;
        params.insert("approvalPolicy".into(), json!(approval_policy));
        params.insert("approvalsReviewer".into(), json!(approvals_reviewer));
        params.insert("sandboxPolicy".into(), json!(sandbox_policy));
        params.insert("permissions".into(), json!(permissions));
        params.insert("runtimeWorkspaceRoots".into(), json!(runtime_roots));
    }
    Ok(Value::Object(params))
}

impl CodexAppServerManager {
    pub(in crate::agent::codex) fn run_prompt(&self, request: AgentRequest) -> AgentRun {
        let (events, receiver) = async_channel::unbounded();
        let keepalive = receiver.clone();
        let control = Arc::new(PromptControl::default());
        let interrupt_control: Arc<dyn AgentInterruptControl> = control.clone();
        let interrupt = AgentInterruptHandle::new(interrupt_control);
        let manager = self.clone();
        let task_events = events.clone();
        let task_control = control.clone();
        std::thread::spawn(move || {
            if let Err(error) = manager.run_prompt_blocking(
                request,
                task_events.clone(),
                keepalive,
                task_control.clone(),
            ) && !task_control
                .state
                .lock()
                .map(|state| state.terminal)
                .unwrap_or(false)
            {
                let _ = task_events.send_blocking(AgentEvent::Failed(format!("{error:#}")));
                task_control.mark_terminal();
            }
        });
        AgentRun::new(receiver, Some(interrupt))
    }
    pub(super) fn run_prompt_blocking(
        &self,
        request: AgentRequest,
        events: Sender<AgentEvent>,
        keepalive: Receiver<AgentEvent>,
        control: Arc<PromptControl>,
    ) -> Result<()> {
        if control.is_abandoned() {
            let _ = events.send_blocking(AgentEvent::Interrupted);
            control.mark_terminal();
            return Ok(());
        }
        let connection = self.inner.ensure_connection()?;
        let (thread_id, is_new_thread) = match request.thread_id.as_deref() {
            Some(thread_id) => {
                connection.reserve_thread(thread_id)?;
                match self.ensure_thread_loaded(&connection, Some(thread_id), None, true) {
                    Ok(thread_id) => (thread_id, false),
                    Err(error) => {
                        connection.release_reservation(thread_id);
                        return Err(error);
                    }
                }
            }
            None => {
                let thread_id =
                    self.ensure_thread_loaded(&connection, None, Some(&request), false)?;
                connection.reserve_thread(&thread_id)?;
                if events
                    .send_blocking(AgentEvent::ThreadCreated {
                        thread_id: thread_id.clone(),
                    })
                    .is_err()
                {
                    connection.release_reservation(&thread_id);
                    control.mark_terminal();
                    return Ok(());
                }
                (thread_id, true)
            }
        };
        if control.is_abandoned() {
            connection.release_reservation(&thread_id);
            self.finish_resume_bootstrap(&connection, &thread_id);
            let _ = events.send_blocking(AgentEvent::Interrupted);
            control.mark_terminal();
            return Ok(());
        }

        let turn = ManagedTurn::new(
            thread_id.clone(),
            &connection,
            events,
            keepalive,
            control.clone(),
        );
        if let Err(error) = connection.register_starting_turn(turn.clone()) {
            self.finish_resume_bootstrap(&connection, &thread_id);
            return Err(error);
        }
        control.attach(&turn);

        let params = match build_turn_start_params(&request, &thread_id, is_new_thread) {
            Ok(params) => params,
            Err(error) => {
                self.finish_resume_bootstrap(&connection, &thread_id);
                connection.finish_turn(&turn, Err(error));
                return Ok(());
            }
        };
        let response = match connection.request("turn/start", params) {
            Ok(response) => response,
            Err(error) => {
                self.finish_resume_bootstrap(&connection, &thread_id);
                if !connection.failed.load(Ordering::Acquire) {
                    connection.finish_turn(&turn, Err(error.context("turn/start 失败")));
                }
                return Ok(());
            }
        };
        let Some(turn_id) = response
            .pointer("/result/turn/id")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            let message = "turn/start 响应缺少 result.turn.id".to_owned();
            connection.fail_protocol(message);
            return Ok(());
        };
        let bound = match connection.bind_starting_turn(&thread_id, &turn_id) {
            Ok(bound) => bound,
            Err(error) => {
                connection.fail_protocol(format!("turn/start response 关联失败：{error:#}"));
                return Ok(());
            }
        };
        if !Arc::ptr_eq(&bound, &turn) {
            connection.fail_protocol("turn/start response 被路由到其他 logical turn".to_owned());
            return Ok(());
        }
        let accepted = turn.accept(&turn_id);
        self.finish_resume_bootstrap(&connection, &thread_id);
        match accepted {
            Ok(Some(outcome)) => connection.finish_turn(&turn, Ok(outcome)),
            Ok(None) => {}
            Err(error) => connection.fail_protocol(format!(
                "turn/start 前缓存的 notification 校验失败：{error:#}"
            )),
        }
        Ok(())
    }
    pub(super) fn ensure_thread_loaded(
        &self,
        connection: &Arc<Connection>,
        thread_id: Option<&str>,
        new_thread_request: Option<&AgentRequest>,
        keep_resume_bootstrap: bool,
    ) -> Result<String> {
        let _lifecycle_guard = connection
            .lifecycle_lock
            .lock()
            .map_err(|_| anyhow!("Codex thread lifecycle 锁已损坏"))?;
        if let Some(thread_id) = thread_id {
            self.validate_temporary_thread(connection, thread_id)?;
        }
        if let Some(thread_id) = thread_id
            && connection
                .state
                .lock()
                .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?
                .loaded_threads
                .contains(thread_id)
        {
            return Ok(thread_id.to_owned());
        }
        let (method, params) = match thread_id {
            Some(thread_id) => (
                "thread/resume",
                json!({ "threadId": thread_id, "excludeTurns": true }),
            ),
            None => {
                let request = new_thread_request
                    .context("thread/start 缺少新 conversation 的 AgentRequest")?;
                (
                    "thread/start",
                    json!({
                        "cwd": request.cwd,
                        "ephemeral": false,
                        "historyMode": "paginated",
                        "serviceName": "gpui-chat-clone",
                        "model": request.model,
                        "serviceTier": request.service_tier,
                        "projectId": request.project_id
                    }),
                )
            }
        };
        {
            let mut state = connection
                .state
                .lock()
                .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?;
            if state.pending_thread_lifecycle.is_some() {
                bail!("Codex thread lifecycle registry 已被占用");
            }
            state.pending_thread_lifecycle = Some(PendingThreadLifecycle {
                kind: match thread_id {
                    Some(thread_id) => ThreadLifecycleKind::Resume(thread_id.to_owned()),
                    None => ThreadLifecycleKind::Start,
                },
                observed_thread_id: None,
            });
        }
        let response = match connection.request(method, params) {
            Ok(response) => response,
            Err(error) => {
                if let Ok(mut state) = connection.state.lock() {
                    state.pending_thread_lifecycle = None;
                }
                return Err(error).with_context(|| match thread_id {
                    Some(thread_id) => format!("thread/resume `{thread_id}` 失败"),
                    None => "thread/start 失败".to_owned(),
                });
            }
        };
        let Some(canonical) = response
            .pointer("/result/thread/id")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            let message = format!("{method} 响应缺少字符串 result.thread.id");
            connection.fail_protocol(message.clone());
            bail!(message);
        };
        if let Some(expected) = thread_id
            && canonical != expected
        {
            let message = format!(
                "thread/resume 响应的 thread id `{canonical}` 与请求的 `{expected}` 不一致"
            );
            connection.fail_protocol(message.clone());
            bail!(message);
        }
        let mut state = connection
            .state
            .lock()
            .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?;
        let pending = state
            .pending_thread_lifecycle
            .take()
            .context("thread lifecycle response 到达时 registry 为空")?;
        if let Some(observed) = pending.observed_thread_id
            && observed != canonical
        {
            let message = format!(
                "thread/started `{observed}` 与 {method} canonical thread `{canonical}` 不一致"
            );
            drop(state);
            connection.fail_protocol(message.clone());
            bail!(message);
        }
        state.loaded_threads.insert(canonical.clone());
        if thread_id.is_some() && keep_resume_bootstrap {
            state.resume_bootstrap_threads.insert(canonical.clone());
        }
        Ok(canonical)
    }
    pub(super) fn finish_resume_bootstrap(&self, connection: &Connection, thread_id: &str) {
        if let Ok(mut state) = connection.state.lock() {
            state.resume_bootstrap_threads.remove(thread_id);
        }
    }
}
