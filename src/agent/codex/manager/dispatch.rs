//! Routing responses, server requests, and connection notifications.

use std::sync::{Arc, atomic::Ordering};

use anyhow::{Context as _, Result, anyhow, bail};
use serde_json::{Value, json};

use super::{
    super::{
        TURN_SCOPED_SERVER_METHODS, ensure_server_method_is_defined,
        is_integrated_server_request_method, parse_agent_notification,
        parse_mcp_server_startup_status_updated, parse_thread_status_changed,
        request_id_from_value, thread_started_id, validate_remote_control_status_changed,
        validate_resume_goal_cleared,
    },
    ManagerInner,
    connection::{Connection, PendingThreadLifecycle, ThreadLifecycleKind, TurnKey},
    protocol::{
        optional_nullable_param_string, required_nullable_param_string, required_param_string,
    },
    turn::turn_id_from_turn_message,
};
use crate::agent::{AgentConnectionEvent, AgentEvent, ProjectChange};

impl ManagerInner {
    pub(super) fn handle_message(
        &self,
        connection: &Arc<Connection>,
        message: &Value,
    ) -> Result<()> {
        if connection.failed.load(Ordering::Acquire) {
            return Ok(());
        }
        if message.get("method").is_none() {
            return connection.handle_response(message.clone());
        }
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .context("Codex JSON-RPC method 必须是字符串")?;
        if message.get("id").is_some() {
            return self.handle_server_request(connection, method, message);
        }
        self.handle_notification(connection, method, message)
    }
    pub(super) fn handle_server_request(
        &self,
        connection: &Arc<Connection>,
        method: &str,
        message: &Value,
    ) -> Result<()> {
        if !is_integrated_server_request_method(method) {
            connection.send_message(json!({
                "id": message.get("id").cloned().unwrap_or(Value::Null),
                "error": {
                    "code": -32601,
                    "message": "This client does not implement this server-initiated request"
                }
            }))?;
            return ensure_server_method_is_defined(message);
        }
        let thread_id = message
            .pointer("/params/threadId")
            .and_then(Value::as_str)
            .context("server request 缺少字符串 params.threadId")?;
        let turn_id = message
            .pointer("/params/turnId")
            .and_then(Value::as_str)
            .context("server request 缺少字符串 params.turnId")?;
        let turn = connection.bind_starting_turn(thread_id, turn_id)?;
        let request_id = request_id_from_value(
            message
                .get("id")
                .context("server request 缺少 JSON-RPC id")?,
        )?;
        let key = TurnKey {
            thread_id: thread_id.to_owned(),
            turn_id: turn_id.to_owned(),
        };
        connection.record_server_request_owner(request_id, key)?;
        if let Some(outcome) = turn.ingest(message)? {
            connection.finish_turn(&turn, Ok(outcome));
        }
        Ok(())
    }
    pub(super) fn handle_notification(
        &self,
        connection: &Arc<Connection>,
        method: &str,
        message: &Value,
    ) -> Result<()> {
        match method {
            "thread/started" => self.handle_thread_started(connection, message),
            "thread/goal/cleared" => self.handle_resume_goal_cleared(connection, message),
            "project/changed" => {
                let project_id = required_param_string(message, "projectId", method)?;
                let change = match required_param_string(message, "changeType", method)?.as_str() {
                    "created" => ProjectChange::Created,
                    "updated" => ProjectChange::Updated,
                    "deleted" => ProjectChange::Deleted,
                    value => bail!("project/changed 的 changeType 为未知值 `{value}`"),
                };
                self.publish_connection_event(AgentConnectionEvent::ProjectChanged {
                    project_id,
                    change,
                });
                Ok(())
            }
            "thread/archived" => {
                let thread_id = required_param_string(message, "threadId", method)?;
                self.publish_connection_event(AgentConnectionEvent::ThreadArchived { thread_id });
                Ok(())
            }
            "thread/unarchived" => {
                let thread_id = required_param_string(message, "threadId", method)?;
                self.publish_connection_event(AgentConnectionEvent::ThreadUnarchived { thread_id });
                Ok(())
            }
            "thread/deleted" => {
                let thread_id = required_param_string(message, "threadId", method)?;
                if let Ok(mut state) = connection.state.lock() {
                    state.loaded_threads.remove(&thread_id);
                }
                self.publish_connection_event(AgentConnectionEvent::ThreadDeleted { thread_id });
                Ok(())
            }
            "thread/name/updated" => {
                let thread_id = required_param_string(message, "threadId", method)?;
                let name = optional_nullable_param_string(message, "threadName", method)?;
                self.publish_connection_event(AgentConnectionEvent::ThreadNameUpdated {
                    thread_id,
                    name,
                });
                Ok(())
            }
            "thread/closed" => {
                let thread_id = required_param_string(message, "threadId", method)?;
                if let Ok(mut state) = connection.state.lock() {
                    state.loaded_threads.remove(&thread_id);
                }
                self.publish_connection_event(AgentConnectionEvent::ThreadClosed { thread_id });
                Ok(())
            }
            "thread/project/updated" => {
                let thread_id = required_param_string(message, "threadId", method)?;
                let project_id = required_nullable_param_string(message, "projectId", method)?;
                self.publish_connection_event(AgentConnectionEvent::ThreadProjectUpdated {
                    thread_id,
                    project_id,
                });
                Ok(())
            }
            "serverRequest/resolved" => {
                let request_id = request_id_from_value(
                    message
                        .pointer("/params/requestId")
                        .context("serverRequest/resolved 缺少 params.requestId")?,
                )?;
                let owner = connection.server_request_owner(&request_id)?;
                let notification_thread = message
                    .pointer("/params/threadId")
                    .and_then(Value::as_str)
                    .context("serverRequest/resolved 缺少字符串 params.threadId")?;
                if notification_thread != owner.thread_id {
                    bail!(
                        "serverRequest/resolved threadId `{notification_thread}` 与 request owner `{}` 不一致",
                        owner.thread_id
                    );
                }
                let turn = connection.turn_for_key(&owner)?;
                let mut dispatch = turn
                    .dispatch
                    .lock()
                    .map_err(|_| anyhow!("Codex managed turn dispatch 锁已损坏"))?;
                if !dispatch.accepted {
                    dispatch.buffered.push(message.clone());
                    return Ok(());
                }
                super::super::handle_server_request_resolved(&turn.session, message, &turn.events)
            }
            "thread/settings/updated" => {
                let thread_id = message
                    .pointer("/params/threadId")
                    .and_then(Value::as_str)
                    .context("thread/settings/updated 缺少字符串 params.threadId")?
                    .to_owned();
                let Some(AgentEvent::ThreadSettingsUpdated(settings)) =
                    parse_agent_notification(message)?
                else {
                    bail!("thread/settings/updated 未映射为 AgentThreadSettings");
                };
                if settings.permissions.is_some() {
                    let waiters = connection
                        .state
                        .lock()
                        .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?
                        .settings_waiters
                        .remove(&thread_id)
                        .unwrap_or_default();
                    for waiter in waiters {
                        let _ = waiter.send_blocking(Ok(settings.clone()));
                    }
                }
                self.publish_connection_event(AgentConnectionEvent::ThreadSettingsUpdated {
                    thread_id,
                    settings,
                });
                Ok(())
            }
            "mcpServer/startupStatus/updated" => {
                self.publish_connection_event(AgentConnectionEvent::McpServerStartupStatusUpdated(
                    parse_mcp_server_startup_status_updated(message)?,
                ));
                Ok(())
            }
            "thread/status/changed" => {
                self.publish_connection_event(AgentConnectionEvent::ThreadStatusChanged(
                    parse_thread_status_changed(message)?,
                ));
                Ok(())
            }
            "account/rateLimits/updated" => {
                let Some(AgentEvent::AccountRateLimitsUpdated(rate_limits)) =
                    parse_agent_notification(message)?
                else {
                    bail!("account/rateLimits/updated 未映射为 AgentAccountRateLimits");
                };
                self.publish_connection_event(AgentConnectionEvent::AccountRateLimitsUpdated(
                    rate_limits,
                ));
                Ok(())
            }
            "warning" => {
                let thread_id = match message.pointer("/params/threadId") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(thread_id)) => Some(thread_id.clone()),
                    Some(_) => bail!("warning params.threadId 必须是字符串或 null"),
                };
                let Some(AgentEvent::Warning { message }) = parse_agent_notification(message)?
                else {
                    bail!("warning 未映射为 Agent warning");
                };
                self.publish_connection_event(AgentConnectionEvent::Warning { thread_id, message });
                Ok(())
            }
            "configWarning" => {
                let Some(AgentEvent::ConfigWarning(warning)) = parse_agent_notification(message)?
                else {
                    bail!("configWarning 未映射为 AgentConfigWarning");
                };
                self.publish_connection_event(AgentConnectionEvent::ConfigWarning(warning));
                Ok(())
            }
            "remoteControl/status/changed" => {
                validate_remote_control_status_changed(message)?;
                connection
                    .state
                    .lock()
                    .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?
                    .remote_control_status = message.get("params").cloned();
                Ok(())
            }
            method if TURN_SCOPED_SERVER_METHODS.contains(&method) => {
                let thread_id = message
                    .pointer("/params/threadId")
                    .and_then(Value::as_str)
                    .with_context(|| format!("{method} 消息缺少字符串 params.threadId"))?;
                let turn_id = turn_id_from_turn_message(message)?;
                let finished = connection
                    .state
                    .lock()
                    .map_err(|_| anyhow!("轮次注册表锁不可用"))?
                    .finished_turns
                    .contains(&super::connection::TurnKey {
                        thread_id: thread_id.to_owned(),
                        turn_id: turn_id.clone(),
                    });
                if finished {
                    return Ok(());
                }
                let turn = connection.bind_starting_turn(thread_id, &turn_id)?;
                if let Some(outcome) = turn.ingest(message)? {
                    connection.finish_turn(&turn, Ok(outcome));
                }
                Ok(())
            }
            _ => ensure_server_method_is_defined(message),
        }
    }
    pub(super) fn handle_thread_started(
        &self,
        connection: &Connection,
        message: &Value,
    ) -> Result<()> {
        let thread_id = thread_started_id(message)?;
        let mut state = connection
            .state
            .lock()
            .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?;
        if state.loaded_threads.contains(&thread_id) {
            return Ok(());
        }
        if let Some(pending) = state.pending_thread_lifecycle.as_mut() {
            if let ThreadLifecycleKind::Resume(expected) = &pending.kind
                && expected != &thread_id
            {
                bail!("thread/resume `{expected}` 收到其他 thread 的 thread/started `{thread_id}`");
            }
            if let Some(observed) = &pending.observed_thread_id
                && observed != &thread_id
            {
                bail!(
                    "同一 thread lifecycle 收到不一致的 thread/started：`{observed}` 与 `{thread_id}`"
                );
            }
            pending.observed_thread_id = Some(thread_id);
            return Ok(());
        }
        bail!("收到未关联 lifecycle 的 thread/started `{thread_id}`")
    }
    pub(super) fn handle_resume_goal_cleared(
        &self,
        connection: &Connection,
        message: &Value,
    ) -> Result<()> {
        let thread_id = message
            .pointer("/params/threadId")
            .and_then(Value::as_str)
            .context("thread/goal/cleared 缺少字符串 params.threadId")?;
        let state = connection
            .state
            .lock()
            .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?;
        let expected = if state.resume_bootstrap_threads.contains(thread_id) {
            thread_id.to_owned()
        } else {
            match state.pending_thread_lifecycle.as_ref() {
                Some(PendingThreadLifecycle {
                    kind: ThreadLifecycleKind::Resume(expected),
                    ..
                }) => expected.clone(),
                _ => bail!("thread/goal/cleared 仅允许出现在 thread/resume bootstrap 阶段"),
            }
        };
        drop(state);
        validate_resume_goal_cleared(message, &expected)
    }
    pub(super) fn publish_connection_event(&self, event: AgentConnectionEvent) {
        if let Ok(mut hub) = self.connection_events.lock() {
            hub.publish(event);
        }
    }
}
