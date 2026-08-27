use std::time::{Duration, Instant};

use gpui::{
    BoxShadow, Context, Div, Entity, FocusHandle, IntoElement, KeyDownEvent, MouseButton,
    PathPromptOptions, Render, StyleRefinement, Transformation, Window, div, hsla, prelude::*, px,
    radians, rgba,
};

gpui::actions!(permission_ui, [DismissPermissionUi]);

use crate::{
    components::{
        composer::RequestFullAccess,
        home::HomeView,
        icons::icon,
        sidebar::{OpenProjectCreation, OpenSettings, SidebarView},
    },
    settings::{ChangeTheme, CloseSettings, SettingsView},
    theme::{Theme, ThemeMode},
};

pub struct ChatApp {
    mode: ThemeMode,
    sidebar: Entity<SidebarView>,
    home: Entity<HomeView>,
    settings: Entity<SettingsView>,
    showing_settings: bool,
    sidebar_collapsed: bool,
    sidebar_reveal: f32,
    sidebar_animation_from: f32,
    sidebar_animation_to: f32,
    sidebar_animation_started_at: Option<Instant>,
    sidebar_animation_duration: Duration,
    sidebar_animation_running: bool,
    permission_confirmation_open: bool,
    project_creation_open: bool,
    project_creation_kind: ProjectCreationKind,
    project_creation_step: ProjectCreationStep,
    project_creation_focused_item: usize,
    project_creation_keyboard_focus: bool,
    project_creation_focus: FocusHandle,
    project_creation_focus_pending: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProjectCreationKind {
    Local,
    Remote,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProjectCreationStep {
    Kind,
    Remote,
}

const SIDEBAR_TRANSITION_DURATION: Duration = Duration::from_millis(400);

fn sidebar_transition_ease(progress: f32) -> f32 {
    fn bezier(t: f32, first: f32, second: f32) -> f32 {
        let inverse = 1.0 - t;
        3.0 * inverse * inverse * t * first + 3.0 * inverse * t * t * second + t * t * t
    }

    let progress = progress.clamp(0.0, 1.0);
    if progress == 0.0 || progress == 1.0 {
        return progress;
    }

    // chat-reference: --cubic-enter: cubic-bezier(.19, 1, .22, 1)
    let (mut lower, mut upper) = (0.0, 1.0);
    for _ in 0..12 {
        let parameter = (lower + upper) * 0.5;
        if bezier(parameter, 0.19, 0.22) < progress {
            lower = parameter;
        } else {
            upper = parameter;
        }
    }
    bezier((lower + upper) * 0.5, 1.0, 1.0)
}

fn titlebar_interaction_area() -> impl IntoElement {
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

fn titlebar_icon_button(
    name: &'static str,
    disabled: bool,
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
        .when(disabled, |button| button.opacity(0.4).cursor_default())
        .when(!disabled, |button| {
            button
                .cursor_pointer()
                .hover(move |style| style.bg(theme.sidebar_hover))
        })
        // The reference SVG declares 20x20, but its `icon-xs` class wins in
        // computed style and renders the glyph at 16x16.
        .child(glyph)
}

impl ChatApp {
    pub fn new(mode: ThemeMode, scroll_sidebar_to_bottom: bool, cx: &mut Context<Self>) -> Self {
        let sidebar = cx.new(|_| SidebarView::new(mode, scroll_sidebar_to_bottom));
        let settings = cx.new(|_| SettingsView::new(mode));
        let home = cx.new(|cx| HomeView::new(mode, cx));
        cx.subscribe(&sidebar, |this, _, _: &OpenSettings, cx| {
            this.showing_settings = true;
            cx.notify();
        })
        .detach();
        cx.subscribe(&sidebar, |this, _, _: &OpenProjectCreation, cx| {
            this.open_project_creation(cx);
        })
        .detach();
        cx.subscribe(&settings, |this, _, _: &CloseSettings, cx| {
            this.showing_settings = false;
            cx.notify();
        })
        .detach();
        cx.subscribe(&settings, |this, _, event: &ChangeTheme, cx| {
            this.mode = event.0;
            this.sidebar.update(cx, |sidebar, cx| {
                sidebar.set_mode(event.0, cx);
            });
            this.home.update(cx, |home, cx| {
                home.set_mode(event.0, cx);
            });
            cx.notify();
        })
        .detach();
        cx.subscribe(&home, |this, _, _: &RequestFullAccess, cx| {
            this.permission_confirmation_open = true;
            cx.notify();
        })
        .detach();
        Self {
            mode,
            sidebar,
            home,
            settings,
            showing_settings: false,
            sidebar_collapsed: false,
            sidebar_reveal: 1.0,
            sidebar_animation_from: 1.0,
            sidebar_animation_to: 1.0,
            sidebar_animation_started_at: None,
            sidebar_animation_duration: Duration::ZERO,
            sidebar_animation_running: false,
            permission_confirmation_open: false,
            project_creation_open: false,
            project_creation_kind: ProjectCreationKind::Local,
            project_creation_step: ProjectCreationStep::Kind,
            project_creation_focused_item: 0,
            project_creation_keyboard_focus: false,
            project_creation_focus: cx.focus_handle().tab_stop(true),
            project_creation_focus_pending: false,
        }
    }

    pub fn open_profile_menu(&mut self, cx: &mut Context<Self>) {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_profile_menu_open(true, cx));
    }

    pub fn open_activity(&mut self, cx: &mut Context<Self>) {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_activity_open(true, cx));
    }

    pub fn open_projects_section_menu(&mut self, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.open_projects_section_menu_for_capture(cx)
        });
    }

    pub fn set_activity_scroll_for_capture(&mut self, offset: f32, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_activity_scroll_for_capture(offset, cx)
        });
    }

    pub fn set_activity_hovered_recent_for_capture(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_activity_hovered_recent_for_capture(index, cx)
        });
    }

    pub fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.showing_settings = true;
        cx.notify();
    }

    pub fn open_settings_page(&mut self, slug: &'static str, cx: &mut Context<Self>) {
        self.settings
            .update(cx, |settings, cx| settings.select(slug, cx));
        self.open_settings(cx);
    }

    pub fn open_model_picker(&mut self, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| home.open_model_picker(cx));
    }

    pub fn open_model_picker_submenu(&mut self, name: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.open_model_picker_submenu(name, cx));
    }

    pub fn open_model_picker_slider_at(
        &mut self,
        index: usize,
        fast: bool,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.open_model_picker_slider_at(index, fast, cx)
        });
    }

    pub fn set_dictation_state_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.set_dictation_state_for_capture(state, cx)
        });
    }

    pub fn set_permission_mode(&mut self, mode: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.set_permission_mode(mode, cx));
    }

    pub fn open_permission_menu(&mut self, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.open_permission_menu(cx));
    }

    pub fn open_permission_confirmation(&mut self, cx: &mut Context<Self>) {
        self.permission_confirmation_open = true;
        cx.notify();
    }

    pub fn open_project_creation(&mut self, cx: &mut Context<Self>) {
        self.project_creation_open = true;
        self.project_creation_focused_item = 0;
        self.project_creation_keyboard_focus = false;
        self.project_creation_focus_pending = true;
        self.permission_confirmation_open = false;
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.close_transient_menus(cx);
            sidebar.set_project_creation_trigger_open(true, cx);
        });
        cx.notify();
    }

    pub fn open_project_creation_remote_for_capture(&mut self, cx: &mut Context<Self>) {
        self.open_project_creation(cx);
        self.project_creation_kind = ProjectCreationKind::Remote;
        self.project_creation_step = ProjectCreationStep::Remote;
        cx.notify();
    }

    fn close_project_creation(&mut self, cx: &mut Context<Self>) {
        self.project_creation_open = false;
        self.project_creation_keyboard_focus = false;
        self.project_creation_focus_pending = false;
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_project_creation_trigger_open(false, cx)
        });
        cx.notify();
    }

    fn cancel_project_creation(&mut self, cx: &mut Context<Self>) {
        self.project_creation_kind = ProjectCreationKind::Local;
        self.project_creation_step = ProjectCreationStep::Kind;
        self.close_project_creation(cx);
    }

    fn advance_project_creation(&mut self, cx: &mut Context<Self>) {
        match self.project_creation_kind {
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
                    let sidebar = self.sidebar.clone();
                    cx.spawn(async move |_, cx| {
                        let Ok(Ok(Some(mut paths))) = paths.await else {
                            return;
                        };
                        let Some(path) = paths.pop() else {
                            return;
                        };
                        let _ =
                            sidebar.update(cx, |sidebar, cx| sidebar.add_local_project(&path, cx));
                    })
                    .detach();
                }
                #[cfg(test)]
                drop(paths);
            }
            ProjectCreationKind::Remote => {
                self.project_creation_step = ProjectCreationStep::Remote;
                self.project_creation_focused_item = 0;
                self.project_creation_keyboard_focus = false;
                cx.notify();
            }
        }
    }

    fn handle_project_creation_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.project_creation_open {
            return;
        }

        let key = event.keystroke.key.as_str();
        if key == "escape" {
            self.close_project_creation(cx);
            cx.stop_propagation();
            return;
        }
        if self.project_creation_step == ProjectCreationStep::Remote {
            if key == "tab" {
                let count = 4;
                self.project_creation_focused_item = if event.keystroke.modifiers.shift {
                    (self.project_creation_focused_item + count - 1) % count
                } else {
                    (self.project_creation_focused_item + 1) % count
                };
                self.project_creation_keyboard_focus = true;
                cx.stop_propagation();
                cx.notify();
            } else if matches!(key, "enter" | "space") {
                match self.project_creation_focused_item {
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
                self.project_creation_focused_item = if event.keystroke.modifiers.shift {
                    (self.project_creation_focused_item + count - 1) % count
                } else {
                    (self.project_creation_focused_item + 1) % count
                };
                self.project_creation_keyboard_focus = true;
            }
            "enter" | "space" => match self.project_creation_focused_item {
                0 => self.project_creation_kind = ProjectCreationKind::Local,
                1 => self.project_creation_kind = ProjectCreationKind::Remote,
                2 => self.advance_project_creation(cx),
                3 => self.close_project_creation(cx),
                _ => {}
            },
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
        let target = if self.sidebar_collapsed { 0.0 } else { 1.0 };

        if cx.reduce_motion() || (target - self.sidebar_reveal).abs() <= f32::EPSILON {
            self.sidebar_reveal = target;
            self.sidebar_animation_from = target;
            self.sidebar_animation_to = target;
            self.sidebar_animation_started_at = None;
            self.sidebar_animation_duration = Duration::ZERO;
            self.sidebar_animation_running = false;
            cx.notify();
            return;
        }

        let was_running = self.sidebar_animation_running;
        self.sidebar_animation_from = self.sidebar_reveal;
        self.sidebar_animation_to = target;
        self.sidebar_animation_started_at = Some(cx.background_executor().now());
        self.sidebar_animation_duration = Duration::from_secs_f32(
            SIDEBAR_TRANSITION_DURATION.as_secs_f32() * (target - self.sidebar_reveal).abs(),
        );
        self.sidebar_animation_running = true;
        cx.notify();

        if !was_running {
            cx.on_next_frame(window, |this, window, cx| {
                this.advance_sidebar_animation(window, cx)
            });
        }
    }

    fn advance_sidebar_animation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.sidebar_animation_running {
            return;
        }

        let elapsed = self
            .sidebar_animation_started_at
            .map_or(Duration::ZERO, |started| {
                cx.background_executor()
                    .now()
                    .saturating_duration_since(started)
            });
        let progress = if self.sidebar_animation_duration.is_zero() {
            1.0
        } else {
            elapsed.as_secs_f32() / self.sidebar_animation_duration.as_secs_f32()
        }
        .clamp(0.0, 1.0);

        self.sidebar_reveal = self.sidebar_animation_from
            + (self.sidebar_animation_to - self.sidebar_animation_from)
                * sidebar_transition_ease(progress);

        if progress >= 1.0 || cx.reduce_motion() {
            self.sidebar_reveal = self.sidebar_animation_to;
            self.sidebar_animation_running = false;
            self.sidebar_animation_started_at = None;
        }
        cx.notify();

        if self.sidebar_animation_running {
            cx.on_next_frame(window, |this, window, cx| {
                this.advance_sidebar_animation(window, cx)
            });
        }
    }
}

