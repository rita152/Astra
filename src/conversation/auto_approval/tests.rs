use super::*;
use crate::agent::{
    AgentAutoApprovalReviewAction, AgentAutoApprovalReviewKey, AgentConnectionEvent,
};

fn state(thread: &str, turn: &str) -> ConversationState {
    let mut state = ConversationState::default();
    state.begin_prompt("Review this action");
    state.apply_agent_event_batch(vec![
        AgentEvent::ThreadCreated {
            thread_id: thread.into(),
        },
        AgentEvent::TurnIdentified {
            thread_id: thread.into(),
            turn_id: turn.into(),
        },
        AgentEvent::Started,
    ]);
    state
}
pub(super) fn review(
    thread: &str,
    turn: &str,
    id: &str,
    status: AgentAutoApprovalReviewStatus,
) -> AgentAutoApprovalReview {
    AgentAutoApprovalReview {
        key: AgentAutoApprovalReviewKey {
            thread_id: thread.into(),
            turn_id: turn.into(),
            review_id: id.into(),
        },
        target_item_id: None,
        action: AgentAutoApprovalReviewAction::NetworkAccess {
            host: "example.com".into(),
            port: 443,
            protocol: "https".into(),
            target: "example.com:443".into(),
        },
        status,
        rationale: Some("Public read".into()),
        risk_level: Some("low".into()),
        user_authorization: Some("high".into()),
        started_at_ms: 100,
        completed_at_ms: (status != AgentAutoApprovalReviewStatus::InProgress).then_some(200),
        decision_source: (status != AgentAutoApprovalReviewStatus::InProgress)
            .then(|| "agent".into()),
    }
}
fn rows(state: &ConversationState) -> Vec<&AutoApprovalReviewPresentation> {
    state
        .activities
        .iter()
        .filter_map(|a| match a {
            ConversationActivity::AutoApprovalReview(r) => Some(r.as_ref()),
            _ => None,
        })
        .collect()
}

#[test]
fn completion_before_start_duplicates_and_separate_reviews_are_monotonic() {
    let mut s = state("a", "turn");
    for status in [
        AgentAutoApprovalReviewStatus::Approved,
        AgentAutoApprovalReviewStatus::Denied,
        AgentAutoApprovalReviewStatus::TimedOut,
        AgentAutoApprovalReviewStatus::Aborted,
    ] {
        let id = format!("{status:?}");
        let done = review("a", "turn", &id, status);
        s.apply_auto_approval_review(done.clone());
        s.apply_auto_approval_review(done);
        s.apply_auto_approval_review(review(
            "a",
            "turn",
            &id,
            AgentAutoApprovalReviewStatus::InProgress,
        ));
    }
    assert_eq!(rows(&s).len(), 4);
    assert!(
        rows(&s)
            .iter()
            .all(|r| r.status() != AgentAutoApprovalReviewStatus::InProgress
                && r.review.completed_at_ms == Some(200))
    );
    assert_eq!(s.phase, ConversationPhase::Thinking);
    assert!(s.approval_responders.is_empty());
    assert!(s.permissions_approval_responders.is_empty());
}

#[test]
fn same_target_and_review_ids_in_other_turns_and_sessions_remain_independent() {
    let mut a = state("a", "turn-1");
    let mut b = state("b", "turn-1");
    let first = review(
        "a",
        "turn-1",
        "shared",
        AgentAutoApprovalReviewStatus::InProgress,
    );
    a.apply_auto_approval_review(first.clone());
    b.apply_auto_approval_review(first);
    b.apply_auto_approval_review(review(
        "b",
        "turn-1",
        "shared",
        AgentAutoApprovalReviewStatus::Denied,
    ));
    a.apply_agent_event_batch(vec![AgentEvent::Completed]);
    a.begin_prompt("Next");
    a.apply_agent_event_batch(vec![
        AgentEvent::TurnIdentified {
            thread_id: "a".into(),
            turn_id: "turn-2".into(),
        },
        AgentEvent::Started,
    ]);
    a.apply_auto_approval_review(review(
        "a",
        "turn-2",
        "shared",
        AgentAutoApprovalReviewStatus::InProgress,
    ));
    a.apply_auto_approval_review(review(
        "a",
        "turn-1",
        "shared",
        AgentAutoApprovalReviewStatus::Approved,
    ));
    assert_eq!(rows(&a)[0].review.key.turn_id, "turn-2");
    assert_eq!(
        rows(&a)[0].status(),
        AgentAutoApprovalReviewStatus::InProgress
    );
    let ConversationActivity::AutoApprovalReview(previous) = &a.transcript[0].activities[0] else {
        panic!()
    };
    assert_eq!(previous.status(), AgentAutoApprovalReviewStatus::Approved);
    assert_eq!(a.transcript[0].phase, ConversationPhase::Complete);
    assert_eq!(rows(&b)[0].status(), AgentAutoApprovalReviewStatus::Denied);
}

