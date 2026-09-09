//! CLI 0.153 automatic-review notification decoding, separate from routing.

use anyhow::{Context as _, Result, bail};
use serde_json::Value;

use super::requests::parse_permission_request_profile;
use crate::agent::{
    AgentAutoApprovalReview, AgentAutoApprovalReviewAction as Action, AgentAutoApprovalReviewKey,
    AgentAutoApprovalReviewStatus as Status, AgentGuardianWarning, AgentStrictReviewRequirement,
};

#[cfg(test)]
mod tests;

pub(super) const REVIEW_METHODS: &[&str] = &[
    "item/autoApprovalReview/started",
    "item/autoApprovalReview/completed",
    "autoApprovalReview/strictReviewRequired",
];

fn string(value: &Value, field: &str) -> Result<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("auto approval 字段 {field} 必须是字符串"))
}

fn optional_string(value: &Value, field: &str) -> Result<Option<String>> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => bail!("auto approval 字段 {field} 必须是字符串或 null"),
    }
}

fn integer(value: &Value, field: &str) -> Result<i64> {
    value
        .get(field)
        .and_then(Value::as_i64)
        .with_context(|| format!("auto approval 字段 {field} 必须是 int64"))
}

fn strings(value: &Value, field: &str) -> Result<Vec<String>> {
    value
        .get(field)
        .and_then(Value::as_array)
        .with_context(|| format!("auto approval 字段 {field} 必须是数组"))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .context("数组元素必须是字符串")
        })
        .collect()
}

fn choice(value: String, choices: &[&str], field: &str) -> Result<String> {
    if !choices.contains(&value.as_str()) {
        bail!("auto approval 未知 {field}: {value}");
    }
    Ok(value)
}

fn absolute(path: String) -> Result<String> {
    let p = std::path::Path::new(&path);
    if !p.is_absolute()
        || p.components().any(|c| {
            matches!(
                c,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
    {
        bail!("auto approval 路径必须是规范化绝对路径: {path}");
    }
    Ok(path)
}

fn action(value: &Value) -> Result<Action> {
    let cwd = || absolute(string(value, "cwd")?);
    let source = || {
        choice(
            string(value, "source")?,
            &["shell", "unifiedExec"],
            "source",
        )
    };
    Ok(match string(value, "type")?.as_str() {
        "command" => Action::Command {
            command: string(value, "command")?,
            cwd: cwd()?,
            source: source()?,
        },
        "execve" => Action::Execve {
            program: string(value, "program")?,
            argv: strings(value, "argv")?,
            cwd: cwd()?,
            source: source()?,
        },
        // writeStdin.cwd is LegacyAppPathString, not AbsolutePathBuf.
        "writeStdin" => Action::WriteStdin {
            approval_id: string(value, "approvalId")?,
            process_id: string(value, "processId")?,
            stdin: string(value, "stdin")?,
            cwd: string(value, "cwd")?,
        },
        "applyPatch" => Action::ApplyPatch {
            cwd: cwd()?,
            files: strings(value, "files")?
                .into_iter()
                .map(absolute)
                .collect::<Result<_>>()?,
        },
        "networkAccess" => Action::NetworkAccess {
            host: string(value, "host")?,
            port: u16::try_from(integer(value, "port")?).context("port 必须是 uint16")?,
            protocol: choice(
                string(value, "protocol")?,
                &["http", "https", "socks5Tcp", "socks5Udp"],
                "protocol",
            )?,
            target: string(value, "target")?,
        },
        "mcpToolCall" => Action::McpToolCall {
            server: string(value, "server")?,
            tool_name: string(value, "toolName")?,
            tool_title: optional_string(value, "toolTitle")?,
            connector_id: optional_string(value, "connectorId")?,
            connector_name: optional_string(value, "connectorName")?,
        },
        "requestPermissions" => Action::RequestPermissions {
            permissions: Box::new(parse_permission_request_profile(
                value
                    .get("permissions")
                    .context("action.permissions 缺失")?,
            )?),
            reason: optional_string(value, "reason")?,
        },
        kind => bail!("auto approval 未知动作 {kind}"),
    })
}

pub(super) fn parse_review(message: &Value) -> Result<AgentAutoApprovalReview> {
    let params = message.get("params").context("auto approval 缺少 params")?;
    let review = params.get("review").context("auto approval 缺少 review")?;
    let status = match string(review, "status")?.as_str() {
        "inProgress" => Status::InProgress,
        "approved" => Status::Approved,
        "denied" => Status::Denied,
        "timedOut" => Status::TimedOut,
        "aborted" => Status::Aborted,
        value => bail!("auto approval 未知状态 {value}"),
    };
    let completed = message.get("method").and_then(Value::as_str) == Some(REVIEW_METHODS[1]);
    let completed_at_ms = completed
        .then(|| integer(params, "completedAtMs"))
        .transpose()?;
    let decision_source = completed
        .then(|| {
            choice(
                string(params, "decisionSource")?,
                &["agent"],
                "decisionSource",
            )
        })
        .transpose()?;
    Ok(AgentAutoApprovalReview {
        key: AgentAutoApprovalReviewKey {
            thread_id: string(params, "threadId")?,
            turn_id: string(params, "turnId")?,
            review_id: string(params, "reviewId")?,
        },
        target_item_id: optional_string(params, "targetItemId")?,
        action: action(params.get("action").context("auto approval 缺少 action")?)?,
        status,
        rationale: optional_string(review, "rationale")?,
        risk_level: optional_string(review, "riskLevel")?
            .map(|s| choice(s, &["low", "medium", "high", "critical"], "riskLevel"))
            .transpose()?,
        user_authorization: optional_string(review, "userAuthorization")?
            .map(|s| {
                choice(
                    s,
                    &["unknown", "low", "medium", "high"],
                    "userAuthorization",
                )
            })
            .transpose()?,
        started_at_ms: integer(params, "startedAtMs")?,
        completed_at_ms,
        decision_source,
    })
}

pub(super) fn parse_strict_review(message: &Value) -> Result<AgentStrictReviewRequirement> {
    let params = message.get("params").context("strict review 缺少 params")?;
    Ok(AgentStrictReviewRequirement {
        thread_id: string(params, "threadId")?,
        turn_id: string(params, "turnId")?,
        started_at_ms: integer(params, "startedAtMs")?,
    })
}

pub(super) fn parse_guardian_warning(message: &Value) -> Result<AgentGuardianWarning> {
    let params = message
        .get("params")
        .context("guardianWarning 缺少 params")?;
    Ok(AgentGuardianWarning {
        thread_id: string(params, "threadId")?,
        message: string(params, "message")?,
    })
}