fn permission_risk_row(
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

fn project_creation_focus_shadow(theme: Theme, visible: bool) -> Vec<BoxShadow> {
    if visible {
        vec![
            BoxShadow::new(px(0.0), px(0.0), theme.accent.alpha(0.76).into())
                .spread_radius(px(2.0)),
        ]
    } else {
        Vec::new()
    }
}

impl ChatApp {
    fn project_kind_card(
        &self,
        index: usize,
        kind: ProjectCreationKind,
        glyph: &'static str,
        label: &'static str,
        detail: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let selected = self.project_creation_kind == kind;
        let keyboard_focused =
            self.project_creation_keyboard_focus && self.project_creation_focused_item == index;
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
                this.project_creation_kind = kind;
                this.project_creation_focused_item = index;
                this.project_creation_keyboard_focus = false;
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

    fn project_creation_close_button(
        &self,
        index: usize,
        label: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let keyboard_focused =
            self.project_creation_keyboard_focus && self.project_creation_focused_item == index;
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

    fn project_creation_kind_dialog(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let next_focused =
            self.project_creation_keyboard_focus && self.project_creation_focused_item == 2;
        div()
            .id("project-creation-dialog")
            .relative()
            .w(px(680.0))
            .h(px(357.796875))
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
                                        ProjectCreationKind::Local,
                                        "project-local",
                                        "本地",
                                        "在你的电脑上编辑、运行和测试文件",
                                        theme,
                                        cx,
                                    ))
                                    .child(self.project_kind_card(
                                        1,
                                        ProjectCreationKind::Remote,
                                        "project-remote",
                                        "远程",
                                        "选择已连接计算机上的文件夹",
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
                                .font_weight(gpui::FontWeight(200.0))
                                .cursor_pointer()
                                .hover(move |style| style.bg(theme.text.alpha(0.8)))
                                .active(move |style| style.bg(theme.text.alpha(0.8)))
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.project_creation_focused_item = 2;
                                    this.project_creation_keyboard_focus = false;
                                    this.advance_project_creation(cx);
                                }))
                                .child("下一步"),
                        ),
                    ),
            )
            .child(self.project_creation_close_button(3, "关闭", theme, cx))
    }

    fn project_creation_remote_dialog(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let keyboard_focus = |index| {
            self.project_creation_keyboard_focus && self.project_creation_focused_item == index
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
            .font_family("PingFang SC")
            .text_size(px(14.0))
            .line_height(px(21.0))
            .font_weight(gpui::FontWeight(300.0))
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
                    .font_weight(gpui::FontWeight(200.0))
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

    fn project_creation_overlay(
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
            .track_focus(&self.project_creation_focus)
            .on_key_down(cx.listener(Self::handle_project_creation_key))
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_project_creation(cx);
            }))
            .child(match self.project_creation_step {
                ProjectCreationStep::Kind => self.project_creation_kind_dialog(theme, cx),
                ProjectCreationStep::Remote => self.project_creation_remote_dialog(theme, cx),
            })
    }
}

