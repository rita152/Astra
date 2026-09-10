//! Per-thread permission operation queues and notification confirmation.
use super::{
    CodexAppServerManager,
    connection::{Connection, PendingThreadLifecycle, ThreadLifecycleKind},
};
use crate::agent::{
    AgentConnectionEvent, AgentPermissionMode, AgentThreadPermissionResult,
    AgentThreadPermissionUpdate, AgentThreadSettings,
};
use anyhow::{Context as _, Result, anyhow, bail};
use async_channel::{Receiver, Sender};
use serde_json::{Value, json};
use std::{
    sync::{Arc, mpsc},
    time::Duration,
};

pub(super) struct QueuedPermissionUpdate {
    request: AgentThreadPermissionUpdate,
    reply: Sender<Result<AgentThreadPermissionResult, String>>,
}

pub(super) struct SettingsWaiter {
    pub(super) expected: Value,
    pub(super) sender: mpsc::SyncSender<Result<AgentThreadSettings, String>>,
    pub(super) observed: Option<AgentThreadSettings>,
}

pub(super) fn settings_match(expected: &Value, settings: &AgentThreadSettings) -> bool {
    let Some(permissions) = &settings.permissions else {
        return false;
    };
    if let Some(policy) = expected
        .get("approvalPolicy")
        .filter(|value| !value.is_null())
        && !approval_policies_match(policy, &permissions.approval_policy)
    {
        return false;
    }
    if let Some(reviewer) = expected.get("approvalsReviewer").and_then(Value::as_str) {
        let normalize = |s| {
            if s == "guardian_subagent" {
                "auto_review"
            } else {
                s
            }
        };
        if normalize(reviewer) != normalize(&permissions.approvals_reviewer) {
            return false;
        }
    }
    if let Some(profile) = expected.get("permissions").and_then(Value::as_str)
        && permissions
            .active_permission_profile
            .as_ref()
            .is_none_or(|active| active.id != profile)
    {
        return false;
    }
    if let Some(sandbox) = expected
        .get("sandboxPolicy")
        .filter(|value| !value.is_null())
        && permissions
            .sandbox_policy
            .as_ref()
            .is_none_or(|actual| !json_contains(actual, sandbox))
    {
        return false;
    }
    true
}

fn approval_policies_match(expected: &Value, actual: &Value) -> bool {
    fn normalized(policy: &Value) -> Value {
        let mut value = policy.clone();
        if let Some(granular) = value.get_mut("granular").and_then(Value::as_object_mut) {
            for key in ["request_permissions", "skill_approval"] {
                granular.entry(key).or_insert(json!(false));
            }
        }
        value
    }
    normalized(expected) == normalized(actual)
}

fn json_contains(actual: &Value, expected: &Value) -> bool {
    match expected {
        Value::Object(fields) => fields.iter().all(|(key, value)| {
            actual
                .get(key)
                .is_some_and(|actual| json_contains(actual, value))
        }),
        _ => actual == expected,
    }
}

