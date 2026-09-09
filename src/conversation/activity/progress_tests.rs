use super::*;
use crate::agent::{
    AgentActivityStatus as S, AgentEvent as E, AgentPlan, AgentPlanStep, AgentPlanStepStatus as P,
    AgentSleep, AgentTurnPlan, AgentWebSearch,
};
use crate::conversation::{ConversationPhase, ConversationState};
use serde_json::json;
fn plan(status: S, text: &str) -> AgentPlan {
    AgentPlan {
        id: "same".into(),
        text: text.into(),
        status,
    }
}
fn search(status: S) -> AgentWebSearch {
    AgentWebSearch {
        id: "same".into(),
        query: "Rust".into(),
        action: json!({"type":"search","extra":1}),
        results: json!([{"future":true}]),
        extra: Default::default(),
        status,
    }
}
fn sleep(status: S) -> AgentSleep {
    AgentSleep {
        id: "same".into(),
        duration_ms: 15000,
        status,
    }
}
fn steps(status: P) -> AgentTurnPlan {
    AgentTurnPlan {
        turn_id: "turn".into(),
        explanation: None,
        steps: vec![AgentPlanStep {
            step: "调查".into(),
            status,
        }],
    }
}
#[test]
fn early_deltas_repeated_starts_final_plan_and_late_deltas_converge() {
    let mut state = ConversationState::default();
    state.begin_prompt("test");
    state.apply_agent_event_batch(vec![
        E::PlanDelta {
            item_id: "same".into(),
            delta: "早到".into(),
        },
        E::PlanUpdated(plan(S::InProgress, "")),
        E::PlanUpdated(plan(S::InProgress, "")),
        E::TurnPlanUpdated(steps(P::Pending)),
        E::TurnPlanUpdated(steps(P::InProgress)),
        E::TurnPlanUpdated(steps(P::InProgress)),
    ]);
    assert_eq!(state.activities.len(), 2);
    assert!(matches!(&state.activities[0],ConversationActivity::Plan(v) if v.text=="早到"));
    state.apply_agent_event_batch(vec![
        E::PlanUpdated(plan(S::Completed, "权威文本")),
        E::PlanUpdated(plan(S::Completed, "权威文本")),
        E::PlanUpdated(plan(S::InProgress, "迟到")),
        E::PlanDelta {
            item_id: "same".into(),
            delta: "迟到".into(),
        },
    ]);
    assert_eq!(state.activities.len(), 2);
    assert!(
        matches!(&state.activities[0],ConversationActivity::Plan(v) if v.text=="权威文本"&&v.status==S::Completed)
    );
    assert!(
        matches!(&state.activities[1],ConversationActivity::TurnPlan(v) if v.steps[0].status==P::InProgress)
    );
    assert_eq!(
        state.phase,
        ConversationPhase::Streaming,
        "item completion cannot finish a turn"
    );
}
#[test]
fn repeated_search_and_sleep_lifecycle_does_not_duplicate_or_reactivate() {
    for completion_first in [false, true] {
        let mut state = ConversationState::default();
        state.begin_prompt("test");
        let mut events = vec![
            E::WebSearchUpdated(search(S::Completed)),
            E::SleepUpdated(sleep(S::Completed)),
        ];
        let starts = vec![
            E::WebSearchUpdated(search(S::InProgress)),
            E::SleepUpdated(sleep(S::InProgress)),
        ];
        if completion_first {
            events.extend(starts)
        } else {
            events.splice(0..0, starts);
        }
        events.extend([
            E::WebSearchUpdated(search(S::Completed)),
            E::SleepUpdated(sleep(S::Completed)),
        ]);
        state.apply_agent_event_batch(events);
        assert_eq!(state.activities.len(), 2);
        assert!(
            matches!(&state.activities[0],ConversationActivity::WebSearch(v) if v.status==S::Completed&&v.results==json!([{"future":true}]))
        );
        assert!(
            matches!(&state.activities[1],ConversationActivity::Sleep(v) if v.status==S::Completed)
        );
        assert_eq!(state.phase, ConversationPhase::Streaming);
    }
}
#[test]
fn interrupted_wait_and_search_keep_completed_siblings_and_ignore_late_events() {
    let mut state = ConversationState::default();
    state.begin_prompt("test");
    state.apply_agent_event_batch(vec![
        E::PlanUpdated(plan(S::Completed, "done")),
        E::WebSearchUpdated(search(S::InProgress)),
        E::SleepUpdated(sleep(S::InProgress)),
        E::Interrupted,
    ]);
    assert_eq!(state.phase, ConversationPhase::Stopped);
    assert!(matches!(&state.activities[0],ConversationActivity::Plan(v) if v.status==S::Completed));
    assert!(
        matches!(&state.activities[1],ConversationActivity::WebSearch(v) if v.status==S::Interrupted)
    );
    assert!(
        matches!(&state.activities[2],ConversationActivity::Sleep(v) if v.status==S::Interrupted)
    );
    let snapshot = state.activities.clone();
    state.apply_agent_event_batch(vec![
        E::SleepUpdated(sleep(S::Completed)),
        E::TurnPlanUpdated(steps(P::Completed)),
        E::PlanDelta {
            item_id: "new".into(),
            delta: "late".into(),
        },
    ]);
    assert_eq!(state.activities, snapshot);
}
#[test]
fn identical_delta_text_is_valid_content_without_transport_sequence_ids() {
    let mut activities = vec![];
    append_plan_delta(&mut activities, "p".into(), "a".into());
    append_plan_delta(&mut activities, "p".into(), "a".into());
    assert!(matches!(&activities[0],ConversationActivity::Plan(v) if v.text=="aa"));
}
#[test]
fn progress_state_is_owned_by_each_conversation_and_resets_for_the_next_turn() {
    let mut a = ConversationState::default();
    let mut b = ConversationState::default();
    a.begin_prompt("a");
    b.begin_prompt("b");
    a.apply_agent_event_batch(vec![
        E::PlanUpdated(plan(S::InProgress, "a")),
        E::SleepUpdated(sleep(S::InProgress)),
    ]);
    b.apply_agent_event_batch(vec![E::PlanUpdated(plan(S::Completed, "b")), E::Completed]);
    assert!(matches!(&a.activities[0],ConversationActivity::Plan(v) if v.text=="a"));
    assert_eq!(a.phase, ConversationPhase::Streaming);
    assert_eq!(b.phase, ConversationPhase::Complete);
    a.apply_agent_event_batch(vec![E::Interrupted]);
    a.begin_prompt("new");
    a.apply_agent_event_batch(vec![E::PlanUpdated(plan(S::InProgress, "new"))]);
    assert_eq!(a.activities.len(), 1);
    assert_eq!(a.transcript.len(), 1);
}
