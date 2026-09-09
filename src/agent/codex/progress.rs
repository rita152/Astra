//! Plan, search and interruptible wait decoding, shared by live and history paths.
use super::{items::required_item_string, notifications::required_notification_string};
use crate::agent::{
    AgentActivityStatus, AgentEvent, AgentPlan, AgentPlanStep, AgentPlanStepStatus, AgentSleep,
    AgentTurnPlan, AgentWebSearch, ThreadHistoryItem,
};
use anyhow::{Context as _, Result, bail};
use serde_json::{Map, Value};

pub(super) fn parse_progress_event(
    item: &Map<String, Value>,
    completed: bool,
) -> Result<AgentEvent> {
    let kind = required_item_string(item, "activity", "type")?;
    let id = required_item_string(item, &kind, "id")?;
    let status = if completed {
        AgentActivityStatus::Completed
    } else {
        AgentActivityStatus::InProgress
    };
    Ok(match kind.as_str() {
        "plan" => AgentEvent::PlanUpdated(AgentPlan {
            id,
            text: required_item_string(item, "plan", "text")?,
            status,
        }),
        "sleep" => AgentEvent::SleepUpdated(AgentSleep {
            id,
            duration_ms: item
                .get("durationMs")
                .and_then(Value::as_u64)
                .context("sleep.durationMs 必须是 uint64")?,
            status,
        }),
        "webSearch" => {
            let action = item.get("action").cloned().unwrap_or(Value::Null);
            validate_search_action(&action)?;
            let results = item.get("results").cloned().unwrap_or(Value::Null);
            if !results.is_null() && !results.is_array() {
                bail!("webSearch.results 必须是数组或 null");
            }
            AgentEvent::WebSearchUpdated(AgentWebSearch {
                id,
                query: required_item_string(item, "webSearch", "query")?,
                action,
                results,
                status,
                extra: item
                    .iter()
                    .filter(|(key, _)| {
                        !["id", "type", "query", "action", "results"].contains(&key.as_str())
                    })
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            })
        }
        _ => bail!("未接入的活动 {kind}"),
    })
}

fn nullable_string(value: Option<&Value>, field: &str) -> Result<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        _ => bail!("{field} 必须是字符串或 null"),
    }
}

fn validate_search_action(action: &Value) -> Result<()> {
    if action.is_null() {
        return Ok(());
    }
    let kind = action
        .get("type")
        .and_then(Value::as_str)
        .context("webSearch.action.type 必须是字符串")?;
    match kind {
        "search" => {
            nullable_string(action.get("query"), "action.query")?;
            if let Some(queries) = action.get("queries").filter(|v| !v.is_null()) {
                let values = queries
                    .as_array()
                    .context("action.queries 必须是数组或 null")?;
                if values.iter().any(|v| !v.is_string()) {
                    bail!("action.queries 项必须是字符串");
                }
            }
        }
        "openPage" => {
            nullable_string(action.get("url"), "action.url")?;
        }
        "findInPage" => {
            nullable_string(action.get("url"), "action.url")?;
            nullable_string(action.get("pattern"), "action.pattern")?;
        }
        "other" => {}
        _ => bail!("未知 webSearch.action.type `{kind}`"),
    }
    Ok(())
}

pub(super) fn parse_turn_plan(message: &Value) -> Result<AgentTurnPlan> {
    let steps = message
        .pointer("/params/plan")
        .and_then(Value::as_array)
        .context("turn/plan/updated.plan 必须是数组")?
        .iter()
        .map(|step| {
            let status = match step.get("status").and_then(Value::as_str) {
                Some("pending") => AgentPlanStepStatus::Pending,
                Some("inProgress") => AgentPlanStepStatus::InProgress,
                Some("completed") => AgentPlanStepStatus::Completed,
                _ => bail!("未知 plan step.status"),
            };
            Ok(AgentPlanStep {
                step: step
                    .get("step")
                    .and_then(Value::as_str)
                    .context("plan step.step 必须是字符串")?
                    .to_owned(),
                status,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(AgentTurnPlan {
        turn_id: required_notification_string(message, "turnId")?,
        explanation: nullable_string(message.pointer("/params/explanation"), "explanation")?,
        steps,
    })
}

pub(super) fn parse_progress_history(value: &Value) -> Result<ThreadHistoryItem> {
    // ThreadItem has no per-item lifecycle field. A persisted item is a snapshot;
    // do not invent elapsed time or reconstruct step progress from plan text.
    Ok(
        match parse_progress_event(value.as_object().context("activity 必须是对象")?, true)? {
            AgentEvent::PlanUpdated(v) => ThreadHistoryItem::Plan(v),
            AgentEvent::WebSearchUpdated(v) => ThreadHistoryItem::WebSearch(v),
            AgentEvent::SleepUpdated(v) => ThreadHistoryItem::Sleep(v),
            _ => unreachable!(),
        },
    )
}

/// Only these informational updates can be discarded after their owner turn ends.
/// Unknown items and methods keep the adapter's fail-fast behavior.
pub(super) fn is_progress_notification(message: &Value) -> bool {
    match message.get("method").and_then(Value::as_str) {
        Some("item/plan/delta" | "turn/plan/updated") => true,
        Some("item/started" | "item/completed") => matches!(
            message.pointer("/params/item/type").and_then(Value::as_str),
            Some("plan" | "webSearch" | "sleep")
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
