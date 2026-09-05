use std::{ops::Range, time::Duration};

use gpui::{
    Animation, AnimationExt, App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable, FontWeight,
    GlobalElementId, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    PaintQuad, Pixels, Point, ShapedLine, SharedString, Style, TextRun, UTF16Selection,
    UnderlineStyle, Window, div, fill, point, prelude::*, px, relative, rgba, size,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::theme::{Theme, ThemeMode};

const PROMPT_FONT_SIZE: f32 = 14.0;
const PROMPT_LINE_HEIGHT: f32 = 20.0;
const PLACEHOLDER_OPACITY: f32 = 0.5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PromptInputKind {
    #[default]
    Composer,
    InlineOther,
}

gpui::actions!(
    prompt_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        Paste,
        Cut,
        Copy,
        Submit
    ]
);

#[derive(Clone)]
pub struct PromptSubmitted(pub String);

pub struct PromptChanged;

pub struct PromptInput {
    mode: ThemeMode,
    kind: PromptInputKind,
    placeholder: SharedString,
    secret: bool,
    focus_handle: FocusHandle,
    content: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    horizontal_scroll: f32,
    is_selecting: bool,
}

impl gpui::EventEmitter<PromptSubmitted> for PromptInput {}
impl gpui::EventEmitter<PromptChanged> for PromptInput {}

impl PromptInput {
    pub fn new(mode: ThemeMode, cx: &mut Context<Self>) -> Self {
        Self {
            mode,
            kind: PromptInputKind::Composer,
            placeholder: "随心输入".into(),
            secret: false,
            focus_handle: cx.focus_handle(),
            content: "".into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            horizontal_scroll: 0.0,
            is_selecting: false,
        }
    }

    pub fn inline_other(
        mode: ThemeMode,
        placeholder: impl Into<SharedString>,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut input = Self::new(mode, cx);
        input.kind = PromptInputKind::InlineOther;
        input.placeholder = placeholder.into();
        input.secret = secret;
        input
    }

    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        cx.notify();
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    #[cfg(test)]
    pub fn set_text(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.set_text_inner(text.into(), true, cx);
    }

    pub fn set_text_silently(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.set_text_inner(text.into(), false, cx);
    }

    fn set_text_inner(&mut self, text: SharedString, emit_changed: bool, cx: &mut Context<Self>) {
        if self.content == text {
            return;
        }
        self.content = text;
        let cursor = self.content.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
        self.last_layout = None;
        self.horizontal_scroll = 0.0;
        if emit_changed {
            cx.emit(PromptChanged);
        }
        cx.notify();
    }

    pub fn configure_inline_other(
        &mut self,
        placeholder: impl Into<SharedString>,
        secret: bool,
        cx: &mut Context<Self>,
    ) {
        self.kind = PromptInputKind::InlineOther;
        self.placeholder = placeholder.into();
        self.secret = secret;
        self.last_layout = None;
        cx.notify();
    }

    #[cfg(test)]
    pub fn is_secret(&self) -> bool {
        self.secret
    }

    fn display_text(&self) -> SharedString {
        if !self.secret {
            return self.content.clone();
        }
        self.content
            .graphemes(true)
            .map(|_| "•")
            .collect::<String>()
            .into()
    }

    fn display_offset_for_content(&self, offset: usize) -> usize {
        let offset = self.clamp_offset(offset);
        if !self.secret {
            return offset;
        }
        self.content[..offset].graphemes(true).count() * "•".len()
    }

    fn content_offset_for_display(&self, offset: usize) -> usize {
        if !self.secret {
            return self.clamp_offset(offset);
        }
        let display = self.display_text();
        let mut display_offset = offset.min(display.len());
        while !display.is_char_boundary(display_offset) {
            display_offset -= 1;
        }
        let grapheme_index = display[..display_offset].graphemes(true).count();
        self.content
            .grapheme_indices(true)
            .nth(grapheme_index)
            .map(|(index, _)| index)
            .unwrap_or(self.content.len())
    }

