//! Right panel behavior and presentation for the application shell.

use gpui::{
    BoxShadow, Context, Div, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Role, StyleRefinement, Window, canvas, div, prelude::*, px, rgba,
};

use super::{
    ChatApp, RIGHT_PANEL_ITEMS, RIGHT_PANEL_MAIN_MIN_WIDTH, RIGHT_PANEL_MIN_WIDTH,
    SUBAGENT_PANEL_DEFAULT_WIDTH, SUBAGENT_PANEL_HEADER_HEIGHT,
    project_creation::project_creation_focus_shadow,
    render::panel_resize_handle,
    state::{RightPanelMode, SubagentPanel},
};
use crate::{
    components::{
        file_change::{DiffReviewCallback, render_diff_review_panel},
        home::{
            HomeView, OpenDiffReview, OpenImagePreview, OpenSubAgentPanel, RetryImageGeneration,
        },
        icons::icon,
    },
    media::read_image_dimensions,
    theme::Theme,
};

impl ChatApp {
    pub(super) fn open_subagent_panel(&mut self, event: OpenSubAgentPanel, cx: &mut Context<Self>) {
        let composer = self.ensure_thread_conversation(event.thread_id.clone(), cx);
        let panel_home = cx.new(|cx| HomeView::new_subagent(self.mode, composer.clone(), cx));

        cx.subscribe(&panel_home, |this, _, nested: &OpenSubAgentPanel, cx| {
            this.open_subagent_panel(nested.clone(), cx)
        })
        .detach();
        cx.subscribe(&panel_home, |this, _, preview: &OpenImagePreview, cx| {
            this.image_preview.path = Some(preview.0.clone());
            this.image_preview.dimensions = read_image_dimensions(&preview.0).ok().flatten();
            this.image_preview.zoom = 1.0;
            cx.notify();
        })
        .detach();
        cx.subscribe(&panel_home, |this, _, review: &OpenDiffReview, cx| {
            this.open_diff_review(review.0.clone(), cx)
        })
        .detach();
        cx.subscribe(&panel_home, |this, _, _: &RetryImageGeneration, cx| {
            if let Some(panel) = &this.right_panel.subagent {
                let composer = panel.home.read(cx).composer_entity();
                composer.update(cx, |composer, cx| composer.retry_image_generation(cx));
            }
        })
        .detach();

        self.right_panel.subagent = Some(SubagentPanel {
            thread_id: event.thread_id,
            name: event.name,
            home: panel_home,
        });
        self.right_panel.subagent_menu_open = false;
        self.right_panel.open = true;
        self.right_panel.mode = None;
        self.right_panel.diff_review = None;
        if self.right_panel.width.is_none() {
            self.right_panel.width = Some(SUBAGENT_PANEL_DEFAULT_WIDTH);
        }
        self.right_panel.keyboard_focus = false;
        self.right_panel.focus_pending = true;
        cx.notify();
    }
    pub fn open_right_panel(&mut self, cx: &mut Context<Self>) {
        self.right_panel.open = true;
        if self.right_panel.mode == Some(RightPanelMode::SideChat) {
            self.ensure_side_chat(false, cx);
            cx.notify();
            return;
        }
        if self.right_panel.mode == Some(RightPanelMode::Review) {
            self.ensure_review(cx);
            cx.notify();
            return;
        }
        if self.right_panel.mode == Some(RightPanelMode::Files) {
            self.ensure_files(cx);
            cx.notify();
            return;
        }
        if self.right_panel.mode == Some(RightPanelMode::Terminal) {
            self.ensure_terminal(cx);
            cx.notify();
            return;
        }
        self.right_panel.mode = None;
        self.right_panel.subagent = None;
        self.right_panel.subagent_menu_open = false;
        self.right_panel.diff_review = None;
        self.right_panel.focused_item = 0;
        self.right_panel.keyboard_focus = false;
        self.right_panel.focus_pending = true;
        cx.notify();
    }
    pub(super) fn close_right_panel(&mut self, cx: &mut Context<Self>) {
        if self.right_panel.open {
            self.deactivate_side_chat(cx);
            self.deactivate_review(cx);
            self.right_panel.open = false;
            self.right_panel.fullscreen = false;
            self.terminal_return_focus_pending = matches!(
                self.right_panel.mode,
                Some(
                    RightPanelMode::Terminal
                        | RightPanelMode::Files
                        | RightPanelMode::Review
                        | RightPanelMode::SideChat
                )
            );
            if !matches!(
                self.right_panel.mode,
                Some(
                    RightPanelMode::Terminal
                        | RightPanelMode::Files
                        | RightPanelMode::Review
                        | RightPanelMode::SideChat
                )
            ) {
                self.right_panel.mode = None;
            }
            self.right_panel.subagent = None;
            self.right_panel.subagent_menu_open = false;
            self.right_panel.diff_review = None;
            self.right_panel.keyboard_focus = false;
            self.right_panel.focus_pending = false;
            self.right_panel.resize_hovered = false;
            self.right_panel.resize_dragging = false;
            cx.notify();
        }
    }
    pub(super) fn toggle_right_panel(&mut self, cx: &mut Context<Self>) {
        if self.right_panel.open {
            self.close_right_panel(cx);
        } else {
            self.open_right_panel(cx);
        }
    }
    pub(super) fn select_right_panel_item(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((mode, _, _, _)) = RIGHT_PANEL_ITEMS.get(index) else {
            return;
        };
        let mode = *mode;
        if mode != RightPanelMode::SideChat {
            self.deactivate_side_chat(cx);
        }
        if mode != RightPanelMode::Review {
            self.deactivate_review(cx);
            self.right_panel.fullscreen = false;
        }
        self.right_panel.mode = Some(mode);
        if mode == RightPanelMode::SideChat {
            self.ensure_side_chat(true, cx);
        }
        if mode == RightPanelMode::Files {
            self.ensure_files(cx);
        }
        if mode == RightPanelMode::Terminal {
            self.ensure_terminal(cx);
        }
        if mode == RightPanelMode::Review {
            self.ensure_review(cx);
        }
        self.right_panel.subagent = None;
        self.right_panel.subagent_menu_open = false;
        self.right_panel.diff_review = None;
        self.right_panel.keyboard_focus = false;
        cx.notify();
    }
    pub(super) fn handle_right_panel_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.right_panel.open {
            return;
        }
        if self.right_panel.subagent.is_some() {
            match event.keystroke.key.as_str() {
                "escape" => {
                    if self.right_panel.subagent_menu_open {
                        self.right_panel.subagent_menu_open = false;
                        cx.notify();
                    } else {
                        self.close_right_panel(cx);
                    }
                    cx.stop_propagation();
                }
                _ => return,
            }
            return;
        }
        match event.keystroke.key.as_str() {
            "down" | "tab" if !event.keystroke.modifiers.shift => {
                self.right_panel.focused_item = if self.right_panel.keyboard_focus {
                    (self.right_panel.focused_item + 1) % RIGHT_PANEL_ITEMS.len()
                } else {
                    0
                };
                self.right_panel.keyboard_focus = true;
            }
            "up" => {
                self.right_panel.focused_item = if self.right_panel.keyboard_focus {
                    (self.right_panel.focused_item + RIGHT_PANEL_ITEMS.len() - 1)
                        % RIGHT_PANEL_ITEMS.len()
                } else {
                    RIGHT_PANEL_ITEMS.len() - 1
                };
                self.right_panel.keyboard_focus = true;
            }
            "tab" => {
                self.right_panel.focused_item = if self.right_panel.keyboard_focus {
                    (self.right_panel.focused_item + RIGHT_PANEL_ITEMS.len() - 1)
                        % RIGHT_PANEL_ITEMS.len()
                } else {
                    RIGHT_PANEL_ITEMS.len() - 1
                };
                self.right_panel.keyboard_focus = true;
            }
            "home" => {
                self.right_panel.focused_item = 0;
                self.right_panel.keyboard_focus = true;
            }
            "end" => {
                self.right_panel.focused_item = RIGHT_PANEL_ITEMS.len() - 1;
                self.right_panel.keyboard_focus = true;
            }
            "enter" | "space" if self.right_panel.mode.is_none() => {
                self.select_right_panel_item(self.right_panel.focused_item, cx);
            }
            // The docked panel is persistent. Escape belongs to the active
            // conversation/tool and must not hide the panel.
            "escape" => return,
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
    pub(super) fn right_panel_menu_item(
        &self,
        index: usize,
        label: &'static str,
        shortcut: &'static str,
        glyph: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let focused = self.right_panel.keyboard_focus && self.right_panel.focused_item == index;
        div()
            .id(("right-panel-menu-item", index))
            .role(Role::Button)
            .aria_label(label)
            .w_full()
            .h(px(40.0))
            .px(px(10.0))
            .py(px(8.0))
            .rounded(px(10.0))
            .bg(theme.text.alpha(0.03))
            .shadow(project_creation_focus_shadow(theme, focused))
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .hover(move |style| style.bg(theme.text.alpha(0.08)))
            .active(move |style| style.bg(theme.text.alpha(0.08)))
            .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                if *hovered {
                    this.right_panel.focused_item = index;
                    this.right_panel.keyboard_focus = false;
                    this.right_panel.focus.focus(window, cx);
                    cx.notify();
                }
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.select_right_panel_item(index, cx);
            }))
            .child(
                icon(glyph, theme.text.alpha(0.65).into())
                    .size(px(16.0))
                    .flex_none(),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .text_size(px(13.0))
                    .line_height(px(18.5714))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .text_color(theme.text)
                    .child(label),
            )
            .child(
                div()
                    .flex_none()
                    .h(px(16.0))
                    .px(px(6.0))
                    .py(px(2.0))
                    .rounded(px(10.0))
                    .bg(theme.text.alpha(0.065))
                    .text_size(px(12.0))
                    .line_height(px(12.0))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .text_color(theme.text.alpha(0.65))
                    .flex()
                    .items_center()
                    .child(shortcut),
            )
    }
    pub(super) fn right_panel_resize_handle(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let entity = cx.entity();
        let line_visible = self.right_panel.resize_hovered || self.right_panel.resize_dragging;
        let input_layer = canvas(
            |bounds, _, _| bounds,
            move |bounds, _, window, _| {
                let mouse_down_entity = entity.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, _, window, cx| {
                    if event.button != MouseButton::Left || !bounds.contains(&event.position) {
                        return;
                    }
                    mouse_down_entity.update(cx, |this, cx| {
                        let divider_x = f32::from(bounds.origin.x) + 8.0;
                        let current_width = f32::from(window.viewport_size().width) - divider_x;
                        this.right_panel.resize_dragging = true;
                        this.right_panel.resize_hovered = true;
                        this.right_panel.resize_pointer_offset =
                            divider_x - f32::from(event.position.x);
                        // Resolve the responsive default to a persisted width as
                        // soon as the user starts dragging it.
                        if this.right_panel.width.is_none() {
                            this.right_panel.width = Some(current_width);
                        }
                        cx.notify();
                    });
                });

                let mouse_move_entity = entity.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, _, window, cx| {
                    let pointer_inside = bounds.contains(&event.position);
                    mouse_move_entity.update(cx, |this, cx| {
                        let mut changed = false;
                        if this.right_panel.resize_dragging {
                            let viewport_width = f32::from(window.viewport_size().width);
                            let revealed_sidebar_width =
                                this.sidebar.read(cx).width() * this.sidebar_layout.reveal;
                            let divider_x = f32::from(event.position.x)
                                + this.right_panel.resize_pointer_offset;
                            let next_width = clamp_right_panel_width(
                                viewport_width - divider_x,
                                viewport_width,
                                revealed_sidebar_width,
                            );
                            if this
                                .right_panel
                                .width
                                .is_none_or(|width| (width - next_width).abs() > f32::EPSILON)
                            {
                                this.right_panel.width = Some(next_width);
                                changed = true;
                            }
                        }
                        let next_hovered = pointer_inside || this.right_panel.resize_dragging;
                        if this.right_panel.resize_hovered != next_hovered {
                            this.right_panel.resize_hovered = next_hovered;
                            changed = true;
                        }
                        if changed {
                            cx.notify();
                        }
                    });
                });

                let mouse_up_entity = entity.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, _, _, cx| {
                    if event.button != MouseButton::Left {
                        return;
                    }
                    mouse_up_entity.update(cx, |this, cx| {
                        if !this.right_panel.resize_dragging {
                            return;
                        }
                        this.right_panel.resize_dragging = false;
                        this.right_panel.resize_hovered = bounds.contains(&event.position);
                        cx.notify();
                    });
                });
            },
        )
        .absolute()
        .inset_0();

        panel_resize_handle(
            "right-panel-resize-handle",
            -8.0,
            line_visible,
            theme,
            input_layer,
        )
    }
    pub(super) fn subagent_right_panel(
        &self,
        panel: SubagentPanel,
        panel_width: gpui::Pixels,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let menu_open = self.right_panel.subagent_menu_open;
        let tab_trigger = div()
            .id("subagent-panel-tab-trigger")
            .h_full()
            .min_w(px(0.0))
            .flex_1()
            .flex()
            .items_center()
            .gap(px(8.0))
            .focusable()
            .tab_stop(true)
            .role(Role::Button)
            .aria_expanded(menu_open)
            .aria_label(if menu_open {
                "关闭面板信息下拉框"
            } else {
                "打开面板信息下拉框"
            })
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
                this.right_panel.subagent_menu_open = !this.right_panel.subagent_menu_open;
                cx.stop_propagation();
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.right_panel.subagent_menu_open = !this.right_panel.subagent_menu_open;
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .child(icon("settings-agent", theme.text.into()).size(px(16.0)))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .text_size(px(13.0))
                    .line_height(px(18.5714))
                    .text_color(theme.text)
                    .child("子智能体"),
            )
            .child(
                icon("chevron-down", theme.text_tertiary.into())
                    .size(px(12.0))
                    .flex_none(),
            );
        let close_button = div()
            .id("subagent-panel-close")
            .size(px(20.0))
            .rounded(px(5.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .focusable()
            .tab_stop(true)
            .role(Role::Button)
            .aria_label("关闭子智能体面板")
            .cursor_pointer()
            .hover(move |style| style.bg(theme.sidebar_hover))
            .active(move |style| style.bg(theme.text.alpha(0.12)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.close_right_panel(cx);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    cx.stop_propagation();
                    this.close_right_panel(cx);
                }
            }))
            .child(icon("close-dialog", theme.text_tertiary.into()).size(px(12.0)));
        let plus_button = div()
            .id("subagent-panel-add-tab")
            .size(px(28.0))
            .rounded(px(8.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .focusable()
            .tab_stop(true)
            .role(Role::Button)
            .aria_label("打开面板选择器")
            .cursor_pointer()
            .hover(move |style| style.bg(theme.sidebar_hover))
            .on_click(cx.listener(|this, _, _, cx| {
                this.right_panel.subagent_menu_open = !this.right_panel.subagent_menu_open;
                cx.stop_propagation();
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.right_panel.subagent_menu_open = !this.right_panel.subagent_menu_open;
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .child(
                div()
                    .text_size(px(21.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(350.0))
                    .text_color(theme.text_tertiary)
                    .child("+"),
            );
        let toolbar = div()
            .id("subagent-panel-toolbar")
            .h(px(46.0))
            .w_full()
            .flex_none()
            .px(px(8.0))
            .bg(theme.surface_under)
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(
                div()
                    .id("subagent-panel-tab")
                    .h(px(28.0))
                    .w(px(156.0))
                    .px(px(8.0))
                    .rounded(px(12.5))
                    .bg(theme.surface)
                    .flex()
                    .items_center()
                    .child(tab_trigger)
                    .child(close_button),
            )
            .child(plus_button);

        let back_button = div()
            .id("subagent-panel-back")
            .size(px(24.0))
            .rounded(px(10.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .focusable()
            .tab_stop(true)
            .role(Role::Button)
            .aria_label("返回子智能体列表")
            .cursor_pointer()
            .hover(move |style| style.bg(theme.sidebar_hover))
            .active(move |style| style.bg(theme.text.alpha(0.12)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.close_right_panel(cx);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    cx.stop_propagation();
                    this.close_right_panel(cx);
                }
            }))
            .child(icon("back", theme.text_tertiary.into()).size(px(16.0)));
        let panel_name = panel.name.clone();
        let panel_label = format!("子智能体 {panel_name}，任务 {}", panel.thread_id);
        let header = div()
            .id("subagent-panel-header")
            .h(px(SUBAGENT_PANEL_HEADER_HEIGHT))
            .w_full()
            .flex_none()
            .px(px(16.0))
            .border_b_1()
            .border_color(theme.command_border)
            .flex()
            .items_center()
            .gap(px(8.0))
            .aria_label(panel_label)
            .child(back_button)
            .child(
                icon("subagent-activity", rgba(0xff7b7fff).into())
                    .size(px(24.0))
                    .flex_none(),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .text_size(px(13.0))
                    .line_height(px(18.5714))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(panel_name.clone()),
            );

        let dropdown = div()
            .id("subagent-panel-menu")
            .absolute()
            .top(px(59.0))
            .left(px(43.0))
            .w(px(240.0))
            .h(px(204.0))
            .p(px(10.0))
            .rounded(px(25.0))
            .bg(theme.elevated)
            .shadow(vec![
                BoxShadow::new(px(0.0), px(0.0), theme.border.into()).spread_radius(px(0.5)),
                BoxShadow::new(px(0.0), px(3.0), rgba(0x0000000a).into()).blur_radius(px(7.5)),
                BoxShadow::new(px(0.0), px(0.0), rgba(0x0000000d).into()).blur_radius(px(20.0)),
            ])
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .px(px(4.0))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .text_color(theme.text_tertiary)
                    .child("环境信息"),
            )
            .child(
                div()
                    .px(px(4.0))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .text_color(theme.text_tertiary)
                    .child("变更"),
            )
            .child(div().h(px(0.5)).mx(px(4.0)).bg(theme.border))
            .child(
                div()
                    .h(px(28.0))
                    .px(px(4.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .text_color(theme.text_tertiary)
                    .child("子智能体")
                    .child("1 完成"),
            )
            .child(
                div()
                    .id("subagent-panel-menu-current")
                    .h(px(40.0))
                    .px(px(8.0))
                    .rounded(px(12.5))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .focusable()
                    .tab_stop(true)
                    .role(Role::Button)
                    .aria_label(format!("子智能体 {panel_name}"))
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.sidebar_hover))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.right_panel.subagent_menu_open = false;
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            this.right_panel.subagent_menu_open = false;
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }))
                    .child(icon("subagent-activity", rgba(0xff7b7fff).into()).size(px(20.0)))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .truncate()
                            .text_size(px(13.0))
                            .line_height(px(18.5714))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(panel_name),
                    ),
            );

        div()
            .id("right-panel")
            .w(panel_width)
            .min_w(panel_width)
            .h_full()
            .flex_none()
            .relative()
            .border_l_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .track_focus(&self.right_panel.focus)
            .on_key_down(cx.listener(Self::handle_right_panel_key))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .flex()
            .flex_col()
            .child(self.right_panel_resize_handle(theme, cx))
            .child(toolbar)
            .child(header)
            .child(
                div()
                    .id("subagent-panel-body")
                    .min_h(px(0.0))
                    .flex_1()
                    .bg(theme.surface)
                    .child(panel.home.cached(StyleRefinement::default().size_full())),
            )
            .when(menu_open, |panel| panel.child(dropdown))
    }
    pub(super) fn right_panel(
        &self,
        panel_width: gpui::Pixels,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        if self.right_panel.mode == Some(RightPanelMode::SideChat)
            && let Some(panel) = self.side_chat_panels.get(&self.active_conversation)
        {
            return div()
                .id("right-panel")
                .w(panel_width)
                .min_w(panel_width)
                .h_full()
                .flex_none()
                .relative()
                .border_l_1()
                .border_color(theme.border)
                .bg(theme.surface)
                .child(panel.clone())
                .child(self.right_panel_resize_handle(theme, cx));
        }
        if self.right_panel.mode == Some(RightPanelMode::Review)
            && let Some(panel) = self.review_panels.get(&self.active_conversation)
        {
            return div()
                .id("right-panel")
                .w(panel_width)
                .min_w(panel_width)
                .h_full()
                .flex_none()
                .relative()
                .border_l_1()
                .border_color(theme.border)
                .bg(theme.surface)
                .child(panel.clone())
                .child(self.right_panel_resize_handle(theme, cx));
        }
        if let Some(panel) = self.right_panel.subagent.clone() {
            return self.subagent_right_panel(panel, panel_width, theme, cx);
        }
        if let Some(review) = self.right_panel.diff_review.clone() {
            let target = cx.entity();
            let callback = DiffReviewCallback::new(move |event, _, cx| {
                target.update(cx, move |app, cx| app.handle_diff_review_event(event, cx));
            });
            return div()
                .id("right-panel")
                .w(panel_width)
                .min_w(panel_width)
                .h_full()
                .flex_none()
                .relative()
                .border_l_1()
                .border_color(theme.border)
                .bg(theme.surface)
                .track_focus(&self.right_panel.focus)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(self.right_panel_resize_handle(theme, cx))
                .child(render_diff_review_panel(&review, theme, callback));
        }

        if self.right_panel.mode == Some(RightPanelMode::Files)
            && let Some(panel) = self.file_panels.get(&self.active_conversation)
        {
            return div()
                .id("right-panel")
                .w(panel_width)
                .min_w(panel_width)
                .h_full()
                .flex_none()
                .relative()
                .border_l_1()
                .border_color(theme.border)
                .bg(theme.surface)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(panel.clone())
                .child(self.right_panel_resize_handle(theme, cx));
        }

        if self.right_panel.mode == Some(RightPanelMode::Terminal)
            && let Some(terminal) = self.terminal_panels.get(&self.active_conversation)
        {
            return div()
                .id("right-panel")
                .w(panel_width)
                .min_w(panel_width)
                .h_full()
                .flex_none()
                .relative()
                .border_l_1()
                .border_color(theme.border)
                .bg(theme.surface)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(terminal.clone())
                .child(self.right_panel_resize_handle(theme, cx));
        }

        let toolbar = div()
            .h(px(46.0))
            .w_full()
            .flex_none()
            .px(px(8.0))
            .flex()
            .items_center()
            .when_some(self.right_panel.mode, |toolbar, mode| {
                let (_, label, _, glyph) = RIGHT_PANEL_ITEMS
                    .iter()
                    .find(|(candidate, _, _, _)| *candidate == mode)
                    .copied()
                    .expect("right panel mode must have a launcher item");
                toolbar.child(
                    div()
                        .id("right-panel-active-tab")
                        .h(px(28.0))
                        .max_w(px(156.0))
                        .px(px(8.0))
                        .rounded(px(10.0))
                        .bg(theme.text.alpha(0.05))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(icon(glyph, theme.text.into()).size(px(16.0)))
                        .child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .text_size(px(13.0))
                                .line_height(px(18.5714))
                                .text_color(theme.text)
                                .child(label),
                        )
                        .child(
                            div()
                                .id("right-panel-close-tab")
                                .size(px(20.0))
                                .rounded(px(5.0))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .hover(move |style| style.bg(theme.sidebar_hover))
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.right_panel.mode = None;
                                    this.right_panel.keyboard_focus = false;
                                    cx.notify();
                                }))
                                .child(
                                    icon("close-dialog", theme.text_tertiary.into()).size(px(12.0)),
                                ),
                        ),
                )
            });

        div()
            .id("right-panel")
            .w(panel_width)
            .min_w(panel_width)
            .h_full()
            .flex_none()
            .relative()
            .border_l_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .track_focus(&self.right_panel.focus)
            .on_key_down(cx.listener(Self::handle_right_panel_key))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .flex()
            .flex_col()
            .child(self.right_panel_resize_handle(theme, cx))
            .child(toolbar)
            .child(
                div()
                    .min_h(px(0.0))
                    .flex_1()
                    .p(px(8.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(self.right_panel.mode.is_none(), |body| {
                        body.child(
                            div()
                                .w_full()
                                .max_w(px(576.0))
                                .px(px(20.0))
                                .flex()
                                .flex_col()
                                .gap(px(4.0))
                                .children(RIGHT_PANEL_ITEMS.iter().enumerate().map(
                                    |(index, (_, label, shortcut, glyph))| {
                                        self.right_panel_menu_item(
                                            index, label, shortcut, glyph, theme, cx,
                                        )
                                    },
                                )),
                        )
                    }),
            )
    }
}

pub(super) fn right_panel_width_limit(viewport_width: f32, revealed_sidebar_width: f32) -> f32 {
    (viewport_width - revealed_sidebar_width - RIGHT_PANEL_MAIN_MIN_WIDTH)
        .max(RIGHT_PANEL_MIN_WIDTH)
}

pub(super) fn clamp_right_panel_width(
    width: f32,
    viewport_width: f32,
    revealed_sidebar_width: f32,
) -> f32 {
    width.clamp(
        RIGHT_PANEL_MIN_WIDTH,
        right_panel_width_limit(viewport_width, revealed_sidebar_width),
    )
}
