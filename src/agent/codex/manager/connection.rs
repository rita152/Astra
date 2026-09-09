//! Connection-scoped RPC registry, thread reservations, and failure cleanup.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use anyhow::{Context as _, Result, anyhow, bail};
use async_channel::Sender;
use serde_json::{Value, json};

use super::{
    super::TurnOutcome,
    ManagerInner,
    transport::{ManagedProcess, SharedJsonWriter},
    turn::ManagedTurn,
};
use crate::agent::{AgentServerRequestId, AgentThreadSettings};

pub(super) struct PendingRpc {
    pub(super) method: String,
    pub(super) sender: Sender<Result<Value, String>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct TurnKey {
    pub(super) thread_id: String,
    pub(super) turn_id: String,
}

pub(super) enum ThreadLifecycleKind {
    Start,
    Resume(String),
    Fork,
}

pub(super) struct PendingThreadLifecycle {
    pub(super) kind: ThreadLifecycleKind,
    pub(super) observed_thread_id: Option<String>,
}

#[derive(Default)]
pub(super) struct ConnectionState {
    pub(super) loaded_threads: HashSet<String>,
    pub(super) pending_thread_lifecycle: Option<PendingThreadLifecycle>,
    pub(super) resume_bootstrap_threads: HashSet<String>,
    pub(super) reserved_threads: HashSet<String>,
    pub(super) starting_turns: HashMap<String, Arc<ManagedTurn>>,
    pub(super) turns: HashMap<TurnKey, Arc<ManagedTurn>>,
    pub(super) server_request_owners: HashMap<AgentServerRequestId, TurnKey>,
    resolved_server_requests: HashMap<AgentServerRequestId, String>,
    resolved_server_request_order: VecDeque<AgentServerRequestId>,
    pub(super) settings_waiters: HashMap<String, Vec<Sender<Result<AgentThreadSettings, String>>>>,
    pub(super) remote_control_status: Option<Value>,
}

impl ConnectionState {
    fn remember_resolved_request(&mut self, request_id: AgentServerRequestId, thread_id: String) {
        // Retain only lightweight tombstones for late/duplicate notifications;
        // never retain a turn, responder, command, or patch after resolution.
        const RESOLVED_REQUEST_LIMIT: usize = 4096;
        if self
            .resolved_server_requests
            .insert(request_id.clone(), thread_id)
            .is_none()
        {
            self.resolved_server_request_order.push_back(request_id);
        }
        while self.resolved_server_request_order.len() > RESOLVED_REQUEST_LIMIT {
            if let Some(id) = self.resolved_server_request_order.pop_front() {
                self.resolved_server_requests.remove(&id);
            }
        }
    }
}

pub(super) struct Connection {
    pub(super) generation: u64,
    pub(super) writer: SharedJsonWriter,
    pub(super) process: Arc<dyn ManagedProcess>,
    pub(super) next_request_id: AtomicU64,
    pub(super) pending_rpcs: Mutex<HashMap<u64, PendingRpc>>,
    pub(super) state: Mutex<ConnectionState>,
    pub(super) lifecycle_lock: Mutex<()>,
    pub(super) settings_lock: Mutex<()>,
    pub(super) failed: AtomicBool,
    pub(super) manager: Weak<ManagerInner>,
}

impl Connection {
    pub(super) fn send_message(&self, message: Value) -> Result<()> {
        let mut writer = self.writer.clone();
        super::super::send(&mut writer, message)
    }

    pub(super) fn request(&self, method: &str, params: Value) -> Result<Value> {
        if self.failed.load(Ordering::Acquire) {
            bail!("Codex app-server connection generation 已失败");
        }
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        if request_id == u64::MAX {
            let message = "Codex JSON-RPC request id 已耗尽".to_owned();
            self.fail_protocol(message.clone());
            bail!(message);
        }
        let (sender, receiver) = async_channel::bounded(1);
        self.pending_rpcs
            .lock()
            .map_err(|_| anyhow!("Codex pending request registry 锁已损坏"))?
            .insert(
                request_id,
                PendingRpc {
                    method: method.to_owned(),
                    sender,
                },
            );
        if let Err(error) = self.send_message(json!({
            "method": method,
            "id": request_id,
            "params": params
        })) {
            if let Ok(mut pending) = self.pending_rpcs.lock() {
                pending.remove(&request_id);
            }
            return Err(error).with_context(|| format!("写入 `{method}` 请求失败"));
        }
        receiver
            .recv_blocking()
            .map_err(|_| anyhow!("`{method}` response channel 在返回前关闭"))?
            .map_err(anyhow::Error::msg)
    }

