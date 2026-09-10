//! CLI 0.153 runtime codecs. Notification policy is deliberately exact-name only.

use anyhow::{Context as _, Result, bail};
use serde_json::Value;

use crate::agent::{
    AgentAuthRecovery, AgentDeprecationNotice, AgentHookOutput, AgentHookPrompt,
    AgentHookPromptFragment, AgentHookRun, AgentHookStatus, AgentRuntimeObservation,
};

/// No goals, server-side queue, skill/app catalog, or moderation-metadata product
/// exists in this client. Context compaction is driven by its item, not the
/// deprecated duplicate notification. Never apply this policy to requests.
pub(super) const OPT_OUT_NOTIFICATION_METHODS: &[&str] = &[
    "thread/goal/updated",
    "thread/goal/cleared",
    "thread/queue/changed",
    "skills/changed",
    "app/list/updated",
    "turn/moderationMetadata",
    "thread/compacted",
];

pub(super) const RUNTIME_METHODS: &[&str] = &[
    "modelProvider/authRecoveryStarted",
    "modelProvider/authRecoveryCompleted",
    "hook/started",
    "hook/completed",
];

fn string(value: &Value, field: &str) -> Result<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("runtime {field} 必须是字符串"))
}

fn optional_string(value: &Value, field: &str) -> Result<Option<String>> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => bail!("runtime {field} 必须是字符串或 null"),
    }
}

fn integer(value: &Value, field: &str) -> Result<i64> {
    value
        .get(field)
        .and_then(Value::as_i64)
        .with_context(|| format!("runtime {field} 必须是 int64"))
}

fn optional_integer(value: &Value, field: &str) -> Result<Option<i64>> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => integer(value, field).map(Some),
    }
}

fn choice(value: &Value, field: &str, choices: &[&str]) -> Result<String> {
    let text = string(value, field)?;
    if !choices.contains(&text.as_str()) {
        bail!("runtime 未知 {field}: {text}");
    }
    Ok(text)
}

pub(super) fn parse_deprecation(message: &Value) -> Result<AgentDeprecationNotice> {
    let params = message
        .get("params")
        .context("deprecationNotice 缺少 params")?;
    Ok(AgentDeprecationNotice {
        summary: string(params, "summary")?,
        details: optional_string(params, "details")?,
    })
}

pub(super) fn parse_runtime(message: &Value) -> Result<AgentRuntimeObservation> {
    let method = string(message, "method")?;
    let params = message.get("params").context("runtime 缺少 params")?;
    let thread_id = string(params, "threadId")?;
    if matches!(
        method.as_str(),
        "modelProvider/authRecoveryStarted" | "modelProvider/authRecoveryCompleted"
    ) {
        let completed = method == "modelProvider/authRecoveryCompleted";
        let message = string(params, "message")?;
        return Ok(AgentRuntimeObservation::AuthRecovery(AgentAuthRecovery {
            thread_id,
            turn_id: string(params, "turnId")?,
            provider: string(params, "provider")?,
            started_message: (!completed).then(|| message.clone()),
            completed_message: completed.then_some(message),
            closed_locally: None,
        }));
    }
    if !matches!(method.as_str(), "hook/started" | "hook/completed") {
        bail!("未知 runtime 方法 {method}");
    }
    let run = params.get("run").context("Hook 缺少 run")?;
    let source_path = string(run, "sourcePath")?;
    let path = std::path::Path::new(&source_path);
    if !path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
    {
        bail!("Hook sourcePath 必须是规范化绝对路径");
    }
    let source = if run.get("source").is_none() {
        "unknown".to_owned()
    } else {
        choice(
            run,
            "source",
            &[
                "system",
                "user",
                "project",
                "mdm",
                "sessionFlags",
                "plugin",
                "cloudRequirements",
                "cloudManagedConfig",
                "legacyManagedConfigFile",
                "legacyManagedConfigMdm",
                "unknown",
            ],
        )?
    };
    Ok(AgentRuntimeObservation::Hook(Box::new(AgentHookRun {
        thread_id,
        turn_id: optional_string(params, "turnId")?,
        id: string(run, "id")?,
        display_order: integer(run, "displayOrder")?,
        event_name: choice(
            run,
            "eventName",
            &[
                "preToolUse",
                "permissionRequest",
                "postToolUse",
                "preCompact",
                "postCompact",
                "sessionStart",
                "sessionEnd",
                "userPromptSubmit",
                "subagentStart",
                "subagentStop",
                "stop",
                "interrupt",
            ],
        )?,
        execution_mode: choice(run, "executionMode", &["sync", "async"])?,
        handler_type: choice(
            run,
            "handlerType",
            &["command", "mcpTool", "prompt", "agent"],
        )?,
        scope: choice(run, "scope", &["thread", "turn"])?,
        source,
        source_path,
        status: match string(run, "status")?.as_str() {
            "running" => AgentHookStatus::Running,
            "completed" => AgentHookStatus::Completed,
            "failed" => AgentHookStatus::Failed,
            "blocked" => AgentHookStatus::Blocked,
            "stopped" => AgentHookStatus::Stopped,
            value => bail!("Hook 未知 status: {value}"),
        },
        status_message: optional_string(run, "statusMessage")?,
        entries: run
            .get("entries")
            .and_then(Value::as_array)
            .context("Hook entries 必须是数组")?
            .iter()
            .map(|entry| {
                Ok(AgentHookOutput {
                    kind: choice(
                        entry,
                        "kind",
                        &["warning", "stop", "feedback", "context", "error"],
                    )?,
                    text: string(entry, "text")?,
                })
            })
            .collect::<Result<_>>()?,
        started_at: integer(run, "startedAt")?,
        completed_at: optional_integer(run, "completedAt")?,
        duration_ms: optional_integer(run, "durationMs")?,
        received_completed: method == "hook/completed",
        closed_locally: None,
    })))
}

pub(super) fn parse_hook_prompt(value: &Value, completed: Option<bool>) -> Result<AgentHookPrompt> {
    if string(value, "type")? != "hookPrompt" {
        bail!("Hook prompt type 必须是 hookPrompt");
    }
    Ok(AgentHookPrompt {
        id: string(value, "id")?,
        completed,
        fragments: value
            .get("fragments")
            .and_then(Value::as_array)
            .context("hookPrompt fragments 必须是数组")?
            .iter()
            .map(|fragment| {
                Ok(AgentHookPromptFragment {
                    hook_run_id: string(fragment, "hookRunId")?,
                    text: string(fragment, "text")?,
                })
            })
            .collect::<Result<_>>()?,
    })
}

#[cfg(test)]
mod tests;
