use super::*;
use super::{
    controls::Action,
    render::{addition_color, deletion_color},
};
use crate::theme::Theme;
use gpui::{AnyElement, Div, MouseButton, Role, Stateful, div, prelude::*, rgba};

impl ReviewPanel {
    pub(super) fn comment_card(&self, c: Comment, draft: bool, cx: &Context<Self>) -> Div {
        let t = Theme::for_mode(self.mode);
        let range = if c.start == c.end {
            c.start.to_string()
        } else {
            format!("{}–{}", c.start, c.end)
        };
        div()
            .pl(px(if self.compact() { 8. } else { 52.5625 }))
            .pr(px(8.))
            .py(px(6.))
            .child(
                div()
                    .w_full()
                    .p(px(12.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(t.border)
                    .bg(t.elevated)
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .text_size(px(12.))
                            .text_color(t.text_tertiary)
                            .gap(px(8.))
                            .child(
                                div()
                                    .size(px(24.))
                                    .rounded_full()
                                    .bg(t.text.alpha(0.18))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(9.))
                                    .child("你"),
                            )
                            .child("你")
                            .when(!c.path.is_empty(), |d| {
                                d.child(div().min_w(px(0.)).text_ellipsis().child(c.path.clone()))
                            })
                            .child(div().flex_1())
                            .child(if self.compact() {
                                format!("{} 行", c.location())
                            } else {
                                format!("第 {}{range} 行的本地评论", if c.old { "L" } else { "R" })
                            }),
                    )
                    .when(draft, |d| {
                        d.child(div().h(px(46.)).child(self.input.clone()))
                    })
                    .when(!draft, |d| {
                        d.child(
                            div()
                                .text_size(px(13.))
                                .line_height(px(21.))
                                .text_color(t.text)
                                .child(c.text),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .when(draft, |d| {
                                d.child(self.button(
                                    "review-comment-cancel",
                                    "取消",
                                    None,
                                    Action::CancelComment,
                                    cx,
                                ))
                                .child(
                                    self.button(
                                        "review-comment-save",
                                        "注释",
                                        None,
                                        Action::SaveComment,
                                        cx,
                                    )
                                    .bg(t.button)
                                    .text_color(t.button_text)
                                    .when(self.input.read(cx).text().trim().is_empty(), |b| {
                                        b.opacity(0.4)
                                    }),
                                )
                            })
                            .when(!draft, |d| {
                                d.child(self.button(
                                    format!("review-comment-edit-{}", c.id),
                                    "编辑评论",
                                    None,
                                    Action::EditComment(c.id),
                                    cx,
                                ))
                                .child(self.button(
                                    format!("review-comment-delete-{}", c.id),
                                    "删除",
                                    None,
                                    Action::DeleteComment(c.id),
                                    cx,
                                ))
                            }),
                    ),
            )
    }

    pub(super) fn dialog(&self, cx: &Context<Self>) -> Stateful<Div> {
        if self.pr_open {
            return self.pull_request_dialog(cx);
        }
        let t = Theme::for_mode(self.mode);
        let confirm = self.confirm.clone();
        let commit = self.commit_open;
        let title = if commit {
            "提交或推送"
        } else {
            "还原更改？"
        };
        let (adds, dels) = if self.commit_all {
            (
                self.snapshot.working_additions,
                self.snapshot.working_deletions,
            )
        } else {
            (self.snapshot.index_additions, self.snapshot.index_deletions)
        };
        let disabled = self.busy
            || (commit && self.new_branch && self.branch_input.read(cx).text().trim() == "codex/");
        let mut card = div()
            .id("review-dialog")
            .role(Role::Dialog)
            .aria_label(title)
            .w(px(420.))
            .max_w_full()
            .m(px(16.))
            .p(px(if commit { 5. } else { 20. }))
            .rounded(px(20.))
            .bg(t.control)
            .border_1()
            .border_color(t.border)
            .shadow_lg()
            .relative()
            .flex()
            .flex_col()
            .gap(px(if commit { 2. } else { 16. }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        if commit {
            card = card
                .child(
                    self.button(
                        "review-commit-branch",
                        if self.new_branch {
                            "新分支".into()
                        } else {
                            self.snapshot.branch.clone()
                        },
                        None,
                        Action::Menu(Menu::CommitBranch),
                        cx,
                    )
                    .h(px(36.))
                    .w_full()
                    .justify_start(),
                )
                .when(self.new_branch, |d| {
                    d.child(
                        div()
                            .h(px(36.))
                            .px(px(12.))
                            .child(self.branch_input.clone()),
                    )
                })
                .child(
                    div()
                        .h(px(80.))
                        .px(px(12.))
                        .py(px(8.))
                        .child(self.commit_input.clone()),
                )
                .child(
                    div()
                        .h(px(36.))
                        .px(px(4.))
                        .flex()
                        .items_center()
                        .child(self.button(
                            "review-commit-all",
                            if self.commit_all {
                                "☑ 包含未暂存的更改"
                            } else {
                                "☐ 包含未暂存的更改"
                            },
                            None,
                            Action::CommitAll,
                            cx,
                        ))
                        .child(div().flex_1())
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(addition_color(self.mode))
                                .child(format!("+{adds}")),
                        )
                        .child(
                            div()
                                .pr(px(8.))
                                .text_size(px(13.))
                                .text_color(deletion_color(self.mode))
                                .child(format!(" -{dels}")),
                        ),
                )
                .child(div().h(px(1.)).my(px(6.)).bg(t.border))
                .child(
                    self.button(
                        "review-dialog-confirm",
                        if self.busy {
                            "正在提交…"
                        } else {
                            "提交"
                        },
                        None,
                        Action::SaveCommit,
                        cx,
                    )
                    .w_full()
                    .justify_start()
                    .h(px(36.))
                    .when(disabled, |b| b.opacity(0.4)),
                )
                .child(
                    self.button(
                        "review-commit-and-push",
                        "提交并推送",
                        None,
                        Action::CommitAndPush,
                        cx,
                    )
                    .w_full()
                    .justify_start()
                    .h(px(36.))
                    .when(disabled, |b| b.opacity(0.4)),
                )
                .child(
                    self.button(
                        "review-push-only",
                        "推送",
                        None,
                        Action::Mutation(Mutation::Push),
                        cx,
                    )
                    .w_full()
                    .justify_start()
                    .h(px(36.))
                    .when(self.busy, |b| b.opacity(0.4)),
                );
        } else {
            card = card
                .child(div().text_size(px(18.)).text_color(t.text).child(title))
                .child(
                    div()
                        .text_size(px(13.))
                        .line_height(px(20.))
                        .text_color(t.text_secondary)
                        .child(match &confirm {
                            Some(Mutation::Discard(path)) => format!("这将还原 {path} 中的更改。"),
                            _ => "这将还原当前列表中的所有文件更改。".into(),
                        }),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.))
                        .child(self.button(
                            "review-dialog-cancel",
                            "取消",
                            None,
                            Action::CancelDialog,
                            cx,
                        ))
                        .child(
                            self.button(
                                "review-dialog-confirm",
                                "还原更改",
                                None,
                                Action::Mutation(confirm.unwrap_or(Mutation::DiscardAll)),
                                cx,
                            )
                            .bg(t.button)
                            .text_color(t.button_text),
                        ),
                );
        }
        card = card.when_some(self.display_error(), |d, error| {
            d.child(
                div()
                    .id("review-operation-error")
                    .role(Role::Alert)
                    .aria_label(error.clone())
                    .px(px(12.))
                    .py(px(8.))
                    .text_size(px(12.))
                    .text_color(deletion_color(self.mode))
                    .child(error),
            )
        });
        if self.menu == Some(Menu::CommitBranch) {
            card = card.child(self.popup(Menu::CommitBranch, cx));
        }
        div()
            .id("review-dialog-overlay")
            .absolute()
            .inset_0()
            .bg(rgba(0x00000055))
            .flex()
            .items_center()
            .justify_center()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|s, _, _, cx| {
                    s.commit_open = false;
                    s.confirm = None;
                    s.menu = None;
                    cx.notify();
                }),
            )
            .child(card)
    }
    pub fn render_overlay(&self, cx: &Context<Self>) -> Option<AnyElement> {
        (self.confirm.is_some() || self.commit_open).then(|| self.dialog(cx).into_any_element())
    }

    fn pull_request_dialog(&self, cx: &Context<Self>) -> Stateful<Div> {
        let t = Theme::for_mode(self.mode);
        let (adds, dels) = if self.commit_all {
            (
                self.snapshot.working_additions,
                self.snapshot.working_deletions,
            )
        } else {
            (self.snapshot.index_additions, self.snapshot.index_deletions)
        };
        let disabled =
            self.busy || (self.new_branch && self.branch_input.read(cx).text().trim() == "codex/");
        let mut card = div()
            .id("review-pr-dialog")
            .role(Role::Dialog)
            .aria_label("创建 PR")
            .w(px(420.))
            .max_w_full()
            .m(px(16.))
            .p(px(5.))
            .rounded(px(20.))
            .bg(t.control)
            .border_1()
            .border_color(t.border)
            .shadow_lg()
            .relative()
            .flex()
            .flex_col()
            .gap(px(2.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .h(px(36.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        self.button(
                            "review-pr-head",
                            if self.new_branch {
                                "新分支".into()
                            } else {
                                self.snapshot.branch.clone()
                            },
                            None,
                            Action::Menu(Menu::CommitBranch),
                            cx,
                        )
                        .min_w(px(0.))
                        .max_w(px(220.))
                        .truncate(),
                    )
                    .child(div().text_color(t.text_tertiary).child("→"))
                    .child(
                        self.button(
                            "review-pr-base",
                            format!("{} ⌄", self.pr_base),
                            None,
                            Action::Menu(Menu::PullRequestBase),
                            cx,
                        )
                        .min_w(px(0.))
                        .flex_1()
                        .truncate(),
                    ),
            );
        if self.pr_existing.is_none() {
            card = card
                .when(self.new_branch, |d| {
                    d.child(
                        div()
                            .h(px(36.))
                            .px(px(12.))
                            .child(self.branch_input.clone()),
                    )
                })
                .child(div().h(px(34.)).px(px(12.)).child(self.pr_title.clone()))
                .child(
                    div()
                        .h(px(80.))
                        .px(px(12.))
                        .child(self.commit_input.clone()),
                )
                .child(
                    div()
                        .h(px(36.))
                        .px(px(4.))
                        .flex()
                        .items_center()
                        .child(self.button(
                            "review-pr-include",
                            if self.commit_all {
                                "☑ 提交并推送本地更改"
                            } else {
                                "☐ 提交并推送本地更改"
                            },
                            None,
                            Action::CommitAll,
                            cx,
                        ))
                        .child(div().flex_1())
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(addition_color(self.mode))
                                .child(format!("+{adds}")),
                        )
                        .child(
                            div()
                                .pr(px(8.))
                                .text_size(px(13.))
                                .text_color(deletion_color(self.mode))
                                .child(format!(" -{dels}")),
                        ),
                )
                .child(div().h(px(1.)).my(px(6.)).bg(t.border))
                .child(
                    self.button(
                        "review-create-draft-pr",
                        if self.busy {
                            "正在创建…"
                        } else {
                            "创建草稿 PR"
                        },
                        None,
                        Action::CreatePullRequest(true),
                        cx,
                    )
                    .h(px(36.))
                    .w_full()
                    .justify_start()
                    .when(disabled, |b| b.opacity(0.4)),
                )
                .child(
                    self.button(
                        "review-create-pr",
                        "创建 Pull Request   ⌘⏎",
                        None,
                        Action::CreatePullRequest(false),
                        cx,
                    )
                    .h(px(36.))
                    .w_full()
                    .justify_start()
                    .when(disabled, |b| b.opacity(0.4)),
                );
        } else {
            card = card.child(
                div()
                    .p(px(12.))
                    .min_h(px(100.))
                    .text_size(px(13.))
                    .text_color(t.text_tertiary)
                    .child("此分支已存在 Pull Request"),
            );
        }
        card = card
            .when_some(self.pr_existing.clone(), |d, url| {
                d.child(
                    self.button(
                        "review-open-pr",
                        "在浏览器中打开 PR",
                        None,
                        Action::OpenPr(url),
                        cx,
                    )
                    .h(px(36.))
                    .w_full()
                    .justify_start(),
                )
            })
            .when_some(self.display_error(), |d, error| {
                d.child(
                    div()
                        .id("review-pr-error")
                        .role(Role::Alert)
                        .aria_label(error.clone())
                        .p(px(12.))
                        .text_size(px(12.))
                        .text_color(deletion_color(self.mode))
                        .child(error),
                )
            });
        if let Some(m) = self
            .menu
            .clone()
            .filter(|m| matches!(m, Menu::CommitBranch | Menu::PullRequestBase))
        {
            card = card.child(self.popup(m, cx));
        }
        div()
            .id("review-pr-overlay")
            .absolute()
            .inset_0()
            .bg(rgba(0x00000055))
            .flex()
            .items_center()
            .justify_center()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|s, _, _, cx| {
                    s.commit_open = false;
                    s.pr_open = false;
                    s.menu = None;
                    cx.notify();
                }),
            )
            .child(card)
    }
}
