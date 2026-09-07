//! Permissions behavior and presentation for the prompt composer.

use gpui::{
    BoxShadow, Context, Div, KeyDownEvent, SharedString, Window, div, hsla, prelude::*, px, rgba,
};

use super::{ComposerView, ConversationChanged, PermissionMode, RequestFullAccessConfirmation};
use crate::{
    agent::AgentEvent,
    components::icons::icon,
    conversation::ConversationActivity,
    theme::{Theme, ThemeMode, ui_font},
};

impl ComposerView {
    pub(super) fn activate_permission_mode(
        &mut self,
        mode: PermissionMode,
        cx: &mut Context<Self>,
    ) {
        if !self.permission_ui_enabled {
            return;
        }
        self.permission_menu_open = false;
        self.permission_menu_keyboard_focus = false;
        if mode == PermissionMode::Full && self.permission_mode != PermissionMode::Full {
            cx.emit(RequestFullAccessConfirmation);
        } else {
            self.request_permission_mode(mode, cx);
        }
        cx.notify();
    }
    pub(super) fn request_permission_mode(&mut self, mode: PermissionMode, cx: &mut Context<Self>) {
        let Some(thread_id) = self.conversation.thread_id.clone() else {
            self.permission_mode = mode;
            self.conversation.permission_error = None;
            return;
        };
        self.permission_update_cycle = self.permission_update_cycle.wrapping_add(1);
        let update_cycle = self.permission_update_cycle;
        let cwd = self.conversation.cwd.clone();
        let receiver = self
            .backend
            .update_thread_permissions(thread_id, cwd, mode.agent_mode());
        cx.spawn(async move |this, cx| {
            let result = receiver
                .recv()
                .await
                .unwrap_or_else(|_| Err("权限设置连接在返回结果前关闭".to_owned()));
            let _ = this.update(cx, |this, cx| {
                if this.permission_update_cycle != update_cycle {
                    return;
                }
                this.apply_permission_update_result(mode, result);
                cx.emit(ConversationChanged);
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn apply_permission_update_result(
        &mut self,
        mode: PermissionMode,
        result: Result<crate::agent::AgentThreadSettings, String>,
    ) {
        match result {
            Ok(settings) => {
                self.permission_mode = mode;
                self.apply_agent_event_batch(vec![AgentEvent::ThreadSettingsUpdated(settings)]);
            }
            Err(error) => {
                let message = format!("无法更新权限模式：{error}");
                self.conversation.permission_error = Some(message.clone());
                self.conversation
                    .activities
                    .push(ConversationActivity::Error { message });
            }
        }
    }
    pub(super) fn handle_permission_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.permission_ui_enabled {
            return;
        }
        let key = event.keystroke.key.as_str();
        if !self.permission_menu_open {
            match key {
                "enter" | "space" | "down" | "up" => {
                    self.menu_open = false;
                    self.submenu = None;
                    self.permission_menu_open = true;
                    self.permission_menu_keyboard_focus = matches!(key, "down" | "up");
                    self.permission_menu_focused_item = if key == "up" { 3 } else { 0 };
                    window.focus(&self.permission_menu_focus, cx);
                }
                _ => return,
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }

        match key {
            "down" => {
                self.permission_menu_focused_item = if self.permission_menu_keyboard_focus {
                    (self.permission_menu_focused_item + 1) % 4
                } else {
                    0
                };
                self.permission_menu_keyboard_focus = true;
            }
            "up" => {
                self.permission_menu_focused_item = if self.permission_menu_keyboard_focus {
                    (self.permission_menu_focused_item + 3) % 4
                } else {
                    3
                };
                self.permission_menu_keyboard_focus = true;
            }
            "home" => {
                self.permission_menu_focused_item = 0;
                self.permission_menu_keyboard_focus = true;
            }
            "end" => {
                self.permission_menu_focused_item = 3;
                self.permission_menu_keyboard_focus = true;
            }
            "enter" | "space" if self.permission_menu_keyboard_focus => {
                let mode = PermissionMode::at_menu_index(self.permission_menu_focused_item);
                self.activate_permission_mode(mode, cx);
                cx.stop_propagation();
                return;
            }
            "escape" => {
                self.permission_menu_open = false;
                self.permission_menu_keyboard_focus = false;
            }
            "tab" => {
                self.permission_menu_open = false;
                self.permission_menu_keyboard_focus = false;
                cx.notify();
                return;
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
    pub fn confirm_full_access(&mut self, cx: &mut Context<Self>) {
        self.request_permission_mode(PermissionMode::Full, cx);
        self.permission_menu_open = false;
        self.permission_menu_keyboard_focus = false;
        cx.notify();
    }
    #[cfg(test)]
    pub(super) fn permission_mode_name(&self) -> &'static str {
        match self.permission_mode {
            PermissionMode::Request => "request",
            PermissionMode::Assist => "assist",
            PermissionMode::Full => "full",
            PermissionMode::Custom => "custom",
        }
    }
    pub(super) fn permission_label(&self) -> (&'static str, &'static str, gpui::Rgba) {
        let theme = Theme::for_mode(self.mode);
        match self.permission_mode {
            PermissionMode::Request => ("请求批准", "permission-request", theme.text_tertiary),
            PermissionMode::Assist => ("帮我批准", "permission-assist", theme.text_tertiary),
            PermissionMode::Full => ("完全访问", "permission", theme.warning),
            PermissionMode::Custom => ("自定义", "permission-custom", theme.text_tertiary),
        }
    }
    pub(super) fn permission_row(
        &self,
        index: usize,
        option: PermissionOption,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let PermissionOption {
            mode,
            title,
            detail,
            glyph,
        } = option;
        let selected = self.permission_mode == mode;
        let warning = mode == PermissionMode::Full;
        let color = if warning { theme.warning } else { theme.text };
        let keyboard_focused =
            self.permission_menu_keyboard_focus && self.permission_menu_focused_item == index;
        let hover_group: SharedString = format!("permission-menu-row-{index}").into();
        div()
            .id(("permission-menu-item", index))
            .group(hover_group.clone())
            .h(px(47.125))
            .px(px(8.0))
            .py(px(5.0))
            .when(mode == PermissionMode::Custom, |row| {
                row.pl(px(9.0)).pr(px(7.0))
            })
            // CDP 99–101: `rounded-lg` resolves to 12.5px in this desktop
            // build, including both hover-highlighted and keyboard-focused rows.
            .rounded(px(12.5))
            .flex()
            .items_center()
            .cursor_pointer()
            .when(keyboard_focused, |row| row.bg(theme.sidebar_hover))
            .hover(move |style| style.bg(theme.sidebar_hover))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.activate_permission_mode(mode, cx);
            }))
            // Although the source SVGs use 20×20 view boxes, the desktop
            // `icon-sm` token resolves to an 18×18 layout box.
            .child(
                icon(glyph, color.into())
                    .size(px(18.0))
                    .opacity(if keyboard_focused { 1.0 } else { 0.75 })
                    .group_hover(hover_group.clone(), |glyph| glyph.opacity(1.0))
                    .when(mode == PermissionMode::Custom, |glyph| glyph.ml(px(-1.0))),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .ml(px(12.0))
                    // CoreText's Chinese glyph run is fractionally wider than
                    // Chromium's system-ui run sampled over CDP.
                    .text_size(px(12.75))
                    .when(mode == PermissionMode::Custom, |column| {
                        column.text_size(px(13.0))
                    })
                    .when(mode == PermissionMode::Custom, |column| {
                        column.font_weight(crate::theme::UI_BODY_FONT_WEIGHT)
                    })
                    .line_height(px(18.5625))
                    // Chromium fits the 21-CJK warning detail exactly in its
                    // 273px flex slot. CoreText rounds that run just over the
                    // boundary, so give the selected warning column two
                    // non-layout pixels without moving the trailing check.
                    .when(selected && warning, |column| column.mr(px(-2.0)))
                    .child(
                        div()
                            .when(mode == PermissionMode::Custom, |text| {
                                text.font_weight(crate::theme::UI_BODY_FONT_WEIGHT)
                            })
                            .text_color(color)
                            .child(title),
                    )
                    .child(
                        div()
                            .when(mode == PermissionMode::Custom, |text| {
                                text.font_weight(crate::theme::UI_BODY_FONT_WEIGHT)
                            })
                            .text_color(if warning {
                                theme.warning
                            } else {
                                theme.text_tertiary
                            })
                            .child(detail),
                    ),
            )
            .when(selected, |row| {
                row.child(
                    icon("permission-check", color.into())
                        // `icon-xs` is a 16×16 layout box in the reference.
                        .size(px(16.0))
                        .ml(px(12.0))
                        .opacity(if keyboard_focused { 1.0 } else { 0.75 })
                        .group_hover(hover_group, |glyph| glyph.opacity(1.0)),
                )
            })
    }
    pub(super) fn permission_menu(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let width = if self.permission_mode == PermissionMode::Full {
            355.0
        } else {
            327.0
        };
        let surface = if self.mode == ThemeMode::Light {
            // The real light popup is a 90%-opaque white surface over the
            // white application background, so its captured pixel is white.
            rgba(0xffffffff)
        } else {
            theme.model_picker_surface
        };
        div()
            .id("permission-menu")
            .absolute()
            .left(px(42.0))
            // CDP: the menu's bottom edge is 1.5px above the 28px trigger.
            .bottom(px(36.5))
            .w(px(width))
            .h(px(222.5))
            .p(px(4.0))
            .rounded(px(15.0))
            .bg(surface)
            .shadow(vec![
                // ChatGPT uses two 0.5px rings. Keeping them as shadows is
                // important: CSS rings do not consume the row's one-pixel
                // layout budget the way a native border would.
                BoxShadow::new(px(0.0), px(0.0), theme.border.into())
                    .blur_radius(px(0.0))
                    .spread_radius(px(0.5)),
                BoxShadow::new(px(0.0), px(0.0), theme.border.into())
                    .blur_radius(px(0.0))
                    .spread_radius(px(0.5)),
                BoxShadow::new(px(0.0), px(8.0), hsla(0.0, 0.0, 0.0, 0.12))
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .font(ui_font())
            .text_size(px(13.0))
            .font_weight(crate::theme::UI_BODY_FONT_WEIGHT)
            .text_color(theme.text)
            .track_focus(&self.permission_menu_focus)
            .on_key_down(cx.listener(Self::handle_permission_menu_key))
            .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
            .child(
                div()
                    .h(px(26.0))
                    // CoreText's header line box sits one raster row below
                    // Chromium despite identical CSS metrics.
                    .relative()
                    .top(px(-1.0))
                    .left(px(-1.0))
                    .px(px(8.0))
                    .py(px(5.0))
                    .flex()
                    .items_start()
                    .text_size(px(13.0))
                    .line_height(px(16.0))
                    .text_color(theme.text_tertiary)
                    .child(div().flex_1().child("应如何批准 ChatGPT 操作？"))
                    .child(
                        div()
                            .id("permission-learn-more")
                            .cursor_pointer()
                            .relative()
                            .left(px(1.0))
                            .text_size(px(13.0))
                            .line_height(px(16.0))
                            .underline()
                            .child("了解更多"),
                    ),
            )
            .child(self.permission_row(
                0,
                PermissionOption {
                    mode: PermissionMode::Request,
                    title: "请求批准",
                    detail: "编辑外部文件和使用互联网时始终询问",
                    glyph: "permission-request",
                },
                theme,
                cx,
            ))
            .child(self.permission_row(
                1,
                PermissionOption {
                    mode: PermissionMode::Assist,
                    title: "帮我批准",
                    detail: "仅对检测到的风险操作请求批准",
                    glyph: "permission-assist",
                },
                theme,
                cx,
            ))
            .child(self.permission_row(
                2,
                PermissionOption {
                    mode: PermissionMode::Full,
                    title: "完全访问权限",
                    detail: "可不受限制地访问互联网和你电脑上的任何文件",
                    glyph: "permission",
                },
                theme,
                cx,
            ))
            .child(self.permission_row(
                3,
                PermissionOption {
                    mode: PermissionMode::Custom,
                    title: "自定义 (config.toml)",
                    detail: "使用 config.toml 中定义的权限",
                    glyph: "permission-custom",
                },
                theme,
                cx,
            ))
    }
}

#[derive(Clone, Copy)]
pub(super) struct PermissionOption {
    pub(super) mode: PermissionMode,
    pub(super) title: &'static str,
    pub(super) detail: &'static str,
    pub(super) glyph: &'static str,
}