#[test]
fn interrupted_cleanup_keeps_wire_status_and_late_result_does_not_resume_turn() {
    let mut s = state("a", "turn");
    s.apply_auto_approval_review(review(
        "a",
        "turn",
        "r",
        AgentAutoApprovalReviewStatus::InProgress,
    ));
    s.apply_agent_event_batch(vec![AgentEvent::Interrupted]);
    assert_eq!(rows(&s)[0].status(), AgentAutoApprovalReviewStatus::Aborted);
    assert_eq!(
        rows(&s)[0].review.status,
        AgentAutoApprovalReviewStatus::InProgress
    );
    assert_eq!(rows(&s)[0].review.completed_at_ms, None);
    s.apply_connection_event(AgentConnectionEvent::AutoApprovalReviewUpdated(Box::new(
        review("a", "turn", "r", AgentAutoApprovalReviewStatus::Approved),
    )));
    assert_eq!(
        rows(&s)[0].status(),
        AgentAutoApprovalReviewStatus::Approved
    );
    assert_eq!(s.phase, ConversationPhase::Stopped);
}

#[test]
fn early_connection_events_replay_after_both_thread_and_turn_are_known() {
    let mut s = ConversationState::default();
    s.begin_prompt("Start");
    let event = AgentConnectionEvent::AutoApprovalReviewUpdated(Box::new(review(
        "a",
        "turn",
        "r",
        AgentAutoApprovalReviewStatus::Denied,
    )));
    assert!(!s.apply_connection_event(event));
    s.apply_agent_event_batch(vec![AgentEvent::ThreadCreated {
        thread_id: "a".into(),
    }]);
    assert!(s.activities.is_empty());
    s.apply_agent_event_batch(vec![AgentEvent::TurnIdentified {
        thread_id: "a".into(),
        turn_id: "turn".into(),
    }]);
    assert_eq!(rows(&s).len(), 1);
    assert!(s.pending_review_events.is_empty());
}

#[test]
fn strict_review_is_deduplicated_per_turn_and_time_and_never_creates_a_responder() {
    let mut s = state("a", "turn");
    for started_at_ms in [1, 1, 2] {
        s.apply_strict_review(AgentStrictReviewRequirement {
            thread_id: "a".into(),
            turn_id: "turn".into(),
            started_at_ms,
        });
    }
    s.apply_strict_review(AgentStrictReviewRequirement {
        thread_id: "b".into(),
        turn_id: "turn".into(),
        started_at_ms: 3,
    });
    assert_eq!(s.activities.len(), 2);
    assert!(s.approval_responders.is_empty());
    assert!(s.server_request_contexts.is_empty());
    s.apply_agent_event_batch(vec![AgentEvent::Failed("connection lost".into())]);
    assert!(
        s.activities
            .iter()
            .filter_map(|a| if let ConversationActivity::StrictReview(r) = a {
                Some(r)
            } else {
                None
            })
            .all(|r| r.turn_finished)
    );
}

#[test]
fn guardian_warning_is_thread_scoped_and_does_not_finish_the_turn() {
    let mut s = state("a", "turn");
    let warning = AgentGuardianWarning {
        thread_id: "a".into(),
        message: "Automatic approval review rejected too many approval requests for this turn"
            .into(),
    };
    s.apply_guardian_warning(warning.clone());
    s.apply_guardian_warning(warning);
    s.apply_guardian_warning(AgentGuardianWarning {
        thread_id: "b".into(),
        message: "other".into(),
    });
    assert_eq!(s.activities.len(), 1);
    assert_eq!(s.phase, ConversationPhase::Thinking);
}

#[test]
fn nullable_target_and_post_terminal_batch_updates_do_not_create_a_new_turn() {
    let mut s = state("a", "turn");
    let mut started = review("a", "turn", "r", AgentAutoApprovalReviewStatus::InProgress);
    started.target_item_id = Some("command".into());
    s.apply_auto_approval_review(started);
    s.apply_agent_event_batch(vec![
        AgentEvent::Interrupted,
        AgentEvent::TextDelta("must not resume".into()),
        AgentEvent::AutoApprovalReviewUpdated(Box::new(review(
            "a",
            "turn",
            "r",
            AgentAutoApprovalReviewStatus::Approved,
        ))),
    ]);
    assert_eq!(rows(&s)[0].review.target_item_id, None);
    assert_eq!(
        rows(&s)[0].status(),
        AgentAutoApprovalReviewStatus::Approved
    );
    assert_eq!(s.phase, ConversationPhase::Stopped);
    assert!(s.assistant_message.is_empty());
}
