use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    time::Duration,
};

use gpui::{
    App, Context, Div, Entity, FocusHandle, Focusable, KeyDownEvent, MouseButton, ObjectFit,
    Render, Role, Window, div, prelude::*, px, uniform_list,
};

use super::{
    file_editor::{EditorEvent, FileEditor},
    file_io::{self, FileEntry, TextFile},
    icons::icon,
    prompt_input::{PromptChanged, PromptInput, PromptSubmitted},
};
use crate::theme::{Theme, ThemeMode};

#[derive(Clone, PartialEq, gpui::Action)]
#[action(no_json)]
pub struct OpenWorkspaceFile {
    pub path: String,
    pub line: Option<usize>,
}

struct Document {
    id: u64,
    path: PathBuf,
    editor: Option<Entity<FileEditor>>,
    saved: Option<TextFile>,
    error: Option<String>,
    loading: bool,
    saving: bool,
    revision: u64,
    image: bool,
    preview: bool,
}
impl Document {
    fn dirty(&self, cx: &App) -> bool {
        self.editor
            .as_ref()
            .zip(self.saved.as_ref())
            .is_some_and(|(e, s)| e.read(cx).buffer.text != s.text)
    }
}
#[derive(Clone)]
struct TreeRow {
    entry: FileEntry,
    depth: usize,
}
pub struct FilePanel {
    cwd: PathBuf,
    mode: ThemeMode,
    documents: Vec<Document>,
    active: Option<u64>,
    next_id: u64,
    tree_open: bool,
    expanded: HashSet<PathBuf>,
    directories: HashMap<PathBuf, Result<Vec<FileEntry>, String>>,
    loading: HashSet<PathBuf>,
    filter: Entity<PromptInput>,
    query: String,
    search_results: Vec<FileEntry>,
    searching: bool,
    open_search_when_ready: bool,
    search_generation: u64,
    focus: FocusHandle,
    focus_filter: bool,
    focus_editor: bool,
    selected_row: usize,
    tree_scroll: gpui::UniformListScrollHandle,
    pending_close: Option<u64>,
}
impl FilePanel {
    pub fn new(cwd: PathBuf, mode: ThemeMode, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| {
            let mut input = PromptInput::inline_other(mode, "筛选文件…", false, cx);
            input.set_accessible_name("筛选文件");
            input
        });
        cx.subscribe(&filter, |s, input, _: &PromptChanged, cx| {
            s.query = input.read(cx).text().into();
            s.search(cx);
        })
        .detach();
        cx.subscribe(&filter, |s, _, _: &PromptSubmitted, cx| {
            s.submit_filter(cx);
        })
        .detach();
        let mut s = Self {
            cwd: cwd.clone(),
            mode,
            documents: Vec::new(),
            active: None,
            next_id: 0,
            tree_open: true,
            expanded: HashSet::from([cwd.clone()]),
            directories: HashMap::new(),
            loading: HashSet::new(),
            filter,
            query: String::new(),
            search_results: Vec::new(),
            searching: false,
            open_search_when_ready: false,
            search_generation: 0,
            focus: cx.focus_handle(),
            focus_filter: false,
            focus_editor: false,
            selected_row: 0,
            tree_scroll: gpui::UniformListScrollHandle::new(),
            pending_close: None,
        };
        s.load_directory(cwd, cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                if this.update(cx, |s, cx| s.refresh_active(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        s
    }
    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.filter.update(cx, |v, cx| v.set_mode(mode, cx));
        for d in &self.documents {
            if let Some(e) = &d.editor {
                e.update(cx, |e, cx| e.set_mode(mode, cx));
            }
        }
        cx.notify();
    }
    pub fn focus(&mut self, cx: &mut Context<Self>) {
        if self.active.is_some() {
            self.focus_editor = true;
        } else {
            self.focus_filter = true;
        }
        cx.notify();
    }
    pub fn show_picker(&mut self, cx: &mut Context<Self>) {
        self.focus_editor = false;
        self.tree_open = true;
        self.focus_filter = true;
        self.load_directory(self.cwd.clone(), cx);
        cx.notify();
    }
    pub fn has_unsaved(&self, cx: &App) -> bool {
        self.documents.iter().any(|d| d.dirty(cx) || d.saving)
    }
    pub fn save_all(&mut self, cx: &mut Context<Self>) {
        for id in self.documents.iter().map(|d| d.id).collect::<Vec<_>>() {
            self.save(id, cx);
        }
    }
    fn current(&self) -> Option<&Document> {
        self.documents.iter().find(|d| Some(d.id) == self.active)
    }
    fn load_directory(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !self.loading.insert(path.clone()) {
            return;
        }
        let task = cx.background_executor().spawn({
            let p = path.clone();
            async move { file_io::read_directory(&p) }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |s, cx| {
                s.loading.remove(&path);
                s.directories.insert(path, result);
                cx.notify();
            });
        })
        .detach();
    }
    fn submit_filter(&mut self, cx: &mut Context<Self>) {
        if self.searching {
            self.open_search_when_ready = true;
        } else {
            self.activate_row(self.selected_row, cx);
        }
    }

