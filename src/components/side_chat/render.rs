//! CDP-measured side chat tabs, launcher, and close confirmation.

use super::{SideChatEvent, SideChatPanel, SideChatTabDrag};
use crate::{
    components::icons::icon,
    theme::{CHAT_CONTENT_HORIZONTAL_GUTTER, Theme},
};
use gpui::{
    AnyElement, BoxShadow, Context, Div, FontWeight, MouseButton, Render, Role, Window, deferred,
    div, prelude::*, px, rgba,
};

const ITEMS: [(&str, &str, &str); 5] = [
    ("审查", "⌃⇧G", "panel-review"),
    ("终端", "⌃`", "panel-terminal"),
    ("浏览器", "⌘T", "panel-browser"),
    ("文件", "⌘P", "panel-files"),
    ("侧边聊天", "⌥⌘S", "side-chat"),
];

pub(crate) fn restore_tab(id: &'static str, theme: Theme) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .role(Role::Tab)
        .aria_label("返回侧边聊天")
        .focusable()
        .tab_stop(true)
        .h(px(28.0))
        .px(px(8.0))
        .rounded(px(8.0))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.0))
        .cursor_pointer()
        .text_size(px(13.0))
        .text_color(theme.text_secondary)
        .hover(move |s| s.bg(theme.sidebar_hover))
        .on_click(|_, window, cx| window.dispatch_action(Box::new(super::RestoreSideChat), cx))
        .on_key_down(|e: &gpui::KeyDownEvent, window, cx| {
            if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                window.dispatch_action(Box::new(super::RestoreSideChat), cx);
                cx.stop_propagation();
            }
        })
        .child(icon("side-chat", theme.text_secondary.into()))
        .child("侧边聊天")
}

impl Render for SideChatTabDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_mode(self.mode);
        div()
            .w(px(156.0))
            .h(px(28.0))
            .px(px(8.0))
            .rounded(px(10.0))
            .bg(theme.control_soft)
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(13.0))
            .text_color(theme.text)
            .child(icon("side-chat", theme.text_secondary.into()))
            .child(div().min_w(px(0.0)).truncate().child(self.title.clone()))
    }
}