impl Render for ChatApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_mode(self.mode);
        if self.project_creation_open && self.project_creation_focus_pending {
            self.project_creation_focus.focus(window, cx);
            self.project_creation_focus_pending = false;
        }
        let sidebar_width = self.sidebar.read(cx).width();
        let sidebar_reveal = self.sidebar_reveal.clamp(0.0, 1.0);
        let revealed_sidebar_width = sidebar_width * sidebar_reveal;
        div()
            .id(if self.showing_settings {
                "app-shell-settings"
            } else {
                "app-shell"
            })
            .size_full()
            .bg(theme.surface)
            .relative()
            .flex()
            .on_click(cx.listener(|this, _, _, cx| {
                this.home.update(cx, |home, cx| home.close_model_picker(cx));
                this.sidebar
                    .update(cx, |sidebar, cx| sidebar.close_transient_menus(cx));
            }))
            .on_key_down(cx.listener(Self::handle_project_creation_key))
            .on_action(cx.listener(|this, _: &DismissPermissionUi, _, cx| {
                if this.project_creation_open {
                    this.close_project_creation(cx);
                } else if this.permission_confirmation_open {
                    this.permission_confirmation_open = false;
                    cx.notify();
                } else {
                    this.home
                        .update(cx, |home, cx| home.close_model_picker(cx));
                }
            }))
            .when(self.showing_settings, |shell| {
                shell.child(self.settings.clone())
            })
            .when(!self.showing_settings, |shell| {
                shell
                    .font_weight(gpui::FontWeight(445.0))
                    .child(
                        div()
                            .w(px(revealed_sidebar_width))
                            .min_w(px(revealed_sidebar_width))
                            .h_full()
                            .flex_none()
                            .overflow_hidden()
                            .child(
                                div()
                                    .w(px(sidebar_width))
                                    .min_w(px(sidebar_width))
                                    .h_full()
                                    .opacity(sidebar_reveal)
                                    .child(self.sidebar.clone()),
                            ),
                    )
                    // Sidebar scrolling dirties its ancestor view by design. Keep the
                    // much larger, static home/composer subtree cached so a wheel or
                    // trackpad frame does not rebuild and repaint the main pane.
                    .child(
                        div().flex_1().min_w(px(0.0)).h_full().child(
                            self.home
                                .clone()
                                .cached(StyleRefinement::default().size_full()),
                        ),
                    )
            })
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
                                                    this.home.update(cx, |home, cx| home.set_permission_mode("full", cx));
                                                    cx.notify();
                                                }))
                                                .child(icon("permission-warning", rgba(0xff6764ff).into()).size(px(16.0)))
                                                .child("确认"),
                                        ),
                                ),
                        ),
                )
            })
            // Keep the draggable titlebar behind its interactive controls so
            // their 28px hover hit areas receive pointer events.
            .child(titlebar_interaction_area())
            .when(!self.showing_settings, |shell| {
                shell
                    .child(
                        div()
                            .absolute()
                            .top(px(9.0))
                            .left(px(88.0))
                            .flex()
                            .gap(px(4.0))
                            .child(
                                titlebar_icon_button("sidebar-toggle", false, theme).on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.toggle_sidebar(window, cx);
                                    }),
                                ),
                            )
                            .child(titlebar_icon_button("back", false, theme))
                            // The captured reference has no forward history, so this
                            // control is intentionally disabled and 40% opaque.
                            .child(titlebar_icon_button("forward", true, theme)),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(9.0))
                            .right(px(8.0))
                            .flex()
                            .gap(px(6.0))
                            .child(titlebar_icon_button("bottom-panel", false, theme))
                            .child(titlebar_icon_button("right-sidebar", false, theme)),
                    )
            })
            .when(self.project_creation_open, |shell| {
                shell.child(self.project_creation_overlay(theme, cx))
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gpui::{
        AppContext, Bounds, MouseButton, TestApp, TestAppWindow, WindowBounds, WindowOptions,
        point, px, size,
    };

    use super::ChatApp;
    use crate::components::sidebar::OpenSettings;
    use crate::theme::ThemeMode;

    fn simulate_next_frame(app: &mut TestApp, window: &TestAppWindow<ChatApp>, elapsed_ms: u64) {
        app.advance_clock(Duration::from_millis(elapsed_ms));
        let handle = window.handle();
        app.update(|cx| {
            cx.update_window(handle.into(), |_, window, cx| {
                window.simulate_next_frame(cx)
            })
            .unwrap()
        });
    }

    #[test]
    fn sidebar_toggle_collapses_and_restores_without_moving_the_titlebar_control() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );

        window.draw();
        assert!(!window.read(|app, _| app.sidebar_collapsed));

        // The control remains at left: 88px, top: 9px in both states.
        let toggle_center = point(px(102.0), px(23.0));
        window.simulate_click(toggle_center, MouseButton::Left);
        assert!(window.read(|app, _| app.sidebar_collapsed));
        assert!(window.read(|app, _| app.sidebar_animation_running));

        simulate_next_frame(&mut app, &window, 200);
        let collapsed_midpoint = window.read(|app, _| app.sidebar_reveal);
        assert!(collapsed_midpoint > 0.0 && collapsed_midpoint < 1.0);

        simulate_next_frame(&mut app, &window, 200);
        assert_eq!(window.read(|app, _| app.sidebar_reveal), 0.0);
        assert!(!window.read(|app, _| app.sidebar_animation_running));

        window.draw();
        window.simulate_click(toggle_center, MouseButton::Left);
        assert!(!window.read(|app, _| app.sidebar_collapsed));
        assert!(window.read(|app, _| app.sidebar_animation_running));

        simulate_next_frame(&mut app, &window, 400);
        assert_eq!(window.read(|app, _| app.sidebar_reveal), 1.0);
        assert!(!window.read(|app, _| app.sidebar_animation_running));
    }

    #[test]
    fn settings_event_from_profile_menu_navigates_to_settings() {
        let mut app = TestApp::new();
        let window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );
        let sidebar = window.read(|app, _| app.sidebar.clone());
        app.update(|cx| sidebar.update(cx, |_, cx| cx.emit(OpenSettings)));
        app.run_until_parked();
        assert!(window.read(|app, _| app.showing_settings));
    }

    #[test]
    fn permission_confirmation_buttons_close_the_modal() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );

        window.update(|chat, _, cx| chat.open_permission_confirmation(cx));
        window.draw();
        window.simulate_click(point(px(553.0), px(500.0)), MouseButton::Left);
        assert!(!window.read(|chat, _| chat.permission_confirmation_open));

        window.update(|chat, _, cx| chat.open_permission_confirmation(cx));
        window.draw();
        window.simulate_click(point(px(645.0), px(500.0)), MouseButton::Left);
        assert!(!window.read(|chat, _| chat.permission_confirmation_open));
    }

    #[test]
    fn clicking_settings_in_profile_menu_opens_settings_surface() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );
        window.draw();
        window.simulate_click(point(px(80.0), px(677.0)), MouseButton::Left);
        window.draw();
        window.simulate_click(point(px(80.0), px(610.0)), MouseButton::Left);
        assert!(window.read(|app, _| app.showing_settings));
    }

    #[test]
    fn projects_menu_closes_when_the_main_surface_is_clicked() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| {
                let mut app = ChatApp::new(ThemeMode::Dark, false, cx);
                app.open_projects_section_menu(cx);
                app
            },
        );

        window.draw();
        assert!(window.read(|app, cx| { app.sidebar.read(cx).projects_section_menu_is_open() }));
        window.simulate_click(point(px(600.0), px(350.0)), MouseButton::Left);
        assert!(!window.read(|app, cx| { app.sidebar.read(cx).projects_section_menu_is_open() }));
    }

    #[test]
    fn project_creation_dialog_matches_reference_close_and_keyboard_behavior() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );

        window.update(|chat, _, cx| chat.open_project_creation(cx));
        window.draw();
        assert!(window.read(|chat, _| chat.project_creation_open));
        assert_eq!(
            window.read(|chat, _| chat.project_creation_kind),
            super::ProjectCreationKind::Local
        );

        window.simulate_keystroke("tab");
        assert_eq!(window.read(|chat, _| chat.project_creation_focused_item), 1);
        window.simulate_keystroke("space");
        assert_eq!(
            window.read(|chat, _| chat.project_creation_kind),
            super::ProjectCreationKind::Remote
        );
        window.simulate_keystroke("tab");
        window.simulate_keystroke("tab");
        assert_eq!(window.read(|chat, _| chat.project_creation_focused_item), 3);
        window.simulate_keystroke("enter");
        assert!(!window.read(|chat, _| chat.project_creation_open));

        window.update(|chat, _, cx| chat.open_project_creation(cx));
        window.draw();
        window.simulate_click(point(px(50.0), px(80.0)), MouseButton::Left);
        assert!(!window.read(|chat, _| chat.project_creation_open));

        window.update(|chat, _, cx| chat.open_project_creation(cx));
        window.draw();
        window.simulate_keystroke("escape");
        assert!(!window.read(|chat, _| chat.project_creation_open));
    }

    #[test]
    fn local_project_next_closes_the_project_type_dialog() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );

        window.update(|chat, _, cx| chat.open_project_creation(cx));
        window.draw();
        window.simulate_click(point(px(732.0), px(493.0)), MouseButton::Left);
        assert!(!window.read(|chat, _| chat.project_creation_open));
    }

    #[test]
    fn remote_project_step_is_preserved_by_dismiss_and_reset_by_cancel() {
        let mut app = TestApp::new();
        let mut window = app.open_window(|_, cx| ChatApp::new(ThemeMode::Dark, false, cx));

        window.update(|chat, _, cx| {
            chat.open_project_creation(cx);
            chat.project_creation_kind = super::ProjectCreationKind::Remote;
            chat.advance_project_creation(cx);
            chat.close_project_creation(cx);
            chat.open_project_creation(cx);
        });
        assert_eq!(
            window.read(|chat, _| chat.project_creation_step),
            super::ProjectCreationStep::Remote
        );

        window.update(|chat, _, cx| chat.cancel_project_creation(cx));
        window.update(|chat, _, cx| chat.open_project_creation(cx));
        assert_eq!(
            window.read(|chat, _| chat.project_creation_step),
            super::ProjectCreationStep::Kind
        );
        assert_eq!(
            window.read(|chat, _| chat.project_creation_kind),
            super::ProjectCreationKind::Local
        );
    }

    #[test]
    fn remote_project_keyboard_order_matches_the_native_dialog() {
        let mut app = TestApp::new();
        let mut window = app.open_window(|_, cx| {
            let mut chat = ChatApp::new(ThemeMode::Dark, false, cx);
            chat.open_project_creation_remote_for_capture(cx);
            chat
        });

        window.draw();
        window.simulate_keystroke("tab");
        assert_eq!(window.read(|chat, _| chat.project_creation_focused_item), 1);
        window.simulate_keystroke("tab");
        assert_eq!(window.read(|chat, _| chat.project_creation_focused_item), 2);
        window.simulate_keystroke("enter");
        assert!(!window.read(|chat, _| chat.project_creation_open));
        assert_eq!(
            window.read(|chat, _| chat.project_creation_step),
            super::ProjectCreationStep::Kind
        );
    }

    #[test]
    fn project_creation_trigger_opens_and_the_same_screen_position_closes_on_the_overlay() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(1440.0), px(900.0)),
                })),
                ..Default::default()
            },
            |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
        );

        window.draw();
        let trigger = point(px(219.0), px(267.0));
        window.simulate_mouse_move(trigger);
        window.draw();
        window.simulate_click(trigger, MouseButton::Left);
        app.run_until_parked();
        assert!(window.read(|chat, _| chat.project_creation_open));

        window.draw();
        window.simulate_click(trigger, MouseButton::Left);
        assert!(!window.read(|chat, _| chat.project_creation_open));
    }

    #[test]
    fn appearance_cards_change_the_application_theme() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| {
                let mut app = ChatApp::new(ThemeMode::Dark, false, cx);
                app.open_settings_page("appearance", cx);
                app
            },
        );

        window.draw();
        window.simulate_click(point(px(583.0), px(250.0)), MouseButton::Left);
        assert_eq!(window.read(|app, _| app.mode), ThemeMode::Light);

        window.draw();
        window.simulate_click(point(px(772.0), px(250.0)), MouseButton::Left);
        assert_eq!(window.read(|app, _| app.mode), ThemeMode::Dark);
    }
}
