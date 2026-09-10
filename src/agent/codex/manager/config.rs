//! Configuration transport, using one generation for read/write/readback.
use super::super::config::{decode_receipt, decode_snapshot, write_params};
use super::{CodexAppServerManager, connection::Connection};
use crate::agent::{
    AgentConfigError, AgentConfigErrorKind, AgentConfigSaveResult, AgentConfigSnapshot,
    AgentConfigWrite,
};
use async_channel::Receiver;
use serde_json::json;
use std::{path::PathBuf, sync::Arc};

fn connection_error(error: anyhow::Error, outcome_unknown: bool) -> AgentConfigError {
    AgentConfigError {
        kind: AgentConfigErrorKind::Connection,
        message: format!("{error:#}"),
        data: None,
        outcome_unknown,
    }
}

impl CodexAppServerManager {
    pub(in crate::agent::codex) fn read_config(
        &self,
        cwd: PathBuf,
    ) -> Receiver<Result<AgentConfigSnapshot, AgentConfigError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .inner
                .ensure_connection()
                .map_err(|error| connection_error(error, false))
                .and_then(|connection| Self::read_config_on(&connection, cwd));
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    pub(super) fn read_config_on(
        connection: &Arc<Connection>,
        cwd: PathBuf,
    ) -> Result<AgentConfigSnapshot, AgentConfigError> {
        let config = connection
            .request("config/read", json!({"cwd":cwd,"includeLayers":true}))
            .map_err(|error| connection_error(error, false))?;
        let requirements = connection
            .request("configRequirements/read", json!({}))
            .map_err(|error| connection_error(error, false))?;
        decode_snapshot(connection.generation, cwd, config, requirements)
    }

    pub(in crate::agent::codex) fn write_config(
        &self,
        write: AgentConfigWrite,
    ) -> Receiver<Result<AgentConfigSaveResult, AgentConfigError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let params = write_params(&write)?;
                let connection = manager
                    .inner
                    .ensure_connection()
                    .map_err(|error| connection_error(error, false))?;
                if connection.generation != write.generation {
                    return Err(AgentConfigError {
                        kind: AgentConfigErrorKind::Connection,
                        message: "连接已重建，请重新读取配置并核对草稿后保存".into(),
                        data: None,
                        outcome_unknown: false,
                    });
                }
                let response = connection
                    .request("config/batchWrite", params)
                    .map_err(|error| connection_error(error, true))?;
                let receipt = decode_receipt(response).map_err(|mut error| {
                    if error.kind == AgentConfigErrorKind::Protocol {
                        error.outcome_unknown = true;
                    }
                    error
                })?;
                let readback = Self::read_config_on(&connection, write.cwd);
                Ok(AgentConfigSaveResult { receipt, readback })
            })();
            let _ = sender.send_blocking(result);
        });
        receiver
    }
}
