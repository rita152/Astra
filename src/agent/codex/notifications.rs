//! Connection and thread notification decoding.

use anyhow::{Context as _, Result, anyhow, bail};
use async_channel::Sender;
use serde::Deserialize;
use serde_json::Value;

use super::methods::summarize_json;
use crate::agent::{
    AgentAccountRateLimits, AgentActivePermissionProfile, AgentConfigWarning, AgentCreditsSnapshot,
    AgentEffectivePermissions, AgentEvent, AgentMcpServerStartupFailureReason,
    AgentMcpServerStartupState, AgentMcpServerStartupStatus, AgentRateLimitWindow,
    AgentSpendControlLimit, AgentThreadActiveFlag, AgentThreadSettings, AgentThreadStatus,
    AgentThreadStatusState, AgentThreadTokenUsage, AgentTokenUsageBreakdown,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ThreadTokenUsageUpdatedNotification {
    pub(super) thread_id: String,
    pub(super) turn_id: String,
    pub(super) token_usage: ThreadTokenUsage,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ThreadTokenUsage {
    pub(super) total: TokenUsageBreakdown,
    pub(super) last: TokenUsageBreakdown,
    pub(super) model_context_window: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TokenUsageBreakdown {
    pub(super) total_tokens: i64,
    pub(super) input_tokens: i64,
    pub(super) cached_input_tokens: i64,
    #[serde(default)]
    pub(super) cache_write_input_tokens: i64,
    pub(super) output_tokens: i64,
    pub(super) reasoning_output_tokens: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AccountRateLimitsUpdatedNotification {
    pub(super) rate_limits: RateLimitSnapshot,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RateLimitSnapshot {
    #[serde(default)]
    pub(super) limit_id: Option<String>,
    #[serde(default)]
    pub(super) limit_name: Option<String>,
    #[serde(default)]
    pub(super) primary: Option<RateLimitWindow>,
    #[serde(default)]
    pub(super) secondary: Option<RateLimitWindow>,
    #[serde(default)]
    pub(super) credits: Option<CreditsSnapshot>,
    #[serde(default)]
    pub(super) individual_limit: Option<SpendControlLimitSnapshot>,
    #[serde(default)]
    pub(super) spend_control_reached: Option<bool>,
    #[serde(default)]
    pub(super) plan_type: Option<String>,
    #[serde(default)]
    pub(super) rate_limit_reached_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RateLimitWindow {
    pub(super) used_percent: i32,
    #[serde(default)]
    pub(super) window_duration_mins: Option<i64>,
    #[serde(default)]
    pub(super) resets_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CreditsSnapshot {
    pub(super) has_credits: bool,
    pub(super) unlimited: bool,
    #[serde(default)]
    pub(super) balance: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SpendControlLimitSnapshot {
    pub(super) limit: String,
    pub(super) used: String,
    pub(super) remaining_percent: i32,
    pub(super) resets_at: i64,
}

pub(super) fn validate_remote_control_status_changed(message: &Value) -> Result<()> {
    let status = required_notification_string(message, "status")?;
    if !matches!(
        status.as_str(),
        "disabled" | "connecting" | "connected" | "errored"
    ) {
        bail!("remoteControl/status/changed 通知字段 params.status 为未知状态 `{status}`");
    }
    let _server_name = required_notification_string(message, "serverName")?;
    let _installation_id = required_notification_string(message, "installationId")?;
    let _environment_id = required_nullable_notification_string(message, "environmentId")?;
    Ok(())
}

pub(super) fn parse_mcp_server_startup_status_updated(
    message: &Value,
) -> Result<AgentMcpServerStartupStatus> {
    let thread_id = optional_string_at(message, "/params/threadId", "params.threadId")?;
    let name = required_notification_string(message, "name")?;
    let raw_state = required_notification_string(message, "status")?;
    let state = match raw_state.as_str() {
        "starting" => AgentMcpServerStartupState::Starting,
        "ready" => AgentMcpServerStartupState::Ready,
        "failed" => AgentMcpServerStartupState::Failed,
        "cancelled" => AgentMcpServerStartupState::Cancelled,
        _ => {
            bail!("mcpServer/startupStatus/updated 通知字段 params.status 为未知状态 `{raw_state}`")
        }
    };
    let error = optional_string_at(message, "/params/error", "params.error")?;
    let failure_reason = match optional_string_at(
        message,
        "/params/failureReason",
        "params.failureReason",
    )? {
        Some(reason) if reason == "reauthenticationRequired" => {
            Some(AgentMcpServerStartupFailureReason::ReauthenticationRequired)
        }
        Some(reason) => bail!(
            "mcpServer/startupStatus/updated 通知字段 params.failureReason 为未知原因 `{reason}`"
        ),
        None => None,
    };
    Ok(AgentMcpServerStartupStatus {
        thread_id,
        name,
        state,
        error,
        failure_reason,
    })
}

pub(super) fn parse_thread_status_changed(message: &Value) -> Result<AgentThreadStatus> {
    let thread_id = required_notification_string(message, "threadId")?;
    let status = message
        .pointer("/params/status")
        .and_then(Value::as_object)
        .context("thread/status/changed 通知缺少对象字段 params.status")?;
    let raw_state = status
        .get("type")
        .and_then(Value::as_str)
        .context("thread/status/changed 通知缺少字符串字段 params.status.type")?;
    let state = match raw_state {
        "notLoaded" => AgentThreadStatusState::NotLoaded,
        "idle" => AgentThreadStatusState::Idle,
        "systemError" => AgentThreadStatusState::SystemError,
        "active" => {
            let raw_flags = status
                .get("activeFlags")
                .and_then(Value::as_array)
                .context(
                    "thread/status/changed active 状态缺少数组字段 params.status.activeFlags",
                )?;
            let active_flags = raw_flags
                .iter()
                .enumerate()
                .map(|(index, flag)| match flag.as_str() {
                    Some("waitingOnApproval") => Ok(AgentThreadActiveFlag::WaitingOnApproval),
                    Some("waitingOnUserInput") => Ok(AgentThreadActiveFlag::WaitingOnUserInput),
                    Some(flag) => bail!(
                        "thread/status/changed 通知字段 params.status.activeFlags[{index}] 为未知 flag `{flag}`"
                    ),
                    None => bail!(
                        "thread/status/changed 通知字段 params.status.activeFlags[{index}] 必须是字符串"
                    ),
                })
                .collect::<Result<Vec<_>>>()?;
            AgentThreadStatusState::Active { active_flags }
        }
        _ => bail!("thread/status/changed 通知字段 params.status.type 为未知状态 `{raw_state}`"),
    };
    Ok(AgentThreadStatus { thread_id, state })
}

pub(super) fn parse_thread_token_usage_updated(message: &Value) -> Result<AgentThreadTokenUsage> {
    let params = message
        .get("params")
        .context("thread/tokenUsage/updated 通知缺少 params")?;
    let notification: ThreadTokenUsageUpdatedNotification = serde_json::from_value(params.clone())
        .context("thread/tokenUsage/updated 通知 params 不符合协议 schema")?;
    let map_breakdown = |usage: TokenUsageBreakdown| AgentTokenUsageBreakdown {
        total_tokens: usage.total_tokens,
        input_tokens: usage.input_tokens,
        cached_input_tokens: usage.cached_input_tokens,
        cache_write_input_tokens: usage.cache_write_input_tokens,
        output_tokens: usage.output_tokens,
        reasoning_output_tokens: usage.reasoning_output_tokens,
    };
    Ok(AgentThreadTokenUsage {
        thread_id: notification.thread_id,
        turn_id: notification.turn_id,
        total: map_breakdown(notification.token_usage.total),
        last: map_breakdown(notification.token_usage.last),
        model_context_window: notification.token_usage.model_context_window,
    })
}

pub(super) fn parse_account_rate_limits_updated(message: &Value) -> Result<AgentAccountRateLimits> {
    let params = message
        .get("params")
        .context("account/rateLimits/updated 通知缺少 params")?;
    let notification: AccountRateLimitsUpdatedNotification = serde_json::from_value(params.clone())
        .context("account/rateLimits/updated 通知 params 不符合协议 schema")?;
    let rate_limits = notification.rate_limits;

    if let Some(plan_type) = rate_limits.plan_type.as_deref()
        && !matches!(
            plan_type,
            "free"
                | "go"
                | "plus"
                | "pro"
                | "prolite"
                | "team"
                | "self_serve_business_prolite"
                | "self_serve_business_usage_based"
                | "business"
                | "ent26"
                | "enterprise_cbp_automation"
                | "enterprise_cbp_usage_based"
                | "enterprise"
                | "edu"
                | "edu_plus"
                | "edu_pro"
                | "unknown"
        )
    {
        bail!(
            "account/rateLimits/updated 通知字段 params.rateLimits.planType 为未知值 `{plan_type}`"
        );
    }
    if let Some(reached_type) = rate_limits.rate_limit_reached_type.as_deref()
        && !matches!(
            reached_type,
            "rate_limit_reached"
                | "workspace_owner_credits_depleted"
                | "workspace_member_credits_depleted"
                | "workspace_owner_usage_limit_reached"
                | "workspace_member_usage_limit_reached"
        )
    {
        bail!(
            "account/rateLimits/updated 通知字段 params.rateLimits.rateLimitReachedType 为未知值 `{reached_type}`"
        );
    }

    let map_window = |window: RateLimitWindow| AgentRateLimitWindow {
        used_percent: window.used_percent,
        window_duration_mins: window.window_duration_mins,
        resets_at: window.resets_at,
    };
    Ok(AgentAccountRateLimits {
        limit_id: rate_limits.limit_id,
        limit_name: rate_limits.limit_name,
        primary: rate_limits.primary.map(map_window),
        secondary: rate_limits.secondary.map(map_window),
        credits: rate_limits.credits.map(|credits| AgentCreditsSnapshot {
            has_credits: credits.has_credits,
            unlimited: credits.unlimited,
            balance: credits.balance,
        }),
        individual_limit: rate_limits
            .individual_limit
            .map(|limit| AgentSpendControlLimit {
                limit: limit.limit,
                used: limit.used,
                remaining_percent: limit.remaining_percent,
                resets_at: limit.resets_at,
            }),
        spend_control_reached: rate_limits.spend_control_reached,
        plan_type: rate_limits.plan_type,
        rate_limit_reached_type: rate_limits.rate_limit_reached_type,
    })
}

pub(super) fn required_notification_string(message: &Value, field: &str) -> Result<String> {
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("未知方法");
    match message.pointer(&format!("/params/{field}")) {
        None => bail!("{method} 通知缺少字符串字段 params.{field}"),
        Some(Value::String(value)) => Ok(value.clone()),
        Some(value) => bail!(
            "{method} 通知字段 params.{field} 必须是字符串，实际为 {}",
            summarize_json(value)
        ),
    }
}

pub(super) fn required_notification_i64(message: &Value, field: &str) -> Result<i64> {
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("未知方法");
    match message.pointer(&format!("/params/{field}")) {
        None => bail!("{method} 通知缺少整数字段 params.{field}"),
        Some(value) => value.as_i64().with_context(|| {
            format!(
                "{method} 通知字段 params.{field} 必须是整数，实际为 {}",
                summarize_json(value)
            )
        }),
    }
}

pub(super) fn required_notification_index(message: &Value, field: &str) -> Result<usize> {
    let value = required_notification_i64(message, field)?;
    usize::try_from(value).with_context(|| {
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("未知方法");
        format!("{method} 通知字段 params.{field} 必须是非负索引，实际为 {value}")
    })
}

pub(super) fn required_notification_strings(message: &Value, field: &str) -> Result<Vec<String>> {
    message
        .pointer(&format!("/params/{field}"))
        .and_then(Value::as_array)
        .with_context(|| {
            let method = message
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("未知方法");
            format!("{method} 通知缺少数组字段 params.{field}")
        })?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .with_context(|| format!("通知字段 params.{field} 必须是字符串数组"))
        })
        .collect()
}

pub(super) fn required_nullable_notification_string(
    message: &Value,
    field: &str,
) -> Result<Option<String>> {
    match message.pointer(&format!("/params/{field}")) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => {
            let method = message
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("未知方法");
            bail!("{method} 通知缺少字符串或 null 字段 params.{field}")
        }
    }
}

pub(super) fn required_string_at(message: &Value, pointer: &str, field: &str) -> Result<String> {
    message
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| {
            let method = message
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("未知方法");
            format!("{method} 通知缺少字符串字段 {field}")
        })
}

pub(super) fn thread_started_id(message: &Value) -> Result<String> {
    required_string_at(message, "/params/thread/id", "params.thread.id")
}

pub(super) fn validate_resume_goal_cleared(
    message: &Value,
    expected_thread_id: &str,
) -> Result<()> {
    if message.get("id").is_some() {
        bail!("thread/goal/cleared 在 resume bootstrap 阶段必须是通知，不能包含 id");
    }
    let thread_id = required_notification_string(message, "threadId")?;
    if thread_id != expected_thread_id {
        bail!(
            "thread/goal/cleared 通知的 thread id `{thread_id}` 与当前 resume thread `{expected_thread_id}` 不一致"
        );
    }
    Ok(())
}

pub(super) fn optional_string_at(
    message: &Value,
    pointer: &str,
    field: &str,
) -> Result<Option<String>> {
    match message.pointer(pointer) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => {
            let method = message
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("未知方法");
            bail!("{method} 通知字段 {field} 必须是字符串或 null")
        }
    }
}

pub(super) fn parse_agent_notification(message: &Value) -> Result<Option<AgentEvent>> {
    let event = match message.get("method").and_then(Value::as_str) {
        Some("item/autoApprovalReview/started" | "item/autoApprovalReview/completed") => {
            AgentEvent::AutoApprovalReviewUpdated(Box::new(super::auto_approval::parse_review(
                message,
            )?))
        }
        Some("autoApprovalReview/strictReviewRequired") => {
            AgentEvent::StrictReviewRequired(super::auto_approval::parse_strict_review(message)?)
        }
        Some("guardianWarning") => {
            AgentEvent::GuardianWarning(super::auto_approval::parse_guardian_warning(message)?)
        }
        Some("turn/started") => {
            let _ = required_notification_string(message, "threadId")?;
            let _ = required_string_at(message, "/params/turn/id", "params.turn.id")?;
            let status = required_string_at(message, "/params/turn/status", "params.turn.status")?;
            if status != "inProgress" {
                bail!(
                    "turn/started 通知的 params.turn.status 必须是 `inProgress`，实际为 `{status}`"
                );
            }
            AgentEvent::Started
        }
        Some("error") => {
            let _ = required_notification_string(message, "threadId")?;
            let _ = required_notification_string(message, "turnId")?;
            AgentEvent::Error {
                message: required_string_at(
                    message,
                    "/params/error/message",
                    "params.error.message",
                )?,
                details: optional_string_at(
                    message,
                    "/params/error/additionalDetails",
                    "params.error.additionalDetails",
                )?,
                will_retry: message
                    .pointer("/params/willRetry")
                    .and_then(Value::as_bool)
                    .context("error 通知缺少布尔字段 params.willRetry")?,
            }
        }
        Some("thread/settings/updated") => {
            let _ = required_notification_string(message, "threadId")?;
            let permissions = match message.pointer("/params/threadSettings/approvalPolicy") {
                None => None,
                Some(_) => {
                    let active_permission_profile =
                        match message.pointer("/params/threadSettings/activePermissionProfile") {
                            None | Some(Value::Null) => None,
                            Some(profile) => Some(AgentActivePermissionProfile {
                                id: profile
                                    .get("id")
                                    .and_then(Value::as_str)
                                    .context("activePermissionProfile.id 必须是字符串")?
                                    .to_owned(),
                                extends: optional_string_at(
                                    message,
                                    "/params/threadSettings/activePermissionProfile/extends",
                                    "activePermissionProfile.extends",
                                )?,
                            }),
                        };
                    Some(AgentEffectivePermissions {
                        approval_policy: parse_approval_policy(
                            message
                                .pointer("/params/threadSettings/approvalPolicy")
                                .context("缺少 approvalPolicy")?,
                        )?,
                        approvals_reviewer: required_string_at(
                            message,
                            "/params/threadSettings/approvalsReviewer",
                            "params.threadSettings.approvalsReviewer",
                        )?,
                        sandbox_policy: message
                            .pointer("/params/threadSettings/sandboxPolicy")
                            .filter(|value| !value.is_null())
                            .cloned(),
                        active_permission_profile,
                    })
                }
            };
            AgentEvent::ThreadSettingsUpdated(AgentThreadSettings {
                model: required_string_at(
                    message,
                    "/params/threadSettings/model",
                    "params.threadSettings.model",
                )?,
                effort: optional_string_at(
                    message,
                    "/params/threadSettings/effort",
                    "params.threadSettings.effort",
                )?,
                service_tier: optional_string_at(
                    message,
                    "/params/threadSettings/serviceTier",
                    "params.threadSettings.serviceTier",
                )?,
                cwd: required_string_at(
                    message,
                    "/params/threadSettings/cwd",
                    "params.threadSettings.cwd",
                )?,
                permissions,
            })
        }
        Some("mcpServer/startupStatus/updated") => AgentEvent::McpServerStartupStatusUpdated(
            parse_mcp_server_startup_status_updated(message)?,
        ),
        Some("thread/status/changed") => {
            AgentEvent::ThreadStatusChanged(parse_thread_status_changed(message)?)
        }
        Some("thread/tokenUsage/updated") => {
            AgentEvent::ThreadTokenUsageUpdated(parse_thread_token_usage_updated(message)?)
        }
        Some("account/rateLimits/updated") => {
            AgentEvent::AccountRateLimitsUpdated(parse_account_rate_limits_updated(message)?)
        }
        Some("warning") => {
            let _ = optional_string_at(message, "/params/threadId", "params.threadId")?;
            AgentEvent::Warning {
                message: required_notification_string(message, "message")?,
            }
        }
        Some("configWarning") => {
            let range = match message.pointer("/params/range") {
                None | Some(Value::Null) => None,
                Some(Value::Object(_)) => Some((
                    message
                        .pointer("/params/range/start/line")
                        .and_then(Value::as_u64)
                        .context("configWarning 通知缺少无符号整数字段 params.range.start.line")?,
                    message
                        .pointer("/params/range/start/column")
                        .and_then(Value::as_u64)
                        .context(
                            "configWarning 通知缺少无符号整数字段 params.range.start.column",
                        )?,
                )),
                Some(_) => bail!("configWarning 通知字段 params.range 必须是对象或 null"),
            };
            AgentEvent::ConfigWarning(AgentConfigWarning {
                summary: required_notification_string(message, "summary")?,
                details: optional_string_at(message, "/params/details", "params.details")?,
                path: optional_string_at(message, "/params/path", "params.path")?,
                line: range.map(|(line, _)| line),
                column: range.map(|(_, column)| column),
            })
        }
        Some("model/rerouted") => {
            let _ = required_notification_string(message, "threadId")?;
            let _ = required_notification_string(message, "turnId")?;
            AgentEvent::ModelRerouted {
                from_model: required_notification_string(message, "fromModel")?,
                to_model: required_notification_string(message, "toModel")?,
                reason: required_notification_string(message, "reason")?,
            }
        }
        Some("model/verification") => {
            let _ = required_notification_string(message, "threadId")?;
            let _ = required_notification_string(message, "turnId")?;
            AgentEvent::ModelVerificationRequired {
                verifications: required_notification_strings(message, "verifications")?,
            }
        }
        Some("model/safetyBuffering/updated") => {
            let _ = required_notification_string(message, "threadId")?;
            let _ = required_notification_string(message, "turnId")?;
            AgentEvent::ModelSafetyBufferingUpdated {
                model: required_notification_string(message, "model")?,
                use_cases: required_notification_strings(message, "useCases")?,
                reasons: required_notification_strings(message, "reasons")?,
                show_buffering_ui: message
                    .pointer("/params/showBufferingUi")
                    .and_then(Value::as_bool)
                    .context(
                        "model/safetyBuffering/updated 通知缺少布尔字段 params.showBufferingUi",
                    )?,
                faster_model: required_nullable_notification_string(message, "fasterModel")?,
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(event))
}

pub(super) fn forward_agent_notification(
    message: &Value,
    events: &Sender<AgentEvent>,
) -> Result<()> {
    if let Some(event) = parse_agent_notification(message)? {
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .context("已解析的 AgentEvent 缺少字符串 JSON-RPC method")?;
        events
            .send_blocking(event)
            .map_err(|_| anyhow!("Composer `{method}` 事件通道已经关闭"))?;
    }
    Ok(())
}

pub(super) fn turn_failure_message(message: &Value) -> Result<String> {
    let message_text = optional_string_at(
        message,
        "/params/turn/error/message",
        "params.turn.error.message",
    )?
    .filter(|message| !message.trim().is_empty())
    .unwrap_or_else(|| "Codex turn 失败".to_owned());
    let details = optional_string_at(
        message,
        "/params/turn/error/additionalDetails",
        "params.turn.error.additionalDetails",
    )?
    .filter(|details| !details.trim().is_empty());
    Ok(match details {
        Some(details) if !message_text.contains(&details) => format!("{message_text}\n{details}"),
        _ => message_text,
    })
}

fn parse_approval_policy(value: &Value) -> anyhow::Result<Value> {
    // AskForApproval is an open string or granular object union. Preserve the
    // structured policy; unknown future strings must not become permissive defaults.
    if value.is_string() || value.get("granular").is_some_and(Value::is_object) {
        Ok(value.clone())
    } else {
        anyhow::bail!("approvalPolicy 必须是字符串或 granular 策略对象")
    }
}

/// Lifecycle responses use `reasoningEffort`/`sandbox`; notifications use
/// `effort`/`sandboxPolicy`. Normalize both through the same settings decoder.
pub(super) fn lifecycle_settings(
    response: &Value,
) -> anyhow::Result<Option<crate::agent::AgentThreadSettings>> {
    let Some(result) = response
        .get("result")
        .filter(|result| result.get("model").is_some())
    else {
        return Ok(None);
    };
    let message = serde_json::json!({"method":"thread/settings/updated","params":{"threadId":result.pointer("/thread/id"),"threadSettings":{
        "model":result.get("model"),"effort":result.get("reasoningEffort"),"serviceTier":result.get("serviceTier"),"cwd":result.get("cwd"),
        "approvalPolicy":result.get("approvalPolicy"),"approvalsReviewer":result.get("approvalsReviewer"),"sandboxPolicy":result.get("sandbox"),"activePermissionProfile":result.get("activePermissionProfile")
    }}});
    match parse_agent_notification(&message)? {
        Some(crate::agent::AgentEvent::ThreadSettingsUpdated(settings)) => Ok(Some(settings)),
        _ => anyhow::bail!("线程响应未包含有效设置"),
    }
}
