//! Per-submit snapshots and acknowledgements, independent of turn lifecycle.
use super::{ConversationActivity, ConversationPhase, ConversationState};
use crate::agent::{AgentPromptContext, AgentTurnIdentity, UserMessageAttachment};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SubmissionStatus {
    Sending,
    Accepted,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SubmissionDraft {
    pub text: String,
    pub context: AgentPromptContext,
    pub comments: Vec<crate::git_review::ReviewComment>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UserSubmission {
    pub id: String,
    pub cycle: u64,
    pub target: Option<AgentTurnIdentity>,
    pub draft: SubmissionDraft,
    pub message: String,
    pub status: SubmissionStatus,
    pub acknowledgement_error: Option<String>,
    pub item_id: Option<String>,
    pub initial: bool,
}

impl ConversationState {
    pub(crate) fn record_submission(
        &mut self,
        draft: SubmissionDraft,
        message: String,
        initial: bool,
    ) -> String {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = format!(
            "gpui-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        self.submissions.push(UserSubmission {
            id: id.clone(),
            cycle: self.cycle,
            target: self.turn_identity.clone(),
            draft,
            message,
            status: SubmissionStatus::Sending,
            acknowledgement_error: None,
            item_id: None,
            initial,
        });
        id
    }

    pub(crate) fn reconcile_history_submissions(&mut self, history: &crate::agent::ThreadHistory) {
        for turn in &history.turns {
            let mut seen_items = std::collections::HashSet::new();
            let mut seen_clients = std::collections::HashSet::new();
            for item in &turn.items {
                let crate::agent::ThreadHistoryItem::UserMessage {
                    item_id,
                    client_message_id,
                    text,
                    images,
                } = item
                else {
                    continue;
                };
                if !seen_items.insert(item_id)
                    || client_message_id
                        .as_ref()
                        .is_some_and(|id| !seen_clients.insert(id))
                {
                    continue;
                }
                if let Some(submission) = self.submissions.iter_mut().find(|s| {
                    s.target.as_ref().is_some_and(|target| {
                        target.thread_id == history.thread.thread_id
                            && target.turn_id == turn.turn_id
                    }) && (client_message_id.as_deref() == Some(s.id.as_str())
                        || s.item_id.as_deref() == Some(item_id.as_str())
                        || (client_message_id.is_none()
                            && s.item_id.is_none()
                            && s.message == *text
                            && s.draft
                                .context
                                .files
                                .iter()
                                .map(|file| {
                                    if file.image {
                                        UserMessageAttachment::Local(file.path.clone())
                                    } else {
                                        UserMessageAttachment::File(file.path.clone())
                                    }
                                })
                                .collect::<Vec<_>>()
                                == *images))
                }) {
                    submission.item_id = Some(item_id.clone());
                    submission.status = SubmissionStatus::Accepted;
                }
            }
        }
    }

    pub(crate) fn steer_target(&self) -> Result<AgentTurnIdentity, String> {
        match self.phase {
            ConversationPhase::Starting => {
                Err("当前轮次正在启动，尚未取得可用轮次标识。输入已保留，请稍后发送。".into())
            }
            ConversationPhase::Stopping => {
                Err("正在停止当前轮次。输入已保留，请等待停止完成后发送。".into())
            }
            ConversationPhase::Thinking | ConversationPhase::Streaming => self
                .turn_identity
                .clone()
                .ok_or_else(|| "当前轮次尚未就绪。输入已保留，请稍后发送。".into()),
            _ => Err("当前轮次已结束。输入已保留，请手动发送。".into()),
        }
    }

    pub(crate) fn resolve_submission(&mut self, id: &str, result: Result<(), String>) {
        if let Some(submission) = self.submissions.iter_mut().find(|s| s.id == id) {
            submission.acknowledgement_error = result.as_ref().err().cloned();
            // An authoritative userMessage proves acceptance, even if the RPC
            // acknowledgement is lost or fails validation later.
            submission.status = if submission.status == SubmissionStatus::Accepted
                || submission.item_id.is_some()
                || result.is_ok()
            {
                SubmissionStatus::Accepted
            } else {
                SubmissionStatus::Failed(result.expect_err("checked error"))
            };
        }
    }

    pub(crate) fn receive_user_message(
        &mut self,
        item_id: String,
        client_id: Option<String>,
        text: String,
        images: Vec<UserMessageAttachment>,
    ) {
        let first_item = self.seen_user_items.is_empty();
        let seen = !self.seen_user_items.insert(item_id.clone());
        if !self
            .submissions
            .iter()
            .any(|s| client_id.as_deref() == Some(s.id.as_str()))
            && let Some(replies) = super::transcript::resumed_question_replies(&text)
        {
            if !seen {
                self.activities
                    .extend(replies.into_iter().map(|(question, answer)| {
                        ConversationActivity::QuestionReply {
                            item_id: item_id.clone(),
                            question,
                            answer,
                        }
                    }));
            }
            return;
        }
        let submission = self.submissions.iter_mut().find(|s| {
            s.cycle == self.cycle
                && s.target
                    .as_ref()
                    .is_none_or(|t| Some(t) == self.turn_identity.as_ref())
                && (client_id.as_deref() == Some(s.id.as_str())
                    || s.item_id.as_deref() == Some(item_id.as_str())
                    || (client_id.is_none()
                        && s.item_id.is_none()
                        && s.message == text
                        && ((s.initial && first_item)
                            || s.draft
                                .context
                                .files
                                .iter()
                                .map(|file| {
                                    if file.image {
                                        UserMessageAttachment::Local(file.path.clone())
                                    } else {
                                        UserMessageAttachment::File(file.path.clone())
                                    }
                                })
                                .collect::<Vec<_>>()
                                == images)))
        });
        let initial = if let Some(submission) = submission {
            if submission
                .item_id
                .as_deref()
                .is_some_and(|existing| existing != item_id)
            {
                return;
            }
            submission.item_id = Some(item_id.clone());
            submission.status = SubmissionStatus::Accepted;
            submission.initial
        } else {
            client_id.is_none()
                && first_item
                && self.submissions.iter().all(|s| s.cycle != self.cycle)
        };
        if initial {
            self.user_message = Some(text);
            self.user_images = images;
            self.activities.retain(|a| !matches!(a, ConversationActivity::UserMessage { item_id: id, .. } if id == &item_id));
        } else if let Some(ConversationActivity::UserMessage { text: current, images: attachments, .. }) =
            self.activities.iter_mut().find(|a| matches!(a, ConversationActivity::UserMessage { item_id: id, .. } if id == &item_id)) {
            *current = text;
            *attachments = images;
        } else if !seen {
            self.activities.push(ConversationActivity::UserMessage { item_id, text, images });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentEvent;
    fn running() -> ConversationState {
        let mut s = ConversationState {
            thread_id: Some("thread".into()),
            ..Default::default()
        };
        s.begin_prompt("initial");
        s.apply_agent_event_batch(vec![
            AgentEvent::TurnReady(AgentTurnIdentity {
                generation: 1,
                thread_id: "thread".into(),
                turn_id: "turn".into(),
            }),
            AgentEvent::Started,
        ]);
        s
    }
    fn submit(s: &mut ConversationState, text: &str) -> String {
        s.record_submission(
            SubmissionDraft {
                text: text.into(),
                context: Default::default(),
                comments: vec![],
            },
            text.into(),
            false,
        )
    }
    #[test]
    fn steer_acceptance_deduplicates_client_and_item_and_preserves_event_order() {
        let mut s = running();
        s.apply_agent_event_batch(vec![
            AgentEvent::AssistantMessageStarted {
                item_id: "a".into(),
            },
            AgentEvent::TextDelta("before".into()),
        ]);
        let first = submit(&mut s, "same");
        let second = submit(&mut s, "same");
        s.receive_user_message("u1".into(), Some(first.clone()), "same".into(), vec![]);
        s.receive_user_message("u1".into(), Some(first.clone()), "same".into(), vec![]);
        s.receive_user_message(
            "duplicate-client".into(),
            Some(first.clone()),
            "same".into(),
            vec![],
        );
        s.apply_agent_event_batch(vec![
            AgentEvent::AssistantMessageStarted {
                item_id: "b".into(),
            },
            AgentEvent::TextDelta("after".into()),
        ]);
        s.receive_user_message("u2".into(), Some(second.clone()), "same".into(), vec![]);
        s.resolve_submission(&second, Ok(()));
        s.resolve_submission(&first, Err("lost acknowledgement".into()));
        assert_eq!(s.activities.len(), 4);
        assert_eq!(s.assistant_message, "beforeafter");
        assert_eq!(s.phase, ConversationPhase::Streaming);
        assert!(
            s.submissions
                .iter()
                .all(|s| s.status == SubmissionStatus::Accepted)
        );
        assert!(
            matches!(&s.activities[0],ConversationActivity::AssistantMessage{item_id,..} if item_id=="a")
        );
        assert!(
            matches!(&s.activities[1],ConversationActivity::UserMessage{item_id,..} if item_id=="u1")
        );
        assert!(
            matches!(&s.activities[2],ConversationActivity::AssistantMessage{item_id,..} if item_id=="b")
        );
        s.apply_agent_event_batch(vec![AgentEvent::Completed]);
        assert_eq!(
            s.assistant_message, "after",
            "copy uses the last response after steering"
        );
    }
    #[test]
    fn steer_late_results_only_change_their_submission_after_terminal_or_new_cycle() {
        let mut s = running();
        let id = submit(&mut s, "late");
        s.apply_agent_event_batch(vec![AgentEvent::Interrupted]);
        s.resolve_submission(&id, Ok(()));
        assert_eq!(s.phase, ConversationPhase::Stopped);
        s.begin_prompt("new draft");
        let cycle = s.cycle;
        s.resolve_submission(&id, Err("late error".into()));
        assert_eq!(s.phase, ConversationPhase::Starting);
        assert_eq!(s.cycle, cycle);
        assert_eq!(s.user_message.as_deref(), Some("new draft"));
        assert!(s.activities.is_empty());
    }
    #[test]
    fn steer_optional_client_ids_match_unclaimed_submissions_without_collapsing_equal_text() {
        let mut s = running();
        let one = submit(&mut s, "same");
        let two = submit(&mut s, "same");
        s.receive_user_message("u1".into(), None, "same".into(), vec![]);
        s.receive_user_message("u2".into(), None, "same".into(), vec![]);
        s.resolve_submission(&one, Ok(()));
        s.resolve_submission(&two, Ok(()));
        assert_eq!(s.activities.len(), 2);
        assert!(s.submissions.iter().all(|s| s.item_id.is_some()));
    }
    #[test]
    fn tool_question_replies_keep_the_existing_history_presentation() {
        let mut s = running();
        let text = "<send_user_message_question_reply>\n[{\"question\":\"范围？\",\"answer\":\"当前文件\"}]\n</send_user_message_question_reply>";
        s.receive_user_message("reply".into(), None, text.into(), vec![]);
        s.receive_user_message("reply".into(), None, text.into(), vec![]);
        assert_eq!(s.activities.len(), 1);
        assert!(
            matches!(&s.activities[0], ConversationActivity::QuestionReply { question, answer, .. } if question == "范围？" && answer == "当前文件")
        );
        assert_eq!(s.user_message.as_deref(), Some("initial"));
    }

    #[test]
    fn steer_ready_response_does_not_wait_for_started_and_duplicate_started_preserves_streaming() {
        let mut s = ConversationState {
            thread_id: Some("thread".into()),
            ..Default::default()
        };
        s.begin_prompt("initial");
        s.apply_agent_event_batch(vec![AgentEvent::TurnReady(AgentTurnIdentity {
            generation: 1,
            thread_id: "thread".into(),
            turn_id: "turn".into(),
        })]);
        assert!(s.steer_target().is_ok());
        s.apply_agent_event_batch(vec![
            AgentEvent::TextDelta("stream".into()),
            AgentEvent::Started,
        ]);
        assert_eq!(s.phase, ConversationPhase::Streaming);
        s.apply_agent_event_batch(vec![AgentEvent::Completed, AgentEvent::Started]);
        assert_eq!(s.phase, ConversationPhase::Complete);
    }

    #[test]
    fn steer_unavailable_targets_are_explicit_and_do_not_mutate_turn_state() {
        let mut s = running();
        assert!(s.steer_target().is_ok());
        for phase in [
            ConversationPhase::Starting,
            ConversationPhase::Stopping,
            ConversationPhase::Complete,
            ConversationPhase::Failed,
        ] {
            s.phase = phase;
            assert!(s.steer_target().is_err());
            assert_eq!(s.phase, phase);
        }
        s.phase = ConversationPhase::Thinking;
        s.turn_identity = None;
        assert!(s.steer_target().is_err());
    }
}
