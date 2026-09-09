//! Render behavior and presentation for the application shell.

use std::path::PathBuf;

use gpui::{
    Animation, AnimationExt, BoxShadow, Context, Div, IntoElement, KeyDownEvent, MouseButton,
    ObjectFit, Render, Role, StyleRefinement, Transformation, Window, div, hsla, linear_color_stop,
    linear_gradient, prelude::*, px, radians, rgba,
};

use super::{right_panel::clamp_right_panel_width, state::RightPanelMode};
#[cfg(not(test))]
use crate::workspace::WorkspaceSnapshot;
use crate::{
    components::{file_panel::OpenWorkspaceFile, icons::icon},
    theme::{CHAT_CONTENT_HORIZONTAL_GUTTER, Theme, ui_font},
};

use super::{
    ChatApp, ConversationKey, DismissPermissionUi, LEADING_TITLEBAR_CONTROLS_TOP, OpenFiles,
    OpenSideChat, RIGHT_PANEL_MIN_WIDTH, STARTUP_LOADING_BLINK_DURATION, STARTUP_LOADING_LOGO_SIZE,
    ToggleReview, ToggleTerminal,
};

pub(super) fn panel_resize_handle(
    id: &'static str,
    left: f32,
    line_visible: bool,
    theme: Theme,
    input_layer: impl IntoElement,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .absolute()
        .top_0()
        .bottom_0()
        .left(px(left))
        .w(px(16.0))
        .cursor_col_resize()
        .child(input_layer)
        .when(line_visible, |handle| {
            handle.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(7.5))
                    .w(px(1.0))
                    .flex()
                    .flex_col()
                    .child(div().flex_1().w_full().bg(linear_gradient(
                        0.0,
                        linear_color_stop(theme.text.alpha(0.0), 0.0),
                        linear_color_stop(theme.text.alpha(0.25), 1.0),
                    )))
                    .child(div().flex_1().w_full().bg(linear_gradient(
                        0.0,
                        linear_color_stop(theme.text.alpha(0.25), 0.0),
                        linear_color_stop(theme.text.alpha(0.0), 1.0),
                    ))),
            )
        })
}

pub(super) fn titlebar_interaction_area() -> impl IntoElement {
    div()
        .id("titlebar-interaction-area")
        .absolute()
        .top_0()
        .left_0()
        .w_full()
        .h(px(46.0))
        .on_click(|event, window, _| {
            if event.click_count() == 2 {
                window.zoom_window();
            }
        })
}

pub(super) fn startup_loading_logo_opacity(progress: f32) -> f32 {
    let blink = ((progress.clamp(0.0, 1.0) * std::f32::consts::TAU).cos() + 1.0) * 0.5;
    0.32 + blink * 0.68
}

#[cfg(not(test))]
pub(super) fn startup_sidebar_resolved(snapshot: &WorkspaceSnapshot) -> bool {
    !snapshot.loading.projects && !snapshot.loading.recent && !snapshot.loading.pinned
}

pub(super) fn startup_loading_view(theme: Theme) -> impl IntoElement {
    let logo = icon("home-mark", theme.home_mark.into())
        .size(px(STARTUP_LOADING_LOGO_SIZE))
        .with_animation(
            "startup-loading-logo-blink",
            Animation::new(STARTUP_LOADING_BLINK_DURATION).repeat(),
            |logo, progress| logo.opacity(startup_loading_logo_opacity(progress)),
        );

    div()
        .id("startup-loading-screen")
        .role(Role::ProgressIndicator)
        .aria_label("GPUI 正在加载")
        .size_full()
        .relative()
        .child(
            div()
                .size_full()
                .bg(theme.sidebar_surface)
                .flex()
                .items_center()
                .justify_center()
                .child(logo),
        )
        .child(titlebar_interaction_area())
}

pub(super) fn titlebar_icon_button(
    name: &'static str,
    disabled: bool,
    active: bool,
    theme: Theme,
) -> gpui::Stateful<gpui::Div> {
    let glyph = icon(name, theme.text_tertiary.into())
        .size(px(16.0))
        .when(name == "right-sidebar", |glyph| {
            glyph.with_transformation(Transformation::rotate(radians(std::f32::consts::PI)))
        });

    div()
        .id(name)
        .size(px(28.0))
        .flex_none()
        .rounded(px(10.0))
        .flex()
        .items_center()
        .justify_center()
        .when(active, |button| button.bg(theme.text.alpha(0.05)))
        .when(disabled, |button| button.opacity(0.4).cursor_default())
        .when(!disabled, |button| {
            button
                .cursor_pointer()
                .hover(move |style| style.bg(theme.sidebar_hover))
                .active(move |style| style.bg(theme.sidebar_hover))
        })
        // The reference SVG declares 20x20, but its `icon-xs` class wins in
        // computed style and renders the glyph at 16x16.
        .child(glyph)
}

