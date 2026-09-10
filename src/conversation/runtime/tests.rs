use super::*;
use crate::agent::{AgentAuthRecovery, AgentHookPromptFragment, AgentLocalClosure};

fn runtime(generation: u64, observation: Observation) -> AgentConnectionEvent {
    AgentConnectionEvent::Runtime(AgentRuntimeEvent {
        generation,
        observation,
    })
}
fn auth(thread: &str, turn: &str, completed: bool) -> Observation {
    Observation::AuthRecovery(AgentAuthRecovery {
        thread_id: thread.into(),
        turn_id: turn.into(),
        provider: "provider".into(),
        started_message: (!completed).then(|| "restoring".into()),
        completed_message: completed.then(|| "restored; request still pending".into()),
        closed_locally: None,
    })
}
fn prompt(completed: Option<bool>) -> AgentHookPrompt {
    AgentHookPrompt {
        id: "prompt".into(),
        completed,
        fragments: vec![
            AgentHookPromptFragment {
                hook_run_id: "run-b".into(),
                text: "first".into(),
            },
            AgentHookPromptFragment {
                hook_run_id: "run-a".into(),
                text: "second".into(),
            },
        ],
    }
}

#[test]
fn runtime_early_snapshot_is_applied_only_after_real_thread_identity() {
    let mut state = ConversationState::default();
    state.apply_connection_event(runtime(1, Observation::GenerationStarted));
    state.apply_connection_event(runtime(1, auth("a", "ta", false)));
    state.apply_connection_event(runtime(1, auth("b", "tb", false)));
    assert!(state.runtime.auth_recoveries.is_empty());
    state.set_workspace_context("/tmp".into(), None, Some("b".into()));
    assert_eq!(state.runtime.auth_recoveries.len(), 1);
    assert_eq!(state.runtime.auth_recoveries[0].thread_id, "b");
    assert_eq!(state.turn_id, None);
    assert_eq!(state.phase, ConversationPhase::Empty);
    state.apply_connection_event(runtime(1, auth("a", "ta", true)));
    assert!(state.runtime.auth_recoveries[0].is_waiting());
    state.set_workspace_context("/tmp".into(), None, Some("c".into()));
    assert!(state.runtime.auth_recoveries.is_empty());
}

#[test]
fn runtime_recovery_result_never_finishes_turn_or_overwrites_model_status() {
    let mut state = ConversationState {
        thread_id: Some("a".into()),
        turn_id: Some("ta".into()),
        phase: ConversationPhase::Streaming,
        model_status: Some("model status".into()),
        ..Default::default()
    };
    state.apply_connection_event(runtime(1, Observation::GenerationStarted));
    state.apply_connection_event(runtime(1, auth("a", "ta", false)));
    state.apply_connection_event(runtime(1, auth("a", "tb", false)));
    state.apply_connection_event(runtime(1, auth("a", "ta", true)));
    state.apply_connection_event(runtime(1, auth("a", "ta", false)));
    assert_eq!(state.phase, ConversationPhase::Streaming);
    assert_eq!(state.model_status.as_deref(), Some("model status"));
    assert!(!state.runtime.auth_recoveries[0].is_waiting());
    assert!(state.runtime.auth_recoveries[1].is_waiting());
    state.apply_agent_event_batch(vec![AgentEvent::Interrupted]);
    assert!(state.runtime.auth_recoveries[1].is_waiting());
    assert_eq!(
        state.runtime.auth_recoveries[0]
            .completed_message
            .as_deref(),
        Some("restored; request still pending")
    );
}

