//! Model catalogs and thread permission settings over the shared connection.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, anyhow, bail};
use async_channel::Receiver;
use serde_json::json;

use super::{
    super::{
        MODEL_LIST_PAGE_SIZE, ModelListResponse, PermissionProfileListResponse,
        thread_settings_update_request,
    },
    CodexAppServerManager,
};
use crate::agent::{
    AgentModel, AgentModelCatalog, AgentPermissionMode, AgentPermissionProfile, AgentThreadSettings,
};

impl CodexAppServerManager {
    pub(in crate::agent::codex) fn load_model_catalog(
        &self,
    ) -> Receiver<Result<AgentModelCatalog, String>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .load_model_catalog_blocking()
                .map_err(|error| format!("{error:#}"));
            let _ = sender.send_blocking(result);
        });
        receiver
    }
    pub(super) fn load_model_catalog_blocking(&self) -> Result<AgentModelCatalog> {
        let connection = self.inner.ensure_connection()?;
        let mut models = Vec::new();
        let mut cursor: Option<String> = None;
        let mut seen_cursors = HashSet::new();
        loop {
            let response = connection.request(
                "model/list",
                json!({
                    "cursor": cursor,
                    "limit": MODEL_LIST_PAGE_SIZE,
                    "includeHidden": false
                }),
            )?;
            let result = response
                .get("result")
                .cloned()
                .context("model/list 响应缺少 result")?;
            let page: ModelListResponse = match serde_json::from_value(result) {
                Ok(page) => page,
                Err(error) => {
                    let message =
                        format!("无法解析 model/list 响应；0.153.0 schema 不匹配：{error}");
                    connection.fail_protocol(message.clone());
                    bail!(message);
                }
            };
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
                let message = format!("model/list 返回了重复分页 cursor `{next_cursor}`");
                connection.fail_protocol(message.clone());
                bail!(message);
            }
            cursor = Some(next_cursor);
        }
        if models.is_empty() {
            bail!("model/list 未返回可显示的模型");
        }
        Ok(AgentModelCatalog { models })
    }
    pub(in crate::agent::codex) fn load_permission_profiles(
        &self,
        cwd: PathBuf,
    ) -> Receiver<Result<Vec<AgentPermissionProfile>, String>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .load_permission_profiles_blocking(&cwd)
                .map_err(|error| format!("{error:#}"));
            let _ = sender.send_blocking(result);
        });
        receiver
    }
    pub(super) fn load_permission_profiles_blocking(
        &self,
        cwd: &Path,
    ) -> Result<Vec<AgentPermissionProfile>> {
        let connection = self.inner.ensure_connection()?;
        let response = connection.request(
            "permissionProfile/list",
            json!({ "cursor": null, "limit": 100, "cwd": cwd }),
        )?;
        let result = response
            .get("result")
            .cloned()
            .context("permissionProfile/list 响应缺少 result")?;
        let page: PermissionProfileListResponse = match serde_json::from_value(result) {
            Ok(page) => page,
            Err(error) => {
                let message = format!("无法解析 permissionProfile/list 响应：{error}");
                connection.fail_protocol(message.clone());
                bail!(message);
            }
        };
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
    pub(in crate::agent::codex) fn update_thread_permissions(
        &self,
        thread_id: String,
        cwd: PathBuf,
        mode: AgentPermissionMode,
    ) -> Receiver<Result<AgentThreadSettings, String>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .update_thread_permissions_blocking(&thread_id, &cwd, mode)
                .map_err(|error| format!("{error:#}"));
            let _ = sender.send_blocking(result);
        });
        receiver
    }
    pub(super) fn update_thread_permissions_blocking(
        &self,
        thread_id: &str,
        cwd: &Path,
        mode: AgentPermissionMode,
    ) -> Result<AgentThreadSettings> {
        let connection = self.inner.ensure_connection()?;
        self.ensure_thread_loaded(&connection, Some(thread_id), None, false)?;
        let _settings_guard = connection
            .settings_lock
            .lock()
            .map_err(|_| anyhow!("Codex thread settings lifecycle 锁已损坏"))?;
        let params = thread_settings_update_request(0, thread_id, cwd, mode)?
            .get("params")
            .cloned()
            .context("thread/settings/update builder 缺少 params")?;
        let (sender, receiver) = async_channel::bounded(1);
        connection
            .state
            .lock()
            .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?
            .settings_waiters
            .entry(thread_id.to_owned())
            .or_default()
            .push(sender);
        if let Err(error) = connection.request("thread/settings/update", params) {
            if let Ok(mut state) = connection.state.lock() {
                state.settings_waiters.remove(thread_id);
            }
            return Err(error);
        }
        receiver
            .recv_blocking()
            .map_err(|_| anyhow!("thread/settings/updated waiter 在返回前关闭"))?
            .map_err(anyhow::Error::msg)
    }
}