fn button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    glyph: &'static str,
    theme: Theme,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(label)
        .focusable()
        .tab_stop(true)
        .size(px(28.0))
        .flex_none()
        .rounded(px(8.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_color(theme.text_tertiary)
        .hover(move |s| s.bg(theme.sidebar_hover).text_color(theme.text))
        .focus_visible(move |s| s.border_1().border_color(theme.text_secondary))
        .child(icon(glyph, theme.text_secondary.into()).size(px(16.0)))
}

impl SideChatPanel {
    fn toolbar(&self, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let mut tabs = div()
            .id("side-chat-tabs")
            .role(Role::TabList)
            .aria_label("侧边聊天标签页")
            .flex()
            .items_center()
            .gap(px(4.0))
            .min_w(px(0.0))
            .overflow_x_scroll()
            .track_scroll(&self.tab_scroll);
        for tab in &self.tabs {
            let id = tab.id;
            let active = self.active == Some(id);
            let title = tab.title.clone();
            let close_label = format!("关闭{}标签页", tab.title);
            let owner = cx.entity_id();
            let drag = SideChatTabDrag {
                owner,
                id,
                title: title.clone(),
                mode: self.mode,
            };
            tabs = tabs.child(
                div()
                    .id(("side-chat-tab", id))
                    .group("side-chat-tab")
                    .role(Role::Tab)
                    .aria_label(title.clone())
                    .aria_selected(active)
                    .focusable()
                    .tab_stop(true)
                    .w(px(156.0))
                    .min_w(px(80.0))
                    .h(px(28.0))
                    .flex_none()
                    .rounded(px(10.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .when(active, |d| d.bg(theme.text.alpha(0.05)))
                    .hover(move |s| s.bg(theme.sidebar_hover))
                    .focus_visible(move |s| s.border_1().border_color(theme.text_secondary))
                    .on_click(cx.listener(move |this, _, _, cx| this.activate(id, cx)))
                    .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
                    .on_drop(cx.listener(move |this, drag: &SideChatTabDrag, _, cx| {
                        if drag.owner == owner {
                            this.reorder_tab(drag.id, id, cx);
                        }
                    }))
                    .on_key_down(cx.listener(move |this, e: &gpui::KeyDownEvent, _, cx| {
                        if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                            this.activate(id, cx);
                            cx.stop_propagation();
                        } else if e.keystroke.key == "delete" {
                            this.request_close(id, cx);
                            cx.stop_propagation();
                        }
                    }))
                    .child(
                        icon(
                            if tab.running || tab.loading {
                                "dictation-spinner"
                            } else {
                                "side-chat"
                            },
                            theme.text_secondary.into(),
                        )
                        .size(px(16.0)),
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
                            .child(title),
                    )
                    .when(tab.unread, |d| {
                        d.child(
                            div()
                                .id(("side-chat-unread", id))
                                .role(Role::Status)
                                .aria_label("未读回复")
                                .size(px(5.0))
                                .rounded_full()
                                .bg(theme.markdown_link),
                        )
                    })
                    .child(
                        div()
                            .id(("side-chat-close", id))
                            .role(Role::Button)
                            .aria_label(close_label)
                            .focusable()
                            .tab_stop(true)
                            .size(px(20.0))
                            .flex_none()
                            .rounded(px(5.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .when(!active, |d| {
                                d.opacity(0.0)
                                    .group_hover("side-chat-tab", |s| s.opacity(1.0))
                                    .focus_visible(|s| s.opacity(1.0))
                            })
                            .hover(move |s| s.bg(theme.sidebar_hover))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.request_close(id, cx);
                                cx.stop_propagation();
                            }))
                            .on_key_down(cx.listener(move |this, e: &gpui::KeyDownEvent, _, cx| {
                                if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                                    this.request_close(id, cx);
                                    cx.stop_propagation();
                                }
                            }))
                            .child(icon("close-dialog", theme.text_tertiary.into()).size(px(12.0))),
                    ),
            );
        }
        div()
            .absolute()
            .top_0()
            .left_0()
            .w_full()
            .h(px(46.0))
            .px(px(8.0))
            .pr(px(76.0))
            .bg(theme.surface)
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(tabs)
            .child(
                button("side-chat-add-tab", "打开侧边面板标签页", "add", theme)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.menu_open = !this.menu_open;
                        this.menu_index = 4;
                        if this.menu_open {
                            this.menu_focus.focus(window, cx);
                        }
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, window, cx| {
                        if matches!(e.keystroke.key.as_str(), "enter" | "space" | "down") {
                            this.menu_open = true;
                            this.menu_index = 4;
                            this.menu_focus.focus(window, cx);
                            cx.notify();
                            cx.stop_propagation();
                        }
                    })),
            )
            .child(div().flex_1())
            .child(
                button(
                    "side-chat-fullscreen",
                    if self.fullscreen {
                        "退出全屏"
                    } else {
                        "进入全屏"
                    },
                    "settings-external",
                    theme,
                )
                .on_click(cx.listener(|_, _, _, cx| cx.emit(SideChatEvent::Fullscreen)))
                .on_key_down(cx.listener(|_, e: &gpui::KeyDownEvent, _, cx| {
                    if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                        cx.emit(SideChatEvent::Fullscreen);
                        cx.stop_propagation();
                    }
                })),
            )
    }

    fn menu(&self, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let mut menu = div()
            .id("side-chat-add-menu")
            .role(Role::Menu)
            .track_focus(&self.menu_focus)
            .absolute()
            .top(px(41.0))
            .right(px(76.0))
            .w(px(280.0))
            .p(px(4.0))
            .rounded(px(12.0))
            .bg(theme.model_picker_surface)
            .border(px(0.5))
            .border_color(theme.border)
            .shadow(vec![
                BoxShadow::new(px(0.0), px(5.0), rgba(0x00000022).into()).blur_radius(px(20.0)),
            ])
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.menu_open = false;
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, _, cx| {
                match e.keystroke.key.as_str() {
                    "up" => this.menu_index = (this.menu_index + 4) % 5,
                    "down" | "tab" => this.menu_index = (this.menu_index + 1) % 5,
                    "enter" | "space" => this.select_menu(this.menu_index, cx),
                    "escape" => {
                        this.menu_open = false;
                        this.focus_pending = true;
                    }
                    _ => return,
                }
                cx.notify();
                cx.stop_propagation();
            }));
        for (index, (label, shortcut, glyph)) in ITEMS.iter().enumerate() {
            menu = menu.child(
                div()
                    .id(("side-chat-menu-item", index))
                    .role(Role::MenuItem)
                    .aria_label(*label)
                    .h(px(28.5703))
                    .px(px(8.0))
                    .rounded(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .text_size(px(13.0))
                    .line_height(px(18.5714))
                    .text_color(theme.text)
                    .when(index == self.menu_index, |d| d.bg(theme.sidebar_hover))
                    .hover(move |s| s.bg(theme.sidebar_hover))
                    .on_click(cx.listener(move |this, _, _, cx| this.select_menu(index, cx)))
                    .child(icon(glyph, theme.text_secondary.into()).size(px(16.0)))
                    .child(div().flex_1().child(*label))
                    .child(div().text_color(theme.text_tertiary).child(*shortcut)),
            );
        }
        deferred(menu).with_priority(10)
    }

    pub fn render_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.close_confirmation?;
        if self.confirm_focus_pending {
            self.confirm_focus.focus(window, cx);
            self.confirm_focus_pending = false;
        }
        let theme = Theme::for_mode(self.mode);
        let checkbox = div()
            .id("side-chat-remember-close")
            .track_focus(&self.remember_focus)
            .role(Role::CheckBox)
            .aria_label("不再询问")
            .aria_toggled(if self.remember_close {
                gpui::Toggled::True
            } else {
                gpui::Toggled::False
            })
            .focusable()
            .tab_stop(true)
            .h(px(28.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .on_click(cx.listener(|this, _, window, cx| {
                this.remember_focus.focus(window, cx);
                this.remember_close = !this.remember_close;
                cx.notify();
            }))
            .child(
                div()
                    .size(px(16.0))
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(theme.text_tertiary)
                    .when(self.remember_close, |d| {
                        d.bg(theme.button)
                            .child(icon("check", theme.button_text.into()).size(px(14.0)))
                    }),
            )
            .child(div().text_size(px(13.0)).child("不再询问"));
        let action = |id: &'static str, label: &'static str, danger: bool| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(label)
                .focusable()
                .tab_stop(true)
                .h(px(36.0))
                .px(px(14.0))
                .rounded(px(10.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .bg(if danger {
                    rgba(0xe02e2aff)
                } else {
                    theme.text.alpha(0.08)
                })
                .text_color(if danger { rgba(0xffffffff) } else { theme.text })
                .child(label)
        };
        Some(
            div()
                .id("side-chat-close-overlay")
                .absolute()
                .inset_0()
                .bg(rgba(0x00000066))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .id("side-chat-close-dialog")
                        .key_context("SideChatCloseDialog")
                        .role(Role::Dialog)
                        .aria_label("关闭侧边聊天？")
                        .w(px(400.0))
                        .max_w_full()
                        .p(px(24.0))
                        .rounded(px(20.0))
                        .bg(theme.model_picker_surface)
                        .border(px(0.5))
                        .border_color(theme.border)
                        .text_color(theme.text)
                        .shadow(vec![
                            BoxShadow::new(px(0.0), px(16.0), rgba(0x00000044).into())
                                .blur_radius(px(40.0)),
                        ])
                        .flex()
                        .flex_col()
                        .gap(px(16.0))
                        .on_action(cx.listener(|this, _: &super::CloseDialogNext, window, cx| {
                            this.move_dialog_focus(false, window, cx)
                        }))
                        .on_action(cx.listener(
                            |this, _: &super::CloseDialogPrevious, window, cx| {
                                this.move_dialog_focus(true, window, cx)
                            },
                        ))
                        .on_action(cx.listener(
                            |this, _: &super::CloseDialogActivate, window, cx| {
                                this.activate_dialog_control(window, cx)
                            },
                        ))
                        .on_action(cx.listener(|this, _: &super::CloseDialogCancel, _, cx| {
                            this.dismiss_transient(cx);
                        }))
                        .child(
                            div()
                                .text_size(px(18.0))
                                .line_height(px(25.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("关闭侧边聊天？"),
                        )
                        .child(
                            div()
                                .text_size(px(13.0))
                                .line_height(px(20.0))
                                .text_color(theme.text_secondary)
                                .child("此侧边聊天将消失且无法恢复。确定要关闭吗？"),
                        )
                        .child(checkbox)
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(8.0))
                                .child(
                                    action("side-chat-close-cancel", "取消", false)
                                        .track_focus(&self.cancel_focus)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.close_confirmation = None;
                                            this.focus_pending = true;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    action("side-chat-close-confirm", "关闭侧边聊天", true)
                                        .track_focus(&self.confirm_focus)
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.confirm_close(cx)),
                                        ),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}

impl Render for SideChatPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_mode(self.mode);
        let active = self.tabs.iter().find(|tab| Some(tab.id) == self.active);
        if self.focus_pending && self.close_confirmation.is_none() {
            if let Some(tab) = active {
                tab.composer
                    .read(cx)
                    .prompt_focus_handle(cx)
                    .focus(window, cx);
            }
            self.focus_pending = false;
        }
        let mut root = div()
            .id("side-chat-panel")
            .size_full()
            .relative()
            .bg(theme.surface)
            .key_context("SideChatPanel")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::handle_key))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| {
                for tab in &this.tabs {
                    tab.composer.update(cx, |c, cx| c.close_side_menus(cx));
                }
                cx.stop_propagation();
            }));
        if let Some(tab) = active {
            root = root.child(
                div()
                    .id("side-chat-content")
                    .debug_selector(|| "side-chat-content".to_owned())
                    .size_full()
                    .px(px(CHAT_CONTENT_HORIZONTAL_GUTTER))
                    .child(tab.home.clone()),
            );
            if tab.error.is_none()
                && tab.composer.read(cx).conversation_render_snapshot().0
                    == crate::conversation::ConversationPhase::Failed
            {
                let composer = tab.composer.clone();
                let key_composer = composer.clone();
                root = root.child(
                    div()
                        .id("side-chat-retry-turn")
                        .role(Role::Button)
                        .aria_label("重试回复")
                        .focusable()
                        .tab_stop(true)
                        .absolute()
                        .bottom(px(tab.composer.read(cx).side_composer_height(cx) + 24.0))
                        .right(px(24.0))
                        .px(px(12.0))
                        .h(px(28.0))
                        .rounded(px(8.0))
                        .bg(theme.control_soft)
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .text_size(px(13.0))
                        .text_color(theme.text)
                        .child("重试回复")
                        .on_click(move |_, _, cx| {
                            composer.update(cx, |composer, cx| composer.retry_side_prompt(cx))
                        })
                        .on_key_down(move |e: &gpui::KeyDownEvent, _, cx| {
                            if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                                key_composer
                                    .update(cx, |composer, cx| composer.retry_side_prompt(cx));
                                cx.stop_propagation();
                            }
                        }),
                );
            }
            if let Some(error) = &tab.error {
                let id = tab.id;
                root = root.child(
                    div()
                        .absolute()
                        .top(px(54.0))
                        .left(px(CHAT_CONTENT_HORIZONTAL_GUTTER))
                        .right(px(CHAT_CONTENT_HORIZONTAL_GUTTER))
                        .p(px(12.0))
                        .rounded(px(12.0))
                        .bg(theme.control_soft)
                        .text_size(px(13.0))
                        .text_color(theme.text_secondary)
                        .child(error.clone())
                        .when(tab.thread_id.is_none(), |d| {
                            d.child(
                                div()
                                    .id("side-chat-retry-open")
                                    .role(Role::Button)
                                    .aria_label("重试")
                                    .focusable()
                                    .tab_stop(true)
                                    .mt(px(8.0))
                                    .cursor_pointer()
                                    .text_color(theme.text)
                                    .child("重试")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.open_tab(id, cx);
                                        cx.notify();
                                    }))
                                    .on_key_down(cx.listener(
                                        move |this, e: &gpui::KeyDownEvent, _, cx| {
                                            if matches!(e.keystroke.key.as_str(), "enter" | "space")
                                            {
                                                this.open_tab(id, cx);
                                                cx.notify();
                                                cx.stop_propagation();
                                            }
                                        },
                                    )),
                            )
                        }),
                )
            }
        }
        root.child(self.toolbar(theme, cx))
            .when(self.menu_open, |d| d.child(self.menu(theme, cx)))
    }
}
