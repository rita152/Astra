//! Codex 0.153.0 configuration wire format (default + experimental schema).
use crate::agent::{
    AgentConfigChoiceSet, AgentConfigError, AgentConfigErrorKind, AgentConfigLayer,
    AgentConfigReceipt, AgentConfigRequirements, AgentConfigSnapshot, AgentConfigSource,
    AgentConfigWrite,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Deserialize)]
struct Origin {
    name: Value,
    version: String,
}
impl From<Origin> for AgentConfigSource {
    fn from(origin: Origin) -> Self {
        source(origin.name, origin.version)
    }
}
fn source(metadata: Value, version: String) -> AgentConfigSource {
    let wire_kind = metadata
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    let kind = match wire_kind.as_str() {
        "system"
        | "mdm"
        | "enterpriseManaged"
        | "legacyManagedConfigTomlFromFile"
        | "legacyManagedConfigTomlFromMdm" => "managed",
        "sessionFlags" => "session",
        "packagedDefaults" => "defaults",
        other => other,
    }
    .to_owned();
    let path = if kind == "project" {
        metadata
            .get("dotCodexFolder")
            .and_then(Value::as_str)
            .map(|folder| PathBuf::from(folder).join("config.toml"))
    } else {
        metadata
            .get("file")
            .and_then(Value::as_str)
            .map(PathBuf::from)
    };
    let name = metadata
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let profile = metadata
        .get("profile")
        .and_then(Value::as_str)
        .map(str::to_owned);
    AgentConfigSource {
        metadata,
        version,
        kind,
        path,
        name,
        profile,
    }
}
fn decode_requirements(raw: Value) -> Result<AgentConfigRequirements, AgentConfigError> {
    let mut allowed = BTreeMap::new();
    let mut enforced = BTreeMap::new();
    for (wire, key) in [
        ("allowedApprovalPolicies", "approval_policy"),
        ("allowedApprovalsReviewers", "approvals_reviewer"),
        ("allowedSandboxModes", "sandbox_mode"),
        ("allowedWebSearchModes", "web_search"),
        ("allowedWindowsSandboxImplementations", "windows.sandbox"),
    ] {
        if let Some(value) = raw.get(wire).filter(|value| !value.is_null()) {
            let values = value
                .as_array()
                .ok_or_else(|| protocol_error(format!("{wire} 必须是数组或 null")))?;
            allowed.insert(key.into(), values.clone());
        }
    }
    if let Some(value) = raw
        .get("allowedPermissionProfiles")
        .filter(|value| !value.is_null())
    {
        let profiles = value
            .as_object()
            .ok_or_else(|| protocol_error("allowedPermissionProfiles 必须是对象或 null"))?;
        if profiles.values().any(|value| !value.is_boolean()) {
            return Err(protocol_error("allowedPermissionProfiles 的值必须是布尔值"));
        }
        allowed.insert(
            "default_permissions".into(),
            profiles
                .iter()
                .filter(|(_, allowed)| allowed.as_bool() == Some(true))
                .map(|(id, _)| json!(id))
                .collect(),
        );
    }
    for (pointer, key) in [
        ("/defaultPermissions", "default_permissions"),
        ("/models/newThread/model", "model"),
        (
            "/models/newThread/modelReasoningEffort",
            "model_reasoning_effort",
        ),
        ("/models/newThread/serviceTier", "service_tier"),
        ("/allowLoginShell", "allow_login_shell"),
        ("/checkForUpdateOnStartup", "check_for_update_on_startup"),
        ("/cliAuthCredentialsStore", "cli_auth_credentials_store"),
        ("/chatgptBaseUrl", "chatgpt_base_url"),
        ("/logDir", "log_dir"),
        ("/sqliteHome", "sqlite_home"),
        ("/modelCatalogJson", "model_catalog_json"),
    ] {
        if let Some(value) = raw.pointer(pointer).filter(|value| !value.is_null()) {
            enforced.insert(key.into(), value.clone());
        }
    }
    if let Some(features) = raw.get("featureRequirements").and_then(Value::as_object) {
        for (key, value) in features {
            enforced.insert(format!("features.{key}"), value.clone());
        }
    }
    Ok(AgentConfigRequirements {
        raw,
        allowed,
        enforced,
    })
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Layer {
    name: Value,
    version: String,
    config: Value,
    disabled_reason: Option<String>,
}
#[derive(Deserialize)]
struct ReadResponse {
    config: Value,
    origins: BTreeMap<String, Origin>,
    layers: Option<Vec<Layer>>,
}

pub(super) fn protocol_error(message: impl Into<String>) -> AgentConfigError {
    AgentConfigError {
        kind: AgentConfigErrorKind::Protocol,
        message: message.into(),
        data: None,
        outcome_unknown: false,
    }
}

pub(super) fn response_result(message: Value) -> Result<Value, AgentConfigError> {
    if let Some(error) = message.get("error") {
        let data = error.get("data").cloned();
        let code = data
            .as_ref()
            .and_then(|data| data.get("config_write_error_code"))
            .and_then(Value::as_str);
        let kind = match code {
            Some("configVersionConflict") => AgentConfigErrorKind::Conflict,
            Some("configValidationError") => AgentConfigErrorKind::Validation,
            Some("configRequirementsError" | "managedConfigError" | "configLayerReadonly") => {
                AgentConfigErrorKind::Restricted
            }
            _ => AgentConfigErrorKind::Write,
        };
        return Err(AgentConfigError {
            kind,
            message: error
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| error.to_string()),
            data,
            outcome_unknown: false,
        });
    }
    message
        .get("result")
        .cloned()
        .ok_or_else(|| protocol_error("配置响应缺少 result"))
}