#[test]
fn runtime_prompt_is_separate_ordered_and_monotonic_across_snapshot_and_item_delivery() {
    let mut state = ConversationState {
        thread_id: Some("a".into()),
        ..Default::default()
    };
    state.begin_prompt("human input");
    state.apply_connection_event(runtime(1, Observation::GenerationStarted));
    state.apply_connection_event(runtime(
        1,
        Observation::HookPrompt(crate::agent::AgentScopedHookPrompt {
            thread_id: "a".into(),
            turn_id: "ta".into(),
            prompt: prompt(Some(true)),
        }),
    ));
    assert!(state.activities.is_empty());
    assert_eq!(state.turn_id, None);
    state.apply_agent_event_batch(vec![
        AgentEvent::TurnReady(crate::agent::AgentTurnIdentity {
            generation: 1,
            thread_id: "a".into(),
            turn_id: "ta".into(),
        }),
        AgentEvent::HookPromptUpdated(prompt(Some(false))),
    ]);
    assert_eq!(state.activities.len(), 1);
    assert_eq!(
        state.activities[0],
        ConversationActivity::HookPrompt(crate::agent::AgentScopedHookPrompt {
            thread_id: "a".into(),
            turn_id: "ta".into(),
            prompt: prompt(Some(true))
        })
    );
    assert_eq!(state.user_message.as_deref(), Some("human input"));
    assert!(state.assistant_message.is_empty());
    assert!(
        state.approval_responders.is_empty()
            && state.user_input_responders.is_empty()
            && state.server_request_contexts.is_empty()
    );
}

#[test]
fn runtime_disconnect_and_rebuild_preserve_observed_results_without_replaying_old_waits() {
    let mut state = ConversationState {
        thread_id: Some("a".into()),
        ..Default::default()
    };
    state.apply_connection_event(runtime(1, Observation::GenerationStarted));
    state.apply_connection_event(runtime(1, auth("a", "old", false)));
    state.apply_connection_event(runtime(1, Observation::Disconnected));
    assert_eq!(
        state.runtime.auth_recoveries[0].closed_locally,
        Some(AgentLocalClosure::Disconnected)
    );
    assert_eq!(state.runtime.auth_recoveries[0].completed_message, None);
    state.apply_connection_event(runtime(2, Observation::GenerationStarted));
    assert_eq!(state.retired_runtime.len(), 1);
    assert!(!state.retired_runtime[0].auth_recoveries[0].is_waiting());
    assert!(state.runtime.auth_recoveries.is_empty());
    state.apply_connection_event(runtime(1, auth("a", "old", true)));
    assert!(state.runtime.auth_recoveries.is_empty());
    state.apply_connection_event(runtime(2, auth("a", "new", false)));
    assert_eq!(state.runtime.auth_recoveries[0].turn_id, "new");
}

#[test]
fn runtime_deprecation_survives_prompt_lifecycle_without_becoming_turn_content() {
    let mut state = ConversationState::default();
    let notice = AgentConnectionEvent::DeprecationNotice(crate::agent::AgentDeprecationNotice {
        summary: "Deprecated".into(),
        details: Some("Migration details".into()),
    });
    assert!(state.apply_connection_event(notice.clone()));
    assert!(!state.apply_connection_event(notice));
    state.begin_prompt("question");
    state.apply_agent_event_batch(vec![AgentEvent::Completed]);
    state.begin_prompt("next");
    assert_eq!(state.deprecation_notices.len(), 1);
    assert_eq!(
        state.deprecation_notices[0].details.as_deref(),
        Some("Migration details")
    );
    assert!(state.activities.is_empty());
    assert!(state.transcript.iter().all(|t| t.activities.is_empty()));
}

#[test]
fn generation_rebuild_discards_deferred_permission_snapshots() {
    let settings = |generation, model: &str| AgentConnectionEvent::ThreadSettingsUpdated {
        thread_id: "a".into(),
        generation,
        settings: crate::agent::AgentThreadSettings {
            model: model.into(),
            effort: None,
            service_tier: None,
            cwd: "/tmp".into(),
            permissions: None,
        },
    };
    let mut state = ConversationState::default();
    state.apply_connection_event(runtime(1, Observation::GenerationStarted));
    state.apply_connection_event(settings(1, "retired-model"));
    state.apply_connection_event(runtime(2, Observation::GenerationStarted));
    state.selected_model = "current-model".into();
    state.set_workspace_context("/tmp".into(), None, Some("a".into()));
    assert_eq!(state.selected_model, "current-model");
    assert!(!state.apply_connection_event(settings(1, "late-model")));
    assert!(state.apply_connection_event(settings(2, "server-model")));
    assert_eq!(state.selected_model, "server-model");
}
