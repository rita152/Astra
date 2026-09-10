//! Supported server methods and strict protocol boundary validation.

use anyhow::{Result, anyhow, bail};
use serde_json::Value;

use super::notifications::{
    parse_account_rate_limits_updated, parse_mcp_server_startup_status_updated,
    parse_thread_status_changed, parse_thread_token_usage_updated, thread_started_id,
    validate_remote_control_status_changed,
};

pub(super) const UNDEFINED_METHOD_PARAMS_LIMIT: usize = 2_000;

pub(super) const TURN_SCOPED_SERVER_METHODS: &[&str] = &[
    "item/autoApprovalReview/started",
    "item/autoApprovalReview/completed",
    "autoApprovalReview/strictReviewRequired",
    "item/commandExecution/requestApproval",
    "item/fileChange/requestApproval",
    "item/permissions/requestApproval",
    "item/tool/requestUserInput",
    "tool/requestUserInput",
    "item/started",
    "item/agentMessage/delta",
    "item/plan/delta",
    "turn/plan/updated",
    "item/commandExecution/outputDelta",
    "item/commandExecution/terminalInteraction",
    "item/fileChange/outputDelta",
    "item/fileChange/patchUpdated",
    "item/reasoning/summaryPartAdded",
    "item/reasoning/summaryTextDelta",
    "item/reasoning/textDelta",
    "item/mcpToolCall/progress",
    "item/completed",
    "turn/diff/updated",
    "turn/started",
    "turn/completed",
    "error",
    "thread/tokenUsage/updated",
    "model/rerouted",
    "model/verification",
    "model/safetyBuffering/updated",
];

pub(super) fn is_defined_server_method(method: &str) -> bool {
    matches!(
        method,
        "item/autoApprovalReview/started"
            | "item/autoApprovalReview/completed"
            | "autoApprovalReview/strictReviewRequired"
            | "guardianWarning"
            | "item/commandExecution/requestApproval"
            | "item/fileChange/requestApproval"
            | "item/permissions/requestApproval"
            | "item/tool/requestUserInput"
            | "tool/requestUserInput"
            | "serverRequest/resolved"
            | "item/started"
            | "item/agentMessage/delta"
            | "item/plan/delta"
            | "turn/plan/updated"
            | "item/commandExecution/outputDelta"
            | "item/commandExecution/terminalInteraction"
            | "item/fileChange/outputDelta"
            | "item/fileChange/patchUpdated"
            | "item/reasoning/summaryPartAdded"
            | "item/reasoning/summaryTextDelta"
            | "item/reasoning/textDelta"
            | "item/mcpToolCall/progress"
            | "item/completed"
            | "thread/started"
            | "thread/archived"
            | "thread/unarchived"
            | "thread/deleted"
            | "thread/name/updated"
            | "thread/closed"
            | "thread/project/updated"
            | "project/changed"
            | "turn/started"
            | "turn/diff/updated"
            | "turn/completed"
            | "error"
            | "thread/settings/updated"
            | "warning"
            | "configWarning"
            | "model/rerouted"
            | "model/verification"
            | "model/safetyBuffering/updated"
    )
}

pub(super) fn ensure_server_method_is_defined(message: &Value) -> Result<()> {
    let Some(method) = message.get("method") else {
        return Ok(());
    };
    let Some(method) = method.as_str() else {
        bail!(
            "Codex JSON-RPC 消息的 `method` 必须是字符串：{}",
            summarize_json(method)
        );
    };
    match method {
        "deprecationNotice" => super::runtime::parse_deprecation(message).map(|_| ()),
        method if super::runtime::RUNTIME_METHODS.contains(&method) => {
            super::runtime::parse_runtime(message).map(|_| ())
        }
        "item/autoApprovalReview/started" | "item/autoApprovalReview/completed" => {
            super::auto_approval::parse_review(message).map(|_| ())
        }
        "autoApprovalReview/strictReviewRequired" => {
            super::auto_approval::parse_strict_review(message).map(|_| ())
        }
        "guardianWarning" => super::auto_approval::parse_guardian_warning(message).map(|_| ()),
        // The request response remains the canonical source of the thread id.
        // This lifecycle notification is still schema-checked and correlated
        // with that response by the active prompt session.
        "thread/started" => thread_started_id(message).map(|_| ()),
        // app-server emits this connection-level status during initialization,
        // including on short-lived model catalog connections. It has no
        // Composer UI, but its protocol payload must remain schema-checked so
        // future shape changes still fail loudly.
        "remoteControl/status/changed" => validate_remote_control_status_changed(message),
        "mcpServer/startupStatus/updated" => {
            parse_mcp_server_startup_status_updated(message).map(|_| ())
        }
        "thread/status/changed" => parse_thread_status_changed(message).map(|_| ()),
        "thread/tokenUsage/updated" => parse_thread_token_usage_updated(message).map(|_| ()),
        "account/rateLimits/updated" => parse_account_rate_limits_updated(message).map(|_| ()),
        method if is_defined_server_method(method) => Ok(()),
        method => Err(undefined_server_method_error(method, message)),
    }
}

pub(super) fn undefined_server_method_error(method: &str, message: &Value) -> anyhow::Error {
    let kind = if message.get("id").is_some() {
        "请求"
    } else {
        "通知"
    };
    let params = message
        .get("params")
        .map(summarize_json)
        .unwrap_or_else(|| "null".to_owned());
    anyhow!("遇到未定义的 Codex JSON-RPC {kind}方法 `{method}`；params={params}")
}

pub(super) fn summarize_json(value: &Value) -> String {
    let rendered = value.to_string();
    let mut characters = rendered.chars();
    let mut summary: String = characters
        .by_ref()
        .take(UNDEFINED_METHOD_PARAMS_LIMIT)
        .collect();
    if characters.next().is_some() {
        summary.push('…');
    }
    summary
}

pub(super) fn is_integrated_server_request_method(method: &str) -> bool {
    matches!(
        method,
        "item/commandExecution/requestApproval"
            | "item/fileChange/requestApproval"
            | "item/tool/requestUserInput"
            | "tool/requestUserInput"
            | "item/permissions/requestApproval"
    )
}
