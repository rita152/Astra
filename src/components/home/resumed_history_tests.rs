use std::collections::HashSet;

use gpui::{Context, Entity, FocusHandle, Render, Window};

use super::{
    conversation::resumed_work_header,
    timeline::{
        ActivityStreamUnit, ConversationListRow, append_turn_activity_rows, resumed_file_summary,
        resumed_work_label,
    },
    *,
};
use crate::{
    agent::AgentFileChangeStatus,
    conversation::{ConversationActivity, ConversationPhase, ResumedTurnPresentation},
    theme::{Theme, ThemeMode},
};

struct HeaderHarness {
    home: Entity<HomeView>,
    focus: FocusHandle,
}

impl Render for HeaderHarness {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let expanded = self.home.read(cx).expanded_resumed_turns.contains("turn");
        resumed_work_header(
            self.home.clone(),
            self.focus.clone(),
            "turn".into(),
            "用时 1秒".into(),
            expanded,
            Theme::for_mode(ThemeMode::Dark),
        )
    }
}

#[test]
fn resumed_header_keyboard_toggles_with_a_persistent_focus_handle() {
    use gpui::{AppContext, TestApp};
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(Default::default(), |_, cx| HeaderHarness {
        home: cx.new(|cx| HomeView::new(ThemeMode::Dark, cx)),
        focus: cx.focus_handle(),
    });
    window.update(|header, window, cx| window.focus(&header.focus, cx));
    window.draw();
    window.simulate_keystroke("enter");
    window.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse("enter").unwrap(),
    });
    assert!(window.read(|header, cx| header.home.read(cx).expanded_resumed_turns.contains("turn")));
    window.draw();
    window.simulate_keystroke("space");
    window.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse("space").unwrap(),
    });
    assert!(
        !window.read(|header, cx| header.home.read(cx).expanded_resumed_turns.contains("turn"))
    );
}

fn message(id: &str) -> ConversationActivity {
    ConversationActivity::AssistantMessage {
        item_id: id.into(),
        text: id.into(),
    }
}

#[test]
fn resumed_summary_merges_repeated_paths_and_excludes_failed_edits() {
    use crate::components::file_change::FileChangeActivityPresentation;
    let first = FileChangeActivityPresentation::edited("one", "src/main.rs", 2, 1);
    let second = FileChangeActivityPresentation::edited("two", "src/main.rs", 3, 2);
    let mut failed = FileChangeActivityPresentation::edited("failed", "secret.rs", 50, 0);
    failed.status = AgentFileChangeStatus::Failed;
    let activities = [first, second, failed]
        .into_iter()
        .map(ConversationActivity::FileChange)
        .collect::<Vec<_>>();
    let turn = ResumedTurnPresentation {
        id: "turn".into(),
        duration_ms: None,
        final_message_ids: vec![],
    };
    let summary = resumed_file_summary(&activities, Some(&turn)).unwrap();
    assert_eq!(summary.files.len(), 1);
    assert_eq!(
        (summary.total_additions(), summary.total_deletions()),
        (5, 3)
    );
    assert!(resumed_file_summary(&activities, None).is_none());
}

#[test]
fn completed_history_hides_only_the_process_prefix_and_expands_in_order() {
    let activities = vec![
        message("commentary"),
        message("answer"),
        message("attachment"),
    ];
    let resumed = ResumedTurnPresentation {
        id: "turn".into(),
        duration_ms: Some(91_000),
        final_message_ids: vec!["answer".into()],
    };
    let mut rows = Vec::new();
    append_turn_activity_rows(
        &mut rows,
        &activities,
        false,
        ConversationPhase::Complete,
        Some(&resumed),
        &HashSet::new(),
    );
    assert!(
        matches!(&rows[0], ConversationListRow::ResumedWork { label, expanded: false, .. } if label == "用时 1分钟 31秒")
    );
    assert_eq!(rows.len(), 3);
    assert!(
        matches!(&rows[1], ConversationListRow::Activity { unit: ActivityStreamUnit::Standalone(ConversationActivity::AssistantMessage { item_id, .. }), .. } if item_id == "answer")
    );
    let mut expanded = Vec::new();
    append_turn_activity_rows(
        &mut expanded,
        &activities,
        false,
        ConversationPhase::Complete,
        Some(&resumed),
        &HashSet::from(["turn".into()]),
    );
    assert_eq!(expanded.len(), 4);
    assert!(
        matches!(&expanded[1], ConversationListRow::Activity { unit: ActivityStreamUnit::Standalone(ConversationActivity::AssistantMessage { item_id, .. }), .. } if item_id == "commentary")
    );
}

#[test]
fn incomplete_failed_and_unidentified_history_stays_visible() {
    let activities = vec![message("commentary"), message("answer")];
    let resumed = ResumedTurnPresentation {
        id: "turn".into(),
        duration_ms: None,
        final_message_ids: vec!["answer".into()],
    };
    for phase in [
        ConversationPhase::Streaming,
        ConversationPhase::Stopped,
        ConversationPhase::Failed,
    ] {
        let mut rows = Vec::new();
        append_turn_activity_rows(
            &mut rows,
            &activities,
            false,
            phase,
            Some(&resumed),
            &HashSet::new(),
        );
        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter()
                .all(|row| matches!(row, ConversationListRow::Activity { .. }))
        );
    }
    let mut rows = Vec::new();
    append_turn_activity_rows(
        &mut rows,
        &activities,
        false,
        ConversationPhase::Complete,
        None,
        &HashSet::new(),
    );
    assert_eq!(rows.len(), 2);
    assert_eq!(resumed_work_label(None), "工作过程");
    assert_eq!(resumed_work_label(Some(3_849_000)), "用时 1小时 4分钟 9秒");
}

#[test]
fn identical_resumed_answers_have_distinct_footer_scopes() {
    use crate::conversation::ConversationTranscriptTurn;
    let turns = ["turn-a", "turn-b"]
        .into_iter()
        .map(|id| ConversationTranscriptTurn {
            turn_id: Some(id.into()),
            phase: ConversationPhase::Complete,
            user_message: "prompt".into(),
            user_images: vec![],
            user_message_time: None,
            assistant_message: "完成".into(),
            assistant_message_time: None,
            activities: vec![],
            resumed: Some(ResumedTurnPresentation {
                id: id.into(),
                duration_ms: None,
                final_message_ids: vec![],
            }),
        })
        .collect();
    let rows = super::timeline::conversation_list_rows(
        turns,
        super::context::CurrentTurnRows {
            phase: ConversationPhase::Empty,
            user_message: String::new(),
            user_images: vec![],
            user_message_time: String::new(),
            assistant_message: String::new(),
            assistant_message_time: None,
            conversation_activity: &[],
            resumed_turn: None,
        },
        &HashSet::new(),
    );
    let ids = rows
        .into_iter()
        .filter_map(|row| match row {
            ConversationListRow::CurrentResponseFooter { id, .. } => Some(id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["turn-a", "turn-b"]);
}