    fn search(&mut self, cx: &mut Context<Self>) {
        self.search_generation += 1;
        self.open_search_when_ready = false;
        self.search_results.clear();
        self.selected_row = 0;
        self.tree_scroll
            .scroll_to_item(0, gpui::ScrollStrategy::Top);
        let generation = self.search_generation;
        if self.query.is_empty() {
            self.searching = false;
            cx.notify();
            return;
        }
        self.searching = true;
        let root = self.cwd.clone();
        let query = self.query.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(180))
                .await;
            if this
                .read_with(cx, |s, _| s.search_generation != generation)
                .unwrap_or(true)
            {
                return;
            }
            let result = cx
                .background_executor()
                .spawn(async move { file_io::search_files(&root, &query) })
                .await;
            let _ = this.update(cx, |s, cx| {
                if s.search_generation == generation {
                    s.searching = false;
                    s.search_results = result.unwrap_or_default();
                    if std::mem::take(&mut s.open_search_when_ready) {
                        s.activate_row(s.selected_row, cx);
                    }
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
    fn rows(&self) -> Vec<TreeRow> {
        if !self.query.is_empty() {
            return self
                .search_results
                .iter()
                .cloned()
                .map(|entry| TreeRow { entry, depth: 0 })
                .collect();
        }
        fn walk(s: &FilePanel, path: &PathBuf, depth: usize, result: &mut Vec<TreeRow>) {
            if depth > 64 {
                return;
            }
            if let Some(Ok(entries)) = s.directories.get(path) {
                for entry in entries {
                    result.push(TreeRow {
                        entry: entry.clone(),
                        depth,
                    });
                    if entry.directory && s.expanded.contains(&entry.path) {
                        walk(s, &entry.path, depth + 1, result);
                    }
                }
            }
        }
        let mut rows = Vec::new();
        walk(self, &self.cwd, 0, &mut rows);
        rows
    }
    fn activate_row(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(row) = self.rows().get(index).cloned() else {
            return;
        };
        self.selected_row = index;
        if row.entry.directory {
            if !self.expanded.remove(&row.entry.path) {
                self.expanded.insert(row.entry.path.clone());
                self.load_directory(row.entry.path, cx);
            }
        } else {
            self.open_path(row.entry.path, None, cx);
        }
        cx.notify();
    }
    pub fn open_path(&mut self, path: PathBuf, line: Option<usize>, cx: &mut Context<Self>) {
        let path = if path.is_absolute() {
            path
        } else {
            self.cwd.join(path)
        };
        if let Some(doc) = self.documents.iter().find(|d| d.path == path) {
            self.active = Some(doc.id);
            if let Some(line) = line
                && let Some(e) = &doc.editor
            {
                e.update(cx, |e, cx| e.go_to_line(line, cx));
            }
            self.focus_editor = true;
            cx.notify();
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.active = Some(id);
        let image = matches!(
            path.extension()
                .and_then(|s| s.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp")
        );
        self.documents.push(Document {
            id,
            path: path.clone(),
            editor: None,
            saved: None,
            error: None,
            loading: !image,
            saving: false,
            revision: 0,
            image,
            preview: matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("md" | "markdown")
            ),
        });
        if image {
            cx.notify();
            return;
        }
        let task = cx
            .background_executor()
            .spawn(async move { TextFile::read(&path) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |s, cx| {
                let Some(index) = s.documents.iter().position(|d| d.id == id) else {
                    return;
                };
                s.documents[index].loading = false;
                match result {
                    Ok(file) => {
                        let language = file
                            .path
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(str::to_string);
                        let editor =
                            cx.new(|cx| FileEditor::new(file.text.clone(), language, s.mode, cx));
                        cx.subscribe(&editor, move |s, _, e: &EditorEvent, cx| {
                            if let Some(d) = s.documents.iter_mut().find(|d| d.id == id) {
                                d.revision += 1;
                            }
                            match e {
                                EditorEvent::Changed => s.schedule_save(id, cx),
                                EditorEvent::Save => s.save(id, cx),
                            }
                            cx.notify();
                        })
                        .detach();
                        if let Some(line) = line {
                            editor.update(cx, |e, cx| e.go_to_line(line, cx));
                        }
                        s.documents[index].saved = Some(file);
                        s.documents[index].editor = Some(editor);
                        s.focus_editor = true;
                    }
                    Err(error) => s.documents[index].error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn schedule_save(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(d) = self.documents.iter().find(|d| d.id == id) else {
            return;
        };
        let revision = d.revision;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
            let _ = this.update(cx, |s, cx| {
                if s.documents
                    .iter()
                    .any(|d| d.id == id && d.revision == revision && d.error.is_none())
                {
                    s.save(id, cx);
                }
            });
        })
        .detach();
    }
    fn save(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(d) = self.documents.iter_mut().find(|d| d.id == id) else {
            return;
        };
        if d.saving || !d.dirty(cx) {
            return;
        }
        let Some(editor) = &d.editor else {
            return;
        };
        if editor.read(cx).composing() {
            return;
        }
        let text = editor.read(cx).buffer.text.clone();
        let saved = d.saved.clone().unwrap();
        d.saving = true;
        let task = cx
            .background_executor()
            .spawn(async move { saved.save(&text) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |s, cx| {
                let Some(d) = s.documents.iter_mut().find(|d| d.id == id) else {
                    return;
                };
                d.saving = false;
                match result {
                    Ok(file) => {
                        d.saved = Some(file);
                        d.error = None;
                        if d.dirty(cx) {
                            s.schedule_save(id, cx);
                        } else if s.pending_close == Some(id) {
                            s.remove_document(id, cx);
                        }
                    }
                    Err(e) => d.error = Some(e),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn refresh_active(&mut self, cx: &mut Context<Self>) {
        let Some(d) = self.current() else {
            return;
        };
        if d.loading
            || d.saving
            || d.dirty(cx)
            || d.error.is_some()
            || d.editor.as_ref().is_some_and(|e| e.read(cx).composing())
        {
            return;
        }
        let Some(saved) = &d.saved else {
            return;
        };
        let path = saved.path.clone();
        let id = d.id;
        let revision = d.revision;
        let task = cx
            .background_executor()
            .spawn(async move { TextFile::read(&path) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |s, cx| {
                let Some(d) = s.documents.iter_mut().find(|d| d.id == id) else {
                    return;
                };
                if d.revision != revision || d.saving || d.dirty(cx) {
                    return;
                }
                match result {
                    Ok(file) => {
                        if d.saved.as_ref().is_some_and(|old| old.bytes != file.bytes) {
                            if let Some(e) = &d.editor {
                                e.update(cx, |e, cx| e.reload(file.text.clone(), cx));
                            }
                            d.saved = Some(file);
                            cx.notify();
                        }
                    }
                    Err(e) => {
                        d.error = Some(e);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
    fn reload(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.documents.iter().find(|d| d.id == id) else {
            return;
        };
        if d.saving {
            return;
        }
        let path = d.path.clone();
        let dirty = d.dirty(cx);
        let answer = dirty.then(|| {
            window.prompt(
                gpui::PromptLevel::Warning,
                "重新加载文件？",
                Some("当前未保存的编辑将被磁盘内容替换。"),
                &["取消", "重新加载"],
                cx,
            )
        });
        cx.spawn(async move |this, cx| {
            if let Some(answer) = answer
                && answer.await.ok() != Some(1)
            {
                return;
            }
            let _ = this.update(cx, |s, cx| {
                s.remove_document(id, cx);
                s.open_path(path, None, cx);
            });
        })
        .detach();
    }
    fn request_close(&mut self, id: u64, cx: &mut Context<Self>) {
        if self
            .documents
            .iter()
            .find(|d| d.id == id)
            .is_some_and(|d| d.dirty(cx) || d.saving)
        {
            self.pending_close = Some(id);
            self.save(id, cx);
        } else {
            self.remove_document(id, cx);
        }
        cx.notify();
    }
    fn remove_document(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(index) = self.documents.iter().position(|d| d.id == id) else {
            return;
        };
        self.documents.remove(index);
        if self.active == Some(id) {
            self.active = self
                .documents
                .get(index.min(self.documents.len().saturating_sub(1)))
                .map(|d| d.id);
        }
        if self.pending_close == Some(id) {
            self.pending_close = None;
        }
        cx.notify();
    }
    fn tree_key(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        let count = self.rows().len();
        if count == 0 {
            return;
        }
        match e.keystroke.key.as_str() {
            "down" => {
                self.selected_row = if self.focus.is_focused(w) {
                    (self.selected_row + 1).min(count - 1)
                } else {
                    0
                }
            }
            "up" => {
                self.selected_row = if self.focus.is_focused(w) {
                    self.selected_row.saturating_sub(1)
                } else {
                    count - 1
                }
            }
            "home" => self.selected_row = 0,
            "end" => self.selected_row = count - 1,
            "enter" | "space" => self.activate_row(self.selected_row, cx),
            "right" => {
                let row = self.rows()[self.selected_row.min(count - 1)].clone();
                if row.entry.directory && !self.expanded.contains(&row.entry.path) {
                    self.activate_row(self.selected_row, cx);
                }
            }
            "left" => {
                let row = self.rows()[self.selected_row.min(count - 1)].clone();
                if !self.expanded.remove(&row.entry.path)
                    && let Some(parent) = row.entry.path.parent()
                {
                    self.selected_row = self
                        .rows()
                        .iter()
                        .position(|r| r.entry.path == parent)
                        .unwrap_or(self.selected_row);
                }
            }
            "escape" => {
                self.query.clear();
                self.filter.update(cx, |v, cx| v.set_text_silently("", cx));
                self.search(cx);
            }
            _ => return,
        };
        self.focus.focus(w, cx);
        self.tree_scroll
            .scroll_to_item(self.selected_row, gpui::ScrollStrategy::Top);
        cx.stop_propagation();
        cx.notify();
    }
    fn control(
        &self,
        id: impl Into<gpui::ElementId>,
        label: &str,
        glyph: &'static str,
        theme: Theme,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .size(px(28.))
            .flex_none()
            .rounded(px(8.))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .role(Role::Button)
            .aria_label(label.to_string())
            .tab_stop(true)
            .hover(move |s| s.bg(theme.sidebar_hover))
            .focus_visible(move |s| s.border_1().border_color(theme.accent))
            .child(icon(glyph, theme.text_tertiary.into()).size(px(16.)))
    }
}
impl Render for FilePanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_mode(self.mode);
        if self.focus_filter {
            self.filter.read(cx).focus_handle(cx).focus(window, cx);
            self.focus_filter = false;
        }
        if self.focus_editor {
            if let Some(e) = self.current().and_then(|d| d.editor.as_ref()) {
                e.read(cx).focus_handle(cx).focus(window, cx);
            }
            self.focus_editor = false;
        }
        let tabs = div()
            .id("file-tabs")
            .h(px(46.))
            .pr(px(110.))
            .pl(px(8.))
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(3.))
            .overflow_x_scroll()
            .children(self.documents.iter().map(|d| {
                let id = d.id;
                let active = self.active == Some(id);
                div()
                    .id(("file-tab", id))
                    .min_w(px(90.))
                    .max_w(px(156.))
                    .h(px(28.))
                    .px(px(8.))
                    .rounded(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .when(active, |s| s.bg(theme.text.alpha(0.05)))
                    .hover(move |s| s.bg(theme.sidebar_hover))
                    .role(Role::Tab)
                    .aria_selected(active)
                    .aria_label(format!(
                        "文件 {}",
                        d.path.file_name().unwrap_or_default().to_string_lossy()
                    ))
                    .tab_stop(true)
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.active = Some(id);
                        s.focus_editor = true;
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(move |s, e: &KeyDownEvent, _, cx| {
                        if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                            s.active = Some(id);
                            s.focus_editor = true;
                            cx.notify();
                            cx.stop_propagation();
                        }
                    }))
                    .child(
                        icon(file_icon(&d.path), theme.text_tertiary.into())
                            .size(px(16.))
                            .flex_none(),
                    )
                    .child(
                        div().flex_1().min_w(px(0.)).truncate().child(
                            d.path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned(),
                        ),
                    )
                    .child(
                        self.control(
                            ("close-file", id),
                            &format!(
                                "关闭 {}",
                                d.path.file_name().unwrap_or_default().to_string_lossy()
                            ),
                            "close-dialog",
                            theme,
                        )
                        .size(px(20.))
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.request_close(id, cx);
                            cx.stop_propagation();
                        })),
                    )
            }))
            .when(self.documents.is_empty(), |t| {
                t.child(
                    div()
                        .px(px(8.))
                        .h(px(28.))
                        .rounded(px(8.))
                        .bg(theme.text.alpha(0.05))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(icon("markdown-file-document", theme.text.into()).size(px(16.)))
                        .child("打开文件"),
                )
            })
            .child(
                self.control("add-file", "打开文件", "add", theme)
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.active = None;
                        s.show_picker(cx);
                    })),
            );
        let mut toolbar = div()
            .h(px(40.))
            .flex_none()
            .px(px(12.))
            .border_b_1()
            .border_color(theme.border)
            .flex()
            .items_center()
            .gap(px(6.));
        if let Some(d) = self.current() {
            let id = d.id;
            let relative = d
                .path
                .strip_prefix(&self.cwd)
                .unwrap_or(&d.path)
                .to_string_lossy()
                .into_owned();
            toolbar = toolbar.child(div().flex_1().min_w(px(0.)).truncate().child(format!(
                "{}  /  {}",
                self.cwd.file_name().unwrap_or_default().to_string_lossy(),
                relative
            )));
            if d.editor.is_some() {
                toolbar = toolbar.child(
                    div()
                        .text_size(px(11.))
                        .text_color(if d.error.is_some() {
                            theme.warning
                        } else {
                            theme.text_tertiary
                        })
                        .child(if d.saving {
                            "保存中…"
                        } else if d.dirty(cx) {
                            "未保存"
                        } else {
                            ""
                        }),
                );
            }
            if matches!(
                d.path.extension().and_then(|s| s.to_str()),
                Some("md" | "markdown")
            ) && d.editor.is_some()
            {
                let preview = d.preview;
                toolbar = toolbar.child(
                    self.control(
                        "file-preview",
                        if preview { "查看源代码" } else { "预览" },
                        if preview {
                            "panel-terminal"
                        } else {
                            "markdown-file-document"
                        },
                        theme,
                    )
                    .w(px(92.))
                    .gap(px(4.))
                    .child(if preview { "查看源代码" } else { "预览" })
                    .on_click(cx.listener(move |s, _, _, cx| {
                        if let Some(d) = s.documents.iter_mut().find(|d| d.id == id) {
                            d.preview = !d.preview;
                            s.focus_editor = !d.preview;
                        }
                        cx.notify();
                    })),
                );
            }
            toolbar = toolbar.child(
                self.control("reload-file", "重新加载", "settings-refresh", theme)
                    .on_click(cx.listener(move |s, _, w, cx| s.reload(id, w, cx))),
            );
        } else {
            toolbar = toolbar.child(div().flex_1().child("/"));
        }
        toolbar = toolbar.child(
            self.control("toggle-file-tree", "切换文件树", "panel-files", theme)
                .when(self.tree_open, |s| s.bg(theme.text.alpha(0.05)))
                .on_click(cx.listener(|s, _, _, cx| {
                    s.tree_open = !s.tree_open;
                    cx.notify();
                })),
        );
        let mut content = div()
            .id("file-content")
            .min_w(px(0.))
            .flex_1()
            .h_full()
            .relative()
            .flex()
            .flex_col();
        if let Some(d) = self.current() {
            let id = d.id;
            if let Some(error) = &d.error
                && d.editor.is_some()
            {
                content = content.child(
                    div()
                        .p(px(12.))
                        .text_size(px(12.))
                        .text_color(theme.warning)
                        .child(error.clone())
                        .child(
                            div()
                                .flex()
                                .gap(px(8.))
                                .child(
                                    self.control(
                                        "retry-file-save",
                                        "重试保存",
                                        "settings-refresh",
                                        theme,
                                    )
                                    .on_click(cx.listener(move |s, _, _, cx| s.save(id, cx))),
                                )
                                .child(
                                    self.control(
                                        "copy-file-content",
                                        "复制当前内容",
                                        "message-copy",
                                        theme,
                                    )
                                    .on_click(cx.listener(
                                        move |s, _, _, cx| {
                                            if let Some(d) = s.documents.iter().find(|d| d.id == id)
                                                && let Some(e) = &d.editor
                                            {
                                                cx.write_to_clipboard(
                                                    gpui::ClipboardItem::new_string(
                                                        e.read(cx).buffer.text.clone(),
                                                    ),
                                                );
                                            }
                                        },
                                    )),
                                ),
                        ),
                );
            }
            if d.loading {
                content = content.child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(theme.text_tertiary)
                        .child("正在读取文件…"),
                );
            } else if d.image {
                content = content.child(
                    div().flex_1().min_h(px(0.)).p(px(16.)).child(
                        gpui::img(d.path.clone())
                            .size_full()
                            .object_fit(ObjectFit::Contain),
                    ),
                );
            } else if let Some(editor) = &d.editor {
                if d.preview {
                    content = content.child(
                        div()
                            .id(("file-markdown", id))
                            .flex_1()
                            .min_h(px(0.))
                            .overflow_y_scroll()
                            .p(px(24.))
                            .child(super::markdown::render_assistant_markdown(
                                &editor.read(cx).buffer.text,
                                theme,
                                &format!("file-{id}"),
                            )),
                    );
                } else {
                    content = content.child(div().flex_1().min_h(px(0.)).child(editor.clone()));
                }
                if editor.read(cx).can_undo() || editor.read(cx).can_redo() {
                    content = content.child(
                        div()
                            .absolute()
                            .bottom(px(20.))
                            .right(px(20.))
                            .h(px(36.))
                            .p(px(4.))
                            .rounded(px(12.))
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.surface)
                            .flex()
                            .child(
                                self.control("file-undo", "撤销", "back", theme)
                                    .opacity(if editor.read(cx).can_undo() { 1. } else { 0.4 })
                                    .on_click(cx.listener(move |s, _, _, cx| {
                                        if let Some(e) = s
                                            .documents
                                            .iter()
                                            .find(|d| d.id == id)
                                            .and_then(|d| d.editor.as_ref())
                                        {
                                            e.update(cx, |e, cx| e.undo(cx));
                                        }
                                    })),
                            )
                            .child(
                                self.control("file-redo", "重做", "forward", theme)
                                    .opacity(if editor.read(cx).can_redo() { 1. } else { 0.4 })
                                    .on_click(cx.listener(move |s, _, _, cx| {
                                        if let Some(e) = s
                                            .documents
                                            .iter()
                                            .find(|d| d.id == id)
                                            .and_then(|d| d.editor.as_ref())
                                        {
                                            e.update(cx, |e, cx| e.redo(cx));
                                        }
                                    })),
                            ),
                    );
                }
            } else if let Some(error) = &d.error {
                let path = d.path.clone();
                content = content.child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(12.))
                        .p(px(24.))
                        .text_color(theme.text_tertiary)
                        .child(
                            icon("markdown-file-document", theme.text_tertiary.into())
                                .size(px(32.)),
                        )
                        .child(error.clone())
                        .child(
                            self.control(
                                "open-file-external",
                                "在默认应用中打开",
                                "settings-external",
                                theme,
                            )
                            .on_click(move |_, _, cx| {
                                cx.open_with_system(&path);
                            }),
                        ),
                );
            }
        } else {
            content = content.child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(12.))
                    .child(icon("panel-files", theme.text_secondary.into()).size(px(32.)))
                    .child(div().text_size(px(16.)).child("打开文件"))
                    .child(
                        div()
                            .text_color(theme.text_secondary)
                            .child("从工作区目录树中选择文件"),
                    ),
            );
        }
        let rows = self.rows();
        let count = rows.len();
        let weak = cx.entity().downgrade();
        let tree = div()
            .id("workspace-file-tree")
            .on_key_down(cx.listener(Self::tree_key))
            .w(px(250.))
            .max_w(gpui::relative(0.48))
            .min_w(px(140.))
            .h_full()
            .flex_none()
            .border_l_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .child(
                div().px(px(8.)).pt(px(8.)).pb(px(1.)).child(
                    div()
                        .h(px(28.))
                        .border_1()
                        .border_color(theme.border)
                        .rounded(px(8.))
                        .flex()
                        .items_center()
                        .child(
                            icon("search", theme.text_tertiary.into())
                                .size(px(16.))
                                .ml(px(8.)),
                        )
                        .child(div().flex_1().min_w(px(0.)).child(self.filter.clone()))
                        .when(!self.query.is_empty(), |bar| {
                            bar.child(
                                self.control(
                                    "clear-file-filter",
                                    "清除文件筛选",
                                    "close-dialog",
                                    theme,
                                )
                                .size(px(22.))
                                .on_click(cx.listener(
                                    |s, _, _, cx| {
                                        s.query.clear();
                                        s.filter.update(cx, |input, cx| {
                                            input.set_text_silently("", cx)
                                        });
                                        s.search(cx);
                                        s.focus_filter = true;
                                    },
                                )),
                            )
                        }),
                ),
            )
            .child(
                div()
                    .id("file-tree-navigation")
                    .role(Role::Tree)
                    .aria_label("工作区目录树")
                    .flex_1()
                    .min_h(px(0.))
                    .px(px(8.))
                    .track_focus(&self.focus)
                    .when(count == 0, |t| {
                        t.child(div().p(px(12.)).text_color(theme.text_tertiary).child(
                            if self.searching || !self.loading.is_empty() {
                                "正在加载…".to_string()
                            } else if let Some(Err(e)) = self.directories.get(&self.cwd) {
                                format!("无法读取目录：{e}")
                            } else if !self.query.is_empty() {
                                "没有匹配的文件".into()
                            } else {
                                "空目录".into()
                            },
                        ))
                    })
                    .when(count > 0, |t| {
                        t.child(
                            uniform_list("file-tree-rows", count, move |range, window, cx| {
                                range
                                    .map(|index| {
                                        let row = rows[index].clone();
                                        let Some(panel) = weak.upgrade() else {
                                            return div().into_any_element();
                                        };
                                        panel.update(cx, |s, cx| {
                                            s.tree_row(
                                                row,
                                                index,
                                                theme,
                                                s.focus.is_focused(window),
                                                cx,
                                            )
                                            .into_any_element()
                                        })
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .track_scroll(&self.tree_scroll)
                            .size_full(),
                        )
                    }),
            )
            .when(!self.query.is_empty(), |t| {
                t.child(
                    div()
                        .px(px(12.))
                        .py(px(4.))
                        .text_size(px(11.))
                        .text_color(theme.text_tertiary)
                        .child(if count == 500 {
                            "显示前 500 个结果".into()
                        } else {
                            format!("{count} 个文件")
                        }),
                )
            });
        let mut panel = div()
            .id("files-panel")
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.surface)
            .text_color(theme.text)
            .text_size(px(13.))
            .line_height(px(18.))
            .child(tabs)
            .child(toolbar)
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .child(content)
                    .when(self.tree_open, |b| b.child(tree)),
            );
        if let Some(id) = self.pending_close
            && let Some(d) = self.documents.iter().find(|d| d.id == id)
            && d.error.is_some()
            && !d.saving
        {
            panel = panel.child(
                div()
                    .p(px(12.))
                    .border_t_1()
                    .border_color(theme.border)
                    .child("未能保存文件，保留编辑或放弃更改？")
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(
                                self.control(
                                    "cancel-file-close",
                                    "保留编辑",
                                    "close-dialog",
                                    theme,
                                )
                                .on_click(cx.listener(
                                    |s, _, _, cx| {
                                        s.pending_close = None;
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(
                                self.control(
                                    "discard-file-changes",
                                    "放弃更改并关闭",
                                    "close-dialog",
                                    theme,
                                )
                                .on_click(
                                    cx.listener(move |s, _, _, cx| s.remove_document(id, cx)),
                                ),
                            ),
                    ),
            );
        }
        panel
    }
}
impl FilePanel {
    fn tree_row(
        &self,
        row: TreeRow,
        index: usize,
        theme: Theme,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let expanded = self.expanded.contains(&row.entry.path);
        let selected = self.current().is_some_and(|d| d.path == row.entry.path);
        let label = if self.query.is_empty() {
            row.entry
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        } else {
            row.entry
                .path
                .strip_prefix(&self.cwd)
                .unwrap_or(&row.entry.path)
                .to_string_lossy()
                .into_owned()
        };
        div()
            .id(("file-row", index))
            .h(px(28.))
            .w_full()
            .pl(px(6. + row.depth as f32 * 14.))
            .pr(px(6.))
            .rounded(px(6.))
            .flex()
            .items_center()
            .gap(px(10.))
            .cursor_pointer()
            .role(Role::TreeItem)
            .aria_selected(selected)
            .when(row.entry.directory, |s| s.aria_expanded(expanded))
            .when(focused && self.selected_row == index, |s| {
                s.aria_active_descendant()
                    .bg(theme.sidebar_hover)
                    .border_1()
                    .border_color(theme.accent)
            })
            .aria_label(format!(
                "{}{}",
                label,
                if row.entry.directory {
                    " 文件夹"
                } else {
                    ""
                }
            ))
            .when(selected, |s| s.bg(theme.text.alpha(0.05)))
            .hover(move |s| s.bg(theme.sidebar_hover))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |s, _, w, cx| {
                    s.focus.focus(w, cx);
                    s.selected_row = index;
                    cx.notify();
                }),
            )
            .on_click(cx.listener(move |s, _, _, cx| s.activate_row(index, cx)))
            .child(
                icon(
                    if row.entry.directory {
                        if expanded {
                            "chevron-down"
                        } else {
                            "settings-chevron-right"
                        }
                    } else {
                        file_icon(&row.entry.path)
                    },
                    theme.text_tertiary.into(),
                )
                .size(px(16.))
                .flex_none(),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .truncate()
                    .font_weight(gpui::FontWeight::NORMAL)
                    .child(label),
            )
    }
}
fn file_icon(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => "markdown-file-rust",
        Some("py") => "markdown-file-python",
        Some("json") => "markdown-file-json",
        _ => "markdown-file-document",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Bounds, point, size};
    #[test]
    fn automatic_saves_undo_and_conflicts_preserve_the_correct_contents() {
        let root = std::env::temp_dir().join(format!("gpui-panel-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("example.rs");
        std::fs::write(&path, "original\n").unwrap();
        let mut app = gpui::TestApp::new();
        app.update(super::super::file_editor::init);
        let mut window = app.open_window_with_options(
            gpui::WindowOptions {
                window_bounds: Some(gpui::WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(800.), px(500.)),
                ))),
                ..Default::default()
            },
            |_, cx| FilePanel::new(root.clone(), ThemeMode::Light, cx),
        );
        window.update(|p, _, cx| p.open_path(path.clone(), None, cx));
        app.run_until_parked();
        window.draw();
        assert!(window.read(|p, _| p.current().unwrap().editor.is_some()));
        window.update(|p, _, cx| {
            p.focus(cx);
            p.show_picker(cx);
        });
        window.draw();
        window.update(|p, w, cx| {
            assert!(
                p.filter.read(cx).focus_handle(cx).is_focused(w),
                "opening the picker must take focus from the editor"
            );
            p.focus_filter = false;
            p.focus_editor = true;
            cx.notify();
        });
        window.draw();
        window.update(|p, _, cx| {
            p.active = None;
            p.query = "example.rs".into();
            p.search(cx);
            p.submit_filter(cx);
        });
        app.advance_clock(Duration::from_millis(200));
        app.run_until_parked();
        window.draw();
        assert!(
            window.read(|p, _| p.current().is_some()),
            "Enter during search waits for the matching file"
        );
        window.simulate_keystroke("cmd-a");
        app.write_to_clipboard(gpui::ClipboardItem::new_string("edited 中文👋\n".into()));
        window.simulate_keystroke("cmd-v");
        app.advance_clock(Duration::from_millis(450));
        app.run_until_parked();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "edited 中文👋\n");
        window.simulate_keystroke("cmd-z");
        app.advance_clock(Duration::from_millis(450));
        app.run_until_parked();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n");
        window.simulate_keystroke("cmd-a");
        window.simulate_input("my pending edit");
        std::fs::write(&path, "external edit").unwrap();
        app.advance_clock(Duration::from_millis(450));
        app.run_until_parked();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "external edit");
        window.read(|p, cx| {
            let d = p.current().unwrap();
            assert!(d.error.as_ref().unwrap().contains("其他程序"));
            assert_eq!(
                d.editor.as_ref().unwrap().read(cx).buffer.text,
                "my pending edit"
            );
        });
        let active = window.read(|p, _| p.active.unwrap());
        window.update(|p, _, cx| p.request_close(active, cx));
        app.run_until_parked();
        window.read(|p, _| {
            assert_eq!(p.pending_close, Some(active));
            assert_eq!(p.documents.len(), 1);
        });
        std::fs::remove_dir_all(root).unwrap();
    }
}
