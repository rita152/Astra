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
