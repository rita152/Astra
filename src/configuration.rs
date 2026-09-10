//! Configuration editor state. Independent of GPUI and configuration transport.
use crate::agent::{
    AgentConfigEdit, AgentConfigError, AgentConfigErrorKind, AgentConfigReceipt,
    AgentConfigSaveResult, AgentConfigSnapshot, AgentConfigWrite, config_value,
};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ConfigOperation {
    #[default]
    Unavailable,
    Loading,
    Ready,
    Saving,
    Failed(AgentConfigError),
    ReadFailed(AgentConfigError),
}

#[derive(Clone, Debug, Default)]
pub struct ConfigEditor {
    pub snapshot: Option<AgentConfigSnapshot>,
    pub target: Option<PathBuf>,
    pub edits: BTreeMap<String, Value>,
    pub operation: ConfigOperation,
    pub receipt: Option<AgentConfigReceipt>,
    pub feedback: Option<String>,
    /// After a conflict, rereading retains the draft and requires explicit review.
    pub needs_review: bool,
    pub cycle: u64,
}

impl ConfigEditor {
    pub fn begin_read(&mut self) -> u64 {
        self.cycle = self.cycle.wrapping_add(1);
        self.operation = ConfigOperation::Loading;
        self.feedback = None;
        self.cycle
    }

    pub fn accept_read(
        &mut self,
        cycle: u64,
        result: Result<AgentConfigSnapshot, AgentConfigError>,
    ) -> bool {
        if cycle != self.cycle {
            return false;
        }
        match result {
            Ok(snapshot) => {
                let changed = self.snapshot.as_ref().is_some_and(|before| {
                    before.cwd != snapshot.cwd
                        || before.generation != snapshot.generation
                        || self.target.as_ref().is_some_and(|path| {
                            before.layer(path).map(|layer| &layer.source.version)
                                != snapshot.layer(path).map(|layer| &layer.source.version)
                        })
                });
                if !self.edits.is_empty() && changed {
                    self.needs_review = true;
                    self.feedback = Some(
                        "已读取最新配置，草稿已保留。请核对来源与有效值，然后确认草稿。".into(),
                    );
                }
                if self
                    .target
                    .as_ref()
                    .is_none_or(|path| snapshot.layer(path).is_none())
                {
                    self.target = snapshot
                        .user_layer()
                        .and_then(|layer| layer.source.file_path());
                }
                self.snapshot = Some(snapshot);
                self.operation = ConfigOperation::Ready;
            }
            Err(error) => self.operation = ConfigOperation::ReadFailed(error),
        }
        true
    }

