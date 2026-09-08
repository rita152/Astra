//! Bottom panel behavior and presentation for the application shell.

use gpui::{
    BoxShadow, Context, Div, KeyDownEvent, MouseButton, Window, deferred, div, prelude::*, px, rgba,
};

use super::{BOTTOM_PANEL_HEIGHT, BOTTOM_PANEL_ITEMS, ChatApp, state::BottomPanelMode};
use crate::{components::icons::icon, theme::Theme};

impl ChatApp {
    pub fn open_bottom_panel(&mut self, cx: &mut Context<Self>) {
        self.bottom_panel.open = true;
        // The real desktop app restores the active terminal tab when the
        // titlebar toggle is used. This clone has no process/session model, so
        // reuse its existing terminal mode without inventing a path or title.
        if self.bottom_panel.tabs.is_empty() {
            self.bottom_panel.tabs.push(BottomPanelMode::Terminal);
            self.bottom_panel.active_tab = Some(0);
        }
        self.bottom_panel.add_menu_open = false;
        self.bottom_panel.keyboard_focus = false;
        self.bottom_panel.focus_pending = false;
        cx.notify();
    }
    pub(super) fn close_bottom_panel(&mut self, cx: &mut Context<Self>) {
        if self.bottom_panel.open {
            self.bottom_panel.open = false;
            self.bottom_panel.add_menu_open = false;
            self.bottom_panel.keyboard_focus = false;
            self.bottom_panel.focus_pending = false;
            cx.notify();
        }
    }
    pub(super) fn toggle_bottom_panel(&mut self, cx: &mut Context<Self>) {
        if self.bottom_panel.open {
            self.close_bottom_panel(cx);
        } else {
            self.open_bottom_panel(cx);
        }
    }
    pub(super) fn close_bottom_panel_menu(&mut self, cx: &mut Context<Self>) {
        if self.bottom_panel.add_menu_open {
            self.bottom_panel.add_menu_open = false;
            self.bottom_panel.keyboard_focus = false;
            self.bottom_panel.focus_pending = false;
            cx.notify();
        }
    }
    pub(super) fn toggle_bottom_panel_menu(&mut self, cx: &mut Context<Self>) {
        self.bottom_panel.add_menu_open = !self.bottom_panel.add_menu_open;
        self.bottom_panel.focused_item = 0;
        self.bottom_panel.keyboard_focus = false;
        self.bottom_panel.focus_pending = self.bottom_panel.add_menu_open;
        cx.notify();
    }
    pub fn open_bottom_panel_menu(&mut self, cx: &mut Context<Self>) {
        self.open_bottom_panel(cx);
        self.bottom_panel.add_menu_open = true;
        self.bottom_panel.focused_item = 0;
        self.bottom_panel.keyboard_focus = false;
        self.bottom_panel.focus_pending = true;
        cx.notify();
    }
    pub(super) fn select_bottom_panel_item(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((mode, _, _, _)) = BOTTOM_PANEL_ITEMS.get(index) else {
            return;
        };
        if *mode == BottomPanelMode::Files {
            self.close_bottom_panel_menu(cx);
            self.open_files(cx);
            return;
        }
        if *mode == BottomPanelMode::Review {
            self.close_bottom_panel_menu(cx);
            self.right_panel.open = true;
            self.select_right_panel_item(4, cx);
            return;
        }
        self.bottom_panel.tabs.push(*mode);
        self.bottom_panel.active_tab = Some(self.bottom_panel.tabs.len() - 1);
        self.bottom_panel.hovered_tab = None;
        self.bottom_panel.add_menu_open = false;
        self.bottom_panel.keyboard_focus = false;
        self.bottom_panel.focus_pending = false;
        cx.notify();
    }
    pub(super) fn activate_bottom_panel_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.bottom_panel.tabs.len() {
            self.bottom_panel.active_tab = Some(index);
            self.close_bottom_panel_menu(cx);
            cx.notify();
        }
    }
    pub(super) fn close_bottom_panel_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.bottom_panel.tabs.len() {
            return;
        }
        self.bottom_panel.tabs.remove(index);
        self.bottom_panel.hovered_tab = None;
        self.bottom_panel.active_tab = match self.bottom_panel.active_tab {
            None => None,
            Some(_) if self.bottom_panel.tabs.is_empty() => None,
            Some(active) if active == index => Some(index.min(self.bottom_panel.tabs.len() - 1)),
            Some(active) if active > index => Some(active - 1),
            Some(active) => Some(active),
        };
        self.close_bottom_panel_menu(cx);
        cx.notify();
    }
    pub(super) fn handle_bottom_panel_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.bottom_panel.add_menu_open {
            return;
        }
        match event.keystroke.key.as_str() {
            "down" => {
                self.bottom_panel.focused_item = if self.bottom_panel.keyboard_focus {
                    (self.bottom_panel.focused_item + 1) % BOTTOM_PANEL_ITEMS.len()
                } else {
                    0
                };
                self.bottom_panel.keyboard_focus = true;
            }
            "up" => {
                self.bottom_panel.focused_item = if self.bottom_panel.keyboard_focus {
                    (self.bottom_panel.focused_item + BOTTOM_PANEL_ITEMS.len() - 1)
                        % BOTTOM_PANEL_ITEMS.len()
                } else {
                    BOTTOM_PANEL_ITEMS.len() - 1
                };
                self.bottom_panel.keyboard_focus = true;
            }
            // Radix keeps focus inside the open dropdown and ignores Tab.
            "tab" => {
                cx.stop_propagation();
                return;
            }
            "home" => {
                self.bottom_panel.focused_item = 0;
                self.bottom_panel.keyboard_focus = true;
            }
            "end" => {
                self.bottom_panel.focused_item = BOTTOM_PANEL_ITEMS.len() - 1;
                self.bottom_panel.keyboard_focus = true;
            }
            "enter" | "space" => {
                self.select_bottom_panel_item(self.bottom_panel.focused_item, cx);
            }
            "escape" => self.close_bottom_panel_menu(cx),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
    pub(super) fn bottom_panel_menu_item(
        &self,
        index: usize,
        label: &'static str,
        shortcut: &'static str,
        glyph: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let focused = self.bottom_panel.keyboard_focus && self.bottom_panel.focused_item == index;
        div()
            .id(("bottom-panel-menu-item", index))
            .w_full()
            .h(px(28.5625))
            .px(px(8.0))
            .py(px(5.0))
            .rounded(px(12.5))
            .when(focused, |item| item.bg(theme.sidebar_hover))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(13.0))
            .line_height(px(18.5714))
            .font_weight(gpui::FontWeight::NORMAL)
            .text_color(theme.text)
            .cursor_pointer()
            .hover(move |style| style.bg(theme.sidebar_hover))
            .active(move |style| style.bg(theme.sidebar_hover))
            .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                if *hovered {
                    this.bottom_panel.focused_item = index;
                    this.bottom_panel.keyboard_focus = false;
                    this.bottom_panel.focus.focus(window, cx);
                    cx.notify();
                }
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.select_bottom_panel_item(index, cx);
            }))
            .child(
                icon(glyph, theme.text.alpha(0.75).into())
                    .size(px(16.0))
                    .flex_none(),
            )
            .child(div().min_w(px(0.0)).flex_1().truncate().child(label))
            .child(
                div()
                    .ml(px(8.0))
                    .flex_none()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(theme.text.alpha(0.65))
                    .child(shortcut),
            )
    }
    pub(super) fn bottom_panel_add_menu(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        div()
            .id("bottom-panel-add-menu")
            .absolute()
            .top(px(30.0))
            .left(px(2.0))
            .w(px(280.0))
            .h(px(150.8125))
            .p(px(4.0))
            .rounded(px(15.0))
            .bg(theme.model_picker_surface)
            .border(px(0.5))
            .border_color(theme.border)
            .shadow(vec![
                BoxShadow::new(px(0.0), px(8.0), rgba(0x0000001f).into())
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .track_focus(&self.bottom_panel.focus)
            .on_key_down(cx.listener(Self::handle_bottom_panel_key))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .flex()
            .flex_col()
            .children(BOTTOM_PANEL_ITEMS.iter().enumerate().map(
                |(index, (_, label, shortcut, glyph))| {
                    self.bottom_panel_menu_item(index, label, shortcut, glyph, theme, cx)
                },
            ))
    }
    pub(super) fn bottom_panel_tab(
        &self,
        index: usize,
        mode: BottomPanelMode,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let active = self.bottom_panel.active_tab == Some(index);
        let hovered = self.bottom_panel.hovered_tab == Some(index);
        let (label, glyph) = bottom_panel_tab_spec(mode);
        div()
            .id(("bottom-panel-tab-wrapper", index))
            .w(px(160.0))
            .h(px(28.0))
            .flex_none()
            .child(
                div()
                    .id(("bottom-panel-tab", index))
                    .h(px(28.0))
                    .w(px(156.0))
                    .min_w(px(0.0))
                    .px(px(8.0))
                    .py(px(4.0))
                    .rounded(px(12.5))
                    .when(active, |tab| tab.bg(theme.text.alpha(0.05)))
                    .hover(move |style| style.bg(theme.text.alpha(0.05)))
                    .active(move |style| style.bg(theme.text.alpha(0.05)))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .on_hover(cx.listener(move |this, is_hovered: &bool, _, cx| {
                        let next = is_hovered.then_some(index);
                        if this.bottom_panel.hovered_tab != next {
                            this.bottom_panel.hovered_tab = next;
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.activate_bottom_panel_tab(index, cx);
                    }))
                    .child(
                        icon(
                            glyph,
                            if active {
                                theme.text.into()
                            } else {
                                theme.text_secondary.into()
                            },
                        )
                        .size(px(16.0))
                        .flex_none(),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .truncate()
                            .text_size(px(13.0))
                            .line_height(px(18.5714))
                            .text_color(if active {
                                theme.text
                            } else {
                                theme.text_secondary
                            })
                            .child(label),
                    )
                    .child(
                        div()
                            .id(("bottom-panel-close-tab", index))
                            .size(px(20.0))
                            .flex_none()
                            .rounded(px(5.0))
                            .when(!active && !hovered, |close| close.opacity(0.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.sidebar_hover))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.close_bottom_panel_tab(index, cx);
                            }))
                            .child(icon("close-dialog", theme.text_tertiary.into()).size(px(12.0))),
                    ),
            )
    }
    pub(super) fn bottom_panel(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let toolbar = div()
            .id("bottom-panel-toolbar")
            .h(px(40.0))
            .w_full()
            .flex_none()
            .px(px(8.0))
            .flex()
            .items_center()
            .child(
                div()
                    .id("bottom-panel-tabs")
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .gap(px(3.0))
                    .children(
                        self.bottom_panel
                            .tabs
                            .iter()
                            .copied()
                            .enumerate()
                            .map(|(index, mode)| self.bottom_panel_tab(index, mode, theme, cx)),
                    ),
            )
            .child(
                div()
                    .id("bottom-panel-add-anchor")
                    .relative()
                    .size(px(28.0))
                    .flex_none()
                    .child(
                        div()
                            .id("bottom-panel-add")
                            .size(px(28.0))
                            .rounded(px(10.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .when(self.bottom_panel.add_menu_open, |button| {
                                button.bg(theme.sidebar_hover)
                            })
                            .hover(move |style| style.bg(theme.sidebar_hover))
                            .active(move |style| style.bg(theme.sidebar_hover))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.toggle_bottom_panel_menu(cx);
                            }))
                            .child(icon("add", theme.text_tertiary.into()).size(px(16.0))),
                    )
                    .when(self.bottom_panel.add_menu_open, |anchor| {
                        anchor.child(deferred(self.bottom_panel_add_menu(theme, cx)))
                    }),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("bottom-panel-close")
                    .size(px(28.0))
                    .flex_none()
                    .rounded(px(10.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.sidebar_hover))
                    .active(move |style| style.bg(theme.sidebar_hover))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.close_bottom_panel(cx);
                    }))
                    .child(icon("close-dialog", theme.text_tertiary.into()).size(px(16.0))),
            );

        div()
            .id("bottom-panel")
            .h(px(BOTTOM_PANEL_HEIGHT))
            .min_h(px(BOTTOM_PANEL_HEIGHT))
            .w_full()
            .flex_none()
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .flex()
            .flex_col()
            .child(toolbar)
            .child(div().min_h(px(0.0)).flex_1().bg(theme.surface))
    }
}

pub(super) fn bottom_panel_tab_spec(mode: BottomPanelMode) -> (&'static str, &'static str) {
    match mode {
        BottomPanelMode::Review => ("审查", "panel-review"),
        BottomPanelMode::Terminal => ("终端", "panel-terminal"),
        // CDP: a browser item appended from the add menu is titled 新标签页.
        BottomPanelMode::Browser => ("新标签页", "panel-browser"),
        BottomPanelMode::Files => ("文件", "panel-files"),
        BottomPanelMode::SideChat => ("侧边聊天", "side-chat"),
    }
}
