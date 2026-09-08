use std::{
    ops::Range,
    time::{Duration, Instant},
};

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementInputHandler, EntityInputHandler,
    FocusHandle, Focusable, KeyDownEvent, MouseButton, Pixels, Point, ShapedLine, TextRun,
    UTF16Selection, Window, canvas, div, fill, point, prelude::*, px, size,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::theme::{Theme, ThemeMode, UI_MONOSPACE_FONT_FAMILY, ui_font};

const FONT_SIZE: f32 = 12.;
const LINE_HEIGHT: f32 = 22.;
const GUTTER: f32 = 46.;
gpui::actions!(
    file_editor,
    [
        EditorCopy,
        EditorCut,
        EditorPaste,
        EditorSelectAll,
        EditorUndo,
        EditorRedo,
        EditorSave,
        EditorBackspace,
        EditorDelete,
        EditorEnter,
        EditorTab
    ]
);
pub fn init(cx: &mut App) {
    cx.bind_keys([
        gpui::KeyBinding::new("cmd-c", EditorCopy, Some("FileEditor")),
        gpui::KeyBinding::new("cmd-x", EditorCut, Some("FileEditor")),
        gpui::KeyBinding::new("cmd-v", EditorPaste, Some("FileEditor")),
        gpui::KeyBinding::new("cmd-a", EditorSelectAll, Some("FileEditor")),
        gpui::KeyBinding::new("cmd-z", EditorUndo, Some("FileEditor")),
        gpui::KeyBinding::new("cmd-shift-z", EditorRedo, Some("FileEditor")),
        gpui::KeyBinding::new("cmd-s", EditorSave, Some("FileEditor")),
        gpui::KeyBinding::new("backspace", EditorBackspace, Some("FileEditor")),
        gpui::KeyBinding::new("delete", EditorDelete, Some("FileEditor")),
        gpui::KeyBinding::new("enter", EditorEnter, Some("FileEditor")),
        gpui::KeyBinding::new("tab", EditorTab, Some("FileEditor")),
    ]);
}
#[derive(Clone)]
struct Snapshot {
    text: String,
    anchor: usize,
    cursor: usize,
}
#[derive(Default)]
pub struct Buffer {
    pub text: String,
    anchor: usize,
    cursor: usize,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}
impl Buffer {
    fn selection(&self) -> Range<usize> {
        self.anchor.min(self.cursor)..self.anchor.max(self.cursor)
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            anchor: self.anchor,
            cursor: self.cursor,
        }
    }
    fn restore(&mut self, s: Snapshot) {
        self.text = s.text;
        self.anchor = s.anchor;
        self.cursor = s.cursor;
    }
    fn replace(&mut self, range: Range<usize>, text: &str, checkpoint: bool) {
        if checkpoint {
            self.undo.push(self.snapshot());
            self.redo.clear();
            while self.undo.len() > 100
                || (self.undo.len() > 1
                    && self.undo.iter().map(|s| s.text.len()).sum::<usize>() > 16 * 1024 * 1024)
            {
                self.undo.remove(0);
            }
        }
        self.text.replace_range(range.clone(), text);
        self.cursor = range.start + text.len();
        self.anchor = self.cursor;
    }
    fn undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            self.redo.push(self.snapshot());
            self.restore(s);
        }
    }
    fn redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            self.undo.push(self.snapshot());
            self.restore(s);
        }
    }
    fn byte_offset_from_utf16(&self, offset: usize) -> usize {
        from_utf16(&self.text, offset)
    }
    fn to_utf16(&self, offset: usize) -> usize {
        self.text[..offset].encode_utf16().count()
    }
}
fn from_utf16(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (i, c) in text.char_indices() {
        if units + c.len_utf16() > offset {
            return i;
        }
        units += c.len_utf16();
    }
    text.len()
}
fn previous(text: &str, offset: usize) -> usize {
    text[..offset]
        .grapheme_indices(true)
        .next_back()
        .map_or(0, |(i, _)| i)
}
fn next(text: &str, offset: usize) -> usize {
    text[offset..]
        .graphemes(true)
        .next()
        .map_or(offset, |g| offset + g.len())
}