pub(super) fn decode_snapshot(
    generation: u64,
    cwd: PathBuf,
    config: Value,
    requirements: Value,
) -> Result<AgentConfigSnapshot, AgentConfigError> {
    let read: ReadResponse = serde_json::from_value(response_result(config)?)
        .map_err(|error| protocol_error(format!("无法解析配置：{error}")))?;
    if !read.config.is_object() {
        return Err(protocol_error("有效配置必须是对象"));
    }
    let requirements = response_result(requirements)?;
    let requirements = requirements
        .get("requirements")
        .filter(|value| !value.is_null())
        .cloned();
    if requirements
        .as_ref()
        .is_some_and(|value| !value.is_object())
    {
        return Err(protocol_error("配置要求必须是对象或 null"));
    }
    let definitions = read.config.get("permissions").and_then(Value::as_object);
    let profile_parents = definitions
        .into_iter()
        .flat_map(|definitions| definitions.iter())
        .filter_map(|(id, definition)| {
            definition
                .get("extends")
                .and_then(Value::as_str)
                .map(|parent| (id.clone(), parent.into()))
        })
        .collect();
    let inherited_default = read.layers.as_ref().into_iter().flatten().any(|layer| {
        layer.disabled_reason.is_none()
            && !(layer.name.get("type").and_then(Value::as_str) == Some("user")
                && layer.name.get("profile").is_none_or(Value::is_null))
            && layer
                .config
                .get("default_permissions")
                .is_some_and(|value| !value.is_null())
    });
    let required_fields =
        if !inherited_default && definitions.is_some_and(|definitions| !definitions.is_empty()) {
            std::collections::BTreeSet::from(["default_permissions".into()])
        } else {
            Default::default()
        };
    Ok(AgentConfigSnapshot {
        generation,
        cwd,
        profile_parents,
        required_fields,
        effective: read.config,
        origins: read
            .origins
            .into_iter()
            .map(|(key, origin)| (key, origin.into()))
            .collect(),
        layers: read.layers.map(|layers| {
            layers
                .into_iter()
                .map(|layer| AgentConfigLayer {
                    source: source(layer.name, layer.version),
                    config: layer.config,
                    disabled_reason: layer.disabled_reason,
                })
                .collect()
        }),
        requirements: requirements.map(decode_requirements).transpose()?,
        value_defaults: BTreeMap::from([(
            "approval_policy".into(),
            json!({"granular":{"request_permissions":false,"skill_approval":false}}),
        )]),
        value_aliases: BTreeMap::from([(
            "approvals_reviewer".into(),
            BTreeMap::from([("guardian_subagent".into(), "auto_review".into())]),
        )]),
    })
}

pub(super) fn write_params(write: &AgentConfigWrite) -> Result<Value, AgentConfigError> {
    if !write.file_path.is_absolute() || write.expected_version.is_empty() || write.edits.is_empty()
    {
        return Err(protocol_error("保存需要绝对配置路径、读取版本和已修改字段"));
    }
    Ok(
        json!({ "filePath": write.file_path, "expectedVersion": write.expected_version,
        "reloadUserConfig": write.reload_user_config,
        "edits": write.edits.iter().map(|edit| json!({"keyPath":edit.key, "value":edit.value,
            "mergeStrategy": "replace"
        })).collect::<Vec<_>>() }),
    )
}

pub(super) fn decode_receipt(message: Value) -> Result<AgentConfigReceipt, AgentConfigError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Receipt {
        status: String,
        version: String,
        file_path: PathBuf,
        overridden_metadata: Option<Value>,
    }
    let receipt: Receipt = serde_json::from_value(response_result(message)?)
        .map_err(|error| protocol_error(format!("无法解析保存结果：{error}")))?;
    Ok(AgentConfigReceipt {
        status: receipt.status,
        version: receipt.version,
        file_path: receipt.file_path,
        overridden: receipt.overridden_metadata,
    })
}

/// Open-ended model/effort/tier choices are supplied by model/list in the UI.
/// Preserve object approval policies and future values already returned by the server.
pub(super) fn config_choices() -> Vec<AgentConfigChoiceSet> {
    [
        (
            "approval_policy",
            &["untrusted", "on-request", "never"][..],
            false,
            false,
        ),
        (
            "sandbox_mode",
            &["read-only", "workspace-write", "danger-full-access"][..],
            false,
            false,
        ),
        (
            "web_search",
            &["disabled", "cached", "indexed", "live"][..],
            false,
            false,
        ),
        (
            "model_verbosity",
            &["low", "medium", "high"][..],
            false,
            false,
        ),
        (
            "model_reasoning_summary",
            &["auto", "concise", "detailed", "none"][..],
            false,
            false,
        ),
        (
            "approvals_reviewer",
            &["user", "auto_review", "guardian_subagent"][..],
            false,
            false,
        ),
        ("default_permissions", &[][..], false, false),
        ("model", &[][..], true, true),
        ("model_reasoning_effort", &[][..], true, true),
        ("plan_mode_reasoning_effort", &[][..], true, true),
        ("service_tier", &[][..], true, true),
        (
            "personality",
            &["none", "friendly", "pragmatic"][..],
            false,
            true,
        ),
    ]
    .into_iter()
    .map(
        |(key, values, allows_custom_string, session_static)| AgentConfigChoiceSet {
            key: key.into(),
            values: values.iter().map(|value| json!(value)).collect(),
            allows_custom_string,
            session_static,
        },
    )
    .collect()
}

#[cfg(test)]
mod tests;
