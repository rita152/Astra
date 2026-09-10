//! Model catalogs and thread permission settings over the shared connection.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, bail};
use async_channel::Receiver;
use serde_json::json;

use super::{
    super::{MODEL_LIST_PAGE_SIZE, ModelListResponse},
    CodexAppServerManager,
};
use crate::agent::{AgentModel, AgentModelCatalog, AgentPermissionProfile};

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
        super::super::catalog::permission_profile_pages(cwd, |params| {
            connection.request("permissionProfile/list", params)
        })
    }
}
