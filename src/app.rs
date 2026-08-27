use std::time::{Duration, Instant};

use gpui::{
    BoxShadow, Context, Div, Entity, IntoElement, Render, StyleRefinement, Transformation, Window,
    div, hsla, prelude::*, px, radians, rgba,
};

gpui::actions!(permission_ui, [DismissPermissionUi]);

use crate::{
    components::{
        composer::RequestFullAccess,
        home::HomeView,
        icons::icon,
        sidebar::{OpenSettings, SidebarView},
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

impl Render for ChatApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_mode(self.mode);
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
            .on_action(cx.listener(|this, _: &DismissPermissionUi, _, cx| {
                if this.permission_confirmation_open {
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
