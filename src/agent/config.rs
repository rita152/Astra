//! Effective configuration, source identity and persistence receipts.
//! No local TOML reader: the backend owns resolution and optimistic writes.

use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentConfigSource {
    /// Full source object, including future source kinds and profile metadata.
    pub metadata: Value,
    pub kind: String,
    pub path: Option<PathBuf>,
    pub name: Option<String>,
    pub profile: Option<String>,
    pub version: String,
}

impl AgentConfigSource {
    pub fn kind(&self) -> &str {
        &self.kind
    }
    pub fn file_path(&self) -> Option<PathBuf> {
        self.path.clone()
    }

    pub fn writable(&self) -> bool {
        self.kind() == "user"
            && self.profile.is_none()
            && !self.version.is_empty()
            && self.path.as_ref().is_some_and(|path| path.is_absolute())
    }

    pub fn label(&self) -> String {
        let label = match self.kind() {
            "user" => "用户配置",
            "project" => "项目配置",
            "managed" => "受管配置",
            "session" => "启动参数",
            "defaults" => "安装包默认值",
            other => other,
        };
        let detail = self
            .path
            .as_ref()
            .map(|path| path.display().to_string())
            .or_else(|| self.name.clone());
        let mut label =
            detail.map_or_else(|| label.to_owned(), |detail| format!("{label} · {detail}"));
        if let Some(profile) = &self.profile {
            label.push_str(&format!(" · 配置方案 {profile}"));
        }
        label
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentConfigLayer {
    pub source: AgentConfigSource,
    pub config: Value,
    pub disabled_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentConfigSnapshot {
    pub generation: u64,
    pub cwd: PathBuf,
    pub effective: Value,
    pub origins: BTreeMap<String, AgentConfigSource>,
    /// Server order is retained; never recreate precedence on the client.
    pub layers: Option<Vec<AgentConfigLayer>>,
    pub requirements: Option<AgentConfigRequirements>,
    pub value_aliases: BTreeMap<String, BTreeMap<String, String>>,
    pub value_defaults: BTreeMap<String, Value>,
    pub profile_parents: BTreeMap<String, String>,
    pub required_fields: std::collections::BTreeSet<String>,
}

pub fn config_value<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    key.split('.')
        .try_fold(value, |value, part| value.get(part))
}

impl AgentConfigSnapshot {
    pub fn origin(&self, key: &str) -> Option<&AgentConfigSource> {
        let mut path = key;
        loop {
            if let Some(origin) = self.origins.get(path) {
                return Some(origin);
            }
            path = path.rsplit_once('.')?.0;
        }
    }

    pub fn layer(&self, path: &std::path::Path) -> Option<&AgentConfigLayer> {
        self.layers.as_ref()?.iter().find(|layer| {
            layer.source.writable() && layer.source.file_path().as_deref() == Some(path)
        })
    }

    pub fn user_layer(&self) -> Option<&AgentConfigLayer> {
        self.layers
            .as_ref()?
            .iter()
            .find(|layer| layer.source.kind() == "user" && layer.source.writable())
    }

    pub fn equivalent(&self, key: &str, left: &Value, right: &Value) -> bool {
        if let Some(aliases) = self.value_aliases.get(key)
            && let (Some(left), Some(right)) = (left.as_str(), right.as_str())
        {
            return aliases.get(left).map(String::as_str).unwrap_or(left)
                == aliases.get(right).map(String::as_str).unwrap_or(right);
        }
        fn defaults(value: &Value, default: &Value) -> Value {
            let mut value = value.clone();
            if let (Some(fields), Some(default_fields)) =
                (value.as_object_mut(), default.as_object())
            {
                for (key, default) in default_fields {
                    let next = fields
                        .get(key)
                        .map(|value| defaults(value, default))
                        .unwrap_or_else(|| default.clone());
                    fields.insert(key.clone(), next);
                }
            }
            value
        }
        if let Some(default) = self.value_defaults.get(key) {
            defaults(left, default) == defaults(right, default)
        } else {
            left == right
        }
    }

    pub fn restriction(&self, key: &str, value: &Value) -> Option<String> {
        if value.is_null() && self.required_fields.contains(key) {
            return Some(format!("已定义具名权限配置，必须保留 {key} 的明确选择"));
        }
        let requirements = self.requirements.as_ref()?;
        if !value.is_null()
            && let Some(allowed) = requirements.allowed.get(key)
            && !allowed.iter().any(|allowed| {
                self.equivalent(key, allowed, value) && self.equivalent(key, value, allowed)
            })
        {
            return Some(format!("管理员限制：{key} 不允许此值"));
        }
        if let Some(forced) = requirements.enforced.get(key) {
            return Some(format!("由管理员强制设置为 {forced}"));
        }

        None
    }
}

/// Normalized controls constraints; the adapter retains the entire source payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentConfigRequirements {
    pub raw: Value,
    pub allowed: BTreeMap<String, Vec<Value>>,
    pub enforced: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentConfigEdit {
    pub key: String,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentConfigWrite {
    pub generation: u64,
    pub cwd: PathBuf,
    pub file_path: PathBuf,
    pub expected_version: String,
    pub edits: Vec<AgentConfigEdit>,
    pub reload_user_config: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentConfigReceipt {
    pub status: String,
    pub version: String,
    pub file_path: PathBuf,
    pub overridden: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentConfigErrorKind {
    Unavailable,
    Connection,
    Conflict,
    Validation,
    Restricted,
    Write,
    Protocol,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentConfigError {
    pub kind: AgentConfigErrorKind,
    pub message: String,
    pub data: Option<Value>,
    /// A broken transport after sending a write has an unknown outcome.
    pub outcome_unknown: bool,
}

impl AgentConfigError {
    pub fn user_message(&self) -> String {
        let mut message = match self.kind {
            AgentConfigErrorKind::Conflict => {
                "配置已在外部修改。草稿已保留，请先重新读取，再核对后保存。".into()
            }
            AgentConfigErrorKind::Validation => {
                format!("配置校验失败，草稿已保留：{}", self.message)
            }
            AgentConfigErrorKind::Restricted => format!("此配置受限制，无法保存：{}", self.message),
            _ => self.message.clone(),
        };
        if self.outcome_unknown {
            message.push_str(" · 写入结果未知，请重新读取并核对。");
        }
        message
    }

    pub fn unavailable() -> Self {
        Self {
            kind: AgentConfigErrorKind::Unavailable,
            message: "当前后端不支持配置".into(),
            data: None,
            outcome_unknown: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentConfigSaveResult {
    pub receipt: AgentConfigReceipt,
    pub readback: Result<AgentConfigSnapshot, AgentConfigError>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentConfigChoiceSet {
    pub key: String,
    pub values: Vec<Value>,
    pub allows_custom_string: bool,
    pub session_static: bool,
}
