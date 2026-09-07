//! Project creation behavior and presentation for the application shell.

use gpui::{
    BoxShadow, Context, Div, KeyDownEvent, MouseButton, PathPromptOptions, Window, div, hsla,
    prelude::*, px, rgba,
};

use super::{
    ChatApp,
    state::{ProjectCreationKind, ProjectCreationStep},
};
use crate::{
    components::icons::icon,
    theme::{Theme, UI_FONT_FAMILY},
};

impl ChatApp {
    pub fn open_project_creation(&mut self, cx: &mut Context<Self>) {
        self.project_creation.open = true;
        self.project_creation.focused_item = 0;
        self.project_creation.keyboard_focus = false;
        self.project_creation.focus_pending = true;
        self.permission_confirmation_open = false;
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.close_transient_menus(cx);
        });
        cx.notify();
    }
    pub(super) fn close_project_creation(&mut self, cx: &mut Context<Self>) {
        self.project_creation.open = false;
        self.project_creation.keyboard_focus = false;
        self.project_creation.focus_pending = false;
        cx.notify();
    }
    pub(super) fn cancel_project_creation(&mut self, cx: &mut Context<Self>) {
        self.project_creation.kind = ProjectCreationKind::Local;
        self.project_creation.step = ProjectCreationStep::Kind;
        self.close_project_creation(cx);
    }
    pub(super) fn advance_project_creation(&mut self, cx: &mut Context<Self>) {
        match self.project_creation.kind {
            ProjectCreationKind::Local => {
                self.close_project_creation(cx);
                let paths = cx.prompt_for_paths(PathPromptOptions {
                    files: false,
                    directories: true,
                    multiple: false,
                    prompt: None,
                });
                #[cfg(not(test))]
                {
                    let workspace_store = self.workspace_store.clone();
                    cx.spawn(async move |_, _cx| {
                        let Ok(Ok(Some(mut paths))) = paths.await else {
                            return;
                        };
                        let Some(path) = paths.pop() else {
                            return;
                        };
                        workspace_store.create_project(path);
                    })
                    .detach();
                }
                #[cfg(test)]
                drop(paths);
            }
            ProjectCreationKind::Remote => {
                self.project_creation.step = ProjectCreationStep::Remote;
                self.project_creation.focused_item = 0;
                self.project_creation.keyboard_focus = false;
                cx.notify();
            }
        }
    }
    pub(super) fn handle_project_creation_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.image_preview.path.is_some() && event.keystroke.key == "escape" {
            self.image_preview.path = None;
            self.image_preview.dimensions = None;
            self.image_preview.zoom = 1.0;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if !self.project_creation.open {
            return;
        }

        let key = event.keystroke.key.as_str();
        if key == "escape" {
            self.close_project_creation(cx);
            cx.stop_propagation();
            return;
        }
        if self.project_creation.step == ProjectCreationStep::Remote {
            if key == "tab" {
                let count = 4;
                self.project_creation.focused_item = if event.keystroke.modifiers.shift {
                    (self.project_creation.focused_item + count - 1) % count
                } else {
                    (self.project_creation.focused_item + 1) % count
                };
                self.project_creation.keyboard_focus = true;
                cx.stop_propagation();
                cx.notify();
            } else if matches!(key, "enter" | "space") {
                match self.project_creation.focused_item {
                    2 => self.cancel_project_creation(cx),
                    3 => self.close_project_creation(cx),
                    _ => return,
                }
                cx.stop_propagation();
            }
            return;
        }

        match key {
            "tab" => {
                let count = 4;
                self.project_creation.focused_item = if event.keystroke.modifiers.shift {
                    (self.project_creation.focused_item + count - 1) % count
                } else {
                    (self.project_creation.focused_item + 1) % count
                };
                self.project_creation.keyboard_focus = true;
            }
            "enter" | "space" => match self.project_creation.focused_item {
                0 => self.project_creation.kind = ProjectCreationKind::Local,
                1 => self.project_creation.kind = ProjectCreationKind::Remote,
                2 => self.advance_project_creation(cx),
                3 => self.close_project_creation(cx),
                _ => {}
            },
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
    pub(super) fn project_kind_card(
        &self,
        index: usize,
        option: ProjectKindOption,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let ProjectKindOption {
            kind,
            glyph,
            label,
            detail,
        } = option;
        let selected = self.project_creation.kind == kind;
        let keyboard_focused =
            self.project_creation.keyboard_focus && self.project_creation.focused_item == index;
        let radio = div()
            .size(px(20.0))
            .flex_none()
            .rounded_full()
            .border_1()
            .border_color(if selected { theme.accent } else { theme.text })
            .flex()
            .items_center()
            .justify_center()
            .when(selected, |radio| {
                radio.child(div().size(px(12.0)).rounded_full().bg(theme.accent))
            });

        div()
            .id(("project-creation-kind", index))
            .w(px(314.0))
            .h(px(144.0))
            .p(px(16.0))
            .rounded(px(20.0))
            .border_1()
            .border_color(if selected {
                rgba(0x00000000)
            } else {
                theme.border
            })
            .bg(if selected {
                theme.text.alpha(0.05)
            } else {
                rgba(0x00000000)
            })
            .shadow(project_creation_focus_shadow(theme, keyboard_focused))
            .flex()
            .flex_col()
            .justify_between()
            .cursor_pointer()
            .hover(move |style| {
                if selected {
                    style.bg(theme.text.alpha(0.05))
                } else {
                    style.bg(theme.text.alpha(0.03))
                }
            })
            .active(move |style| {
                if selected {
                    style.bg(theme.text.alpha(0.05))
                } else {
                    style.bg(theme.text.alpha(0.03))
                }
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.project_creation.kind = kind;
                this.project_creation.focused_item = index;
                this.project_creation.keyboard_focus = false;
                cx.notify();
            }))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_start()
                    .justify_between()
                    .child(icon(glyph, theme.text_tertiary.into()).size(px(20.0)))
                    .child(radio),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .ml(px(1.0))
                    .child(
                        div()
                            .h(px(21.0))
                            .text_size(px(14.0))
                            .line_height(px(21.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(label),
                    )
                    .child(
                        div()
                            .h(px(19.25))
                            .text_size(px(14.0))
                            .line_height(px(19.25))
                            .font_weight(gpui::FontWeight::NORMAL)
                            .text_color(theme.text_tertiary)
                            .child(detail),
                    ),
            )
    }
    pub(super) fn project_creation_close_button(
        &self,
        index: usize,
        label: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let keyboard_focused =
            self.project_creation.keyboard_focus && self.project_creation.focused_item == index;
        div()
            .id("project-creation-close")
            .absolute()
            .top(px(16.0))
            .right(px(16.0))
            .size(px(24.0))
            .rounded(px(4.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_color(theme.text.alpha(0.8))
            .shadow(project_creation_focus_shadow(theme, keyboard_focused))
            .hover(move |style| style.bg(theme.sidebar_hover))
            .active(move |style| style.bg(theme.sidebar_hover))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.close_project_creation(cx);
            }))
            .child(icon("close-dialog", theme.text.alpha(0.8).into()).size(px(16.0)))
            .child(div().invisible().absolute().child(label))
    }
    pub(super) fn project_creation_kind_dialog(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let next_focused =
            self.project_creation.keyboard_focus && self.project_creation.focused_item == 2;
        div()
            .id("project-creation-dialog")
            .relative()
            .w(px(680.0))
            .h(px(357.796_88))
            .rounded(px(25.0))
            .bg(theme.project_dialog_surface)
            .shadow(vec![
                BoxShadow::new(px(0.0), px(0.0), theme.border.into()).spread_radius(px(0.5)),
                BoxShadow::new(px(0.0), px(4.0), hsla(0.0, 0.0, 0.0, 0.10))
                    .blur_radius(px(8.0))
                    .spread_radius(px(-2.0)),
            ])
            .font_family(".SystemUIFont")
            .text_color(theme.text)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .size_full()
                    .p(px(20.0))
                    .flex()
                    .flex_col()
                    .gap(px(28.0))
                    .child(
                        div()
                            .h(px(28.796875))
                            .relative()
                            .top(px(-1.0))
                            .ml(px(1.0))
                            // CoreText's system Chinese advances are slightly
                            // narrower than Chromium's at the computed 24px.
                            .text_size(px(25.0))
                            .line_height(px(28.8))
                            .font_weight(gpui::FontWeight(500.0))
                            .child("创建项目"),
                    )
                    .child(
                        div()
                            .h(px(189.0))
                            .pt(px(12.0))
                            .flex()
                            .flex_col()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .h(px(21.0))
                                    .text_size(px(14.0))
                                    .line_height(px(21.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child("项目类型"),
                            )
                            .child(
                                div()
                                    .h(px(144.0))
                                    .flex()
                                    .gap(px(12.0))
                                    .child(self.project_kind_card(
                                        0,
                                        ProjectKindOption {
                                            kind: ProjectCreationKind::Local,
                                            glyph: "project-local",
                                            label: "本地",
                                            detail: "在你的电脑上编辑、运行和测试文件",
                                        },
                                        theme,
                                        cx,
                                    ))
                                    .child(self.project_kind_card(
                                        1,
                                        ProjectKindOption {
                                            kind: ProjectCreationKind::Remote,
                                            glyph: "project-remote",
                                            label: "远程",
                                            detail: "选择已连接计算机上的文件夹",
                                        },
                                        theme,
                                        cx,
                                    )),
                            ),
                    )
                    .child(
                        div().h(px(44.0)).pt(px(12.0)).flex().justify_end().child(
                            div()
                                .id("project-creation-next")
                                .h(px(32.0))
                                .px(px(16.0))
                                .rounded(px(12.5))
                                .border_1()
                                .border_color(theme.border)
                                .bg(theme.button)
                                .text_color(theme.button_text)
                                .shadow(project_creation_focus_shadow(theme, next_focused))
                                .flex()
                                .items_center()
                                .text_size(px(14.0))
                                .line_height(px(18.0))
                                .font_weight(crate::theme::UI_BODY_FONT_WEIGHT)
                                .cursor_pointer()
                                .hover(move |style| style.bg(theme.text.alpha(0.8)))
                                .active(move |style| style.bg(theme.text.alpha(0.8)))
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.project_creation.focused_item = 2;
                                    this.project_creation.keyboard_focus = false;
                                    this.advance_project_creation(cx);
                                }))
                                .child("下一步"),
                        ),
                    ),
            )
            .child(self.project_creation_close_button(3, "关闭", theme, cx))
    }
    pub(super) fn project_creation_remote_dialog(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let keyboard_focus = |index| {
            self.project_creation.keyboard_focus && self.project_creation.focused_item == index
        };
        div()
            .id("project-creation-remote-dialog")
            .relative()
            .w(px(520.0))
            .h(px(331.0))
            .rounded(px(25.0))
            .bg(theme.project_dialog_surface)
            .shadow(vec![
                BoxShadow::new(px(0.0), px(0.0), theme.border.into()).spread_radius(px(0.5)),
                BoxShadow::new(px(0.0), px(4.0), hsla(0.0, 0.0, 0.0, 0.10))
                    .blur_radius(px(8.0))
                    .spread_radius(px(-2.0)),
            ])
            .p(px(20.0))
            .font_family(UI_FONT_FAMILY)
            .text_size(px(14.0))
            .line_height(px(21.0))
            .font_weight(crate::theme::UI_BODY_FONT_WEIGHT)
            .text_color(theme.text)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .relative()
                    .top(px(0.0))
                    .ml(px(1.0))
                    .font_family(".SystemUIFont")
                    .text_size(px(20.5))
                    .line_height(px(28.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child("新建远程项目"),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .relative()
                    .top(px(1.0))
                    .font_weight(crate::theme::UI_BODY_FONT_WEIGHT)
                    .text_color(theme.text_tertiary)
                    .child("先设置远程主机。然后可在此处选择主机和文件夹。"),
            )
            .child(
                div()
                    .mt(px(16.0))
                    .h(px(40.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.control)
                    .shadow(project_creation_focus_shadow(theme, keyboard_focus(0)))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_color(theme.text_tertiary)
                    .child(
                        div()
                            .size(px(40.0))
                            .border_r_1()
                            .border_color(theme.border)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon("folder", theme.text_tertiary.into())),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(18.5714))
                            .child("项目名称"),
                    ),
            )
            .child(
                div()
                    .mt(px(12.0))
                    .relative()
                    .top(px(1.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child("远程主机"),
            )
            .child(
                div()
                    .mt(px(8.0))
                    .h(px(40.0))
                    .px(px(12.0))
                    .rounded(px(15.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.control)
                    .shadow(project_creation_focus_shadow(theme, keyboard_focus(1)))
                    .flex()
                    .items_center()
                    .text_size(px(13.0))
                    .line_height(px(18.5714))
                    .text_color(theme.text_tertiary)
                    .child(div().flex_1().child("没有已连接的远程目标"))
                    .child(icon("chevron-down", theme.text_tertiary.into())),
            )
            .child(
                div()
                    .mt(px(12.0))
                    .relative()
                    .top(px(1.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child("源文件夹"),
            )
            .child(
                div()
                    .h(px(68.0))
                    .pt(px(12.0))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(24.0))
                            .relative()
                            .top(px(3.0))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(px(13.0))
                            .line_height(px(20.0))
                            .text_color(theme.warning)
                            .child(icon("settings-warning", theme.warning.into()))
                            .child("目前没有连接任何远程主机。"),
                    )
                    .child(
                        div()
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .id("project-creation-remote-cancel")
                                    .h(px(32.0))
                                    .px(px(16.0))
                                    .rounded(px(12.5))
                                    .border_1()
                                    .border_color(rgba(0x00000000))
                                    .shadow(project_creation_focus_shadow(theme, keyboard_focus(2)))
                                    .text_color(theme.text_tertiary)
                                    .flex()
                                    .items_center()
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(theme.sidebar_hover))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.cancel_project_creation(cx);
                                    }))
                                    .child(div().relative().left(px(2.0)).child("取消")),
                            )
                            .child(
                                div()
                                    .h(px(32.0))
                                    .px(px(16.0))
                                    .rounded(px(12.5))
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(theme.button)
                                    .text_color(theme.button_text)
                                    .opacity(0.4)
                                    .flex()
                                    .items_center()
                                    .child("添加项目"),
                            ),
                    ),
            )
            .child(self.project_creation_close_button(3, "关闭对话框", theme, cx))
    }
    pub(super) fn project_creation_overlay(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        div()
            .id("project-creation-overlay")
            .absolute()
            .inset_0()
            .occlude()
            .bg(rgba(0x00000022))
            .flex()
            .items_center()
            .justify_center()
            .track_focus(&self.project_creation.focus)
            .on_key_down(cx.listener(Self::handle_project_creation_key))
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_project_creation(cx);
            }))
            .child(match self.project_creation.step {
                ProjectCreationStep::Kind => self.project_creation_kind_dialog(theme, cx),
                ProjectCreationStep::Remote => self.project_creation_remote_dialog(theme, cx),
            })
    }
}

pub(super) fn project_creation_focus_shadow(theme: Theme, visible: bool) -> Vec<BoxShadow> {
    if visible {
        vec![
            BoxShadow::new(px(0.0), px(0.0), theme.accent.alpha(0.76).into())
                .spread_radius(px(2.0)),
        ]
    } else {
        Vec::new()
    }
}

#[derive(Clone, Copy)]
pub(super) struct ProjectKindOption {
    pub(super) kind: ProjectCreationKind,
    pub(super) glyph: &'static str,
    pub(super) label: &'static str,
    pub(super) detail: &'static str,
}
