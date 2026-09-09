use super::*;

fn message(id: &str, phase: Option<&str>) -> ThreadHistoryItem {
    ThreadHistoryItem::AssistantMessage {
        item_id: id.into(),
        text: id.into(),
        phase: phase.map(str::to_owned),
    }
}

#[test]
fn explicit_final_phase_wins_over_later_commentary() {
    assert_eq!(
        resumed_final_message_ids(&[
            message("progress", Some("commentary")),
            message("answer", Some("final_answer")),
            message("later", Some("commentary"))
        ]),
        vec!["answer"]
    );
    assert!(resumed_final_message_ids(&[message("progress", Some("commentary"))]).is_empty());
}

#[test]
fn clarification_does_not_end_a_continued_turns_work_disclosure() {
    assert_eq!(
        resumed_final_message_ids(&[
            message("clarification", Some("final_answer")),
            message("continued-work", Some("commentary")),
            message("actual-answer", Some("final_answer")),
        ]),
        vec!["actual-answer"]
    );
}

#[test]
fn legacy_history_uses_its_last_unphased_message() {
    assert_eq!(
        resumed_final_message_ids(&[message("progress", None), message("answer", None)]),
        vec!["answer"]
    );
    assert!(resumed_final_message_ids(&[]).is_empty());
}
#[test]
fn answered_clarification_retains_question_and_answer_separately() {
    let input = r#"<send_user_message_question_reply>
[{"question":"哪个区域？","answer":"左侧栏"}]
</send_user_message_question_reply>"#;
    assert_eq!(
        super::resumed_question_replies(input),
        Some(vec![("哪个区域？".into(), "左侧栏".into())])
    );
    assert_eq!(super::resumed_question_replies("普通消息"), None);
    assert_eq!(
        super::resumed_question_replies(
            "<send_user_message_question_reply>invalid</send_user_message_question_reply>"
        ),
        None
    );
}

fn progress_history(status: HistoryTurnStatus, items: Vec<ThreadHistoryItem>) -> ThreadHistory {
    use crate::agent::{HistoryItemDetail, ThreadActivity, ThreadSummary, ThreadTurn};
    ThreadHistory {
        thread: ThreadSummary {
            thread_id: "history".into(),
            title: "test".into(),
            preview: String::new(),
            cwd: "/tmp".into(),
            project_id: None,
            section: None,
            created_at: 0,
            updated_at: 0,
            recency_at: None,
            activity: ThreadActivity::Idle,
        },
        turns: vec![ThreadTurn {
            turn_id: "turn".into(),
            status,
            items_view: HistoryItemDetail::Full,
            items,
            started_at: None,
            completed_at: None,
            duration_ms: None,
            error: None,
        }],
        next_turn_cursor: None,
        backwards_turn_cursor: None,
    }
}
#[test]
fn history_progress_uses_final_plan_and_preserves_opaque_search_results() {
    use crate::agent::{AgentActivityStatus as S, AgentPlan, AgentSleep, AgentWebSearch};
    let plan = AgentPlan {
        id: "plan".into(),
        text: "final".into(),
        status: S::Completed,
    };
    let search = AgentWebSearch {
        id: "search".into(),
        query: "Rust".into(),
        action: serde_json::json!({"type":"findInPage","pattern":"Rust","url":null,"ext":true}),
        results: serde_json::json!([{"future":42}]),
        extra: Default::default(),
        status: S::Completed,
    };
    let sleep = AgentSleep {
        id: "sleep".into(),
        duration_ms: 15000,
        status: S::Completed,
    };
    let mut state = ConversationState::default();
    state.hydrate_history(progress_history(
        HistoryTurnStatus::Completed,
        vec![
            ThreadHistoryItem::Plan(plan.clone()),
            ThreadHistoryItem::Plan(plan.clone()),
            ThreadHistoryItem::WebSearch(search.clone()),
            ThreadHistoryItem::Sleep(sleep.clone()),
        ],
    ));
    assert_eq!(
        state.activities,
        vec![
            ConversationActivity::Plan(plan),
            ConversationActivity::WebSearch(search),
            ConversationActivity::Sleep(sleep)
        ]
    );
    assert_eq!(state.phase, ConversationPhase::Complete);
}
#[test]
fn history_tail_wait_tracks_turn_outcome_without_inventing_elapsed_duration() {
    use crate::agent::{AgentActivityStatus as S, AgentSleep};
    for (turn, expected) in [
        (HistoryTurnStatus::InProgress, S::InProgress),
        (HistoryTurnStatus::Interrupted, S::Interrupted),
        (HistoryTurnStatus::Failed, S::Failed),
        (HistoryTurnStatus::Completed, S::Completed),
    ] {
        let mut state = ConversationState::default();
        state.hydrate_history(progress_history(
            turn,
            vec![
                ThreadHistoryItem::Sleep(AgentSleep {
                    id: "earlier".into(),
                    duration_ms: 0,
                    status: S::Completed,
                }),
                ThreadHistoryItem::Sleep(AgentSleep {
                    id: "tail".into(),
                    duration_ms: 15000,
                    status: S::Completed,
                }),
            ],
        ));
        assert!(
            matches!(&state.activities[0],ConversationActivity::Sleep(v) if v.status==S::Completed)
        );
        assert!(
            matches!(&state.activities[1],ConversationActivity::Sleep(v) if v.status==expected&&v.duration_ms==15000)
        );
    }
}

