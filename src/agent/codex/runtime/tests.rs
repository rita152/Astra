use super::*;
use crate::agent::{AgentLocalClosure, AgentRuntimeEvent, AgentRuntimeState, ThreadHistoryItem};
use serde_json::json;

pub(crate) fn hook(method: &str, thread: &str, turn: Value) -> Value {
    json!({"method":method,"params":{"threadId":thread,"turnId":turn,"run":{
        "id":"hook-1","displayOrder":-1,"eventName":"sessionStart","executionMode":"sync",
        "handlerType":"command","scope":"thread","sourcePath":"/tmp/hooks.json",
        "status":"running","entries":[],"startedAt":100
    }}})
}

fn apply(state: &mut AgentRuntimeState, generation: u64, observation: AgentRuntimeObservation) {
    state
        .apply(AgentRuntimeEvent {
            generation,
            observation,
        })
        .unwrap();
}

#[test]
fn runtime_hook_schema_enums_and_nullable_fields_are_strict() {
    let fields: &[(&str, &[&str])] = &[
        (
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
        ),
        ("executionMode", &["sync", "async"]),
        ("handlerType", &["command", "mcpTool", "prompt", "agent"]),
        ("scope", &["thread", "turn"]),
        (
            "status",
            &["running", "completed", "failed", "blocked", "stopped"],
        ),
        (
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
        ),
    ];
    for method in ["hook/started", "hook/completed"] {
        for (field, values) in fields {
            for value in *values {
                let mut message = hook(method, "a", Value::Null);
                message["params"]["run"][field] = json!(value);
                assert!(parse_runtime(&message).is_ok(), "{message}");
            }
            let mut message = hook(method, "a", Value::Null);
            message["params"]["run"][field] = json!("future");
            assert!(parse_runtime(&message).is_err(), "{field}");
            message["params"]["run"][field] = Value::Null;
            assert!(parse_runtime(&message).is_err(), "{field}");
        }
        let mut message = hook(method, "a", Value::Null);
        message["params"].as_object_mut().unwrap().remove("turnId");
        for field in ["completedAt", "durationMs", "statusMessage"] {
            message["params"]["run"][field] = Value::Null;
        }
        let AgentRuntimeObservation::Hook(parsed) = parse_runtime(&message).unwrap() else {
            panic!()
        };
        assert_eq!(parsed.turn_id, None);
        assert_eq!(parsed.source, "unknown");
        assert_eq!(parsed.received_completed, method == "hook/completed");
        for kind in ["warning", "stop", "feedback", "context", "error"] {
            message["params"]["run"]["entries"] = json!([{"kind":kind,"text":"original"}]);
            assert!(parse_runtime(&message).is_ok());
        }
        for (field, value) in [
            ("startedAt", json!(1.1)),
            ("durationMs", json!("12")),
            ("sourcePath", json!("relative")),
            ("entries", json!([{"kind":"text","text":"bad"}])),
        ] {
            let mut message = hook(method, "a", Value::Null);
            message["params"]["run"][field] = value;
            assert!(parse_runtime(&message).is_err(), "{field}");
        }
    }
}

#[test]
fn runtime_hook_completion_is_monotonic_and_turnless_runs_do_not_close_with_turn() {
    let mut state = AgentRuntimeState::default();
    apply(&mut state, 1, AgentRuntimeObservation::GenerationStarted);
    let start = hook("hook/started", "a", Value::Null);
    apply(&mut state, 1, parse_runtime(&start).unwrap());
    apply(&mut state, 1, parse_runtime(&start).unwrap());
    assert_eq!(state.hooks.len(), 1);
    apply(
        &mut state,
        1,
        AgentRuntimeObservation::TurnClosed {
            thread_id: "a".into(),
            turn_id: "t".into(),
            reason: AgentLocalClosure::Interrupted,
        },
    );
    assert!(state.hooks[0].is_waiting());
    let mut done = start.clone();
    done["method"] = json!("hook/completed");
    done["params"]["run"]["status"] = json!("blocked");
    apply(&mut state, 1, parse_runtime(&done).unwrap());
    apply(&mut state, 1, parse_runtime(&start).unwrap());
    assert_eq!(state.hooks[0].status, AgentHookStatus::Blocked);
    assert!(!state.hooks[0].is_waiting());
    let mut another_turn = done.clone();
    another_turn["params"]["turnId"] = json!("other");
    apply(&mut state, 1, parse_runtime(&another_turn).unwrap());
    assert_eq!(state.hooks.len(), 2);
    assert_eq!(state.hooks[0].turn_id, None);
    assert_eq!(state.hooks[1].turn_id.as_deref(), Some("other"));
}

#[test]
fn runtime_same_hook_run_id_in_different_turns_remains_independent() {
    let mut state = AgentRuntimeState::default();
    apply(&mut state, 1, AgentRuntimeObservation::GenerationStarted);
    for turn in [json!("first"), json!("second"), Value::Null] {
        apply(
            &mut state,
            1,
            parse_runtime(&hook("hook/started", "thread", turn)).unwrap(),
        );
    }
    apply(
        &mut state,
        1,
        AgentRuntimeObservation::TurnClosed {
            thread_id: "thread".into(),
            turn_id: "first".into(),
            reason: AgentLocalClosure::Interrupted,
        },
    );
    assert_eq!(state.hooks.len(), 3);
    assert!(!state.hooks[0].is_waiting());
    assert!(state.hooks[1].is_waiting() && state.hooks[2].is_waiting());
    let mut completed = hook("hook/completed", "thread", json!("first"));
    completed["params"]["run"]["status"] = json!("failed");
    apply(&mut state, 1, parse_runtime(&completed).unwrap());
    apply(
        &mut state,
        1,
        parse_runtime(&hook("hook/started", "thread", json!("first"))).unwrap(),
    );
    assert_eq!(state.hooks[0].status, AgentHookStatus::Failed);
    assert!(state.hooks[1].is_waiting() && state.hooks[2].is_waiting());
    let mut replay = AgentRuntimeState::default();
    for event in state.snapshot() {
        replay.apply(event).unwrap();
    }
    assert_eq!(state, replay);
}