    fn marked_display_range(&self) -> Option<Range<usize>> {
        let range = self.marked_range.as_ref()?;
        let start = self.display_offset_for_content(range.start);
        let end = self.display_offset_for_content(range.end);
        (start < end).then_some(start..end)
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.content = "".into();
        self.selected_range = 0..0;
        self.selection_reversed = false;
        self.marked_range = None;
        self.last_layout = None;
        self.horizontal_scroll = 0.0;
        cx.emit(PromptChanged);
        cx.notify();
    }

    pub fn submit(&mut self, cx: &mut Context<Self>) {
        let prompt = self.content.trim().to_owned();
        if !prompt.is_empty() {
            cx.emit(PromptSubmitted(prompt));
        }
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let previous = self.previous_boundary(self.cursor_offset());
            if previous == self.cursor_offset() {
                return;
            }
            self.select_to(previous, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());
            if next == self.cursor_offset() {
                return;
            }
            self.select_to(next, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn paste_action(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        self.copy(&Copy, window, cx);
        if !self.selected_range.is_empty() {
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn submit_action(&mut self, _: &Submit, _: &mut Window, cx: &mut Context<Self>) {
        self.submit(cx);
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        self.is_selecting = true;
        let index = self.index_for_mouse_position(event.position);
        if event.modifiers.shift {
            self.select_to(index, cx);
        } else {
            self.move_to(index, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = self.clamp_offset(offset);
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = self.clamp_offset(offset);
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        // A collapsed native selection has no direction. Keeping the reversed
        // bit after Shift+Left then Shift+Right reaches the anchor makes GPUI's
        // UTF16Selection disagree with the actual empty range.
        if self.selected_range.is_empty() {
            self.selection_reversed = false;
        }
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        // When the prompt is empty, `last_layout` contains the placeholder. Its
        // glyph indices do not belong to `content` and must never become an edit
        // range (doing so makes the next native input event slice past byte 0).
        if self.content.is_empty() {
            return 0;
        }
        let (Some(bounds), Some(line)) = (self.last_bounds, self.last_layout.as_ref()) else {
            return 0;
        };
        let local_x =
            f32::from(position.x - bounds.left()).clamp(0.0, f32::from(bounds.size.width).max(0.0));
        self.content_offset_for_display(
            line.closest_index_for_x(px(local_x + self.horizontal_scroll)),
        )
    }

    fn clamp_offset(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.content.len());
        while !self.content.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    fn normalized_range(&self, range: Range<usize>) -> Range<usize> {
        let start = self.clamp_offset(range.start);
        let end = self.clamp_offset(range.end);
        start.min(end)..start.max(end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(self.content.len())
    }

    fn offset_from_utf16_in_text(text: &str, offset: usize) -> usize {
        let mut utf8 = 0;
        let mut utf16 = 0;
        for character in text.chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += character.len_utf16();
            utf8 += character.len_utf8();
        }
        utf8
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        Self::offset_from_utf16_in_text(&self.content, offset)
    }

    fn range_from_utf16_in_text(text: &str, range: &Range<usize>) -> Range<usize> {
        Self::offset_from_utf16_in_text(text, range.start)
            ..Self::offset_from_utf16_in_text(text, range.end)
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let offset = self.clamp_offset(offset);
        self.content[..offset].encode_utf16().count()
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }
}

impl EntityInputHandler for PromptInput {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range);
        adjusted_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.marked_range.take().is_some() {
            // The composing underline is paint state, so committing an IME
            // composition must schedule a redraw even though content is stable.
            cx.notify();
        }
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        let range = self.normalized_range(range);
        self.content = format!(
            "{}{}{}",
            &self.content[..range.start],
            new_text,
            &self.content[range.end..]
        )
        .into();
        let cursor = range.start + new_text.len();
        self.selected_range = cursor..cursor;
        // A committed replacement establishes a new collapsed selection.
        // Its direction must not inherit the range that was replaced.
        self.selection_reversed = false;
        self.marked_range = None;
        cx.emit(PromptChanged);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        let range = self.normalized_range(range);
        self.content = format!(
            "{}{}{}",
            &self.content[..range.start],
            new_text,
            &self.content[range.end..]
        )
        .into();
        self.marked_range =
            (!new_text.is_empty()).then_some(range.start..range.start + new_text.len());
        self.selected_range = new_selected_range
            .as_ref()
            // GPUI supplies this selection relative to the newly inserted
            // marked text. Converting it against the complete content makes
            // a non-ASCII prefix shift the cursor to unrelated UTF-8 bytes.
            .map(|selection| Self::range_from_utf16_in_text(new_text, selection))
            .map(|selection| range.start + selection.start..range.start + selection.end)
            .unwrap_or_else(|| {
                let cursor = range.start + new_text.len();
                cursor..cursor
            });
        // `new_selected_range` describes the new marked text and carries no
        // reversed-direction bit. Treat it (or the fallback caret) as a fresh
        // forward selection rather than retaining the replaced selection's
        // direction.
        self.selection_reversed = false;
        cx.emit(PromptChanged);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range);
        let range = self.display_offset_for_content(range.start)
            ..self.display_offset_for_content(range.end);
        Some(Bounds::from_corners(
            point(
                bounds.left() + line.x_for_index(range.start) - px(self.horizontal_scroll),
                bounds.top(),
            ),
            point(
                bounds.left() + line.x_for_index(range.end) - px(self.horizontal_scroll),
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let line = self.last_layout.as_ref()?;
        let local_x =
            f32::from(point.x - bounds.left()).clamp(0.0, f32::from(bounds.size.width).max(0.0));
        let index = line.closest_index_for_x(px(local_x + self.horizontal_scroll));
        Some(self.offset_to_utf16(self.content_offset_for_display(index)))
    }
}

struct PromptTextElement {
    input: Entity<PromptInput>,
    text_color: gpui::Hsla,
    placeholder_color: gpui::Hsla,
    caret_visible: bool,
    font_size: f32,
    font_weight: FontWeight,
    line_height: f32,
}

struct PromptPrepaint {
    line: ShapedLine,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
    horizontal_scroll: f32,
}

fn text_runs_with_marked_underline(
    base_run: TextRun,
    marked_range: Option<Range<usize>>,
) -> Vec<TextRun> {
    let Some(marked_range) = marked_range.filter(|range| {
        range.start < range.end && range.start <= base_run.len && range.end <= base_run.len
    }) else {
        return vec![base_run];
    };

    let total_len = base_run.len;
    [
        TextRun {
            len: marked_range.start,
            ..base_run.clone()
        },
        TextRun {
            len: marked_range.end - marked_range.start,
            underline: Some(UnderlineStyle {
                color: Some(base_run.color),
                thickness: px(1.0),
                wavy: false,
            }),
            ..base_run.clone()
        },
        TextRun {
            len: total_len - marked_range.end,
            ..base_run
        },
    ]
    .into_iter()
    .filter(|run| run.len > 0)
    .collect()
}

impl IntoElement for PromptTextElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for PromptTextElement {
    type RequestLayoutState = ();
    type PrepaintState = PromptPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = px(self.line_height).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> PromptPrepaint {
        let input = self.input.read(cx);
        let empty = input.content.is_empty();
        let text: SharedString = if empty {
            input.placeholder.clone()
        } else {
            input.display_text()
        };
        let color = if empty {
            self.placeholder_color
        } else {
            self.text_color
        };
        // Live CDP: composer system-ui 14/20 at 430; inline controls stay 400.
        // Pin the weight here because this text is shaped and painted manually;
        // otherwise an ancestor's text style can silently change the glyph run.
        let mut font = window.text_style().font();
        font.weight = self.font_weight;
        let run = TextRun {
            len: text.len(),
            font,
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let marked_display_range = (!empty).then(|| input.marked_display_range()).flatten();
        let runs = text_runs_with_marked_underline(run, marked_display_range);
        let line = window
            .text_system()
            .shape_line(text, px(self.font_size), &runs, None);
        let viewport_width = f32::from(bounds.size.width).max(0.0);
        // The caret is painted as a 1px quad. Include that width in the scroll
        // extent so an End caret starts inside, rather than exactly at, the
        // right-hand clipping boundary.
        let max_scroll = (f32::from(line.width()) - viewport_width + 1.0).max(0.0);
        let caret_x =
            f32::from(line.x_for_index(input.display_offset_for_content(input.cursor_offset())));
        let mut horizontal_scroll = input.horizontal_scroll.clamp(0.0, max_scroll);
        if caret_x < horizontal_scroll {
            horizontal_scroll = caret_x.max(0.0);
        } else if caret_x > horizontal_scroll + viewport_width - 1.0 {
            horizontal_scroll = (caret_x - viewport_width + 1.0).clamp(0.0, max_scroll);
        }
        let selection = (!input.selected_range.is_empty()).then(|| {
            let start = input.display_offset_for_content(input.selected_range.start);
            let end = input.display_offset_for_content(input.selected_range.end);
            fill(
                Bounds::from_corners(
                    point(
                        bounds.left() + line.x_for_index(start) - px(horizontal_scroll),
                        bounds.top(),
                    ),
                    point(
                        bounds.left() + line.x_for_index(end) - px(horizontal_scroll),
                        bounds.bottom(),
                    ),
                ),
                rgba(0x539af84d),
            )
        });
        let cursor = (input.selected_range.is_empty() && self.caret_visible).then(|| {
            fill(
                Bounds::new(
                    point(
                        bounds.left()
                            + line.x_for_index(
                                input.display_offset_for_content(input.cursor_offset()),
                            )
                            - px(horizontal_scroll),
                        bounds.top(),
                    ),
                    size(px(1.0), px(self.line_height)),
                ),
                self.text_color,
            )
        });
        PromptPrepaint {
            line,
            cursor,
            selection,
            horizontal_scroll,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        state: &mut PromptPrepaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(selection) = state.selection.take() {
            window.paint_quad(selection);
        }
        state
            .line
            .paint(
                point(
                    bounds.origin.x - px(state.horizontal_scroll),
                    bounds.origin.y,
                ),
                px(self.line_height),
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            )
            .unwrap();
        if focus.is_focused(window)
            && let Some(cursor) = state.cursor.take()
        {
            window.paint_quad(cursor);
        }
        self.input.update(cx, |input, _| {
            input.last_layout = Some(state.line.clone());
            input.last_bounds = Some(bounds);
            input.horizontal_scroll = state.horizontal_scroll;
        });
    }
}

impl PromptInput {
    pub fn element(
        &self,
        text_color: gpui::Hsla,
        placeholder_color: gpui::Hsla,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let inline = self.kind == PromptInputKind::InlineOther;
        let element_id = if inline {
            "user-input-native-other"
        } else {
            "prompt-input"
        };
        let height = if inline { 28.0 } else { 44.0 };
        let horizontal_padding = if inline { 0.0 } else { 4.0 };
        let top_padding = if inline { 4.0 } else { 1.0 };
        div()
            .id(element_id)
            .w_full()
            .h(px(height))
            .overflow_hidden()
            .px(px(horizontal_padding))
            .pt(px(top_padding))
            .flex()
            .items_start()
            .key_context("PromptInput")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste_action))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::submit_action))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .with_animation(
                "prompt-input-caret-blink",
                Animation::new(Duration::from_millis(1_000)).repeat(),
                {
                    let input = cx.entity();
                    move |prompt, progress| {
                        prompt.child(PromptTextElement {
                            input: input.clone(),
                            text_color,
                            placeholder_color,
                            caret_visible: progress < 0.55,
                            font_size: if inline { 13.0 } else { PROMPT_FONT_SIZE },
                            font_weight: if inline {
                                FontWeight::NORMAL
                            } else {
                                crate::theme::UI_BODY_FONT_WEIGHT
                            },
                            line_height: PROMPT_LINE_HEIGHT,
                        })
                    }
                },
            )
    }
}

impl Render for PromptInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_mode(self.mode);
        let placeholder = if self.kind == PromptInputKind::InlineOther {
            // The request-user-input control uses the card's captured
            // `text-secondary` token directly. The main Composer placeholder
            // instead applies a second 50% opacity layer to text-tertiary.
            match self.mode {
                ThemeMode::Light => rgba(0x1a1c1f6a),
                ThemeMode::Dark => rgba(0xffffff63),
            }
        } else {
            theme
                .text_tertiary
                .alpha(theme.text_tertiary.a * PLACEHOLDER_OPACITY)
        };
        self.element(theme.text.into(), placeholder.into(), cx)
    }
}

impl Focusable for PromptInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{KeyBinding, TestApp, WindowBounds, WindowOptions};

    fn bind_prompt_keys(app: &mut TestApp) {
        app.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("backspace", Backspace, Some("PromptInput")),
                KeyBinding::new("delete", Delete, Some("PromptInput")),
                KeyBinding::new("left", Left, Some("PromptInput")),
                KeyBinding::new("right", Right, Some("PromptInput")),
                KeyBinding::new("shift-left", SelectLeft, Some("PromptInput")),
                KeyBinding::new("shift-right", SelectRight, Some("PromptInput")),
                KeyBinding::new("cmd-a", SelectAll, Some("PromptInput")),
                KeyBinding::new("cmd-v", Paste, Some("PromptInput")),
                KeyBinding::new("cmd-c", Copy, Some("PromptInput")),
                KeyBinding::new("cmd-x", Cut, Some("PromptInput")),
                KeyBinding::new("home", Home, Some("PromptInput")),
                KeyBinding::new("end", End, Some("PromptInput")),
                KeyBinding::new("enter", Submit, Some("PromptInput")),
            ]);
        });
    }

    #[test]
    fn inline_other_uses_native_input_for_ime_cursor_selection_and_clipboard_editing() {
        let mut app = TestApp::new();
        bind_prompt_keys(&mut app);
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(420.0), px(44.0)),
                })),
                ..Default::default()
            },
            |_, cx| PromptInput::inline_other(ThemeMode::Light, "请输入其他答案", false, cx),
        );

        window.draw();
        window.simulate_click(point(px(10.0), px(10.0)), MouseButton::Left);
        window.update(|input, window, cx| {
            assert!(input.focus_handle(cx).is_focused(window));
        });

        // `simulate_input` enters through GPUI's EntityInputHandler, the same
        // route used for committed IME text instead of a top-level key event.
        window.simulate_input("甲乙C");
        assert_eq!(window.read(|input, _| input.text().to_owned()), "甲乙C");

        window.simulate_keystrokes("left backspace");
        assert_eq!(window.read(|input, _| input.text().to_owned()), "甲C");
        window.simulate_keystroke("delete");
        assert_eq!(window.read(|input, _| input.text().to_owned()), "甲");

        app.write_to_clipboard(ClipboardItem::new_string("贴".to_owned()));
        window.simulate_keystroke("cmd-v");
        assert_eq!(window.read(|input, _| input.text().to_owned()), "甲贴");

        window.simulate_keystrokes("home delete end shift-left cmd-c");
        assert_eq!(window.read(|input, _| input.text().to_owned()), "贴");
        assert_eq!(
            app.read_from_clipboard().and_then(|item| item.text()),
            Some("贴".to_owned())
        );
        window.simulate_keystrokes("cmd-x cmd-v");
        assert_eq!(window.read(|input, _| input.text().to_owned()), "贴");
    }

    #[test]
    fn long_inline_other_scrolls_to_keep_home_and_end_carets_visible() {
        let mut app = TestApp::new();
        bind_prompt_keys(&mut app);
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(120.0), px(44.0)),
                })),
                ..Default::default()
            },
            |_, cx| PromptInput::inline_other(ThemeMode::Light, "请输入其他答案", false, cx),
        );

        window.draw();
        window.simulate_click(point(px(10.0), px(10.0)), MouseButton::Left);
        window.simulate_input("这是一段足够长的原生 Other 输入，用于验证光标视窗跟随。");
        window.draw();
        let end_scroll = window.read(|input, _| input.horizontal_scroll);
        assert!(end_scroll > 0.0);

        window.simulate_keystroke("home");
        window.draw();
        window.read(|input, _| {
            assert_eq!(input.horizontal_scroll, 0.0);
            let bounds = input.last_bounds.expect("input bounds after draw");
            let line = input.last_layout.as_ref().expect("input layout after draw");
            let caret_x = f32::from(
                line.x_for_index(input.display_offset_for_content(input.cursor_offset())),
            ) - input.horizontal_scroll;
            assert!(caret_x >= 0.0);
            assert!(caret_x + 1.0 <= f32::from(bounds.size.width));
        });

        window.simulate_keystroke("end");
        window.draw();
        window.read(|input, _| {
            assert_eq!(input.horizontal_scroll, end_scroll);
            let bounds = input.last_bounds.expect("input bounds after draw");
            let line = input.last_layout.as_ref().expect("input layout after draw");
            let caret_x = f32::from(
                line.x_for_index(input.display_offset_for_content(input.cursor_offset())),
            ) - input.horizontal_scroll;
            assert!(caret_x >= 0.0);
            assert!(
                caret_x + 1.0 <= f32::from(bounds.size.width),
                "end caret must fit inside the viewport: x={caret_x}, width={}",
                f32::from(bounds.size.width)
            );
        });
    }

    #[test]
    fn collapsing_a_reversed_selection_at_its_anchor_clears_direction() {
        let mut app = TestApp::new();
        bind_prompt_keys(&mut app);
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(420.0), px(44.0)),
                })),
                ..Default::default()
            },
            |_, cx| PromptInput::inline_other(ThemeMode::Light, "请输入其他答案", false, cx),
        );

        window.draw();
        window.simulate_click(point(px(10.0), px(10.0)), MouseButton::Left);
        window.simulate_input("ABC");
        window.simulate_keystrokes("end shift-left shift-right");

        window.update(|input, window, cx| {
            assert_eq!(input.selected_range, 3..3);
            assert!(!input.selection_reversed);
            let native_selection = input.selected_text_range(false, window, cx).unwrap();
            assert_eq!(native_selection.range, 3..3);
            assert!(!native_selection.reversed);
        });
    }

    #[test]
    fn secret_inline_other_masks_graphemes_without_corrupting_native_offsets() {
        let mut app = TestApp::new();
        bind_prompt_keys(&mut app);
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(420.0), px(44.0)),
                })),
                ..Default::default()
            },
            |_, cx| PromptInput::inline_other(ThemeMode::Dark, "秘密答案", true, cx),
        );

        window.draw();
        window.simulate_click(point(px(10.0), px(10.0)), MouseButton::Left);
        window.simulate_input("密🙂e\u{301}");

        window.read(|input, _| {
            assert!(input.is_secret());
            assert_eq!(input.text(), "密🙂e\u{301}");
            assert_eq!(input.display_text().as_ref(), "•••");
            assert_eq!(input.display_offset_for_content(input.text().len()), 9);
            assert_eq!(input.content_offset_for_display(6), "密🙂".len());
        });

        window.simulate_keystrokes("left backspace");
        assert_eq!(
            window.read(|input, _| input.text().to_owned()),
            "密e\u{301}"
        );
        assert_eq!(
            window.read(|input, _| input.display_text().to_string()),
            "••"
        );
    }

    #[test]
    fn marked_text_selection_is_relative_to_new_text_after_non_ascii_prefix() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(420.0), px(44.0)),
                })),
                ..Default::default()
            },
            |_, cx| PromptInput::inline_other(ThemeMode::Light, "请输入其他答案", false, cx),
        );

        window.update(|input, window, cx| {
            input.set_text("🙂前缀旧", cx);
            let insertion_point = "🙂前缀".len();
            input.selected_range = insertion_point..input.text().len();
            input.selection_reversed = true;

            // `1..3` is the UTF-16 range of the emoji within the new marked
            // text, not within the already-present `🙂前缀` prefix. The
            // replaced range was reversed, but the new marked selection is not.
            input.replace_and_mark_text_in_range(None, "候🙂选", Some(1..3), window, cx);

            assert_eq!(input.text(), "🙂前缀候🙂选");
            assert_eq!(
                input.marked_range,
                Some(insertion_point..input.text().len())
            );
            let expected_start = insertion_point + "候".len();
            assert_eq!(
                input.selected_range,
                expected_start..expected_start + "🙂".len()
            );
            assert!(!input.selection_reversed);
            let native_selection = input.selected_text_range(false, window, cx).unwrap();
            assert_eq!(native_selection.range, 5..7);
            assert!(!native_selection.reversed);
            assert!(input.text().is_char_boundary(input.selected_range.start));
            assert!(input.text().is_char_boundary(input.selected_range.end));
        });
    }

    #[test]
    fn marked_text_uses_an_underlined_display_run_and_unmarks_cleanly() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(420.0), px(44.0)),
                })),
                ..Default::default()
            },
            |_, cx| PromptInput::inline_other(ThemeMode::Light, "请输入其他答案", false, cx),
        );

        window.update(|input, window, cx| {
            input.set_text("前缀尾", cx);
            let insertion_point = "前缀".len();
            input.selected_range = insertion_point..insertion_point;
            input.replace_and_mark_text_in_range(None, "候选", None, window, cx);

            let marked_display_range = input
                .marked_display_range()
                .expect("non-empty composition has a display range");
            assert_eq!(
                marked_display_range,
                insertion_point..input.text().len() - "尾".len()
            );

            let runs = text_runs_with_marked_underline(
                TextRun {
                    len: input.text().len(),
                    color: rgba(0x1a1c1fff).into(),
                    ..TextRun::default()
                },
                Some(marked_display_range),
            );
            assert_eq!(
                runs.iter().map(|run| run.len).collect::<Vec<_>>(),
                vec![6, 6, 3]
            );
            assert!(runs[0].underline.is_none());
            let underline = runs[1]
                .underline
                .as_ref()
                .expect("the composing run is underlined");
            assert_eq!(underline.thickness, px(1.0));
            assert!(!underline.wavy);
            assert!(runs[2].underline.is_none());

            let content_before_unmark = input.text().to_owned();
            input.unmark_text(window, cx);
            assert_eq!(input.text(), content_before_unmark);
            assert_eq!(input.marked_range, None);
            assert_eq!(input.marked_display_range(), None);
        });
    }

    #[test]
    fn committed_replacement_resets_reversed_selection_to_forward_caret() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(420.0), px(44.0)),
                })),
                ..Default::default()
            },
            |_, cx| PromptInput::inline_other(ThemeMode::Light, "请输入其他答案", false, cx),
        );

        window.update(|input, window, cx| {
            input.set_text("甲🙂乙", cx);
            input.selected_range = "甲".len().."甲🙂".len();
            input.selection_reversed = true;

            input.replace_text_in_range(None, "新", window, cx);

            assert_eq!(input.text(), "甲新乙");
            let cursor = "甲新".len();
            assert_eq!(input.selected_range, cursor..cursor);
            assert!(!input.selection_reversed);
            let native_selection = input.selected_text_range(false, window, cx).unwrap();
            assert_eq!(native_selection.range, 2..2);
            assert!(!native_selection.reversed);
        });
    }
}