#[test]
fn steer_history_preserves_interleaved_messages_and_attachments() {
    use crate::agent::{
        HistoryItemDetail, ThreadActivity, ThreadSummary, ThreadTurn, UserMessageAttachment,
    };
    let user = |id: &str, text: &str| ThreadHistoryItem::UserMessage {
        client_message_id: None,
        item_id: id.into(),
        text: text.into(),
        images: vec![
            UserMessageAttachment::File("/tmp/context.txt".into()),
            UserMessageAttachment::Local("/tmp/image.png".into()),
        ],
    };
    let mut state = ConversationState {
        turn_identity: Some(crate::agent::AgentTurnIdentity {
            generation: 1,
            thread_id: "thread".into(),
            turn_id: "turn".into(),
        }),
        ..Default::default()
    };
    let receipt = state.record_submission(
        crate::conversation::SubmissionDraft {
            text: "same".into(),
            context: Default::default(),
            comments: vec![],
        },
        "same".into(),
        false,
    );
    state.resolve_submission(&receipt, Ok(()));
    let mut history = ThreadHistory {
        thread: ThreadSummary {
            thread_id: "thread".into(),
            title: "history".into(),
            preview: String::new(),
            cwd: "/tmp".into(),
            project_id: None,
            section: None,
            created_at: 1,
            updated_at: 2,
            recency_at: Some(2),
            activity: ThreadActivity::Idle,
        },
        turns: vec![ThreadTurn {
            turn_id: "turn".into(),
            status: HistoryTurnStatus::Completed,
            items_view: HistoryItemDetail::Full,
            items: vec![
                user("u0", "initial"),
                message("before", Some("commentary")),
                user("u1", "same"),
                user("u1", "same"),
                message("between", Some("commentary")),
                user("u2", "same"),
                message("answer", Some("final_answer")),
            ],
            started_at: Some(1),
            completed_at: Some(2),
            duration_ms: Some(1000),
            error: None,
        }],
        next_turn_cursor: None,
        backwards_turn_cursor: None,
    };
    if let ThreadHistoryItem::UserMessage {
        client_message_id, ..
    } = &mut history.turns[0].items[2]
    {
        *client_message_id = Some(receipt);
    }
    state.hydrate_history(history);
    assert_eq!(state.submissions[0].item_id.as_deref(), Some("u1"));
    assert_eq!(state.user_message.as_deref(), Some("initial"));
    assert_eq!(state.user_images.len(), 2);
    assert_eq!(state.activities.len(), 5);
    assert!(
        matches!(&state.activities[1],ConversationActivity::UserMessage{item_id,images,..} if item_id=="u1" && images.len()==2)
    );
    assert!(
        matches!(&state.activities[3],ConversationActivity::UserMessage{item_id,..} if item_id=="u2")
    );
    assert_eq!(state.assistant_message, "answer");
    assert_eq!(state.phase, ConversationPhase::Complete);
}
