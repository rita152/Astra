//! Submission feedback and explicit draft recovery shared by both composers.
use super::ComposerView;
use crate::{conversation::SubmissionStatus, theme::Theme};
use gpui::{Context, Div, SharedString, div, prelude::*, px};
impl ComposerView {
    pub(super) fn submission_feedback_height(&self) -> f32 {
        let count = self
            .conversation
            .submissions
            .iter()
            .filter(|s| {
                matches!(
                    s.status,
                    SubmissionStatus::Sending | SubmissionStatus::Failed(_)
                ) || s.acknowledgement_error.is_some()
                    || (!s.initial && s.item_id.is_none())
            })
            .count();
        ((count
            + usize::from(self.submission_error.is_some())
            + usize::from(self.conversation.permission_error.is_some())) as f32
            * 32.0)
            .min(128.0)
    }
    pub(super) fn submission_feedback(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let mut view = div()
            .id("submission-feedback")
            .max_h(px(128.0))
            .overflow_y_scroll()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(0.0))
            .text_size(px(14.0))
            .text_color(theme.text);
        if let Some(error) = &self.conversation.permission_error {
            view = view.child(
                div()
                    .id("permission-update-error")
                    .role(gpui::Role::Alert)
                    .px(px(10.))
                    .min_h(px(32.))
                    .text_size(px(12.))
                    .text_color(theme.warning)
                    .child(error.clone()),
            );
        }
        if let Some(error) = &self.submission_error {
            view = view.child(
                div()
                    .px(px(10.0))
                    .h(px(32.0))
                    .truncate()
                    .child(error.clone()),
            );
        }
        for submission in self.conversation.submissions.iter().filter(|s| {
            matches!(
                s.status,
                SubmissionStatus::Sending | SubmissionStatus::Failed(_)
            ) || s.acknowledgement_error.is_some()
                || (!s.initial && s.item_id.is_none())
        }) {
            let id = submission.id.clone();
            let recover_accepted = submission.status == SubmissionStatus::Accepted
                && submission.item_id.is_none()
                && (submission.cycle != self.conversation.cycle || !self.is_running());
            let label = match &submission.status {
                SubmissionStatus::Accepted if recover_accepted => "已接受 · 恢复副本",
                SubmissionStatus::Sending => "发送中…",
                SubmissionStatus::Accepted if submission.acknowledgement_error.is_some() => {
                    "已接受 · 确认异常"
                }
                SubmissionStatus::Accepted => "已接受",
                SubmissionStatus::Failed(_) => "发送失败 · 恢复输入",
            };
            let recoverable =
                matches!(submission.status, SubmissionStatus::Failed(_)) || recover_accepted;
            view = view.child(
                div()
                    .id(SharedString::from(format!("submission-{id}")))
                    .px(px(10.0))
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(div().flex_1().truncate().child(if submission.initial {
                        String::new()
                    } else {
                        submission.draft.text.clone()
                    }))
                    .child(
                        div()
                            .id(SharedString::from(format!("restore-{id}")))
                            .h(px(24.0))
                            .px(px(8.0))
                            .rounded_full()
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .text_color(theme.text_tertiary)
                            .flex()
                            .items_center()
                            .child(label)
                            .when(recoverable, |button| {
                                let key_id = id.clone();
                                button
                                    .role(gpui::Role::Button)
                                    .aria_label(label)
                                    .focusable()
                                    .tab_stop(true)
                                    .hover(move |s| s.bg(theme.sidebar_hover))
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.restore_submission(&id, cx)
                                    }))
                                    .on_key_down(cx.listener(
                                        move |this, event: &gpui::KeyDownEvent, _, cx| {
                                            if matches!(
                                                event.keystroke.key.as_str(),
                                                "enter" | "space"
                                            ) {
                                                this.restore_submission(&key_id, cx);
                                                cx.stop_propagation();
                                            }
                                        },
                                    ))
                            }),
                    ),
            );
        }
        div().child(view)
    }
}
