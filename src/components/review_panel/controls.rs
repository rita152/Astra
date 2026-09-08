use super::render::{addition_color, deletion_color};
use super::*;
use crate::{components::icons::icon, theme::Theme};
use gpui::{
    ClipboardItem, Div, MouseButton, Render, Role, SharedString, Stateful, div, prelude::*, rgba,
};

#[derive(Clone)]
pub(super) enum Action {
    Menu(Menu),
    Scope(Scope),
    Refresh,
    Wrap,
    Context(usize),
    LoadFiles,
    Rich,
    Words,
    Whitespace,
    CopyPatch,
    Collapse,
    Split,
    Tree,
    Close,
    AddTab,
    Fullscreen,
    Jump(usize),
    Toggle(usize),
    Copy(String),
    Open(usize),
    OpenTab(String),
    Reveal(usize),
    Mutation(Mutation),
    Confirm(Mutation),
    Commit,
    SaveCommit,
    SaveComment,
    EditComment(u64),
    CancelComment,
    DeleteComment(u64),
    CancelDialog,
    CommitAll,
    CommitAndPush,
    PullRequest,
    CreatePullRequest(bool),
    PrBase(String),
    OpenPr(String),
    Viewed(usize),
    NewBranch(bool),
}

impl ReviewPanel {
    pub(super) fn action(&mut self, a: Action, w: &mut Window, cx: &mut Context<Self>) {
        if matches!(&a,Action::Open(i)|Action::Reveal(i)|Action::Viewed(i)|Action::Toggle(i)|Action::Jump(i)|Action::Context(i)|Action::Menu(Menu::File(i)) if *i>=self.snapshot.files.len())
        {
            return;
        }
        let save = matches!(
            a,
            Action::Wrap
                | Action::LoadFiles
                | Action::Rich
                | Action::Words
                | Action::Whitespace
                | Action::Split
                | Action::Tree
        );
        match a {
            Action::Menu(m) => {
                let jump = m == Menu::Jump;
                self.toggle_menu(m, cx);
                if jump {
                    self.jump.read(cx).focus_handle(cx).focus(w, cx);
                    self.focus_pending = false;
                }
            }
            Action::Scope(s) => {
                self.history_pinned = false;
                self.last_turn = self.latest_turn.clone();
                self.change_scope(s, cx);
            }
            Action::Refresh => {
                self.operation_error = None;
                self.menu = None;
                self.refresh(cx);
            }
            Action::Wrap => {
                self.wrap = !self.wrap;
                self.menu = None;
                self.scroll.remeasure();
            }
            Action::Context(file) => {
                let path = self.snapshot.files[file].path.clone();
                if !self.expanded_files.remove(&path) {
                    self.expanded_files.insert(path);
                }
                self.menu = None;
                self.generation += 1;
                self.loading = false;
                self.refresh(cx);
            }
            Action::LoadFiles => {
                self.load_files = !self.load_files;
                self.menu = None;
                self.generation += 1;
                self.loading = false;
                self.refresh(cx);
            }
            Action::Rich => {
                self.rich = !self.rich;
                self.menu = None;
                self.rebuild(cx);
            }
            Action::Words => {
                self.words = !self.words;
                self.menu = None;
            }
            Action::Whitespace => {
                self.whitespace = !self.whitespace;
                self.menu = None;
                self.refresh(cx);
            }
            Action::CopyPatch => {
                cx.write_to_clipboard(ClipboardItem::new_string(git_review::apply_command(
                    &self.snapshot.files,
                )));
                self.menu = None;
                self.show_notice("已复制 git apply 命令".into(), cx);
            }
            Action::Collapse => self.toggle_all(cx),
            Action::Split => {
                self.split = !self.split;
                self.rebuild(cx);
            }
            Action::Tree => self.tree_open = !self.tree_open,
            Action::Close => cx.emit(ReviewEvent::Close),
            Action::AddTab => cx.emit(ReviewEvent::AddTab),
            Action::Fullscreen => cx.emit(ReviewEvent::Fullscreen),
            Action::Jump(i) => self.jump_to(i, cx),
            Action::Toggle(i) => self.toggle_file(i, cx),
            Action::Copy(s) => {
                cx.write_to_clipboard(ClipboardItem::new_string(s));
                self.menu = None;
                self.show_notice("已复制".into(), cx);
            }
            Action::Viewed(i) => {
                let f = &self.snapshot.files[i];
                if self.viewed.remove(&f.path).is_some() {
                    self.collapsed.remove(&f.path);
                } else {
                    self.viewed.insert(f.path.clone(), f.patch.clone());
                    self.collapsed.insert(f.path.clone());
                }
                self.rebuild(cx);
            }
            Action::OpenTab(path) => cx.emit(ReviewEvent::OpenFile { path, line: None }),
            Action::Open(i) => {
                let file = &self.snapshot.files[i];
                cx.emit(ReviewEvent::OpenFile {
                    path: self.snapshot.root.join(&file.path).to_string_lossy().into(),
                    line: None,
                });
                self.menu = None;
            }
            Action::Reveal(i) => {
                let _ = std::process::Command::new("open")
                    .arg("-R")
                    .arg(self.snapshot.root.join(&self.snapshot.files[i].path))
                    .spawn();
                self.menu = None;
            }
            Action::Mutation(op) => self.mutate(op, cx),
            Action::Confirm(op) => {
                self.pr_open = false;
                self.menu = None;
                self.confirm = Some(op);
            }
            Action::Commit => self.open_commit(cx),
            Action::SaveCommit => {
                if self.busy
                    || (self.new_branch && self.branch_input.read(cx).text().trim() == "codex/")
                {
                    return;
                }
                let message = self.commit_input.read(cx).text().to_owned();
                self.mutate(
                    Mutation::Commit {
                        message,
                        stage_all: self.commit_all,
                        branch: self
                            .new_branch
                            .then(|| self.branch_input.read(cx).text().trim().to_owned()),
                    },
                    cx,
                );
            }
            Action::SaveComment => self.save_comment(cx),
            Action::EditComment(id) => self.edit_comment(id, cx),
            Action::CancelComment => {
                self.draft = None;
                self.editing_comment = None;
                self.rebuild(cx);
                self.focus_pending = true;
            }
            Action::DeleteComment(id) => {
                if self.editing_comment == Some(id) {
                    self.editing_comment = None;
                }
                self.comments.retain(|c| c.id != id);
                self.emit_comments(cx);
                self.rebuild(cx);
            }
            Action::CancelDialog => {
                self.pr_open = false;
                self.confirm = None;
                self.commit_open = false;
                self.focus_pending = true;
            }
            Action::CommitAll => self.commit_all = !self.commit_all,
            Action::CommitAndPush => {
                if self.busy
                    || (self.new_branch && self.branch_input.read(cx).text().trim() == "codex/")
                {
                    return;
                }
                self.push_after_commit = true;
                self.action(Action::SaveCommit, w, cx);
            }
            Action::NewBranch(new) => {
                self.new_branch = new;
                self.menu = None;
            }
            Action::PullRequest => self.open_pull_request(cx),
            Action::PrBase(base) => {
                self.pr_base = base;
                self.menu = None;
            }
            Action::OpenPr(url) => cx.open_url(&url),
            Action::CreatePullRequest(draft) => {
                if self.busy
                    || (self.new_branch && self.branch_input.read(cx).text().trim() == "codex/")
                {
                    return;
                }
                if let Some(url) = &self.pr_existing {
                    cx.open_url(url);
                    return;
                }
                self.mutate(
                    Mutation::PullRequest(git_review::PullRequestOptions {
                        title: self.pr_title.read(cx).text().into(),
                        body: self.commit_input.read(cx).text().into(),
                        base: self.pr_base.clone(),
                        branch: self
                            .new_branch
                            .then(|| self.branch_input.read(cx).text().trim().into()),
                        include_local: self.commit_all,
                        draft,
                    }),
                    cx,
                );
            }
        }
        if save {
            self.save_preferences(cx);
        }
        cx.notify();
    }

