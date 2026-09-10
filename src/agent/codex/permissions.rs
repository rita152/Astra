//! Codex sandbox settings and permission profile encoding.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{Value, json};

use crate::agent::{
    AgentAdditionalFileSystemPermissions, AgentAdditionalNetworkPermissions, AgentFileSystemAccess,
    AgentFileSystemPath, AgentFileSystemPermissionEntry, AgentFileSystemSpecialPath,
    AgentOptionalField, AgentPermissionMode, AgentPermissionRequestProfile,
};

pub(super) fn insert_optional_field<T>(
    object: &mut serde_json::Map<String, Value>,
    field: &str,
    value: &AgentOptionalField<T>,
    serialize: impl FnOnce(&T) -> Value,
) {
    match value {
        AgentOptionalField::Unspecified => {}
        AgentOptionalField::Null => {
            object.insert(field.to_owned(), Value::Null);
        }
        AgentOptionalField::Value(value) => {
            object.insert(field.to_owned(), serialize(value));
        }
    }
}

pub(super) fn permission_profile_value(profile: &AgentPermissionRequestProfile) -> Value {
    let mut object = serde_json::Map::new();
    insert_optional_field(
        &mut object,
        "fileSystem",
        &profile.file_system,
        file_system_permissions_value,
    );
    insert_optional_field(
        &mut object,
        "network",
        &profile.network,
        network_permissions_value,
    );
    Value::Object(object)
}

pub(super) fn file_system_permissions_value(
    permissions: &AgentAdditionalFileSystemPermissions,
) -> Value {
    let mut object = serde_json::Map::new();
    insert_optional_field(&mut object, "read", &permissions.read, |paths| json!(paths));
    insert_optional_field(&mut object, "write", &permissions.write, |paths| {
        json!(paths)
    });
    insert_optional_field(
        &mut object,
        "globScanMaxDepth",
        &permissions.glob_scan_max_depth,
        |depth| json!(depth),
    );
    insert_optional_field(&mut object, "entries", &permissions.entries, |entries| {
        Value::Array(entries.iter().map(file_system_entry_value).collect())
    });
    Value::Object(object)
}

pub(super) fn network_permissions_value(permissions: &AgentAdditionalNetworkPermissions) -> Value {
    let mut object = serde_json::Map::new();
    insert_optional_field(&mut object, "enabled", &permissions.enabled, |enabled| {
        json!(enabled)
    });
    Value::Object(object)
}

pub(super) fn file_system_entry_value(entry: &AgentFileSystemPermissionEntry) -> Value {
    let access = match entry.access {
        AgentFileSystemAccess::Read => "read",
        AgentFileSystemAccess::Write => "write",
        AgentFileSystemAccess::Deny => "deny",
    };
    json!({
        "path": file_system_path_value(&entry.path),
        "access": access
    })
}

pub(super) fn file_system_path_value(path: &AgentFileSystemPath) -> Value {
    match path {
        AgentFileSystemPath::Path(path) => json!({ "type": "path", "path": path }),
        AgentFileSystemPath::GlobPattern(pattern) => {
            json!({ "type": "glob_pattern", "pattern": pattern })
        }
        AgentFileSystemPath::Special(value) => {
            json!({ "type": "special", "value": file_system_special_path_value(value) })
        }
    }
}

pub(super) fn file_system_special_path_value(path: &AgentFileSystemSpecialPath) -> Value {
    match path {
        AgentFileSystemSpecialPath::Root => json!({ "kind": "root" }),
        AgentFileSystemSpecialPath::Minimal => json!({ "kind": "minimal" }),
        AgentFileSystemSpecialPath::ProjectRoots { subpath } => {
            let mut value = serde_json::Map::new();
            value.insert("kind".to_owned(), json!("project_roots"));
            insert_optional_field(&mut value, "subpath", subpath, |subpath| json!(subpath));
            Value::Object(value)
        }
        AgentFileSystemSpecialPath::Tmpdir => json!({ "kind": "tmpdir" }),
        AgentFileSystemSpecialPath::SlashTmp => json!({ "kind": "slash_tmp" }),
        AgentFileSystemSpecialPath::Unknown { path, subpath } => {
            let mut value = serde_json::Map::new();
            value.insert("kind".to_owned(), json!("unknown"));
            value.insert("path".to_owned(), json!(path));
            insert_optional_field(&mut value, "subpath", subpath, |subpath| json!(subpath));
            Value::Object(value)
        }
    }
}

pub(super) fn workspace_roots(cwd: &Path, thread_id: &str) -> Vec<String> {
    let mut roots = vec![cwd.to_string_lossy().into_owned()];
    if let Some(home) = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex")))
    {
        let dated = chrono::Local::now().format("%Y/%m/%d").to_string();
        roots.push(
            home.join("visualizations")
                .join(dated)
                .join(thread_id)
                .to_string_lossy()
                .into_owned(),
        );
    }
    roots
}

/// Explicit wire fields shared by turn/start and thread/settings/update.
pub(super) struct PermissionFields {
    pub(super) approval_policy: Option<Value>,
    pub(super) approvals_reviewer: Option<String>,
    pub(super) sandbox_policy: Option<Value>,
    pub(super) permissions: Option<String>,
    pub(super) runtime_workspace_roots: Option<Vec<String>>,
}

pub(super) fn permission_fields(
    mode: AgentPermissionMode,
    cwd: &Path,
    thread_id: &str,
    existing_thread_update: bool,
) -> Result<PermissionFields> {
    let roots = workspace_roots(cwd, thread_id);
    let (approval_policy, approvals_reviewer, sandbox_policy, permissions, runtime_workspace_roots) =
        match mode {
            AgentPermissionMode::Request => (
                Some(json!("on-request")),
                Some("user".into()),
                None,
                Some(":workspace".into()),
                None,
            ),
            AgentPermissionMode::Assist => (
                Some(json!("on-request")),
                Some("auto_review".into()),
                None,
                Some(":workspace".into()),
                None,
            ),
            AgentPermissionMode::Full => (
                Some(json!("never")),
                Some("user".into()),
                None,
                Some(":danger-full-access".into()),
                (!existing_thread_update).then_some(roots),
            ),
            // The new thread inherits resolved server defaults. Existing threads are
            // resolved by the manager before invoking settings/update.
            AgentPermissionMode::Custom => (None, None, None, None, None),
            AgentPermissionMode::Profile(id) => (None, None, None, Some(id), None),
        };
    Ok(PermissionFields {
        approval_policy,
        approvals_reviewer,
        sandbox_policy,
        permissions,
        runtime_workspace_roots,
    })
}

pub(super) fn thread_settings_update_request(
    id: u64,
    thread_id: &str,
    cwd: &Path,
    mode: AgentPermissionMode,
) -> Result<Value> {
    let PermissionFields {
        approval_policy,
        approvals_reviewer,
        sandbox_policy,
        permissions,
        ..
    } = permission_fields(mode, cwd, thread_id, true)?;
    let mut params = serde_json::Map::new();
    params.insert("threadId".into(), json!(thread_id));
    if let Some(policy) = approval_policy {
        params.insert("approvalPolicy".into(), policy);
    }
    if let Some(reviewer) = approvals_reviewer {
        params.insert("approvalsReviewer".into(), json!(reviewer));
    }
    if let Some(profile) = permissions {
        params.insert("permissions".into(), json!(profile));
    } else if let Some(sandbox) = sandbox_policy {
        params.insert("sandboxPolicy".into(), sandbox);
    }
    Ok(json!({ "method": "thread/settings/update", "id": id, "params": params }))
}
