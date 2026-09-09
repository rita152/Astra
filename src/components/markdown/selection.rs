//! Native selection for rendered prose; uses the exact shaped text layout.
use gpui::{
    App, Bounds, ClipboardItem, DispatchPhase, Element, ElementId, GlobalElementId, Hitbox,
    HitboxBehavior, InspectorElementId, IntoElement, KeyDownEvent, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, StyledText, TextLayout, Window, div,
    fill, point, prelude::*, px, rgba, size,
};
use std::{cell::RefCell, rc::Rc};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Default)]
struct Selection {
    anchor: usize,
    cursor: usize,
    dragging: bool,
}
impl Selection {
    fn range(&self) -> std::ops::Range<usize> {
        self.anchor.min(self.cursor)..self.anchor.max(self.cursor)
    }
}

pub(super) fn selectable(id: u64, text: StyledText) -> impl IntoElement {
    div()
        .id(("selectable-prose", id))
        .min_w(px(0.0))
        .focusable()
        .tab_stop(false)
        .cursor_text()
        .child(SelectableText { id, text })
}
#[derive(Clone)]
struct NativeSelection {
    selection: Rc<RefCell<Selection>>,
    focus: gpui::FocusHandle,
}
struct SelectableText {
    id: u64,
    text: StyledText,
}
impl IntoElement for SelectableText {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for SelectableText {
    type RequestLayoutState = ();
    type PrepaintState = (Hitbox, NativeSelection);
    fn id(&self) -> Option<ElementId> {
        Some(ElementId::Integer(self.id))
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        self.text.request_layout(None, inspector, window, cx)
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let native = window.with_element_state::<NativeSelection, _>(id.unwrap(), |state, _| {
            let state = state.unwrap_or_else(|| NativeSelection {
                selection: Default::default(),
                focus: cx.focus_handle(),
            });
            (state.clone(), state)
        });
        window.set_focus_handle(&native.focus, cx);
        self.text
            .prepaint(None, inspector, bounds, state, window, cx);
        (window.insert_hitbox(bounds, HitboxBehavior::Normal), native)
    }
    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut (),
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let layout = self.text.layout().clone();
        let (hitbox, native) = prepaint;
        let selection = native.selection.clone();
        let focus = native.focus.clone();
        let text = layout.text();
        let range = selection.borrow().range();
        if range.end <= text.len() && range.start < range.end {
            paint_selection(&layout, range, window);
        }
        self.text
            .paint(None, inspector, bounds, state, &mut (), window, cx);
        let selection_down = selection.clone();
        let layout_down = layout.clone();
        let hit = hitbox.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble
                && event.button == MouseButton::Left
                && hit.is_hovered(window)
            {
                let ix = layout_down
                    .index_for_position(event.position)
                    .unwrap_or_else(|i| i)
                    .min(layout_down.len());
                let mut s = selection_down.borrow_mut();
                if event.click_count >= 2 {
                    let text = layout_down.text();
                    let range = word_range(&text, ix);
                    s.anchor = range.start;
                    s.cursor = range.end;
                } else {
                    if !event.modifiers.shift {
                        s.anchor = ix;
                    }
                    s.cursor = ix;
                }
                s.dragging = true;
                focus.focus(window, cx);
                window.prevent_default();
                window.refresh();
            }
        });
        let selection_move = selection.clone();
        let layout_move = layout.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble
                && event.pressed_button == Some(MouseButton::Left)
                && selection_move.borrow().dragging
            {
                selection_move.borrow_mut().cursor = layout_move
                    .index_for_position(event.position)
                    .unwrap_or_else(|i| i)
                    .min(layout_move.len());
                window.refresh();
                cx.stop_propagation();
            }
        });
        let selection_up = selection.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble
                && event.button == MouseButton::Left
                && selection_up.borrow().dragging
            {
                let mut s = selection_up.borrow_mut();
                s.dragging = false;
                if !s.range().is_empty() {
                    cx.stop_propagation();
                }
                window.refresh();
            }
        });
        window.on_key_event(move |event: &KeyDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            let mut s = selection.borrow_mut();
            let text = layout.text();
            match event.keystroke.key.as_str() {
                "c" if event.keystroke.modifiers.platform => {
                    if let Some(text) = text.get(s.range()).filter(|v| !v.is_empty()) {
                        cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
                        cx.stop_propagation();
                    }
                }
                "a" if event.keystroke.modifiers.platform => {
                    s.anchor = 0;
                    s.cursor = text.len();
                    cx.stop_propagation();
                    window.refresh();
                }
                "escape" => {
                    s.anchor = s.cursor;
                    cx.stop_propagation();
                    window.refresh();
                }
                "left" | "right" if event.keystroke.modifiers.shift => {
                    s.cursor = next_boundary(&text, s.cursor, event.keystroke.key == "right");
                    cx.stop_propagation();
                    window.refresh();
                }
                _ => {}
            }
        });
    }
}
fn word_range(text: &str, index: usize) -> std::ops::Range<usize> {
    text.split_word_bound_indices()
        .find_map(|(start, part)| {
            (index >= start && index < start + part.len()).then_some(start..start + part.len())
        })
        .unwrap_or(index..index)
}
fn next_boundary(text: &str, index: usize, forward: bool) -> usize {
    if forward {
        text.grapheme_indices(true)
            .map(|(i, _)| i)
            .find(|i| *i > index)
            .unwrap_or(text.len())
    } else {
        text.grapheme_indices(true)
            .map(|(i, _)| i)
            .take_while(|i| *i < index)
            .last()
            .unwrap_or(0)
    }
}
fn paint_selection(layout: &TextLayout, range: std::ops::Range<usize>, window: &mut Window) {
    let Some(start) = layout.position_for_index(range.start) else {
        return;
    };
    let Some(end) = layout.position_for_index(range.end) else {
        return;
    };
    let bounds = layout.bounds();
    let h = layout.line_height();
    let mut y = start.y;
    while y <= end.y {
        let left = if y == start.y { start.x } else { bounds.left() };
        let right = if y == end.y { end.x } else { bounds.right() };
        if right > left {
            window.paint_quad(fill(
                Bounds::new(point(left, y), size(right - left, h)),
                rgba(0x3b82f655),
            ));
        }
        y += h;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_boundaries_keep_combining_characters_and_emoji_intact() {
        let text = "á👩‍🚀中";
        assert_eq!(next_boundary(text, 0, true), "á".len());
        assert_eq!(next_boundary(text, "á".len(), true), "á👩‍🚀".len());
        assert_eq!(next_boundary(text, text.len(), false), "á👩‍🚀".len());
        assert_eq!(word_range("read plan", 6), 5..9);
    }
    #[test]
    fn rendered_prose_drag_copy_and_shift_selection_use_shaped_indices() {
        use gpui::{Context, KeyDownEvent, Keystroke, Render, TestApp, WindowOptions};
        struct Probe;
        impl Render for Probe {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div()
                    .w(px(300.0))
                    .text_size(px(20.0))
                    .line_height(px(28.0))
                    .child(selectable(1, StyledText::new("á👩‍🚀中")))
            }
        }
        let mut app = TestApp::new();
        let mut view = app.open_window_with_options(WindowOptions::default(), |_, _| Probe);
        view.draw();
        view.simulate_mouse_move(point(px(1.0), px(10.0)));
        view.simulate_event(MouseDownEvent {
            position: point(px(1.0), px(10.0)),
            button: MouseButton::Left,
            modifiers: Default::default(),
            click_count: 1,
            first_mouse: false,
        });
        view.draw();
        view.simulate_event(MouseMoveEvent {
            position: point(px(299.0), px(10.0)),
            pressed_button: Some(MouseButton::Left),
            modifiers: Default::default(),
        });
        view.draw();
        view.simulate_event(MouseUpEvent {
            position: point(px(299.0), px(10.0)),
            button: MouseButton::Left,
            modifiers: Default::default(),
            click_count: 1,
        });
        view.draw();
        view.simulate_event(KeyDownEvent {
            keystroke: Keystroke::parse("cmd-c").unwrap(),
            is_held: false,
            prefer_character_input: false,
        });
        assert_eq!(
            app.read_from_clipboard().and_then(|s| s.text()),
            Some("á👩‍🚀中".to_owned())
        );
        view.simulate_keystroke("shift-left");
        view.simulate_keystroke("cmd-c");
        assert_eq!(
            app.read_from_clipboard().and_then(|s| s.text()),
            Some("á👩‍🚀".to_owned())
        );
    }
}