    pub fn select_target(&mut self, path: PathBuf) -> Result<(), String> {
        if self.busy() {
            return Err("正在读取或保存配置".into());
        }
        if !self.edits.is_empty() {
            return Err("请先保存或放弃当前文件的修改".into());
        }
        let layer = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.layer(&path))
            .ok_or("配置层不可写")?;
        if let Some(reason) = &layer.disabled_reason {
            return Err(reason.clone());
        }
        self.target = Some(path);
        self.receipt = None;
        self.feedback = None;
        Ok(())
    }

    pub fn busy(&self) -> bool {
        matches!(
            self.operation,
            ConfigOperation::Loading | ConfigOperation::Saving
        )
    }

    pub fn value(&self, key: &str) -> Option<&Value> {
        self.edits.get(key).or_else(|| {
            self.snapshot
                .as_ref()?
                .layer(self.target.as_ref()?)
                .and_then(|layer| config_value(&layer.config, key))
        })
    }

    pub fn restriction(&self, key: &str, value: &Value) -> Option<String> {
        let snapshot = self.snapshot.as_ref()?;
        if let Some(layer) = self.target.as_ref().and_then(|path| snapshot.layer(path))
            && let Some(reason) = &layer.disabled_reason
        {
            return Some(reason.clone());
        }
        snapshot.restriction(key, value)
    }

    pub fn edit(&mut self, key: &str, value: Value) -> Result<(), String> {
        if !matches!(
            self.operation,
            ConfigOperation::Ready | ConfigOperation::Failed(_) | ConfigOperation::ReadFailed(_)
        ) || self.snapshot.is_none()
        {
            return Err("配置尚未就绪".into());
        }
        if let Some(reason) = self.restriction(key, &value) {
            return Err(reason);
        }
        let snapshot = self.snapshot.as_ref().ok_or("请先读取配置")?;
        let layer = snapshot
            .layer(self.target.as_ref().ok_or("请选择可写配置文件")?)
            .ok_or("配置文件不可写")?;
        let original = config_value(&layer.config, key).unwrap_or(&Value::Null);
        if original == &value {
            self.edits.remove(key);
        } else {
            self.edits.insert(key.into(), value);
        }
        self.receipt = None;
        self.feedback = None;
        Ok(())
    }

    pub fn discard(&mut self) {
        if self.busy() {
            return;
        }
        self.edits.clear();
        self.needs_review = false;
        self.feedback = None;
        self.receipt = None;
    }

    pub fn confirm_review(&mut self) {
        if self.operation == ConfigOperation::Ready {
            self.needs_review = false;
        }
    }

    pub fn prepare_write(
        &mut self,
        fields: &[crate::agent::AgentConfigChoiceSet],
    ) -> Result<(u64, AgentConfigWrite), String> {
        if matches!(self.operation, ConfigOperation::ReadFailed(_)) {
            return Err("读取失败后需要先成功重新读取配置，才能保存草稿".into());
        }
        if self.busy() || self.needs_review {
            return Err("请等待配置就绪并核对草稿".into());
        }
        if let ConfigOperation::Failed(error) = &self.operation
            && (error.kind == AgentConfigErrorKind::Conflict
                || error.outcome_unknown
                || error.kind == AgentConfigErrorKind::Connection)
        {
            return Err("请重新读取配置并核对草稿，保存不会自动重试".into());
        }
        let snapshot = self.snapshot.as_ref().ok_or("请先读取配置")?;
        let file_path = self.target.clone().ok_or("没有可写配置文件")?;
        let layer = snapshot
            .layer(&file_path)
            .ok_or("配置层缺少版本，无法安全保存")?;
        if let Some(reason) = &layer.disabled_reason {
            return Err(reason.clone());
        }
        if self.edits.is_empty() {
            return Err("没有待保存的修改".into());
        }
        for (key, value) in &self.edits {
            if let Some(reason) = snapshot.restriction(key, value) {
                return Err(reason);
            }
        }
        let edits = self
            .edits
            .iter()
            .map(|(key, value)| AgentConfigEdit {
                key: key.clone(),
                value: value.clone(),
            })
            .collect();
        let write = AgentConfigWrite {
            generation: snapshot.generation,
            cwd: snapshot.cwd.clone(),
            file_path,
            expected_version: layer.source.version.clone(),
            edits,
            reload_user_config: layer.source.kind() == "user"
                && self.edits.keys().any(|key| {
                    !fields
                        .iter()
                        .any(|field| field.key == *key && field.session_static)
                }),
        };
        self.cycle = self.cycle.wrapping_add(1);
        self.operation = ConfigOperation::Saving;
        self.feedback = None;
        Ok((self.cycle, write))
    }

    pub fn accept_save(
        &mut self,
        cycle: u64,
        result: Result<AgentConfigSaveResult, AgentConfigError>,
    ) -> bool {
        if cycle != self.cycle {
            return false;
        }
        match result {
            Err(error) => {
                self.needs_review =
                    error.kind == AgentConfigErrorKind::Conflict || error.outcome_unknown;
                self.operation = ConfigOperation::Failed(error);
            }
            Ok(result) => {
                self.receipt = Some(result.receipt.clone());
                match result.readback {
                    Err(error) => {
                        self.feedback = Some(
                            "已写入文件，但有效配置回读失败。草稿已保留，请重新读取后核对。".into(),
                        );
                        self.needs_review = true;
                        self.operation = ConfigOperation::Failed(error);
                    }
                    Ok(snapshot) => {
                        let target = &result.receipt.file_path;
                        let mut overridden = Vec::new();
                        let mut different = Vec::new();
                        let written_layer = snapshot.layer(target);
                        if written_layer
                            .is_none_or(|layer| layer.source.version != result.receipt.version)
                        {
                            different.push("文件版本（写入后发生变化或配置层不可见）".into());
                        }
                        for (key, value) in &self.edits {
                            let stored =
                                written_layer.and_then(|layer| config_value(&layer.config, key));
                            if value.is_null() && stored.is_some_and(|stored| !stored.is_null()) {
                                different.push(key.clone());
                            }
                            let effective = config_value(&snapshot.effective, key);
                            if !value.is_null()
                                && stored
                                    .is_none_or(|stored| !snapshot.equivalent(key, value, stored))
                            {
                                different.push(format!("{key}（文件值）"));
                            }
                            if !value.is_null()
                                && effective.is_none_or(|effective| {
                                    !snapshot.equivalent(key, value, effective)
                                })
                            {
                                if snapshot.origin(key).is_some_and(|source| {
                                    written_layer.is_none_or(|layer| {
                                        source.metadata != layer.source.metadata
                                    })
                                }) {
                                    overridden.push(key.clone());
                                } else {
                                    different.push(key.clone());
                                }
                            }
                        }
                        let mut feedback = format!("已写入 {}，并回读有效配置。", target.display());
                        if result.receipt.status == "okOverridden" || !overridden.is_empty() {
                            feedback.push_str(&format!(
                                " 部分设置被更高优先级配置覆盖：{}。",
                                overridden.join("、")
                            ));
                        } else if result.receipt.status != "ok" {
                            feedback
                                .push_str(&format!(" 服务端返回状态：{}。", result.receipt.status));
                        }
                        if !different.is_empty() {
                            feedback.push_str(&format!(
                                " 回读与草稿不同：{}；请核对。",
                                different.join("、")
                            ));
                            self.needs_review = true;
                        } else {
                            self.edits.clear();
                            self.needs_review = false;
                        }
                        feedback.push_str(" 模型、推理强度、Plan 推理强度、服务等级和个性默认值仅用于新会话；当前轮次权限不随配置保存改变。");
                        self.feedback = Some(feedback);
                        self.snapshot = Some(snapshot);
                        self.target = Some(target.clone());
                        self.operation = ConfigOperation::Ready;
                    }
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests;