pub(super) fn permission_risk_row(
    icon_name: &'static str,
    title: &'static str,
    detail: &'static str,
    separated: bool,
    theme: Theme,
) -> Div {
    div()
        .h(px(51.0))
        .mx(px(16.0))
        .when(separated, |row| row.border_t_1().border_color(theme.border))
        .flex()
        .items_center()
        // Preserve the reference's authored multicolor fills.
        .child(gpui::img(format!("icons/{icon_name}.svg")).size(px(24.0)))
        .child(
            div()
                .ml(px(12.0))
                .flex_1()
                .flex()
                .flex_col()
                .text_size(px(13.0))
                .line_height(px(18.0))
                .child(div().text_color(theme.text).child(title))
                .child(div().text_color(theme.text_tertiary).child(detail)),
        )
}

impl Render for ChatApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let review_overlay =
            if self.right_panel.open && self.right_panel.mode == Some(RightPanelMode::Review) {
                self.review_panels
                    .get(&self.active_conversation)
                    .and_then(|p| p.update(cx, |p, cx| p.render_overlay(cx)))
            } else {
                None
            };
        let side_chat_overlay = if self.right_panel.open
            && self.right_panel.mode == Some(RightPanelMode::SideChat)
        {
            self.side_chat_panels
                .get(&self.active_conversation)
                .and_then(|panel| panel.update(cx, |panel, cx| panel.render_overlay(window, cx)))
        } else {
            None
        };
        if self.terminal_return_focus_pending {
            if let Some(host) = self.conversation_hosts.get(&self.active_conversation) {
                host.composer
                    .read(cx)
                    .prompt_focus_handle(cx)
                    .focus(window, cx);
            }
            self.terminal_return_focus_pending = false;
        }
        let viewport = window.viewport_size();
        if window.focused(cx).is_none() {
            self.root_focus.focus(window, cx);
        }
        let theme = Theme::for_window(
            self.mode,
            window.is_window_active(),
            f32::from(viewport.width),
            f32::from(viewport.height),
            window.scale_factor(),
        );
        if !(self.startup_model_catalog_resolved
            && self.startup_sidebar_resolved
            && self.startup_minimum_duration_elapsed)
        {
            return startup_loading_view(theme).into_any_element();
        }
        if self.image_preview.path.is_some() && !self.image_preview.focus_active {
            self.image_preview.previous_focus = window.focused(cx);
            self.image_preview.focus.focus(window, cx);
            self.image_preview.focus_active = true;
        } else if self.image_preview.path.is_none() && self.image_preview.focus_active {
            if let Some(previous) = self.image_preview.previous_focus.take() {
                previous.focus(window, cx);
            }
            self.image_preview.focus_active = false;
        }
        if self.project_creation.open && self.project_creation.focus_pending {
            self.project_creation.focus.focus(window, cx);
            self.project_creation.focus_pending = false;
        }
        if self.right_panel.open && self.right_panel.focus_pending {
            self.right_panel.focus.focus(window, cx);
            self.right_panel.focus_pending = false;
        }
        if self.bottom_panel.add_menu_open && self.bottom_panel.focus_pending {
            self.bottom_panel.focus.focus(window, cx);
            self.bottom_panel.focus_pending = false;
        }
        let sidebar_width = self.sidebar.read(cx).width();
        let sidebar_reveal = self.sidebar_layout.reveal.clamp(0.0, 1.0);
        let revealed_sidebar_width = sidebar_width * sidebar_reveal;
        let resumed_title = match &self.active_conversation {
            ConversationKey::Thread(id) if !self.showing_settings => {
                self.workspace_store.snapshot().thread(id).map(|thread| {
                    (
                        thread.title.clone(),
                        crate::workspace::project_id_for_thread(
                            thread,
                            &self.workspace_store.snapshot().projects,
                        )
                        .is_some(),
                    )
                })
            }
            _ => None,
        };
        // CDP at both 2560×1410 and the project's 1440×900 target showed a
        // persisted 1418.21875 px panel, clamped to leave the main thread at
        // its measured 773.09375 px right edge on narrower windows.
        let viewport_width = f32::from(window.viewport_size().width);
        let default_right_panel_width = (window.viewport_size().width
            - px(if self.right_panel.mode == Some(RightPanelMode::Review) {
                759.66406
            } else {
                773.09375
            }))
        .min(px(1_418.218_8))
        .max(px(RIGHT_PANEL_MIN_WIDTH));
        let review_fullscreen = self.right_panel.open
            && matches!(
                self.right_panel.mode,
                Some(RightPanelMode::Review | RightPanelMode::SideChat)
            )
            && self.right_panel.fullscreen;
        let right_panel_width = if review_fullscreen {
            px(viewport_width - revealed_sidebar_width)
        } else {
            px(clamp_right_panel_width(
                self.right_panel
                    .width
                    .unwrap_or(f32::from(default_right_panel_width)),
                viewport_width,
                revealed_sidebar_width,
            ))
        };
        div()
            .id(if self.showing_settings {
                "app-shell-settings"
            } else {
                "app-shell"
            })
            .size_full()
            .track_focus(&self.root_focus)
            .relative()
            .flex()
            .font(ui_font())
            .on_click(cx.listener(|this, _, _, cx| {
                this.home.update(cx, |home, cx| home.close_model_picker(cx));
                this.sidebar
                    .update(cx, |sidebar, cx| sidebar.close_transient_menus(cx));
                this.close_bottom_panel_menu(cx);
                if this.right_panel.subagent_menu_open {
                    this.right_panel.subagent_menu_open = false;
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &OpenFiles, _, cx| {
                this.open_files(cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this,_:&crate::components::file_panel::OpenWorkspaceReview,_,cx|{this.right_panel.open=true;this.select_right_panel_item(4,cx);cx.stop_propagation();}))
            .on_action(cx.listener(|this, event: &OpenWorkspaceFile, _, cx| {
                this.open_files(cx);
                this.file_panels[&this.active_conversation].update(cx, |p,cx| p.open_path(PathBuf::from(&event.path),event.line,cx));
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &ToggleTerminal, _, cx| {
                if this.right_panel.open && this.right_panel.mode == Some(RightPanelMode::Terminal) { this.close_right_panel(cx); }
                else { this.right_panel.open = true; this.select_right_panel_item(2, cx); }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &ToggleReview, _, cx| {
                if this.right_panel.open && this.right_panel.mode == Some(RightPanelMode::Review) { this.close_right_panel(cx); }
                else { this.open_review(cx); }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &OpenSideChat, _, cx| {
                this.right_panel.open = true;
                this.select_right_panel_item(0, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::components::side_chat::RestoreSideChat, _, cx| {
                this.deactivate_review(cx);
                this.right_panel.open = true;
                this.right_panel.mode = Some(RightPanelMode::SideChat);
                this.right_panel.fullscreen = false;
                this.right_panel.diff_review = None;
                this.ensure_side_chat(false, cx);
                cx.stop_propagation();
            }))
            .on_key_down(cx.listener(Self::handle_project_creation_key))
            .on_action(cx.listener(|_,_:&super::CaptureFrame,_window,_cx| {
                #[cfg(feature = "screenshot")]
                if let Ok(path) = std::env::var("GPUI_CAPTURE_OUTPUT") { crate::capture_frame(_window,path,3); }
            }))
            .on_action(cx.listener(|this, _: &DismissPermissionUi, window, cx| {
                if this.right_panel.open && this.right_panel.mode == Some(RightPanelMode::SideChat)
                    && let Some(panel) = this.side_chat_panels.get(&this.active_conversation)
                    && panel.update(cx, |panel, cx| panel.dismiss_transient(cx)) {
                    cx.stop_propagation(); return;
                }
                if this.right_panel.open&&this.right_panel.mode==Some(RightPanelMode::Review)
                    &&let Some(panel)=this.review_panels.get(&this.active_conversation)
                    &&panel.update(cx,|p,cx|p.dismiss_transient(window,cx)) {
                    cx.stop_propagation();return;
                }
                if this.image_preview.path.is_some() {
                    this.image_preview.path = None;
                    this.image_preview.dimensions = None;
                    this.image_preview.zoom = 1.0;
                    cx.stop_propagation();
                    cx.notify();
                } else if this.bottom_panel.add_menu_open {
                    this.close_bottom_panel_menu(cx);
                } else if this.project_creation.open {
                    this.close_project_creation(cx);
                } else if this.permission_confirmation_open {
                    this.permission_confirmation_open = false;
                    cx.notify();
                } else {
                    this.home.update(cx, |home, cx| { home.close_model_picker(cx); home.dismiss_plan_popovers(cx); });
                }
            }))
            .when(self.showing_settings, |shell| {
                shell.child(self.settings.clone())
            })
            .when(!self.showing_settings, |shell| {
                shell
                    .child(
                        div()
                            .w(px(revealed_sidebar_width))
                            .min_w(px(revealed_sidebar_width))
                            .h_full()
                            .flex_none()
                            .overflow_hidden()
                            // Electron paints the translucent surface on the
                            // outer aside; only its inner contents fade while
                            // the panel collapses.
                            .bg(theme.sidebar_surface)
                            .child(
                                div()
                                    .w(px(sidebar_width))
                                    .min_w(px(sidebar_width))
                                    .h_full()
                                    // Avoid putting the settled sidebar foreground
                                    // through an opacity context. On a translucent
                                    // window, text already uses grayscale AA; an
                                    // additional alpha blend makes glyph and SVG
                                    // edges look soft over bright backgrounds.
                                    .when(sidebar_reveal < 1.0, |content| {
                                        content.opacity(sidebar_reveal)
                                    })
                                    .child(self.sidebar.clone()),
                            ),
                    )
                    // Sidebar scrolling dirties its ancestor view by design. Keep the
                    // much larger, static home/composer subtree cached so a wheel or
                    // trackpad frame does not rebuild and repaint the main pane.
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .h_full()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .w_full()
                                    .min_h(px(0.0))
                                    .flex_1()
                                    .flex()
                                    .when(!review_fullscreen,|row|row.child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .h_full()
                                            // Keep the conversation surface away from both
                                            // workspace edges when a side panel narrows the main
                                            // column. Max-width content remains unchanged on wide
                                            // windows because HomeView still centers it internally.
                                            .px(px(CHAT_CONTENT_HORIZONTAL_GUTTER))
                                            .bg(theme.surface)
                                            .child(
                                                if self.home.read(cx).needs_live_interaction_render(cx) {
                                                    // Interactive activity focus, AX nodes and text selection
                                                    // must remain registered on every frame.
                                                    self.home.clone().into_any_element()
                                                } else {
                                                    self.home.clone().cached(
                                                        StyleRefinement::default().size_full(),
                                                    ).into_any_element()
                                                },
                                            ),
                                    ))
                                    .when(self.right_panel.open, |row| {
                                        row.child(self.right_panel(
                                            right_panel_width,
                                            theme,
                                            cx,
                                        ))
                                    }),
                            )
                            .when(self.bottom_panel.open, |workspace| {
                                workspace.child(self.bottom_panel(theme, cx))
                            }),
                    )
            })
            .when_some(review_overlay,|shell,overlay|shell.child(overlay))
            .when_some(side_chat_overlay,|shell,overlay|shell.child(overlay))
            .when(self.permission_confirmation_open, |shell| {
                shell.child(
                    div()
                        .id("permission-confirmation-overlay")
                        .absolute()
                        .inset_0()
                        // Electron computed style: #00000022.
                        .bg(rgba(0x00000022))
                        .flex()
                        .items_center()
                        .justify_center()
                        .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
                        .child(
                            div()
                                .id("permission-confirmation-dialog")
                                .w(px(520.0))
                                .h(px(376.6875))
                                .rounded(px(25.0))
                                .border(px(0.5))
                                .border_color(theme.border)
                                .bg(theme.model_picker_surface)
                                .shadow(vec![
                                    BoxShadow::new(px(0.0), px(4.0), hsla(0.0, 0.0, 0.0, 0.10))
                                        .blur_radius(px(8.0))
                                        .spread_radius(px(-2.0)),
                                ])
                                .p(px(20.0))
                                .flex()
                                .flex_col()
                                .text_color(theme.text)
                                .child(
                                    div()
                                        .h(px(28.0))
                                        .flex()
                                        .items_start()
                                        .gap(px(8.0))
                                        .child(icon("permission-warning", theme.text.into()).size(px(20.0)))
                                        .child(
                                            div()
                                                .text_size(px(20.0))
                                                .line_height(px(24.0))
                                                .font_weight(gpui::FontWeight(600.0))
                                                .child("要开启完整访问权限吗？"),
                                        ),
                                )
                                .child(
                                    div()
                                        .mt(px(12.0))
                                        .text_size(px(14.0))
                                        .line_height(px(21.0))
                                        .text_color(theme.text_tertiary)
                                        .child("Codex 将能够在未经您许可的情况下，在这台计算机上的任何位置运行命令、\n使用互联网，以及创建和编辑文件。这包括但不限于："),
                                )
                                .child(
                                    div()
                                        .mt(px(12.0))
                                        .h(px(162.0))
                                        .rounded(px(17.0))
                                        .bg(theme.elevated)
                                        .child(permission_risk_row("permission-dialog-folder", "文件和文件夹", "读取、创建、修改、上传或删除此计算机上任意位置的文件", false, theme))
                                        .child(permission_risk_row("permission-dialog-terminal", "终端命令", "运行命令、安装软件和更改系统设置", true, theme))
                                        .child(permission_risk_row("permission-dialog-internet", "互联网和已连接的应用", "访问网站、发送数据并使用已启用的插件", true, theme)),
                                )
                                .child(
                                    div()
                                        .mt(px(12.0))
                                        .h(px(21.0))
                                        .flex()
                                        .items_center()
                                        .text_size(px(14.0))
                                        .line_height(px(21.0))
                                        .text_color(theme.text_tertiary)
                                        .child(div().flex_1().child("这会带来敏感数据丢失或泄露、提示注入等风险。你可以将其关闭。"))
                                        .child(div().text_color(theme.accent).child("了解更多")),
                                )
                                .child(
                                    div()
                                        .mt(px(12.0))
                                        .h(px(36.0))
                                        .flex()
                                        .justify_end()
                                        .gap(px(12.0))
                                        .child(
                                            div()
                                                .id("permission-confirmation-cancel")
                                                .h(px(36.0))
                                                .px(px(20.0))
                                                .rounded_full()
                                                .bg(theme.text.alpha(0.05))
                                                .flex()
                                                .items_center()
                                                .text_size(px(14.0))
                                                .cursor_pointer()
                                                .hover(move |style| style.bg(theme.text.alpha(0.10)))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.permission_confirmation_open = false;
                                                    cx.notify();
                                                }))
                                                .child("取消"),
                                        )
                                        .child(
                                            div()
                                                .id("permission-confirmation-confirm")
                                                .h(px(36.0))
                                                .px(px(20.0))
                                                .rounded_full()
                                                .bg(rgba(0xff67641a))
                                                .flex()
                                                .items_center()
                                                .gap(px(4.0))
                                                .text_size(px(14.0))
                                                .text_color(rgba(0xff6764ff))
                                                .cursor_pointer()
                                                .hover(|style| style.bg(rgba(0xff676433)))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.permission_confirmation_open = false;
                                                    if let Some(composer) = this.permission_confirmation_target.take() {
                                                        composer.update(cx, |composer, cx| composer.confirm_full_access(cx));
                                                    } else { this.home.update(cx, |home, cx| home.confirm_full_access(cx)); }
                                                    cx.notify();
                                                }))
                                                .child(icon("permission-warning", rgba(0xff6764ff).into()).size(px(16.0)))
                                                .child("确认"),
                                        ),
                                ),
                        ),
                )
            })
            .when_some(resumed_title.filter(|_| !review_fullscreen), |shell, (title, in_project)| {
                // The resumed thread has its own opaque sticky header. Paint
                // it over the virtual list's overdraw band, just as Electron
                // masks scrolling Markdown beneath its 46px titlebar.
                shell.child(div()
                    .id("resumed-thread-header")
                    .absolute().top_0().left(px(revealed_sidebar_width))
                    .right(if self.right_panel.open { right_panel_width } else { px(0.0) })
                    .h(px(46.0)).bg(theme.surface).border_b_1().border_color(theme.border)
                    .pl(px(if sidebar_reveal < 0.5 { 184.0 } else { 14.0 })).pr(px(100.0))
                    .flex().items_center().gap(px(12.0))
                        .text_size(px(14.0)).line_height(px(20.0)).font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme.text)
                    .when(in_project, |header| header.child(icon("folder", theme.text.into()).size(px(16.0)).flex_none()))
                    .child(div().min_w(px(0.0)).truncate().child(title)))
            })
            .when(!self.showing_settings && sidebar_reveal == 1.0, |shell| {
                shell.child(self.sidebar_resize_handle(theme, revealed_sidebar_width, cx))
            })
            // Keep the draggable titlebar behind its interactive controls so
            // their 28px hover hit areas receive pointer events.
            .child(titlebar_interaction_area())
            .when(!self.showing_settings, |shell| {
                shell
                    .child(
                        div()
                            .absolute()
                            .top(px(LEADING_TITLEBAR_CONTROLS_TOP))
                            .left(px(88.0))
                            .flex()
                            .gap(px(4.0))
                            .child(
                                titlebar_icon_button("sidebar-toggle", false, false, theme).on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.toggle_sidebar(window, cx);
                                    }),
                                ),
                            )
                            .child(titlebar_icon_button("back", false, false, theme))
                            // The captured reference has no forward history, so this
                            // control is intentionally disabled and 40% opaque.
                            .child(titlebar_icon_button("forward", true, false, theme)),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(9.0))
                            .right(px(8.0))
                            .flex()
                            .gap(px(6.0))
                            .child(
                                titlebar_icon_button(
                                    "bottom-panel",
                                    false,
                                    self.bottom_panel.open,
                                    theme,
                                )
                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation()
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.toggle_bottom_panel(cx);
                                })),
                            )
                            .child(
                                titlebar_icon_button(
                                    "right-sidebar",
                                    false,
                                    self.right_panel.open,
                                    theme,
                                )
                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation()
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.toggle_right_panel(cx);
                                })),
                            ),
                    )
            })
            .when(self.project_creation.open, |shell| {
                shell.child(self.project_creation_overlay(theme, cx))
            })
            .when_some(self.image_preview.path.clone(), |shell, path| {
                let viewport = window.viewport_size();
                let zoom = self.image_preview.zoom;
                let image_width = (f32::from(viewport.width) - 64.0).max(160.0) * zoom;
                let image_height = (f32::from(viewport.height) - 128.0).max(120.0) * zoom;
                let percentage = format!("{}%", (zoom * 100.0).round() as i32);
                let preview_dimensions = self.image_preview.dimensions;
                shell.child(
                    div()
                        .id("image-preview-dialog")
                        .track_focus(&self.image_preview.focus)
                        .role(Role::Dialog)
                        .aria_label("图片预览")
                        .absolute()
                        .inset_0()
                        .bg(theme.surface)
                        .overflow_hidden()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.image_preview.path = None;
                            this.image_preview.dimensions = None;
                            this.image_preview.zoom = 1.0;
                            cx.notify();
                        }))
                        .child(
                            div()
                                .id("image-preview-image-scroll")
                                .absolute()
                                .inset_0()
                                .pt(px(48.0))
                                .pb(px(80.0))
                                .px(px(32.0))
                                .overflow_scroll()
                                .flex()
                                .items_center()
                                .justify_center()
                                .on_click(|_, _, cx| cx.stop_propagation())
                                .child(
                                    gpui::img(path.clone())
                                        .w(px(image_width))
                                        .h(px(image_height))
                                        .flex_none()
                                        .rounded(px(12.5))
                                        .object_fit(ObjectFit::Contain),
                                ),
                        )
                        .child(
                            div()
                                .absolute()
                                .top(px(12.0))
                                .right(px(12.0))
                                .flex()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .id("image-preview-open-original")
                                        .h(px(40.0))
                                        .min_w(px(40.0))
                                        .px(px(12.0))
                                        .rounded_full()
                                        .bg(theme.model_picker_surface.alpha(0.95))
                                        .shadow(vec![
                                            BoxShadow::new(
                                                px(0.0),
                                                px(2.0),
                                                rgba(0x00000014).into(),
                                            )
                                            .blur_radius(px(4.0))
                                            .spread_radius(px(-1.0)),
                                        ])
                                        .role(Role::Button)
                                        .aria_label("下载图片")
                                        .focusable()
                                        .tab_stop(true)
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .hover(move |button| button.bg(theme.elevated))
                                        .on_click({
                                            let path = path.clone();
                                            cx.listener(move |this, _, _, cx| {
                                                this.download_preview_image(path.clone(), cx);
                                                cx.stop_propagation();
                                            })
                                        })
                                        .on_key_down({
                                            let path = path.clone();
                                            cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                                                if matches!(
                                                    event.keystroke.key.as_str(),
                                                    "enter" | "space"
                                                ) {
                                                    this.download_preview_image(path.clone(), cx);
                                                    cx.stop_propagation();
                                                }
                                            })
                                        })
                                        .child(
                                            icon("image-download", theme.text.into()).size(px(20.0)),
                                        ),
                                )
                                .child(
                                    div()
                                        .id("image-preview-close")
                                        .h(px(40.0))
                                        .min_w(px(40.0))
                                        .px(px(12.0))
                                        .rounded_full()
                                        .bg(theme.model_picker_surface.alpha(0.95))
                                        .shadow(vec![
                                            BoxShadow::new(
                                                px(0.0),
                                                px(2.0),
                                                rgba(0x00000014).into(),
                                            )
                                            .blur_radius(px(4.0))
                                            .spread_radius(px(-1.0)),
                                        ])
                                        .role(Role::Button)
                                        .aria_label("关闭图片预览")
                                        .focusable()
                                        .tab_stop(true)
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .hover(move |button| button.bg(theme.elevated))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.image_preview.path = None;
                                            this.image_preview.dimensions = None;
                                            this.image_preview.zoom = 1.0;
                                            cx.stop_propagation();
                                            cx.notify();
                                        }))
                                        .on_key_down(cx.listener(
                                            |this, event: &KeyDownEvent, _, cx| {
                                                if matches!(
                                                    event.keystroke.key.as_str(),
                                                    "enter" | "space"
                                                ) {
                                                    this.image_preview.path = None;
                                                    this.image_preview.dimensions = None;
                                                    this.image_preview.zoom = 1.0;
                                                    cx.stop_propagation();
                                                    cx.notify();
                                                }
                                            },
                                        ))
                                        .child(icon("close-dialog", theme.text.into()).size(px(21.0))),
                                ),
                        )
                        .child(
                            div()
                                .absolute()
                                .bottom(px(32.0))
                                .left_0()
                                .right_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    div()
                                        .id("image-preview-zoom-controls")
                                        .h(px(36.0))
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .on_click(|_, _, cx| cx.stop_propagation())
                                        .when_some(preview_dimensions, |controls, (width, height)| {
                                            controls.child(
                                                div()
                                                    .px(px(10.0))
                                                    .text_size(px(13.0))
                                                    .text_color(theme.text_secondary)
                                                    .child(format!("{width} × {height}")),
                                            )
                                        })
                                        .child(
                                            div()
                                                .id("image-preview-zoom-out")
                                                .size(px(36.0))
                                                .rounded_full()
                                                .bg(theme.text.alpha(0.10))
                                                .role(Role::Button)
                                                .aria_label("缩小图片")
                                                .focusable()
                                                .tab_stop(true)
                                                .cursor_pointer()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_size(px(20.0))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.image_preview.zoom =
                                                        (this.image_preview.zoom - 0.25).max(0.5);
                                                    cx.notify();
                                                }))
                                                .child("−"),
                                        )
                                        .child(
                                            div()
                                                .w(px(56.0))
                                                .text_center()
                                                .text_size(px(13.0))
                                                .text_color(theme.text)
                                                .child(percentage),
                                        )
                                        .child(
                                            div()
                                                .id("image-preview-zoom-in")
                                                .size(px(36.0))
                                                .rounded_full()
                                                .bg(theme.text.alpha(0.10))
                                                .role(Role::Button)
                                                .aria_label("放大图片")
                                                .focusable()
                                                .tab_stop(true)
                                                .cursor_pointer()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_size(px(20.0))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.image_preview.zoom =
                                                        (this.image_preview.zoom + 0.25).min(3.0);
                                                    cx.notify();
                                                }))
                                                .child("+"),
                                        ),
                                ),
                        ),
                )
            })
            .when_some(self.plan_export_error.clone(), |root, error| root.child(div().id("plan-export-error").role(Role::Alert).absolute().bottom(px(24.0)).right(px(24.0)).max_w(px(400.0)).p(px(12.0)).rounded(px(12.0)).bg(theme.surface).border_1().border_color(theme.border).text_color(theme.text).child(error)))
            .into_any_element()
    }
}
