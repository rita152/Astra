use super::*;
use crate::agent::{
    AgentAutoApprovalReview, AgentAutoApprovalReviewAction, AgentAutoApprovalReviewKey,
    AgentAutoApprovalReviewStatus as Status, CommandExecution,
};
use crate::conversation::AutoApprovalReviewPresentation;

fn review(id: &str, target: Option<&str>, status: Status) -> ConversationActivity {
    ConversationActivity::AutoApprovalReview(Box::new(AutoApprovalReviewPresentation {
        review: AgentAutoApprovalReview {
            key: AgentAutoApprovalReviewKey {
                thread_id: "thread".into(),
                turn_id: "turn".into(),
                review_id: id.into(),
            },
            target_item_id: target.map(str::to_owned),
            action: AgentAutoApprovalReviewAction::Command {
                command: "pwd".into(),
                cwd: "/tmp".into(),
                source: "shell".into(),
            },
            status,
            rationale: Some("Read workspace path".into()),
            risk_level: Some("low".into()),
            user_authorization: None,
            started_at_ms: 1,
            completed_at_ms: (status != Status::InProgress).then_some(2),
            decision_source: (status != Status::InProgress).then(|| "agent".into()),
        },
        closed_locally: false,
        attached_to_item: false,
    }))
}
fn command() -> ConversationActivity {
    ConversationActivity::Command(CommandExecution {
        id: "command".into(),
        command: "pwd".into(),
        cwd: "/tmp".into(),
        output: "/tmp".into(),
        actions: Vec::new(),
        status: CommandExecutionStatus::Completed,
        exit_code: Some(0),
        terminal_process_id: None,
    })
}

#[test]
fn auto_approval_early_targetless_and_multiple_target_reviews_have_independent_rows() {
    let early = review("one", Some("command"), Status::InProgress);
    assert!(
        matches!(&activity_stream_units(std::slice::from_ref(&early))[0],ActivityStreamUnit::Standalone(ConversationActivity::AutoApprovalReview(r)) if !r.attached_to_item)
    );
    let units = activity_stream_units(&[
        early,
        review("two", Some("command"), Status::TimedOut),
        command(),
        review("network", None, Status::Denied),
    ]);
    let ActivityStreamUnit::ToolGroup(group) = &units[0] else {
        panic!("expected reviewed tool group")
    };
    assert_eq!(group.activities.len(), 3);
    assert!(group.is_active());
    let reviews = group
        .activities
        .iter()
        .filter_map(|a| {
            if let ConversationActivity::AutoApprovalReview(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        reviews
            .iter()
            .map(|r| r.review.key.review_id.as_str())
            .collect::<Vec<_>>(),
        ["one", "two"]
    );
    assert!(reviews.iter().all(|r| r.attached_to_item));
    assert!(
        matches!(&units[1],ActivityStreamUnit::Standalone(ConversationActivity::AutoApprovalReview(r)) if r.review.key.review_id=="network"&&!r.attached_to_item)
    );
}

#[test]
fn auto_approval_approved_observations_are_retained_but_not_rendered() {
    let activities = vec![review("approved", None, Status::Approved)];
    assert!(activity_stream_units(&activities).is_empty());
    assert_eq!(activities.len(), 1);
    let units = activity_stream_units(&[
        command(),
        review("approved", Some("command"), Status::Approved),
    ]);
    let ActivityStreamUnit::ToolGroup(group) = &units[0] else {
        panic!()
    };
    assert_eq!(group.activities.len(), 1);
    assert!(!group.is_active());
}

#[test]
fn auto_approval_group_uses_a_separate_persistent_view_for_each_review() {
    let mut app = gpui::TestApp::new();
    let mut window = app.open_window_with_options(gpui::WindowOptions::default(), |_, cx| {
        HomeView::new(ThemeMode::Dark, cx)
    });
    let activities = vec![
        command(),
        review("one", Some("command"), Status::InProgress),
        review("two", Some("command"), Status::InProgress),
    ];
    window.update(|home, _, cx| {
        home.conversation_phase = ConversationPhase::Thinking;
        home.conversation_activity = Rc::new(activities.clone());
        home.conversation_rows = Rc::new(
            activity_stream_units(&activities)
                .into_iter()
                .map(|unit| ConversationListRow::Activity {
                    unit,
                    show_thinking_tail: false,
                })
                .collect(),
        );
        home.conversation_cache_dirty = true;
        cx.notify();
    });
    window.draw();
    window.read(|home, cx| {
        assert_eq!(home.auto_review_views.len(), 2);
        assert!(
            home.auto_review_views
                .values()
                .all(|view| view.read(cx).attached)
        );
        let ids = home
            .auto_review_views
            .values()
            .map(|view| view.entity_id())
            .collect::<HashSet<_>>();
        assert_eq!(ids.len(), 2);
    });
    let review = window.read(|home, _| home.auto_review_views.values().next().unwrap().clone());
    app.update(|cx| {
        review.update(cx, |view, cx| {
            view.details_expanded = true;
            cx.notify();
        })
    });
    for _ in 0..4 {
        window.draw();
        app.update(|cx| {
            cx.update_window(window.handle().into(), |_, window, cx| {
                window.simulate_next_frame(cx);
            })
            .unwrap()
        });
    }
    assert_eq!(window.read(|home, _| home.auto_review_views.len()), 2);
}

#[test]
fn guardian_warning_without_a_prompt_has_no_fabricated_user_bubble() {
    let activities = vec![ConversationActivity::GuardianWarning(
        crate::agent::AgentGuardianWarning {
            thread_id: "thread".into(),
            message: "Review unavailable".into(),
        },
    )];
    let rows = conversation_list_rows(
        Vec::new(),
        CurrentTurnRows {
            phase: ConversationPhase::Empty,
            user_message: String::new(),
            user_images: Vec::new(),
            user_message_time: String::new(),
            assistant_message: String::new(),
            assistant_message_time: None,
            conversation_activity: &activities,
            resumed_turn: None,
        },
        &HashSet::new(),
    );
    assert_eq!(rows.len(), 1);
    assert!(matches!(
        &rows[0],
        ConversationListRow::Activity {
            unit: ActivityStreamUnit::Standalone(ConversationActivity::GuardianWarning(_)),
            ..
        }
    ));
}
