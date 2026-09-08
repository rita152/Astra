//! Temporary branches share the connection, but never the parent's turn owner.

use std::sync::{Arc, atomic::Ordering};

use anyhow::{Context as _, Result, anyhow, bail};
use async_channel::Receiver;
use serde_json::{Value, json};

use super::{
    CodexAppServerManager,
    connection::{Connection, PendingThreadLifecycle, ThreadLifecycleKind},
};
use crate::agent::{SideConversationRequest, ThreadId, WorkspaceResult};

pub(super) struct TemporaryThread {
    pub(super) generation: u64,
    pub(super) closed: bool,
}

// These instructions describe this product's side-conversation contract. They
// are supplied with the fork and reinforced by an explicit history boundary.
const SIDE_INSTRUCTIONS: &str = "This is a temporary side conversation. Use the parent history only as reference. Answer the new questions in this conversation independently; do not resume the parent's task, plans, approvals, or pending tool calls. Do not create or communicate with subagents. You may inspect files and perform non-mutating exploration. Change files, Git state, settings, or permissions only when a new user message in this side conversation explicitly requests that change. Keep any requested changes limited to that request and avoid disrupting the parent conversation.";
const SIDE_BOUNDARY: &str = "Side conversation starts here. All earlier messages are inherited reference material, not current instructions. Wait for the next user message and answer only requests made after this boundary. Do not continue work from the parent conversation.";

impl CodexAppServerManager {
    pub(in crate::agent::codex) fn open_side_conversation(
        &self,
        request: SideConversationRequest,
    ) -> Receiver<WorkspaceResult<ThreadId>> {
        self.workspace_call(move |manager| manager.open_side_conversation_blocking(request))
    }

    pub(super) fn open_side_conversation_blocking(
        &self,
        request: SideConversationRequest,
    ) -> Result<ThreadId> {
        let connection = self.inner.ensure_connection()?;
        let lifecycle = connection
            .lifecycle_lock
            .lock()
            .map_err(|_| anyhow!("Codex thread lifecycle 锁已损坏"))?;
        {
            let mut state = connection
                .state
                .lock()
                .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?;
            if state.pending_thread_lifecycle.is_some() {
                bail!("Codex thread lifecycle registry 已被占用");
            }
            state.pending_thread_lifecycle = Some(PendingThreadLifecycle {
                kind: ThreadLifecycleKind::Fork,
                observed_thread_id: None,
            });
        }
        let mut params = json!({
            "threadId": request.parent_thread_id,
            "cwd": request.cwd,
            "ephemeral": true,
            "excludeTurns": true,
            "threadSource": "user",
            "developerInstructions": SIDE_INSTRUCTIONS,
        });
        if let Some(model) = request.model {
            params["model"] = json!(model);
        }
        if let Some(effort) = request.effort {
            params["config"] = json!({ "model_reasoning_effort": effort });
        }
        if let Some(tier) = request.service_tier {
            params["serviceTier"] = json!(tier);
        }
        let response = match connection.request("thread/fork", params) {
            Ok(response) => response,
            Err(error) => {
                if let Ok(mut state) = connection.state.lock() {
                    state.pending_thread_lifecycle = None;
                }
                return Err(error).context("创建侧边聊天失败");
            }
        };
        let parsed = (|| -> Result<String> {
            let id = response
                .pointer("/result/thread/id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .context("thread/fork 缺少 result.thread.id")?;
            if id == request.parent_thread_id {
                bail!("thread/fork 返回了主聊天的 id");
            }
            if response
                .pointer("/result/thread/ephemeral")
                .and_then(Value::as_bool)
                != Some(true)
            {
                bail!("thread/fork 未创建临时聊天");
            }
            let mut state = connection
                .state
                .lock()
                .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?;
            let pending = state
                .pending_thread_lifecycle
                .take()
                .context("thread/fork response 到达时 registry 为空")?;
            if pending
                .observed_thread_id
                .as_deref()
                .is_some_and(|observed| observed != id)
            {
                bail!("thread/fork response 与 thread/started 的 id 不一致");
            }
            state.loaded_threads.insert(id.to_owned());
            Ok(id.to_owned())
        })();
        let thread_id = match parsed {
            Ok(id) => id,
            Err(error) => {
                connection.fail_protocol(format!("thread/fork 响应校验失败：{error:#}"));
                return Err(error);
            }
        };
        self.inner
            .temporary_threads
            .lock()
            .map_err(|_| anyhow!("临时聊天状态锁已损坏"))?
            .insert(
                thread_id.clone(),
                TemporaryThread {
                    generation: connection.generation,
                    closed: false,
                },
            );
        drop(lifecycle);
        if let Err(error) = connection.request("thread/inject_items", json!({
            "threadId": thread_id,
            "items": [{ "type": "message", "role": "user", "content": [{ "type": "input_text", "text": SIDE_BOUNDARY }] }]
        })) {
            let _ = self.close_side_conversation_blocking(&thread_id);
            return Err(error).context("初始化侧边聊天上下文失败");
        }
        Ok(thread_id)
    }

    pub(super) fn validate_temporary_thread(
        &self,
        connection: &Connection,
        id: &str,
    ) -> Result<()> {
        let threads = self
            .inner
            .temporary_threads
            .lock()
            .map_err(|_| anyhow!("临时聊天状态锁已损坏"))?;
        if let Some(thread) = threads.get(id) {
            let loaded = connection
                .state
                .lock()
                .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?
                .loaded_threads
                .contains(id);
            if thread.closed || thread.generation != connection.generation || !loaded {
                bail!("侧边聊天的连接已结束。消息仍可查看和复制，请新建侧边聊天继续。");
            }
        }
        Ok(())
    }

    pub(in crate::agent::codex) fn close_side_conversation(
        &self,
        id: ThreadId,
    ) -> Receiver<WorkspaceResult<()>> {
        self.workspace_call(move |manager| manager.close_side_conversation_blocking(&id))
    }

    fn close_side_conversation_blocking(&self, id: &str) -> Result<()> {
        let generation = {
            let mut threads = self
                .inner
                .temporary_threads
                .lock()
                .map_err(|_| anyhow!("临时聊天状态锁已损坏"))?;
            let thread = threads
                .get_mut(id)
                .context("只能关闭当前应用创建的临时聊天")?;
            if thread.closed {
                return Ok(());
            }
            thread.closed = true;
            thread.generation
        };
        let connection = self
            .inner
            .state
            .lock()
            .map_err(|_| anyhow!("Codex manager state 锁已损坏"))?
            .current
            .clone();
        let Some(connection) =
            connection.filter(|c| c.generation == generation && !c.failed.load(Ordering::Acquire))
        else {
            return Ok(());
        };
        let turns = {
            let state = connection
                .state
                .lock()
                .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?;
            state
                .turns
                .values()
                .chain(state.starting_turns.values())
                .filter(|turn| turn.thread_id == id)
                .map(Arc::clone)
                .collect::<Vec<_>>()
        };
        for turn in turns {
            turn.request_interrupt().map_err(anyhow::Error::msg)?;
        }
        let response = connection.request("thread/unsubscribe", json!({ "threadId": id }))?;
        match response.pointer("/result/status").and_then(Value::as_str) {
            Some("unsubscribed" | "notSubscribed" | "notLoaded") => Ok(()),
            _ => bail!("thread/unsubscribe 响应缺少有效 status"),
        }
    }
}