impl CodexAppServerManager {
    pub(in crate::agent::codex) fn load_thread_settings(
        &self,
        thread_id: String,
        generation: u64,
    ) -> Receiver<Result<crate::agent::AgentThreadSettingsSnapshot, String>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<crate::agent::AgentThreadSettingsSnapshot> {
                let connection = manager.inner.ensure_connection()?;
                if connection.generation != generation {
                    bail!("连接已变化，请重新读取权限配置");
                }
                manager.ensure_thread_loaded(&connection, Some(&thread_id), None, false)?;
                let settings = connection
                    .state
                    .lock()
                    .map_err(|_| anyhow!("连接状态不可用"))?
                    .thread_settings
                    .get(&thread_id)
                    .cloned()
                    .context("服务端未提供线程有效设置")?;
                manager.validate_temporary_thread(&connection, &thread_id)?;
                if connection.failed.load(std::sync::atomic::Ordering::Acquire) {
                    bail!("读取有效权限时连接已关闭");
                }
                Ok(crate::agent::AgentThreadSettingsSnapshot {
                    thread_id,
                    generation,
                    settings,
                })
            })()
            .map_err(|error| format!("{error:#}"));
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    pub(in crate::agent::codex) fn update_thread_permissions(
        &self,
        request: AgentThreadPermissionUpdate,
    ) -> Receiver<Result<AgentThreadPermissionResult, String>> {
        let (reply, receiver) = async_channel::bounded(1);
        let mut queues = match self.inner.permission_queues.lock() {
            Ok(queues) => queues,
            Err(_) => {
                let _ = reply.try_send(Err("权限更新队列不可用".into()));
                return receiver;
            }
        };
        let sender = queues.entry(request.thread_id.clone()).or_insert_with(|| {
            let (sender, receiver) = async_channel::unbounded::<QueuedPermissionUpdate>();
            let manager = Arc::downgrade(&self.inner);
            std::thread::spawn(move || {
                while let Ok(queued) = receiver.recv_blocking() {
                    let Some(inner) = manager.upgrade() else {
                        let _ = queued.reply.send_blocking(Err("权限更新连接已关闭".into()));
                        break;
                    };
                    let manager = CodexAppServerManager { inner };
                    let result = manager
                        .update_thread_permissions_blocking(queued.request)
                        .map_err(|error| format!("{error:#}"));
                    let _ = queued.reply.send_blocking(result);
                }
            });
            sender
        });
        if let Err(error) = sender.try_send(QueuedPermissionUpdate { request, reply }) {
            let queued = error.into_inner();
            let _ = queued.reply.try_send(Err("权限更新队列已关闭".into()));
        }
        receiver
    }

    pub(super) fn update_thread_permissions_blocking(
        &self,
        request: AgentThreadPermissionUpdate,
    ) -> Result<AgentThreadPermissionResult> {
        let connection = self.inner.ensure_connection()?;
        if request
            .expected_generation
            .is_some_and(|generation| generation != connection.generation)
        {
            bail!("权限配置的连接已变化，请重新读取权限列表后再试");
        }
        self.validate_temporary_thread(&connection, &request.thread_id)?;
        self.ensure_thread_loaded(&connection, Some(&request.thread_id), None, false)?;
        let params = if request.mode == AgentPermissionMode::Custom {
            self.resolve_default_permission_params(&connection, &request.cwd, &request.thread_id)?
        } else {
            super::super::thread_settings_update_request(
                0,
                &request.thread_id,
                &request.cwd,
                request.mode.clone(),
            )?["params"]
                .clone()
        };
        // The service decides profile availability. Never synthesize an allowed
        // profile or bypass a managed allowlist for one of the legacy modes.
        if let Some(profile) = params.get("permissions").and_then(Value::as_str) {
            let profiles =
                super::super::catalog::permission_profile_pages(&request.cwd, |params| {
                    connection.request("permissionProfile/list", params)
                })?;
            if profiles
                .iter()
                .find(|entry| entry.id == profile)
                .is_none_or(|entry| !entry.allowed)
            {
                bail!("服务端不允许使用权限配置 {profile}");
            }
        }
        self.validate_temporary_thread(&connection, &request.thread_id)?;
        let (sender, receiver) = mpsc::sync_channel(1);
        connection
            .state
            .lock()
            .map_err(|_| anyhow!("连接状态不可用"))?
            .settings_waiters
            .insert(
                request.thread_id.clone(),
                SettingsWaiter {
                    expected: params.clone(),
                    sender,
                    observed: None,
                },
            );
        let response = connection.request("thread/settings/update", params);
        if let Err(error) = response {
            connection
                .state
                .lock()
                .map_err(|_| anyhow!("连接状态不可用"))?
                .settings_waiters
                .remove(&request.thread_id);
            return Err(error);
        }
        let settings = match receiver.recv_timeout(Duration::from_secs(20)) {
            Ok(Ok(settings)) => settings,
            Ok(Err(error)) => {
                connection
                    .state
                    .lock()
                    .map_err(|_| anyhow!("连接状态不可用"))?
                    .settings_waiters
                    .remove(&request.thread_id);
                return Err(anyhow!(error));
            }
            Err(_) => {
                let message =
                    "权限请求已返回，但未收到匹配的有效权限通知；结果未确认，请重新连接后核对";
                connection.fail_protocol(message.into());
                bail!(message);
            }
        };
        self.validate_temporary_thread(&connection, &request.thread_id)?;
        if connection.failed.load(std::sync::atomic::Ordering::Acquire) {
            bail!("确认权限前连接已关闭");
        }
        {
            let mut state = connection
                .state
                .lock()
                .map_err(|_| anyhow!("连接状态不可用"))?;
            state.settings_waiters.remove(&request.thread_id);
            state
                .thread_settings
                .insert(request.thread_id.clone(), settings.clone());
            let history = state
                .confirmed_settings
                .entry(request.thread_id.clone())
                .or_default();
            if history.back() != Some(&settings) {
                history.push_back(settings.clone());
            }
            while history.len() > 32 {
                history.pop_front();
            }
        }
        self.inner
            .publish_connection_event(AgentConnectionEvent::ThreadSettingsUpdated {
                thread_id: request.thread_id.clone(),
                generation: connection.generation,
                settings: settings.clone(),
            });
        Ok(AgentThreadPermissionResult {
            thread_id: request.thread_id,
            generation: connection.generation,
            operation_id: request.operation_id,
            settings,
        })
    }

    /// Resolve missing/inherited defaults with the same server thread/start
    /// path as a new chat. This in-memory probe never starts a turn and is
    /// unsubscribed before returning; no client-side TOML merge/default guesses.
    fn resolve_default_permission_params(
        &self,
        connection: &Arc<Connection>,
        cwd: &std::path::Path,
        target: &str,
    ) -> Result<Value> {
        let _guard = connection
            .lifecycle_lock
            .lock()
            .map_err(|_| anyhow!("线程生命周期锁不可用"))?;
        connection
            .state
            .lock()
            .map_err(|_| anyhow!("连接状态不可用"))?
            .pending_thread_lifecycle = Some(PendingThreadLifecycle {
            kind: ThreadLifecycleKind::Start,
            observed_thread_id: None,
        });
        let response = match connection.request("thread/start", json!({"cwd":cwd,"ephemeral":true}))
        {
            Ok(response) => response,
            Err(error) => {
                if let Ok(mut state) = connection.state.lock() {
                    state.pending_thread_lifecycle = None;
                }
                return Err(error);
            }
        };
        let parsed = (|| -> Result<(&Value, &str)> {
            let result = response
                .get("result")
                .context("权限默认值响应缺少 result")?;
            let id = result
                .pointer("/thread/id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .context("权限默认值响应缺少 thread.id")?;
            let mut state = connection
                .state
                .lock()
                .map_err(|_| anyhow!("连接状态不可用"))?;
            let pending = state
                .pending_thread_lifecycle
                .take()
                .context("权限默认值生命周期登记缺失")?;
            if id == target
                || result.pointer("/thread/ephemeral").and_then(Value::as_bool) != Some(true)
                || pending
                    .observed_thread_id
                    .is_some_and(|observed| observed != id)
            {
                bail!("权限默认值解析线程身份不匹配");
            }
            // Swap pending identity for a bounded, generation-local tombstone
            // atomically: thread/started can follow the RPC or unsubscribe.
            state.permission_probe_threads.push_back(id.to_owned());
            while state.permission_probe_threads.len() > 64 {
                state.permission_probe_threads.pop_front();
            }
            Ok((result, id))
        })();
        let (result, id) = match parsed {
            Ok(parsed) => parsed,
            Err(error) => {
                connection.fail_protocol(format!("无法确认临时权限解析线程：{error:#}"));
                return Err(error);
            }
        };
        // Ensure cleanup happens even if a required response field is malformed.
        let decoded = (|| -> Result<Value> {
            let mut params = json!({"threadId":target,
                "approvalPolicy":result.get("approvalPolicy").context("缺少有效 approvalPolicy")?,
                "approvalsReviewer":result.get("approvalsReviewer").context("缺少有效 approvalsReviewer")?});
            if let Some(profile) = result
                .pointer("/activePermissionProfile/id")
                .and_then(Value::as_str)
            {
                params["permissions"] = json!(profile);
            } else {
                params["sandboxPolicy"] =
                    result.get("sandbox").context("缺少有效 sandbox")?.clone();
            }
            Ok(params)
        })();
        let closed = connection.request("thread/unsubscribe", json!({"threadId":id}));
        connection
            .state
            .lock()
            .map_err(|_| anyhow!("连接状态不可用"))?
            .loaded_threads
            .remove(id);
        match closed {
            Ok(response)
                if matches!(
                    response.pointer("/result/status").and_then(Value::as_str),
                    Some("unsubscribed" | "notSubscribed" | "notLoaded")
                ) => {}
            result => {
                let error = result
                    .err()
                    .unwrap_or_else(|| anyhow!("thread/unsubscribe 响应缺少有效 status"));
                connection.fail_protocol(format!("无法回收权限解析线程：{error:#}"));
                return Err(error);
            }
        }
        decoded
    }
}