pub enum EditorEvent {
    Changed,
    Save,
}
pub struct FileEditor {
    pub buffer: Buffer,
    pub mode: ThemeMode,
    language: Option<String>,
    prose_label: Option<String>,
    placeholder: String,
    focus: FocusHandle,
    marked: Option<Range<usize>>,
    rows: Vec<Row>,
    layout_width: f32,
    layout_dirty: bool,
    scroll: f32,
    bounds: Option<Bounds<Pixels>>,
    selecting: bool,
    ensure_cursor: bool,
    desired_x: Option<f32>,
    last_input: Option<(Instant, usize)>,
    input_error: Option<&'static str>,
}
struct Row {
    line: ShapedLine,
    map: Vec<(usize, usize)>,
    number: Option<usize>,
}
impl Row {
    fn start(&self) -> usize {
        self.map.first().unwrap().1
    }
    fn end(&self) -> usize {
        self.map.last().unwrap().1
    }
    fn source(&self, display: usize) -> usize {
        self.map
            .iter()
            .min_by_key(|(d, _)| d.abs_diff(display))
            .unwrap()
            .1
    }
    fn display(&self, source: usize) -> usize {
        self.map
            .iter()
            .min_by_key(|(_, s)| s.abs_diff(source))
            .unwrap()
            .0
    }
}
impl gpui::EventEmitter<EditorEvent> for FileEditor {}
impl Focusable for FileEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl FileEditor {
    pub fn new(
        text: String,
        language: Option<String>,
        mode: ThemeMode,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            buffer: Buffer {
                text,
                ..Default::default()
            },
            language,
            prose_label: None,
            placeholder: String::new(),
            mode,
            focus: cx.focus_handle(),
            marked: None,
            rows: Vec::new(),
            layout_width: 0.,
            layout_dirty: true,
            scroll: 0.,
            bounds: None,
            selecting: false,
            ensure_cursor: false,
            desired_x: None,
            last_input: None,
            input_error: None,
        }
    }
    /// Reuse the editor's native IME, wrapping, selection and undo for review text.
    pub fn prose(mode: ThemeMode, label: &str, cx: &mut Context<Self>) -> Self {
        let mut editor = Self::new(String::new(), None, mode, cx);
        editor.prose_label = Some(label.into());
        editor.placeholder = label.into();
        editor
    }
    pub fn text(&self) -> &str {
        &self.buffer.text
    }
    pub fn set_accessible_name(&mut self, label: impl Into<String>) {
        self.prose_label = Some(label.into());
    }
    pub fn set_placeholder(&mut self, placeholder: impl Into<String>, cx: &mut Context<Self>) {
        self.placeholder = placeholder.into();
        cx.notify();
    }
    pub fn set_text_silently(&mut self, text: &str, cx: &mut Context<Self>) {
        self.reload(text.into(), cx);
    }
    fn line_height(&self) -> f32 {
        if self.prose_label.is_some() {
            22.75
        } else {
            LINE_HEIGHT
        }
    }
    fn font_size(&self) -> f32 {
        if self.prose_label.is_some() {
            13.0
        } else {
            FONT_SIZE
        }
    }
    fn gutter(&self) -> f32 {
        if self.prose_label.is_some() {
            0.0
        } else {
            GUTTER
        }
    }
    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.layout_dirty = true;
        cx.notify();
    }
    pub fn reload(&mut self, text: String, cx: &mut Context<Self>) {
        let cursor = self.buffer.cursor.min(text.len());
        self.buffer = Buffer {
            text,
            ..Default::default()
        };
        self.buffer.cursor = cursor;
        while !self.buffer.text.is_char_boundary(self.buffer.cursor) {
            self.buffer.cursor -= 1;
        }
        self.buffer.anchor = self.buffer.cursor;
        self.marked = None;
        self.layout_dirty = true;
        cx.notify();
    }
    pub fn can_undo(&self) -> bool {
        !self.buffer.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.buffer.redo.is_empty()
    }
    pub fn composing(&self) -> bool {
        self.marked.is_some()
    }
    pub fn undo(&mut self, cx: &mut Context<Self>) {
        if self.can_undo() {
            self.last_input = None;
            self.buffer.undo();
            self.marked = None;
            self.changed(cx);
        }
    }
    pub fn redo(&mut self, cx: &mut Context<Self>) {
        if self.can_redo() {
            self.last_input = None;
            self.buffer.redo();
            self.marked = None;
            self.changed(cx);
        }
    }
    fn changed(&mut self, cx: &mut Context<Self>) {
        self.layout_dirty = true;
        self.ensure_cursor = true;
        self.desired_x = None;
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }
    fn accepts(&mut self, range: &Range<usize>, text: &str, cx: &mut Context<Self>) -> bool {
        if self.buffer.text.len() - range.len() + text.len()
            > super::file_io::MAX_TEXT_BYTES as usize
        {
            self.input_error = Some("输入后文件将超过 2 MB，请缩小粘贴内容");
            cx.notify();
            return false;
        }
        let mut value = self.buffer.text.clone();
        value.replace_range(range.clone(), text);
        if value.lines().any(|line| line.len() > 65_536) {
            self.input_error = Some("单行不能超过 64 KB，请拆分长行");
            cx.notify();
            return false;
        }
        self.input_error = None;
        true
    }

    fn replace(&mut self, text: &str, cx: &mut Context<Self>) {
        self.last_input = None;
        let range = self.marked.clone().unwrap_or(self.buffer.selection());
        if !self.accepts(&range, text, cx) {
            return;
        }
        self.marked = None;
        self.buffer.replace(range, text, true);
        self.changed(cx);
    }
    fn copy(&self, cx: &mut Context<Self>) {
        let range = self.buffer.selection();
        if !range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.buffer.text[range].into()));
        }
    }
    fn delete(&mut self, back: bool, cx: &mut Context<Self>) {
        self.last_input = None;
        let mut r = self.buffer.selection();
        if r.is_empty() {
            if back {
                r.start = previous(&self.buffer.text, r.start)
            } else {
                r.end = next(&self.buffer.text, r.end)
            }
        }
        if !r.is_empty() {
            self.buffer.replace(r, "", true);
            self.marked = None;
            self.changed(cx);
        }
    }
    fn move_to(&mut self, offset: usize, select: bool, cx: &mut Context<Self>) {
        self.last_input = None;
        self.buffer.cursor = offset;
        if !select {
            self.buffer.anchor = offset;
        }
        self.ensure_cursor = true;
        self.marked = None;
        cx.notify();
    }
    fn row_for(&self, offset: usize) -> usize {
        self.rows
            .iter()
            .rposition(|r| r.start() <= offset)
            .unwrap_or(0)
    }
    pub fn go_to_line(&mut self, line: usize, cx: &mut Context<Self>) {
        let offset = self
            .buffer
            .text
            .split_inclusive('\n')
            .take(line.saturating_sub(1))
            .map(str::len)
            .sum();
        self.move_to(offset, false, cx);
    }
    fn handle_key(&mut self, e: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let k = &e.keystroke;
        if self.prose_label.is_some() && k.modifiers.platform && k.key == "enter" {
            return;
        }
        let text = &self.buffer.text;
        let cursor = self.buffer.cursor;
        let select = k.modifiers.shift;
        let line_start = text[..cursor].rfind('\n').map_or(0, |i| i + 1);
        let line_end = text[cursor..].find('\n').map_or(text.len(), |i| cursor + i);
        let target = match k.key.as_str() {
            "left" if k.modifiers.platform => line_start,
            "right" if k.modifiers.platform => line_end,
            "left" if k.modifiers.alt => {
                let mut p = previous(text, cursor);
                while p > line_start && text[p..].chars().next().is_some_and(char::is_whitespace) {
                    p = previous(text, p);
                }
                while p > line_start
                    && text[..p]
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_')
                {
                    p = previous(text, p);
                }
                p
            }
            "right" if k.modifiers.alt => {
                let mut p = next(text, cursor);
                while p < line_end
                    && text[p..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_')
                {
                    p = next(text, p);
                }
                while p < line_end && text[p..].chars().next().is_some_and(char::is_whitespace) {
                    p = next(text, p);
                }
                p
            }
            "left" => {
                if !select && !self.buffer.selection().is_empty() {
                    self.buffer.selection().start
                } else {
                    previous(text, cursor)
                }
            }
            "right" => {
                if !select && !self.buffer.selection().is_empty() {
                    self.buffer.selection().end
                } else {
                    next(text, cursor)
                }
            }
            "home" => line_start,
            "end" => line_end,
            "up" if k.modifiers.platform => 0,
            "down" if k.modifiers.platform => text.len(),
            "up" | "down" | "pageup" | "pagedown" => {
                if self.rows.is_empty() {
                    return;
                }
                let row = self.row_for(cursor);
                let x = self.desired_x.unwrap_or_else(|| {
                    f32::from(
                        self.rows[row]
                            .line
                            .x_for_index(self.rows[row].display(cursor)),
                    )
                });
                self.desired_x = Some(x);
                let step = if k.key.starts_with("page") {
                    self.bounds.map_or(10, |b| {
                        (f32::from(b.size.height) / self.line_height()) as usize
                    })
                } else {
                    1
                };
                let index = if matches!(k.key.as_str(), "up" | "pageup") {
                    row.saturating_sub(step)
                } else {
                    (row + step).min(self.rows.len() - 1)
                };
                self.rows[index].source(self.rows[index].line.closest_index_for_x(px(x)))
            }
            _ => return,
        };
        if !matches!(k.key.as_str(), "up" | "down" | "pageup" | "pagedown") {
            self.desired_x = None;
        }
        self.move_to(target, select, cx);
        cx.stop_propagation();
    }
    fn offset_at(&self, p: Point<Pixels>) -> usize {
        let Some(b) = self.bounds else {
            return 0;
        };
        if self.rows.is_empty() {
            return 0;
        }
        let row = ((f32::from(p.y - b.top()) + self.scroll).max(0.) / self.line_height()) as usize;
        let row = &self.rows[row.min(self.rows.len() - 1)];
        row.source(
            row.line
                .closest_index_for_x((p.x - b.left() - px(self.gutter())).max(px(0.))),
        )
    }
    fn prepare(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.bounds = Some(bounds);
        let width = (f32::from(bounds.size.width) - self.gutter() - 16.).max(24.);
        if self.layout_dirty || (self.layout_width - width).abs() > 0.5 {
            self.rows.clear();
            self.layout_width = width;
            self.layout_dirty = false;
            let theme = Theme::for_mode(self.mode);
            let mut font = ui_font();
            font.family = UI_MONOSPACE_FONT_FAMILY.into();
            let spans = super::markdown::file_editor_runs(
                &self.buffer.text,
                self.language.as_deref(),
                theme,
            );
            let mut source = 0;
            let mut span_index = 0;
            for (number, raw) in self.buffer.text.split('\n').enumerate() {
                let mut display = String::new();
                let mut map = Vec::new();
                let mut runs: Vec<TextRun> = Vec::new();
                for (offset, g) in raw.grapheme_indices(true) {
                    let start = source + offset;
                    map.push((display.len(), start));
                    while span_index + 1 < spans.len() && spans[span_index].0.end <= start {
                        span_index += 1;
                    }
                    let expanded = if g == "\t" {
                        "    "
                    } else if g == "\r" {
                        ""
                    } else {
                        g
                    };
                    display.push_str(expanded);
                    let mut run = spans
                        .get(span_index)
                        .map(|s| s.1.clone())
                        .unwrap_or(TextRun {
                            len: 0,
                            font: font.clone(),
                            color: theme.text.into(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        });
                    if self.prose_label.is_some() {
                        run.font = ui_font();
                        run.color = theme.text.into();
                    }
                    run.len = expanded.len();
                    if self
                        .marked
                        .as_ref()
                        .is_some_and(|r| r.start <= start && start < r.end)
                    {
                        run.underline = Some(gpui::UnderlineStyle {
                            thickness: px(1.),
                            color: None,
                            wavy: false,
                        });
                    }
                    runs.push(run);
                }
                map.push((display.len(), source + raw.len()));
                let full = window.text_system().shape_line(
                    display.clone().into(),
                    px(self.font_size()),
                    &runs,
                    None,
                );
                let mut start = 0;
                let mut first = true;
                loop {
                    let desired = full.closest_index_for_x(full.x_for_index(start) + px(width));
                    let mut end = map
                        .iter()
                        .filter(|(d, _)| *d > start && *d <= desired)
                        .map(|(d, _)| *d)
                        .next_back()
                        .unwrap_or_else(|| {
                            map.iter()
                                .find(|(d, _)| *d > start)
                                .map_or(display.len(), |(d, _)| *d)
                        });
                    if f32::from(full.width() - full.x_for_index(start)) <= width {
                        end = display.len();
                    }
                    let mut run_start = 0;
                    let row_runs = runs
                        .iter()
                        .filter_map(|r| {
                            let a = run_start;
                            run_start += r.len;
                            let overlap = start.max(a)..end.min(run_start);
                            if overlap.start < overlap.end {
                                let mut r = r.clone();
                                r.len = overlap.len();
                                Some(r)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                    let line = window.text_system().shape_line(
                        display[start..end].to_string().into(),
                        px(self.font_size()),
                        &row_runs,
                        None,
                    );
                    let row_map = map
                        .iter()
                        .filter(|(d, _)| *d >= start && *d <= end)
                        .map(|(d, s)| (d - start, *s))
                        .collect();
                    self.rows.push(Row {
                        line,
                        map: row_map,
                        number: first.then_some(number + 1),
                    });
                    first = false;
                    if end == display.len() {
                        break;
                    }
                    start = end;
                }
                source += raw.len() + 1;
            }
        }
        let height = f32::from(bounds.size.height);
        if self.ensure_cursor {
            let top = self.row_for(self.buffer.cursor) as f32 * self.line_height();
            if top < self.scroll {
                self.scroll = top;
            } else if top + self.line_height() > self.scroll + height {
                self.scroll = top + self.line_height() - height;
            }
            self.ensure_cursor = false;
        }
        self.scroll = self.scroll.clamp(
            0.,
            (self.rows.len() as f32 * self.line_height() + 16. - height).max(0.),
        );
        let _ = cx;
    }
    fn paint(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let theme = Theme::for_mode(self.mode);
        if self.buffer.text.is_empty() && !self.placeholder.is_empty() {
            let mut run = window.text_style().to_run(self.placeholder.len());
            run.font = ui_font();
            run.color = theme.text_tertiary.into();
            let line = window.text_system().shape_line(
                self.placeholder.clone().into(),
                px(self.font_size()),
                &[run],
                None,
            );
            let _ = line.paint(
                bounds.origin,
                px(self.line_height()),
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            );
        }
        let selection = self.buffer.selection();
        let start = (self.scroll / self.line_height()).floor() as usize;
        let count = (f32::from(bounds.size.height) / self.line_height()).ceil() as usize + 1;
        for (index, row) in self.rows.iter().enumerate().skip(start).take(count) {
            let y = bounds.top() + px(index as f32 * self.line_height() - self.scroll);
            let origin = point(bounds.left() + px(self.gutter()), y);
            if !selection.is_empty() && selection.end > row.start() && selection.start <= row.end()
            {
                let left = row
                    .line
                    .x_for_index(row.display(selection.start.max(row.start())));
                let mut right = row
                    .line
                    .x_for_index(row.display(selection.end.min(row.end())));
                if selection.end > row.end() {
                    right += px(7.);
                }
                window.paint_quad(fill(
                    Bounds::new(
                        origin + point(left, px(0.)),
                        size((right - left).max(px(2.)), px(self.line_height())),
                    ),
                    theme.accent.alpha(0.18),
                ));
            }
            if let Some(number) = row.number.filter(|_| self.prose_label.is_none()) {
                let mut font = ui_font();
                font.family = UI_MONOSPACE_FONT_FAMILY.into();
                let text = number.to_string();
                let line = window.text_system().shape_line(
                    text.clone().into(),
                    px(self.font_size()),
                    &[TextRun {
                        len: text.len(),
                        font,
                        color: theme.text_tertiary.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                );
                let _ = line.paint(
                    point(bounds.left() + px(self.gutter() - 12.) - line.width(), y),
                    px(self.line_height()),
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
            let _ = row.line.paint(
                origin,
                px(self.line_height()),
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            );
            if self.focus.is_focused(window)
                && selection.is_empty()
                && index == self.row_for(self.buffer.cursor)
            {
                let x = row.line.x_for_index(row.display(self.buffer.cursor));
                window.paint_quad(fill(
                    Bounds::new(origin + point(x, px(3.)), size(px(1.), px(16.))),
                    theme.text,
                ));
            }
        }
        if self.rows.len() as f32 * self.line_height() > f32::from(bounds.size.height) {
            let height = f32::from(bounds.size.height);
            let total = self.rows.len() as f32 * self.line_height() + 16.;
            let thumb = (height * height / total).max(24.);
            let top = self.scroll / (total - height) * (height - thumb);
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.right() - px(6.), bounds.top() + px(top)),
                    size(px(4.), px(thumb)),
                ),
                theme.text.alpha(0.2),
            ));
        }
    }
}
impl Render for FileEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let paint_entity = entity.clone();
        div()
            .id("file-editor")
            .when(self.prose_label.is_none(), |d| {
                d.bg(Theme::for_mode(self.mode).file_editor_surface)
            })
            .relative()
            .size_full()
            .overflow_hidden()
            .cursor(CursorStyle::IBeam)
            .key_context("FileEditor")
            .track_focus(&self.focus)
            .role(gpui::Role::TextInput)
            .aria_label(
                self.prose_label
                    .clone()
                    .unwrap_or_else(|| "文件内容，可直接编辑并自动保存".into()),
            )
            .aria_placeholder(self.placeholder.clone())
            .aria_value(self.buffer.text.clone())
            .on_key_down(cx.listener(Self::handle_key))
            .on_action(cx.listener(|s, _: &EditorCopy, _, cx| {
                s.copy(cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|s, _: &EditorCut, _, cx| {
                s.copy(cx);
                if !s.buffer.selection().is_empty() {
                    s.replace("", cx);
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|s, _: &EditorPaste, _, cx| {
                if let Some(t) = cx.read_from_clipboard().and_then(|v| v.text()) {
                    s.replace(&t.replace("\r\n", "\n"), cx);
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|s, _: &EditorSelectAll, _, cx| {
                s.buffer.anchor = 0;
                s.buffer.cursor = s.buffer.text.len();
                cx.notify();
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|s, _: &EditorUndo, _, cx| {
                s.undo(cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|s, _: &EditorRedo, _, cx| {
                s.redo(cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|_, _: &EditorSave, _, cx| {
                cx.emit(EditorEvent::Save);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|s, _: &EditorBackspace, _, cx| {
                s.delete(true, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|s, _: &EditorDelete, _, cx| {
                s.delete(false, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|s, _: &EditorEnter, _, cx| {
                let start = s.buffer.text[..s.buffer.cursor]
                    .rfind('\n')
                    .map_or(0, |i| i + 1);
                let indent = s.buffer.text[start..s.buffer.cursor]
                    .chars()
                    .take_while(|c| matches!(c, ' ' | '\t'))
                    .collect::<String>();
                s.replace(&format!("\n{indent}"), cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|s, _: &EditorTab, _, cx| {
                s.replace("    ", cx);
                cx.stop_propagation();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|s, e: &gpui::MouseDownEvent, w, cx| {
                    s.focus.focus(w, cx);
                    let p = s.offset_at(e.position);
                    if !e.modifiers.shift {
                        s.buffer.anchor = p;
                    }
                    s.buffer.cursor = p;
                    s.marked = None;
                    s.selecting = true;
                    s.desired_x = None;
                    if e.click_count == 2 {
                        let text = &s.buffer.text;
                        let mut a = p;
                        let mut b = p;
                        while a > 0
                            && text[..a]
                                .chars()
                                .next_back()
                                .is_some_and(|c| c.is_alphanumeric() || c == '_')
                        {
                            a = previous(text, a);
                        }
                        while b < text.len()
                            && text[b..]
                                .chars()
                                .next()
                                .is_some_and(|c| c.is_alphanumeric() || c == '_')
                        {
                            b = next(text, b);
                        }
                        s.buffer.anchor = a;
                        s.buffer.cursor = b;
                    } else if e.click_count >= 3 {
                        let a = s.buffer.text[..p].rfind('\n').map_or(0, |i| i + 1);
                        let b = s.buffer.text[p..]
                            .find('\n')
                            .map_or(s.buffer.text.len(), |i| p + i + 1);
                        s.buffer.anchor = a;
                        s.buffer.cursor = b;
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|s, e: &gpui::MouseMoveEvent, _, cx| {
                if s.selecting {
                    if let Some(b) = s.bounds {
                        if e.position.y < b.top() {
                            s.scroll = (s.scroll - s.line_height()).max(0.);
                        } else if e.position.y > b.bottom() {
                            s.scroll += s.line_height();
                        }
                    }
                    s.buffer.cursor = s.offset_at(e.position);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|s, _, _, _| s.selecting = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|s, _, _, _| s.selecting = false),
            )
            .on_scroll_wheel(cx.listener(|s, e: &gpui::ScrollWheelEvent, _, cx| {
                s.scroll -= f32::from(e.delta.pixel_delta(px(s.line_height())).y);
                s.ensure_cursor = false;
                cx.notify();
                cx.stop_propagation();
            }))
            .when_some(self.input_error, |e, error| {
                e.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .p(px(8.))
                        .bg(Theme::for_mode(self.mode).surface)
                        .text_color(Theme::for_mode(self.mode).warning)
                        .child(error),
                )
            })
            .child(
                canvas(
                    move |b, w, cx| {
                        entity.update(cx, |s, cx| s.prepare(b, w, cx));
                    },
                    move |b, _, w, cx| {
                        paint_entity.update(cx, |s, cx| {
                            w.handle_input(
                                &s.focus,
                                ElementInputHandler::new(b, paint_entity.clone()),
                                cx,
                            );
                            s.paint(b, w, cx);
                        });
                    },
                )
                .size_full(),
            )
    }
}
impl EntityInputHandler for FileEditor {
    fn text_for_range(
        &mut self,
        r: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let r =
            self.buffer.byte_offset_from_utf16(r.start)..self.buffer.byte_offset_from_utf16(r.end);
        *adjusted = Some(self.buffer.to_utf16(r.start)..self.buffer.to_utf16(r.end));
        Some(self.buffer.text[r].into())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let r = self.buffer.selection();
        Some(UTF16Selection {
            range: self.buffer.to_utf16(r.start)..self.buffer.to_utf16(r.end),
            reversed: self.buffer.cursor < self.buffer.anchor,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked
            .as_ref()
            .map(|r| self.buffer.to_utf16(r.start)..self.buffer.to_utf16(r.end))
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        self.changed(cx);
    }
    fn replace_text_in_range(
        &mut self,
        r: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let checkpoint = self.marked.is_none();
        let r = r
            .map(|r| {
                self.buffer.byte_offset_from_utf16(r.start)
                    ..self.buffer.byte_offset_from_utf16(r.end)
            })
            .or(self.marked.take())
            .unwrap_or(self.buffer.selection());
        if !self.accepts(&r, text, cx) {
            return;
        }
        let now = Instant::now();
        let group = checkpoint
            && r.is_empty()
            && !text.contains('\n')
            && self.last_input.is_some_and(|(time, end)| {
                now.duration_since(time) < Duration::from_millis(750) && end == r.start
            });
        self.buffer.replace(r, text, checkpoint && !group);
        self.last_input = (!text.contains('\n') && checkpoint).then_some((now, self.buffer.cursor));
        self.changed(cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        r: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let checkpoint = self.marked.is_none();
        let r = r
            .map(|r| {
                self.buffer.byte_offset_from_utf16(r.start)
                    ..self.buffer.byte_offset_from_utf16(r.end)
            })
            .or(self.marked.take())
            .unwrap_or(self.buffer.selection());
        if !self.accepts(&r, text, cx) {
            return;
        }
        self.last_input = None;
        let start = r.start;
        self.buffer.replace(r, text, checkpoint);
        self.marked = (!text.is_empty()).then_some(start..start + text.len());
        if let Some(r) = selected {
            self.buffer.anchor = start + from_utf16(text, r.start);
            self.buffer.cursor = start + from_utf16(text, r.end);
        }
        self.changed(cx);
    }
    fn bounds_for_range(
        &mut self,
        r: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let b = self.bounds?;
        let p = self.buffer.byte_offset_from_utf16(r.start);
        let index = self.row_for(p);
        let row = self.rows.get(index)?;
        Some(Bounds::new(
            point(
                b.left() + px(self.gutter()) + row.line.x_for_index(row.display(p)),
                b.top() + px(index as f32 * self.line_height() - self.scroll),
            ),
            size(px(1.), px(self.line_height())),
        ))
    }
    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.buffer.to_utf16(self.offset_at(p)))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_editor_handles_wrapping_selection_clipboard_and_ime() {
        let mut app = gpui::TestApp::new();
        app.update(init);
        let mut window = app.open_window_with_options(
            gpui::WindowOptions {
                window_bounds: Some(gpui::WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(260.), px(140.)),
                ))),
                ..Default::default()
            },
            |_, cx| {
                FileEditor::new(
                    "中文👩‍💻\n    value\n".into(),
                    Some("rs".into()),
                    ThemeMode::Light,
                    cx,
                )
            },
        );
        window.draw();
        window.simulate_click(point(px(50.), px(10.)), MouseButton::Left);
        window.simulate_keystrokes("cmd-a");
        window.simulate_input("αβ\n    中文👋");
        window.simulate_keystrokes("enter");
        window.simulate_input("next");
        assert_eq!(
            window.read(|e, _| e.buffer.text.clone()),
            "αβ\n    中文👋\n    next"
        );
        window.simulate_keystrokes("cmd-z cmd-z");
        assert_eq!(window.read(|e, _| e.buffer.text.clone()), "αβ\n    中文👋");
        window.simulate_keystrokes("shift-left cmd-c");
        assert_eq!(
            app.read_from_clipboard().and_then(|v| v.text()),
            Some("👋".into())
        );
        window.update(|e, w, cx| {
            e.replace_and_mark_text_in_range(None, "ni", Some(0..2), w, cx);
            e.replace_and_mark_text_in_range(None, "你", Some(1..1), w, cx);
            e.replace_text_in_range(None, "你好", w, cx);
            assert_eq!(e.buffer.text, "αβ\n    中文你好");
            e.undo(cx);
            assert_eq!(e.buffer.text, "αβ\n    中文👋");
            e.reload("中文👋".repeat(100), cx);
            e.go_to_line(1, cx);
        });
        window.draw();
        window.read(|e, _| {
            assert!(e.rows.len() > 10);
            assert!(
                e.rows
                    .iter()
                    .all(|r| e.buffer.text.is_char_boundary(r.start())
                        && e.buffer.text.is_char_boundary(r.end()))
            );
        });
        window.simulate_keystrokes("cmd-down");
        window.draw();
        assert!(window.read(|e, _| e.scroll > 0.));
    }

    #[test]
    fn unicode_selection_replacement_and_undo() {
        let mut b = Buffer {
            text: "中文👩‍💻\nabc".into(),
            ..Default::default()
        };
        assert_eq!(from_utf16(&b.text, 2), 6);
        assert_eq!(next(&b.text, 6), 17);
        assert_eq!(previous(&b.text, 17), 6);
        b.replace(6..17, "👋", true);
        assert_eq!(b.text, "中文👋\nabc");
        b.undo();
        assert_eq!(b.text, "中文👩‍💻\nabc");
        b.redo();
        assert_eq!(b.text, "中文👋\nabc");
    }
    #[test]
    fn new_edit_invalidates_redo() {
        let mut b = Buffer::default();
        b.replace(0..0, "a", true);
        b.undo();
        b.replace(0..0, "b", true);
        b.redo();
        assert_eq!(b.text, "b");
    }
}