    pub(super) fn handle_response(&self, message: Value) -> Result<()> {
        let request_id = message
            .get("id")
            .and_then(Value::as_u64)
            .context("Codex JSON-RPC response id 必须是 uint64")?;
        let pending = self
            .pending_rpcs
            .lock()
            .map_err(|_| anyhow!("Codex pending request registry 锁已损坏"))?
            .remove(&request_id)
            .with_context(|| format!("收到未知或重复的 JSON-RPC response id `{request_id}`"))?;
        let (result, fatal_error) = match (message.get("result"), message.get("error")) {
            (Some(_), None) => (Ok(message), None),
            (None, Some(error)) => (
                Err(format!(
                    "Codex JSON-RPC `{}` 请求 {request_id} 失败：{error}",
                    pending.method
                )),
                None,
            ),
            (Some(_), Some(_)) => {
                let error =
                    format!("Codex JSON-RPC response {request_id} 同时包含 result 与 error");
                (Err(error.clone()), Some(error))
            }
            (None, None) => {
                let error = format!("Codex JSON-RPC response {request_id} 缺少 result 或 error");
                (Err(error.clone()), Some(error))
            }
        };
        let _ = pending.sender.send_blocking(result);
        if let Some(error) = fatal_error {
            bail!(error);
        }
        Ok(())
    }

    pub(super) fn fail_protocol(&self, message: String) {
        if let Some(manager) = self.manager.upgrade() {
            manager.fail_generation(self.generation, message);
        }
    }

    pub(super) fn reserve_thread(&self, thread_id: &str) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?;
        if state.reserved_threads.contains(thread_id)
            || state.starting_turns.contains_key(thread_id)
            || state.turns.keys().any(|key| key.thread_id == thread_id)
        {
            bail!("thread `{thread_id}` 已有 active turn，不能并发启动新的 turn");
        }
        state.reserved_threads.insert(thread_id.to_owned());
        Ok(())
    }

