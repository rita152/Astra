//! Native, interactive workspace review. Geometry and controls are captured from
//! ChatGPT's review pane through a dedicated CDP instance (see capture script).

#[cfg(test)]
mod benchmarks;
mod cache;
mod comments;
mod controls;
mod dialogs;
mod files;
mod render;
mod scroll;
#[cfg(test)]
mod tests;

use super::{
    file_change::{DiffLineKind, DiffReviewPresentation},
    prompt_input::{PromptChanged, PromptInput, PromptSubmitted},
};
use crate::git_review::ReviewComment as Comment;
use crate::{
    git_review::{self, FileDiff, Line, LineKind, Mutation, Scope, Snapshot},
    theme::ThemeMode,
};
use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyDownEvent,
    ListAlignment, ListOffset, ListState, Window, px,
};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

#[derive(Clone, Debug)]
pub enum ReviewEvent {
    Close,
    AddTab,
    Fullscreen,
    OpenFile { path: String, line: Option<usize> },
    CommentsChanged(Vec<Comment>),
    PreferencesChanged(crate::workspace::ReviewPreferences),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Menu {
    Scope,
    View,
    Git,
    Branch,
    Commits,
    Jump,
    File(usize),
    CommitBranch,
    PullRequestBase,
}

#[derive(Clone)]
enum Row {
    Header(usize),
    Hunk(usize, usize),
    Code {
        file: usize,
        left: Option<Line>,
        right: Option<Line>,
    },
    Comment(u64),
    Draft,
    Binary(usize),
    Empty(usize),
    Preview(usize),
}

#[derive(Clone)]
struct Draft {
    file: usize,
    start: u32,
    end: u32,
    old: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TextCursor {
    row: usize,
    byte: usize,
}
#[derive(Clone, Copy)]
struct Selection {
    anchor: TextCursor,
    head: TextCursor,
    old: bool,
}

pub struct ReviewPanel {
    cwd: PathBuf,
    mode: ThemeMode,
    scope: Scope,
    snapshot: Arc<Snapshot>,
    file_tabs: Vec<String>,
    last_turn: Vec<FileDiff>,
    latest_turn: Vec<FileDiff>,
    history_pinned: bool,
    error: Option<String>,
    operation_error: Option<String>,
    notice: Option<String>,
    notice_serial: u64,
    last_render: std::time::Instant,
    active: bool,
    loading: bool,
    busy: bool,
    generation: u64,
    focus: FocusHandle,
    focus_pending: bool,
    focus_input: bool,
    menu: Option<Menu>,
    menu_selected: usize,
    filter: Entity<PromptInput>,
    jump: Entity<PromptInput>,
    input: Entity<super::file_editor::FileEditor>,
    branch_input: Entity<PromptInput>,
    commit_input: Entity<super::file_editor::FileEditor>,
    pr_title: Entity<PromptInput>,
    pr_open: bool,
    pr_base: String,
    pr_existing: Option<String>,
    query: String,
    jump_query: String,
    tree_open: bool,
    split: bool,
    wrap: bool,
    expanded_files: HashSet<String>,
    load_files: bool,
    rich: bool,
    words: bool,
    whitespace: bool,
    collapsed: HashSet<String>,
    viewed: HashMap<String, String>,
    folder_collapsed: HashSet<String>,
    selected_file: usize,
    rows: Arc<Vec<Row>>,
    render_cache: cache::RenderCache,
    scroll: ListState,
    horizontal_offset: f32,
    horizontal_drag: Option<(gpui::Pixels, f32)>,
    horizontal_focus: FocusHandle,
    max_line_width: f32,
    diff_width: f32,
    panel_width: f32,
    comments: Vec<Comment>,
    editing_comment: Option<u64>,
    draft: Option<Draft>,
    next_comment: u64,
    selection: Option<Selection>,
    selecting: bool,
    gutter_drag: Option<Draft>,
    confirm: Option<Mutation>,
    commit_open: bool,
    commit_all: bool,
    new_branch: bool,
    push_after_commit: bool,
    previews: HashMap<String, Entity<super::markdown::MarkdownPreview>>,
}

impl EventEmitter<ReviewEvent> for ReviewPanel {}
impl Focusable for ReviewPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl ReviewPanel {
    pub fn new(cwd: PathBuf, mode: ThemeMode, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| {
            let mut i = PromptInput::inline_other(mode, "筛选文件…", false, cx);
            i.set_accessible_name("筛选审查文件");
            i
        });
        let jump = cx.new(|cx| {
            let mut i = PromptInput::inline_other(mode, "跳转到文件", false, cx);
            i.set_accessible_name("跳转到文件");
            i
        });
        let input = cx.new(|cx| super::file_editor::FileEditor::prose(mode, "请求更改", cx));
        let commit_input = cx.new(|cx| super::file_editor::FileEditor::prose(mode, "提交信息", cx));
        let pr_title = cx.new(|cx| {
            let mut i = PromptInput::inline_other(mode, "标题", false, cx);
            i.set_accessible_name("PR 标题");
            i
        });
        cx.subscribe(&pr_title, |_, _, _: &PromptChanged, cx| cx.notify())
            .detach();
        cx.subscribe(
            &commit_input,
            |_, _, _: &super::file_editor::EditorEvent, cx| cx.notify(),
        )
        .detach();
        let branch_input = cx.new(|cx| {
            let mut i = PromptInput::inline_other(mode, "codex/", false, cx);
            i.set_accessible_name("分支名称");
            i
        });
        cx.subscribe(&branch_input, |_, _, _: &PromptChanged, cx| cx.notify())
            .detach();
        cx.subscribe(&filter, |s, i, _: &PromptChanged, cx| {
            s.query = i.read(cx).text().into();
            s.rebuild(cx);
        })
        .detach();
        cx.subscribe(&jump, |s, i, _: &PromptChanged, cx| {
            s.jump_query = i.read(cx).text().into();
            s.menu_selected = 0;
            cx.notify();
        })
        .detach();
        cx.subscribe(&filter, |s, _, _: &PromptSubmitted, cx| {
            if let Some(i) = s.matching_files(&s.query).first().copied() {
                s.jump_to(i, cx);
            }
        })
        .detach();
        cx.subscribe(&jump, |s, _, _: &PromptSubmitted, cx| {
            if let Some(i) = s
                .matching_files(&s.jump_query)
                .get(s.menu_selected)
                .copied()
            {
                s.jump_to(i, cx);
            }
        })
        .detach();
        cx.subscribe(&input, |_, _, _: &super::file_editor::EditorEvent, cx| {
            cx.notify()
        })
        .detach();
        let mut panel = Self {
            cwd,
            mode,
            scope: Scope::Uncommitted,
            snapshot: Arc::new(Snapshot::default()),
            file_tabs: Vec::new(),
            last_turn: vec![],
            latest_turn: vec![],
            history_pinned: false,
            error: None,
            operation_error: None,
            notice: None,
            notice_serial: 0,
            last_render: std::time::Instant::now(),
            active: true,
            loading: false,
            busy: false,
            generation: 0,
            focus: cx.focus_handle().tab_stop(true),
            focus_pending: true,
            focus_input: false,
            menu: None,
            menu_selected: 0,
            filter,
            jump,
            input,
            branch_input,
            commit_input,
            pr_title,
            pr_open: false,
            pr_base: String::new(),
            pr_existing: None,
            query: String::new(),
            jump_query: String::new(),
            tree_open: true,
            split: false,
            wrap: false,
            expanded_files: HashSet::new(),
            load_files: true,
            rich: false,
            words: false,
            whitespace: false,
            collapsed: HashSet::new(),
            viewed: HashMap::new(),
            folder_collapsed: HashSet::new(),
            selected_file: 0,
            rows: Arc::new(vec![]),
            render_cache: cache::RenderCache::default(),
            scroll: ListState::new(0, ListAlignment::Top, px(200.)),
            horizontal_offset: 0.,
            horizontal_drag: None,
            horizontal_focus: cx.focus_handle().tab_stop(true),
            max_line_width: 0.,
            diff_width: 0.,
            panel_width: 800.,
            comments: vec![],
            editing_comment: None,
            draft: None,
            next_comment: 1,
            selection: None,
            selecting: false,
            gutter_drag: None,
            confirm: None,
            commit_open: false,
            commit_all: true,
            new_branch: false,
            push_after_commit: false,
            previews: HashMap::new(),
        };
        panel.refresh(cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(3)).await;
                if this
                    .update(cx, |s, cx| {
                        if s.active
                            && s.last_render.elapsed() < Duration::from_secs(4)
                            && !s.busy
                            && !s.loading
                            && s.draft.is_none()
                            && s.editing_comment.is_none()
                            && s.menu.is_none()
                            && !s.commit_open
                            && s.confirm.is_none()
                        {
                            s.refresh(cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        panel
    }

    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.input.update(cx, |i, cx| i.set_mode(mode, cx));
        self.commit_input.update(cx, |i, cx| i.set_mode(mode, cx));
        for i in [&self.filter, &self.jump, &self.branch_input, &self.pr_title] {
            i.update(cx, |i, cx| i.set_mode(mode, cx));
        }
        for p in self.previews.values() {
            p.update(cx, |p, cx| p.set_mode(mode, cx));
        }
        cx.notify();
    }
    pub fn focus(&mut self, cx: &mut Context<Self>) {
        self.active = true;
        self.focus_pending = true;
        self.refresh(cx);
        cx.notify();
    }
    pub fn deactivate(&mut self) {
        self.active = false;
    }
    fn compact(&self) -> bool {
        self.panel_width < 560.
    }
    pub fn set_file_tabs(&mut self, paths: Vec<String>, cx: &mut Context<Self>) {
        self.file_tabs = paths;
        cx.notify();
    }
    pub fn apply_preferences(
        &mut self,
        p: crate::workspace::ReviewPreferences,
        cx: &mut Context<Self>,
    ) {
        self.split = p.split;
        self.wrap = p.wrap;
        self.load_files = p.load_files;
        self.rich = p.rich;
        self.words = p.words;
        self.whitespace = p.ignore_whitespace;
        self.tree_open = p.tree_open;
        self.generation += 1;
        self.loading = false;
        self.rebuild(cx);
        self.refresh(cx);
    }
    fn save_preferences(&self, cx: &mut Context<Self>) {
        cx.emit(ReviewEvent::PreferencesChanged(
            crate::workspace::ReviewPreferences {
                split: self.split,
                wrap: self.wrap,
                load_files: self.load_files,
                rich: self.rich,
                words: self.words,
                ignore_whitespace: self.whitespace,
                tree_open: self.tree_open,
            },
        ));
    }
    pub fn set_last_turn(
        &mut self,
        review: DiffReviewPresentation,
        open: bool,
        cx: &mut Context<Self>,
    ) {
        let files = if let Some(raw) = review.raw_diff.as_deref() {
            let mut files = git_review::parse_unified(raw);
            for file in &mut files {
                if let Ok(path) = std::path::Path::new(&file.path).strip_prefix(&self.cwd) {
                    file.path = path.to_string_lossy().into();
                }
                if file.status == 'A' {
                    file.new_text = Some(
                        file.hunks
                            .iter()
                            .flat_map(|h| &h.lines)
                            .filter(|l| l.kind != LineKind::Deleted)
                            .map(|l| format!("{}\n", l.text))
                            .collect(),
                    );
                }
            }
            files
        } else {
            review
                .files
                .into_iter()
                .map(|f| FileDiff {
                    path: f.path,
                    old_path: None,
                    status: 'M',
                    binary: false,
                    patch: String::new(),
                    additions: f.additions as usize,
                    deletions: f.deletions as usize,
                    old_text: None,
                    new_text: None,
                    hunks: vec![git_review::Hunk {
                        header: String::new(),
                        patch: String::new(),
                        lines: f
                            .lines
                            .into_iter()
                            .map(|l| Line {
                                old: l.old_line,
                                new: l.new_line,
                                text: l.content,
                                kind: match l.kind {
                                    DiffLineKind::Added => LineKind::Added,
                                    DiffLineKind::Deleted => LineKind::Deleted,
                                    DiffLineKind::Context => LineKind::Context,
                                },
                            })
                            .collect(),
                    }],
                })
                .collect()
        };
        if open {
            self.last_turn = files;
            self.change_scope(Scope::LastTurn, cx);
            self.history_pinned = true;
        } else {
            self.latest_turn = files;
            if !self.history_pinned && self.last_turn != self.latest_turn {
                self.last_turn = self.latest_turn.clone();
                if self.scope == Scope::LastTurn {
                    self.install_last_turn(cx);
                }
            }
        }
    }

    fn install_last_turn(&mut self, cx: &mut Context<Self>) {
        let anchor = self.scroll_anchor();
        let mut snap = (*self.snapshot).clone();
        snap.files = self.last_turn.clone();
        self.snapshot = Arc::new(snap);
        self.rebuild(cx);
        self.restore_scroll(anchor);
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.loading || self.busy {
            return;
        }
        self.loading = true;
        self.generation += 1;
        let generation = self.generation;
        let cwd = self.cwd.clone();
        let scope = self.scope.clone();
        let expanded_files = self.expanded_files.clone();
        let whitespace = self.whitespace;
        let load_files = self.load_files;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    git_review::load_with_options(
                        &cwd,
                        &scope,
                        false,
                        whitespace,
                        load_files,
                        &expanded_files,
                    )
                    .map_err(|e| format!("{e:#}"))
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                if generation != s.generation {
                    return;
                }
                s.loading = false;
                match result {
                    Ok(mut snapshot) => {
                        if s.scope == Scope::LastTurn {
                            snapshot.files = s.last_turn.clone();
                        }
                        if snapshot != *s.snapshot {
                            let anchor = s.scroll_anchor();
                            for file in &snapshot.files {
                                if s.viewed.get(&file.path).is_some_and(|p| p != &file.patch) {
                                    s.viewed.remove(&file.path);
                                    s.collapsed.remove(&file.path);
                                }
                            }
                            s.snapshot = Arc::new(snapshot);
                            s.previews.clear();
                            s.rebuild(cx);
                            s.restore_scroll(anchor);
                        }
                        s.error = None;
                    }
                    Err(e) => {
                        s.error = Some(e);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn change_scope(&mut self, scope: Scope, cx: &mut Context<Self>) {
        self.scope = scope;
        self.menu = None;
        self.draft = None;
        self.editing_comment = None;
        self.selection = None;
        self.generation += 1;
        self.loading = false;
        self.collapsed.clear();
        self.selected_file = 0;
        self.scroll.scroll_to(ListOffset::default());
        let mut snapshot = (*self.snapshot).clone();
        snapshot.files.clear();
        self.snapshot = Arc::new(snapshot);
        if self.scope == Scope::LastTurn {
            self.install_last_turn(cx);
        } else {
            self.rebuild(cx);
        }
        self.refresh(cx);
    }
    fn matching_files(&self, query: &str) -> Vec<usize> {
        let q = query.to_lowercase();
        self.snapshot
            .files
            .iter()
            .enumerate()
            .filter(|(_, f)| f.path.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect()
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        // Row indices change after folding, context expansion and comments.
        // Never apply an old UTF-8 selection range to a different code line.
        self.selection = None;
        self.selecting = false;
        self.render_cache.prepare(&self.snapshot, self.mode);
        self.max_line_width = self.render_cache.max_line_width;
        let anchor = self.scroll_anchor();
        let mut rows = Vec::new();
        for (i, file) in self.snapshot.files.iter().enumerate() {
            if !file
                .path
                .to_lowercase()
                .contains(&self.query.to_lowercase())
            {
                continue;
            }
            rows.push(Row::Header(i));
            if self.collapsed.contains(&file.path) {
                continue;
            }
            if file.binary {
                rows.push(Row::Binary(i));
                continue;
            }
            if self.rich && file.path.ends_with(".md") && file.new_text.is_some() {
                rows.push(Row::Preview(i));
                continue;
            }
            if file.hunks.is_empty() {
                rows.push(Row::Empty(i));
            }
            for (h, hunk) in file.hunks.iter().enumerate() {
                if !hunk.header.is_empty() {
                    rows.push(Row::Hunk(i, h));
                }
                let mut n = 0;
                while n < hunk.lines.len() {
                    let line = &hunk.lines[n];
                    if self.split && line.kind == LineKind::Deleted {
                        let start = n;
                        while n < hunk.lines.len() && hunk.lines[n].kind == LineKind::Deleted {
                            n += 1;
                        }
                        let added = n;
                        while n < hunk.lines.len() && hunk.lines[n].kind == LineKind::Added {
                            n += 1;
                        }
                        for j in 0..(added - start).max(n - added) {
                            let left = (j < added - start).then(|| hunk.lines[start + j].clone());
                            let right = (j < n - added).then(|| hunk.lines[added + j].clone());
                            rows.push(Row::Code {
                                file: i,
                                left: left.clone(),
                                right: right.clone(),
                            });
                            self.comment_rows(&mut rows, i, left.as_ref(), right.as_ref());
                        }
                    } else {
                        let (left, right) = if self.split {
                            match line.kind {
                                LineKind::Added => (None, Some(line.clone())),
                                _ => (Some(line.clone()), Some(line.clone())),
                            }
                        } else {
                            (None, Some(line.clone()))
                        };
                        rows.push(Row::Code {
                            file: i,
                            left: left.clone(),
                            right: right.clone(),
                        });
                        self.comment_rows(&mut rows, i, left.as_ref(), right.as_ref());
                        n += 1;
                    }
                }
            }
        }
        // Comments remain reviewable even after staging, filtering, or changing
        // scopes removes their original source line from the visible diff.
        for comment in &self.comments {
            if !rows
                .iter()
                .any(|row| matches!(row, Row::Comment(id) if *id == comment.id))
            {
                rows.push(Row::Comment(comment.id));
            }
        }
        self.rows = Arc::new(rows);
        self.scroll
            .reset_with_uniform_height(self.rows.len(), px(21.6));
        self.restore_scroll(anchor);
        cx.notify();
    }
    fn comment_rows(
        &self,
        rows: &mut Vec<Row>,
        file: usize,
        left: Option<&Line>,
        right: Option<&Line>,
    ) {
        let matches = |old: bool, end: u32| {
            if old {
                left.or(right).and_then(|l| l.old) == Some(end)
            } else {
                right.and_then(|l| l.new) == Some(end)
            }
        };
        for c in &self.comments {
            if c.path == self.snapshot.files[file].path && matches(c.old, c.end) {
                rows.push(Row::Comment(c.id));
            }
        }
        if self
            .draft
            .as_ref()
            .is_some_and(|d| d.file == file && matches(d.old, d.end))
        {
            rows.push(Row::Draft);
        }
    }
    fn toggle_file(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some(file) = self.snapshot.files.get(i) else {
            return;
        };
        let path = file.path.clone();
        if !self.collapsed.remove(&path) {
            self.collapsed.insert(path);
        }
        self.rebuild(cx);
    }
    fn toggle_all(&mut self, cx: &mut Context<Self>) {
        if self.collapsed.len() == self.snapshot.files.len() {
            self.collapsed.clear();
        } else {
            self.collapsed = self.snapshot.files.iter().map(|f| f.path.clone()).collect();
        }
        self.rebuild(cx);
    }
    fn jump_to(&mut self, i: usize, cx: &mut Context<Self>) {
        if i >= self.snapshot.files.len() {
            return;
        }
        self.selected_file = i;
        self.menu = None;
        self.focus_pending = true;
        self.collapsed.remove(&self.snapshot.files[i].path);
        self.rebuild(cx);
        if let Some(item_ix) = self
            .rows
            .iter()
            .position(|r| matches!(r,Row::Header(n) if *n==i))
        {
            self.scroll.scroll_to(ListOffset {
                item_ix,
                offset_in_item: px(0.),
            });
        }
        cx.notify();
    }
    fn toggle_menu(&mut self, m: Menu, cx: &mut Context<Self>) {
        self.menu = if self.menu.as_ref() == Some(&m) {
            None
        } else {
            Some(m)
        };
        self.menu_selected = 0;
        self.focus_pending = true;
        cx.notify();
    }

    fn mutate(&mut self, op: Mutation, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.menu = None;
        self.confirm = None;
        self.operation_error = None;
        let snapshot = self.snapshot.clone();
        let scope = self.scope.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    git_review::apply(&snapshot, &scope, &op).map_err(|e| format!("{e:#}"))
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.busy = false;
                match result {
                    Ok(message) => {
                        if s.pr_open && message.starts_with("https://") {
                            cx.open_url(&message);
                            s.show_notice("已创建 Pull Request".into(), cx);
                        } else {
                            s.show_notice(message, cx);
                        }
                        s.commit_open = false;
                        s.pr_open = false;
                        if s.push_after_commit {
                            s.push_after_commit = false;
                            s.refresh_then_push(cx);
                            return;
                        }
                    }
                    Err(e) => {
                        s.operation_error = Some(e);
                        s.push_after_commit = false;
                    }
                }
                s.refresh(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn refresh_then_push(&mut self, cx: &mut Context<Self>) {
        let cwd = self.cwd.clone();
        let scope = self.scope.clone();
        self.busy = true;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let s = git_review::load(&cwd, &scope, false, false)?;
                    git_review::apply(&s, &scope, &Mutation::Push)
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.busy = false;
                match result {
                    Ok(m) => s.show_notice(m, cx),
                    Err(e) => s.operation_error = Some(format!("提交已完成；推送失败：{e:#}")),
                }
                s.refresh(cx);
            });
        })
        .detach();
    }
    fn open_commit(&mut self, cx: &mut Context<Self>) {
        self.pr_open = false;
        self.push_after_commit = false;
        self.menu = None;
        self.commit_open = true;
        self.commit_all = true;
        self.new_branch = self.snapshot.detached;
        self.branch_input
            .update(cx, |i, cx| i.set_text_silently("codex/", cx));
        self.commit_input.update(cx, |i, cx| {
            i.set_accessible_name("提交信息");
            i.set_placeholder("提交信息（留空将自动生成）…", cx);
            i.set_text_silently("", cx);
        });
        self.focus_input = true;
        cx.notify();
    }
    fn open_pull_request(&mut self, cx: &mut Context<Self>) {
        self.open_commit(cx);
        self.pr_open = true;
        self.pr_existing = None;
        self.operation_error = None;
        self.pr_title
            .update(cx, |i, cx| i.set_text_silently("", cx));
        self.commit_input.update(cx, |i, cx| {
            i.set_accessible_name("PR 说明");
            i.set_placeholder("说明（留空将自动生成）", cx);
        });
        self.pr_base = if self
            .snapshot
            .branches
            .iter()
            .any(|b| b == "main" || b == "origin/main")
        {
            "main".into()
        } else if self
            .snapshot
            .branches
            .iter()
            .any(|b| b == "master" || b == "origin/master")
        {
            "master".into()
        } else {
            self.snapshot
                .upstream
                .clone()
                .unwrap_or_else(|| self.snapshot.branch.clone())
                .trim_start_matches("origin/")
                .into()
        };
        let root = self.snapshot.root.clone();
        cx.spawn(async move |this, cx| {
            let existing = cx
                .background_executor()
                .spawn(async move { git_review::existing_pull_request(&root) })
                .await;
            let _ = this.update(cx, |s, cx| {
                if s.pr_open && !s.new_branch {
                    s.pr_existing = existing;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
    fn display_error(&self) -> Option<String> {
        self.operation_error.clone().or_else(|| self.error.clone())
    }
    fn show_notice(&mut self, text: String, cx: &mut Context<Self>) {
        self.notice = Some(text);
        self.notice_serial += 1;
        let serial = self.notice_serial;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            let _ = this.update(cx, |s, cx| {
                if s.notice_serial == serial {
                    s.notice = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }
    #[cfg(feature = "screenshot")]
    pub fn capture_filter(&mut self, query: &str, cx: &mut Context<Self>) {
        self.query = query.into();
        self.filter
            .update(cx, |input, cx| input.set_text_silently(query, cx));
        self.rebuild(cx);
    }
    #[cfg(feature = "screenshot")]
    pub fn capture_ready(&self) -> Result<bool, String> {
        if let Some(error) = &self.error {
            Err(error.clone())
        } else {
            Ok(!self.loading && !self.snapshot.root.as_os_str().is_empty())
        }
    }
    fn copy_selection(&self, cx: &mut Context<Self>) {
        let Some(s) = self.selection else {
            return;
        };
        let a = s.anchor.min(s.head);
        let b = s.anchor.max(s.head);
        let text = self
            .rows
            .iter()
            .enumerate()
            .filter(|(i, _)| *i >= a.row && *i <= b.row)
            .filter_map(|(i, r)| {
                if let Row::Code { left, right, .. } = r {
                    let l = if s.old { left.as_ref() } else { right.as_ref() }?;
                    let start = if i == a.row {
                        a.byte.min(l.text.len())
                    } else {
                        0
                    };
                    let end = if i == b.row {
                        b.byte.min(l.text.len())
                    } else {
                        l.text.len()
                    };
                    l.text.get(start..end)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
    }
    fn key_down(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        let key = e.keystroke.key.as_str();
        let cmd = e.keystroke.modifiers.platform;
        if key == "escape" {
            self.dismiss_transient(w, cx);
            cx.stop_propagation();
            return;
        }
        if cmd && key == "enter" && self.commit_open {
            self.action(
                if self.pr_open {
                    controls::Action::CreatePullRequest(false)
                } else {
                    controls::Action::SaveCommit
                },
                w,
                cx,
            );
            cx.stop_propagation();
            return;
        }
        if cmd && key == "enter" && (self.draft.is_some() || self.editing_comment.is_some()) {
            self.save_comment(cx);
            cx.stop_propagation();
            return;
        }
        if cmd && key == "r" {
            self.refresh(cx);
            cx.stop_propagation();
            return;
        }
        if cmd && key == "f" {
            if self.compact() {
                self.toggle_menu(Menu::Jump, cx);
                self.jump.read(cx).focus_handle(cx).focus(w, cx);
                self.focus_pending = false;
                cx.stop_propagation();
                return;
            }
            self.tree_open = true;
            self.filter.read(cx).focus_handle(cx).focus(w, cx);
            cx.stop_propagation();
            return;
        }
        if self.commit_input.read(cx).focus_handle(cx).is_focused(w)
            || self.input.read(cx).focus_handle(cx).is_focused(w)
            || self.filter.read(cx).focus_handle(cx).is_focused(w)
            || self.branch_input.read(cx).focus_handle(cx).is_focused(w)
            || self.pr_title.read(cx).focus_handle(cx).is_focused(w)
        {
            return;
        }
        if let Some(menu) = self.menu.clone() {
            let count = self.menu_actions(&menu).len();
            match key {
                "home" => {
                    self.menu_selected = 0;
                }
                "end" => {
                    self.menu_selected = count.saturating_sub(1);
                }
                "down" => {
                    self.menu_selected = (self.menu_selected + 1) % count.max(1);
                }
                "up" => {
                    self.menu_selected = (self.menu_selected + count.max(1) - 1) % count.max(1);
                }
                "enter" | "space" => {
                    self.activate_menu(menu, self.menu_selected, w, cx);
                }
                _ => return,
            }
            cx.notify();
            cx.stop_propagation();
            return;
        }
        match key {
            "c" if cmd => self.copy_selection(cx),
            "a" if cmd => {
                if !self.rows.is_empty() {
                    self.selection = Some(Selection {
                        anchor: TextCursor { row: 0, byte: 0 },
                        head: TextCursor {
                            row: self.rows.len() - 1,
                            byte: usize::MAX,
                        },
                        old: false,
                    });
                }
            }
            "home" => self.scroll.scroll_to(ListOffset::default()),
            "end" => self.scroll.scroll_to_end(),
            "up" => self.scroll.scroll_by(px(-40.)),
            "down" => self.scroll.scroll_by(px(40.)),
            "pageup" => self
                .scroll
                .scroll_by(-self.scroll.viewport_bounds().size.height),
            "pagedown" => self
                .scroll
                .scroll_by(self.scroll.viewport_bounds().size.height),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
    pub fn dismiss_transient(&mut self, w: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.menu.take().is_none() && self.confirm.take().is_none() {
            if self.commit_open {
                self.commit_open = false;
                self.pr_open = false;
            } else if self.draft.take().is_some() || self.editing_comment.take().is_some() {
                self.rebuild(cx);
            } else {
                return false;
            }
        }
        self.focus.focus(w, cx);
        cx.notify();
        true
    }
}