#[test]
fn runtime_late_events_and_snapshot_replay_preserve_server_results_and_isolation() {
    let mut state = AgentRuntimeState::default();
    apply(&mut state, 1, AgentRuntimeObservation::GenerationStarted);
    apply(
        &mut state,
        1,
        AgentRuntimeObservation::TurnClosed {
            thread_id: "a".into(),
            turn_id: "t".into(),
            reason: AgentLocalClosure::Interrupted,
        },
    );
    for thread in ["a", "b"] {
        apply(
            &mut state,
            1,
            parse_runtime(&hook("hook/started", thread, json!("t"))).unwrap(),
        );
    }
    assert_eq!(
        state.hooks[0].closed_locally,
        Some(AgentLocalClosure::Interrupted)
    );
    assert!(state.hooks[1].is_waiting());
    apply(&mut state, 1, AgentRuntimeObservation::Disconnected);
    assert_eq!(state.hooks[1].status, AgentHookStatus::Running);
    assert!(!state.hooks[1].received_completed);
    let mut late = hook("hook/completed", "a", json!("t"));
    late["params"]["run"]["status"] = json!("failed");
    // A terminal payload can correct the start time after duplicate starts.
    late["params"]["run"]["startedAt"] = json!(99);
    apply(&mut state, 1, parse_runtime(&late).unwrap());
    assert_eq!(state.hooks[0].status, AgentHookStatus::Failed);
    assert_eq!(
        state.hooks[0].closed_locally,
        Some(AgentLocalClosure::Interrupted)
    );
    let mut replay = AgentRuntimeState::default();
    for event in state.snapshot() {
        replay.apply(event).unwrap();
    }
    assert_eq!(state, replay);
    apply(&mut state, 2, AgentRuntimeObservation::GenerationStarted);
    apply(&mut state, 1, parse_runtime(&late).unwrap());
    assert!(state.hooks.is_empty());
}

#[test]
fn runtime_auth_recovery_completion_is_only_a_recovery_result() {
    let mut state = AgentRuntimeState::default();
    apply(&mut state, 1, AgentRuntimeObservation::GenerationStarted);
    for thread in ["a", "b"] {
        apply(&mut state,1,parse_runtime(&json!({"method":"modelProvider/authRecoveryStarted","params":{"threadId":thread,"turnId":"t","provider":"openai","message":"Refreshing credentials"}})).unwrap());
    }
    let mut done = json!({"method":"modelProvider/authRecoveryCompleted","params":{"threadId":"a","turnId":"t","provider":"openai","message":"Retrying request"}});
    apply(&mut state, 1, parse_runtime(&done).unwrap());
    done["method"] = json!("modelProvider/authRecoveryStarted");
    apply(&mut state, 1, parse_runtime(&done).unwrap());
    assert_eq!(state.auth_recoveries.len(), 2);
    assert!(!state.auth_recoveries[0].is_waiting());
    assert!(state.auth_recoveries[1].is_waiting());
    assert_eq!(
        state.auth_recoveries[0].completed_message.as_deref(),
        Some("Retrying request")
    );
    for field in ["threadId", "turnId", "provider", "message"] {
        let mut malformed = done.clone();
        malformed["params"][field] = Value::Null;
        assert!(parse_runtime(&malformed).is_err());
    }
}

#[test]
fn runtime_hook_prompt_history_preserves_fragments_without_inventing_completion() {
    let item = json!({"type":"hookPrompt","id":"prompt","fragments":[{"hookRunId":"z","text":"first"},{"hookRunId":"a","text":"second"},{"hookRunId":"z","text":""}]});
    let live = parse_hook_prompt(&item, Some(true)).unwrap();
    let ThreadHistoryItem::HookPrompt(history) =
        super::super::workspace_protocol::parse_history_item(&item).unwrap()
    else {
        panic!()
    };
    assert_eq!(live.fragments, history.fragments);
    assert_eq!(history.completed, None);
    for fragments in [
        Value::Null,
        json!(["text"]),
        json!([{"text":"missing identity"}]),
        json!([{"hookRunId":"r","text":null}]),
    ] {
        let mut invalid = item.clone();
        invalid["fragments"] = fragments;
        assert!(parse_hook_prompt(&invalid, None).is_err());
    }
}

#[test]
fn runtime_deprecation_nullability_and_exact_notification_policy() {
    for params in [
        json!({"summary":"s"}),
        json!({"summary":"s","details":null}),
        json!({"summary":"s","details":"migration"}),
    ] {
        assert!(parse_deprecation(&json!({"method":"deprecationNotice","params":params})).is_ok());
    }
    for params in [
        json!({}),
        json!({"summary":null}),
        json!({"summary":"s","details":{}}),
    ] {
        assert!(parse_deprecation(&json!({"method":"deprecationNotice","params":params})).is_err());
    }
    assert_eq!(
        OPT_OUT_NOTIFICATION_METHODS
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        7
    );
    for method in [
        "item/started",
        "item/completed",
        "serverRequest/resolved",
        "turn/completed",
        "thread/settings/updated",
        "model/safetyBuffering/updated",
        "error",
        "item/permissions/requestApproval",
    ] {
        assert!(!OPT_OUT_NOTIFICATION_METHODS.contains(&method));
    }
}