    pub(super) fn release_reservation(&self, thread_id: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.reserved_threads.remove(thread_id);
        }
    }

    pub(super) fn register_starting_turn(&self, turn: Arc<ManagedTurn>) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?;
        state.reserved_threads.remove(&turn.thread_id);
        if state.starting_turns.contains_key(&turn.thread_id)
            || state
                .turns
                .keys()
                .any(|key| key.thread_id == turn.thread_id)
        {
            bail!(
                "thread `{}` 已有 active turn，不能覆盖 registry",
                turn.thread_id
            );
        }
        state.starting_turns.insert(turn.thread_id.clone(), turn);
        Ok(())
    }

    pub(super) fn bind_starting_turn(
        &self,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<Arc<ManagedTurn>> {
        let key = TurnKey {
            thread_id: thread_id.to_owned(),
            turn_id: turn_id.to_owned(),
        };
        let turn = {
            let state = self
                .state
                .lock()
                .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?;
            if let Some(turn) = state.turns.get(&key) {
                return Ok(turn.clone());
            }
            state
                .starting_turns
                .get(thread_id)
                .cloned()
                .with_context(|| {
                    format!("收到未知 turn 的消息：threadId=`{thread_id}`，turnId=`{turn_id}`")
                })?
        };
        turn.bind_turn_id(turn_id)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?;
        if let Some(existing) = state.turns.get(&key) {
            if Arc::ptr_eq(existing, &turn) {
                return Ok(turn);
            }
            bail!("turn registry key `{thread_id}`/`{turn_id}` 已被其他 turn 占用");
        }
        if state
            .starting_turns
            .get(thread_id)
            .is_some_and(|candidate| Arc::ptr_eq(candidate, &turn))
        {
            state.starting_turns.remove(thread_id);
        }
        state.turns.insert(key, turn.clone());
        Ok(turn)
    }

    pub(super) fn turn_for_key(&self, key: &TurnKey) -> Result<Arc<ManagedTurn>> {
        self.state
            .lock()
            .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?
            .turns
            .get(key)
            .cloned()
            .with_context(|| {
                format!(
                    "serverRequest/resolved 指向未知 turn：threadId=`{}`，turnId=`{}`",
                    key.thread_id, key.turn_id
                )
            })
    }

    pub(super) fn record_server_request_owner(
        &self,
        request_id: AgentServerRequestId,
        key: TurnKey,
    ) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?;
        if state.resolved_server_requests.contains_key(&request_id) {
            bail!("Codex server request id {request_id:?} 已经 resolved，拒绝重复请求");
        }
        if let Some(existing) = state.server_request_owners.get(&request_id)
            && existing != &key
        {
            bail!(
                "Codex server request id {request_id:?} 已属于其他 turn `{}`/`{}`",
                existing.thread_id,
                existing.turn_id
            );
        }
        state.server_request_owners.insert(request_id, key);
        Ok(())
    }

    pub(super) fn resolve_server_request_owner(
        &self,
        request_id: &AgentServerRequestId,
        thread_id: &str,
    ) -> Result<Option<TurnKey>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?;
        let owner = state.server_request_owners.get(request_id).cloned();
        let expected_thread = owner
            .as_ref()
            .map(|owner| &owner.thread_id)
            .or_else(|| state.resolved_server_requests.get(request_id))
            .with_context(|| format!("serverRequest/resolved 引用了未知 request {request_id:?}"))?;
        if expected_thread != thread_id {
            bail!(
                "serverRequest/resolved threadId `{thread_id}` 与 request owner `{expected_thread}` 不一致"
            );
        }
        if owner.is_some() {
            state.server_request_owners.remove(request_id);
            state.remember_resolved_request(request_id.clone(), thread_id.to_owned());
        }
        Ok(owner)
    }

    pub(super) fn finish_turn(&self, turn: &Arc<ManagedTurn>, result: Result<TurnOutcome>) {
        turn.finish(result);
        if let Ok(mut state) = self.state.lock() {
            state.reserved_threads.remove(&turn.thread_id);
            if state
                .starting_turns
                .get(&turn.thread_id)
                .is_some_and(|candidate| Arc::ptr_eq(candidate, turn))
            {
                state.starting_turns.remove(&turn.thread_id);
            }
            state
                .turns
                .retain(|_, candidate| !Arc::ptr_eq(candidate, turn));
            let owned_keys = state
                .server_request_owners
                .iter()
                .filter_map(|(request_id, key)| {
                    (key.thread_id == turn.thread_id
                        && turn
                            .turn_id()
                            .as_ref()
                            .is_some_and(|turn_id| turn_id == &key.turn_id))
                    .then_some(request_id.clone())
                })
                .collect::<Vec<_>>();
            for request_id in owned_keys {
                state.server_request_owners.remove(&request_id);
                state.remember_resolved_request(request_id, turn.thread_id.clone());
            }
        }
    }

    pub(super) fn fail_all(&self, message: &str) {
        let pending = self
            .pending_rpcs
            .lock()
            .map(|mut pending| std::mem::take(&mut *pending))
            .unwrap_or_default();
        for (_, request) in pending {
            let _ = request.sender.send_blocking(Err(message.to_owned()));
        }

        let (turns, settings_waiters) = self
            .state
            .lock()
            .map(|mut state| {
                let mut turns = state
                    .starting_turns
                    .drain()
                    .map(|(_, turn)| turn)
                    .collect::<Vec<_>>();
                turns.extend(state.turns.drain().map(|(_, turn)| turn));
                turns.sort_by_key(|turn| Arc::as_ptr(turn) as usize);
                turns.dedup_by(|left, right| Arc::ptr_eq(left, right));
                state.loaded_threads.clear();
                state.pending_thread_lifecycle = None;
                state.resume_bootstrap_threads.clear();
                state.reserved_threads.clear();
                state.server_request_owners.clear();
                state.resolved_server_requests.clear();
                state.resolved_server_request_order.clear();
                let settings_waiters = std::mem::take(&mut state.settings_waiters);
                (turns, settings_waiters)
            })
            .unwrap_or_default();
        for turn in turns {
            turn.finish(Err(anyhow!(message.to_owned())));
        }
        for (_, waiters) in settings_waiters {
            for waiter in waiters {
                let _ = waiter.send_blocking(Err(message.to_owned()));
            }
        }
    }
}