    pub(super) fn button(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        glyph: Option<&'static str>,
        a: Action,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let t = Theme::for_mode(self.mode);
        let label = label.into();
        let hint = label.clone();
        let key_action = a.clone();
        div()
            .id(id.into())
            .role(Role::Button)
            .aria_label(label.clone())
            .focusable()
            .tab_stop(true)
            .h(px(28.))
            .px(px(if glyph.is_some() { 6. } else { 8. }))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(4.))
            .rounded(px(12.5))
            .text_size(px(13.))
            .line_height(px(18.))
            .cursor_pointer()
            .text_color(t.text_secondary)
            .hover(move |s| s.bg(t.sidebar_hover).text_color(t.text))
            .focus_visible(move |s| s.border_1().border_color(t.accent))
            .tooltip(move |_, cx| cx.new(|_| ReviewTooltip(hint.clone())).into())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |s, _, w, cx| {
                cx.stop_propagation();
                s.action(a.clone(), w, cx);
            }))
            .on_key_down(cx.listener(move |s, e: &KeyDownEvent, w, cx| {
                if e.keystroke.key == "enter" || e.keystroke.key == "space" {
                    if s.menu.is_some() && matches!(key_action, Action::Menu(_)) {
                        s.key_down(e, w, cx);
                    } else {
                        cx.stop_propagation();
                        s.action(key_action.clone(), w, cx);
                    }
                }
            }))
            .when_some(glyph, |b, g| {
                b.child(icon(g, t.text_secondary.into()).size(px(16.)))
            })
            .when(glyph.is_none(), |b| b.child(label))
    }

    pub(super) fn menu_actions(&self, menu: &Menu) -> Vec<(String, Action)> {
        let toggle = |on: bool, yes: &str, no: &str| if on { yes.into() } else { no.into() };
        match menu {
            Menu::Scope => vec![
                ("上一轮".into(), Action::Scope(Scope::LastTurn)),
                ("未提交".into(), Action::Scope(Scope::Uncommitted)),
                ("未暂存".into(), Action::Scope(Scope::Unstaged)),
                ("已暂存".into(), Action::Scope(Scope::Staged)),
                ("已提交  ›".into(), Action::Menu(Menu::Commits)),
                (
                    "分支".into(),
                    Action::Scope(Scope::Branch(
                        self.snapshot.upstream.clone().unwrap_or_else(|| {
                            self.snapshot
                                .branches
                                .iter()
                                .find(|s| s.as_str() == "main")
                                .cloned()
                                .unwrap_or_else(|| "HEAD".into())
                        }),
                    )),
                ),
            ],
            Menu::View => vec![
                ("刷新".into(), Action::Refresh),
                (
                    toggle(self.wrap, "禁用自动换行", "启用自动换行"),
                    Action::Wrap,
                ),
                (
                    toggle(self.load_files, "不加载完整文件", "加载完整文件"),
                    Action::LoadFiles,
                ),
                (
                    toggle(self.rich, "禁用富文本预览", "启用富文本预览"),
                    Action::Rich,
                ),
                (
                    toggle(self.words, "禁用文字差异", "启用文字差异"),
                    Action::Words,
                ),
                (
                    toggle(self.whitespace, "显示空白字符", "隐藏空白字符"),
                    Action::Whitespace,
                ),
                ("复制 git apply 命令".into(), Action::CopyPatch),
            ],
            Menu::Git => vec![
                ("提交或推送".into(), Action::Commit),
                ("创建 Pull Request".into(), Action::PullRequest),
            ],
            Menu::CommitBranch => vec![
                (self.snapshot.branch.clone(), Action::NewBranch(false)),
                ("新分支".into(), Action::NewBranch(true)),
            ],
            Menu::PullRequestBase => {
                let mut names = self
                    .snapshot
                    .branches
                    .iter()
                    .map(|s| s.trim_start_matches("origin/").to_owned())
                    .collect::<Vec<_>>();
                names.sort();
                names.dedup();
                names
                    .into_iter()
                    .map(|s| (s.clone(), Action::PrBase(s)))
                    .collect()
            }
            Menu::Branch => self
                .snapshot
                .branches
                .iter()
                .map(|s| (s.clone(), Action::Scope(Scope::Branch(s.clone()))))
                .collect(),
            Menu::Commits => self
                .snapshot
                .commits
                .iter()
                .map(|(sha, title)| {
                    (
                        format!("{}  {title}", &sha[..7.min(sha.len())]),
                        Action::Scope(Scope::Commit(sha.clone())),
                    )
                })
                .collect(),
            Menu::Jump => self
                .matching_files(&self.jump_query)
                .into_iter()
                .map(|i| (self.snapshot.files[i].path.clone(), Action::Jump(i)))
                .collect(),
            Menu::File(i) => {
                let file = &self.snapshot.files[*i];
                let mut a = vec![
                    ("打开文件".into(), Action::Open(*i)),
                    ("在访达中显示".into(), Action::Reveal(*i)),
                    ("复制路径".into(), Action::Copy(file.path.clone())),
                    (
                        "复制绝对路径".into(),
                        Action::Copy(self.snapshot.root.join(&file.path).to_string_lossy().into()),
                    ),
                ];
                if self.scope.editable() {
                    a.push((
                        if self.scope == Scope::Staged {
                            "取消暂存"
                        } else {
                            "暂存更改"
                        }
                        .into(),
                        Action::Mutation(if self.scope == Scope::Staged {
                            Mutation::Unstage(Some(file.path.clone()))
                        } else {
                            Mutation::Stage(Some(file.path.clone()))
                        }),
                    ));
                    a.push((
                        "撤销更改…".into(),
                        Action::Confirm(Mutation::Discard(file.path.clone())),
                    ));
                }
                a
            }
        }
    }
    pub(super) fn activate_menu(
        &mut self,
        m: Menu,
        index: usize,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((_, a)) = self.menu_actions(&m).get(index) {
            self.action(a.clone(), w, cx);
        }
    }

    pub(super) fn popup(&self, m: Menu, cx: &Context<Self>) -> Stateful<Div> {
        let t = Theme::for_mode(self.mode);
        let width = if m == Menu::Jump || m == Menu::Commits {
            360.
        } else {
            224.
        };
        let mut menu = div()
            .id("review-popup")
            .role(Role::Menu)
            .absolute()
            .top(px(
                if matches!(m, Menu::CommitBranch | Menu::PullRequestBase) {
                    36.
                } else if m == Menu::Branch {
                    112.
                } else {
                    80.
                },
            ))
            .w(px(width))
            .max_w_full()
            .p(px(5.))
            .rounded(px(13.))
            .bg(t.elevated)
            .border_1()
            .border_color(t.border)
            .shadow_lg()
            .flex()
            .flex_col()
            .on_mouse_down_out(cx.listener(|s, _, _, cx| {
                s.menu = None;
                cx.notify();
            }));
        menu = match m {
            Menu::Scope | Menu::Branch => menu.left(px(8.)),
            _ => menu.right(px(8.)),
        };
        if m == Menu::Jump {
            menu = menu.child(div().h(px(34.)).px(px(8.)).child(self.jump.clone()));
        }
        let actions = self.menu_actions(&m);
        let mut items = div()
            .id("review-menu-scroll")
            .max_h(px(360.))
            .overflow_y_scroll()
            .flex()
            .flex_col();
        if actions.is_empty() {
            items = items.child(
                div()
                    .p(px(12.))
                    .text_color(t.text_tertiary)
                    .text_size(px(13.))
                    .child(if m == Menu::Commits {
                        "分支上暂无提交记录"
                    } else {
                        "没有匹配的文件"
                    }),
            );
        }
        for (i, (label, a)) in actions.into_iter().enumerate() {
            let key_a = a.clone();
            let selected = self.menu_selected == i;
            items = items.child(
                div()
                    .id(("review-menu-item", i))
                    .role(Role::MenuItem)
                    .aria_label(label.clone())
                    .aria_selected(selected)
                    .focusable()
                    .tab_stop(true)
                    .min_h(px(29.))
                    .px(px(8.))
                    .py(px(5.))
                    .rounded(px(8.))
                    .text_size(px(13.))
                    .line_height(px(19.))
                    .text_color(t.text)
                    .cursor_pointer()
                    .when(selected, |b| b.bg(t.sidebar_hover))
                    .hover(move |b| b.bg(t.sidebar_hover))
                    .on_click(cx.listener(move |s, _, w, cx| {
                        cx.stop_propagation();
                        s.action(a.clone(), w, cx);
                    }))
                    .on_key_down(cx.listener(move |s, e: &KeyDownEvent, w, cx| {
                        if e.keystroke.key == "enter" || e.keystroke.key == "space" {
                            cx.stop_propagation();
                            s.action(key_a.clone(), w, cx);
                        }
                    }))
                    .child(label),
            );
        }
        menu.child(items)
    }

    pub(super) fn toolbar(&self, cx: &Context<Self>) -> Div {
        let t = Theme::for_mode(self.mode);
        let (adds, dels) = self.counts();
        div()
            .w_full()
            .min_h(px(40.))
            .flex_none()
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(3.))
            .border_b_1()
            .border_color(t.border)
            .child(
                self.button(
                    "review-scope",
                    format!("{}  ⌄", self.scope.label()),
                    None,
                    Action::Menu(Menu::Scope),
                    cx,
                )
                .text_size(px(14.)),
            )
            .child(div().flex().gap(px(4.)).text_size(px(13.)).when(
                adds + dels > 0 && self.panel_width > 440.,
                |d| {
                    d.child(
                        div()
                            .text_color(addition_color(self.mode))
                            .child(format!("+{adds}")),
                    )
                    .child(
                        div()
                            .text_color(deletion_color(self.mode))
                            .child(format!("-{dels}")),
                    )
                },
            ))
            .child(div().flex_1().min_w(px(0.)))
            .child(self.button(
                "review-options",
                "查看选项",
                Some("review-options"),
                Action::Menu(Menu::View),
                cx,
            ))
            .child(self.button(
                "review-collapse",
                if self.collapsed.len() == self.snapshot.files.len() {
                    "展开全部差异"
                } else {
                    "折叠全部差异"
                },
                Some("review-collapse"),
                Action::Collapse,
                cx,
            ))
            .child(self.button(
                "review-jump",
                "跳转到文件",
                Some("review-jump"),
                Action::Menu(Menu::Jump),
                cx,
            ))
            .child(
                self.button(
                    "review-split",
                    if self.split {
                        "切换到统一差异视图"
                    } else {
                        "切换到拆分差异视图"
                    },
                    Some("review-split"),
                    Action::Split,
                    cx,
                )
                .when(self.split, |b| b.bg(t.sidebar_hover)),
            )
            .when(!self.compact(), |d| {
                d.child(
                    self.button(
                        "review-tree",
                        if self.tree_open {
                            "隐藏文件"
                        } else {
                            "显示文件"
                        },
                        Some("review-tree"),
                        Action::Tree,
                        cx,
                    )
                    .when(self.tree_open, |b| b.bg(t.sidebar_hover)),
                )
            })
            .child(
                self.button(
                    "review-git",
                    "提交或推送",
                    Some("review-commit"),
                    Action::Commit,
                    cx,
                )
                .border_1()
                .border_color(t.border),
            )
            .child(self.button(
                "review-more-git",
                "更多 Git 操作",
                Some("chevron-down"),
                Action::Menu(Menu::Git),
                cx,
            ))
    }
    pub(super) fn counts(&self) -> (usize, usize) {
        self.snapshot
            .files
            .iter()
            .fold((0, 0), |(a, d), f| (a + f.additions, d + f.deletions))
    }
}

struct ReviewTooltip(SharedString);
impl Render for ReviewTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.))
            .py(px(5.))
            .rounded(px(6.))
            .bg(rgba(0x333333ff))
            .text_color(rgba(0xffffffff))
            .text_size(px(12.))
            .child(self.0.clone())
    }
}
