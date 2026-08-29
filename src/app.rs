use std::time::{Duration, Instant};

use gpui::{
    BoxShadow, Context, Div, Entity, FocusHandle, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathPromptOptions, Render, StyleRefinement,
    Transformation, Window, WindowAppearance, canvas, deferred, div, hsla, linear_color_stop,
    linear_gradient, prelude::*, px, radians, rgba,
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
    theme::{Theme, ThemeMode, UI_FONT_FAMILY, ui_font},
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
    bottom_panel_open: bool,
    bottom_panel_tabs: Vec<BottomPanelMode>,
    bottom_panel_active_tab: Option<usize>,
    bottom_panel_hovered_tab: Option<usize>,
    bottom_panel_add_menu_open: bool,
    bottom_panel_focused_item: usize,
    bottom_panel_keyboard_focus: bool,
    bottom_panel_focus: FocusHandle,
    bottom_panel_focus_pending: bool,
    right_panel_open: bool,
    right_panel_mode: Option<RightPanelMode>,
    right_panel_focused_item: usize,
    right_panel_keyboard_focus: bool,
    right_panel_focus: FocusHandle,
    right_panel_focus_pending: bool,
    right_panel_width: Option<f32>,
    right_panel_resize_hovered: bool,
    right_panel_resize_dragging: bool,
    right_panel_resize_pointer_offset: f32,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RightPanelMode {
    SideChat,
    Browser,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BottomPanelMode {
    Review,
    Terminal,
    Browser,
    Files,
    SideChat,
}

const BOTTOM_PANEL_ITEMS: &[(BottomPanelMode, &str, &str, &str)] = &[
    (BottomPanelMode::Review, "审查", "⌃⇧G", "panel-review"),
    (BottomPanelMode::Terminal, "终端", "⌃`", "panel-terminal"),
    (BottomPanelMode::Browser, "浏览器", "⌘T", "panel-browser"),
    (BottomPanelMode::Files, "文件", "⌘P", "panel-files"),
    (BottomPanelMode::SideChat, "侧边聊天", "⌥⌘S", "side-chat"),
];
const BOTTOM_PANEL_HEIGHT: f32 = 280.0;

fn bottom_panel_tab_spec(mode: BottomPanelMode) -> (&'static str, &'static str) {
    match mode {
        BottomPanelMode::Review => ("审查", "panel-review"),
        BottomPanelMode::Terminal => ("终端", "panel-terminal"),
        // CDP: a browser item appended from the add menu is titled 新标签页.
        BottomPanelMode::Browser => ("新标签页", "panel-browser"),
        BottomPanelMode::Files => ("文件", "panel-files"),
        BottomPanelMode::SideChat => ("侧边聊天", "side-chat"),
    }
}

const RIGHT_PANEL_ITEMS: &[(RightPanelMode, &str, &str, &str)] = &[
    (RightPanelMode::SideChat, "侧边聊天", "⌥⌘S", "side-chat"),
    (RightPanelMode::Browser, "浏览器", "⌘T", "panel-browser"),
    (RightPanelMode::Terminal, "终端", "⌃`", "panel-terminal"),
];
const RIGHT_PANEL_MIN_WIDTH: f32 = 320.0;
const RIGHT_PANEL_MAIN_MIN_WIDTH: f32 = 384.0;
const MAIN_CONTENT_HORIZONTAL_GUTTER: f32 = 24.0;
// The native 14px traffic lights start at y=18px, so their center is y=25px.
// Center the 28px leading titlebar controls on that same horizontal axis.
const LEADING_TITLEBAR_CONTROLS_TOP: f32 = 11.0;

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

fn right_panel_width_limit(viewport_width: f32, revealed_sidebar_width: f32) -> f32 {
    (viewport_width - revealed_sidebar_width - RIGHT_PANEL_MAIN_MIN_WIDTH)
        .max(RIGHT_PANEL_MIN_WIDTH)
}

fn clamp_right_panel_width(width: f32, viewport_width: f32, revealed_sidebar_width: f32) -> f32 {
    width.clamp(
        RIGHT_PANEL_MIN_WIDTH,
        right_panel_width_limit(viewport_width, revealed_sidebar_width),
    )
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
            cx.set_window_appearance(Some(match event.0 {
                ThemeMode::Light => WindowAppearance::VibrantLight,
                ThemeMode::Dark => WindowAppearance::VibrantDark,
            }));
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
            bottom_panel_open: false,
            bottom_panel_tabs: Vec::new(),
            bottom_panel_active_tab: None,
            bottom_panel_hovered_tab: None,
            bottom_panel_add_menu_open: false,
            bottom_panel_focused_item: 0,
            bottom_panel_keyboard_focus: false,
            bottom_panel_focus: cx.focus_handle().tab_stop(true),
            bottom_panel_focus_pending: false,
            right_panel_open: false,
            right_panel_mode: None,
            right_panel_focused_item: 0,
            right_panel_keyboard_focus: false,
            right_panel_focus: cx.focus_handle().tab_stop(true),
            right_panel_focus_pending: false,
            right_panel_width: None,
            right_panel_resize_hovered: false,
            right_panel_resize_dragging: false,
            right_panel_resize_pointer_offset: 0.0,
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

    pub fn open_project_menu_for_capture(&mut self, index: usize, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.open_project_menu_for_capture(index, cx)
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

    pub fn submit_prompt_for_capture(&mut self, prompt: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.submit_prompt_for_capture(prompt, cx));
    }

    pub fn show_user_message_actions_for_capture(&mut self, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.show_user_message_actions_for_capture(cx)
        });
    }

    pub fn set_command_tool_for_capture(
        &mut self,
        running: bool,
        expanded: bool,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.set_command_tool_for_capture(running, expanded, cx)
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

    pub fn open_bottom_panel(&mut self, cx: &mut Context<Self>) {
        self.bottom_panel_open = true;
        // The real desktop app restores the active terminal tab when the
        // titlebar toggle is used. This clone has no process/session model, so
        // reuse its existing terminal mode without inventing a path or title.
        if self.bottom_panel_tabs.is_empty() {
            self.bottom_panel_tabs.push(BottomPanelMode::Terminal);
            self.bottom_panel_active_tab = Some(0);
        }
        self.bottom_panel_add_menu_open = false;
        self.bottom_panel_keyboard_focus = false;
        self.bottom_panel_focus_pending = false;
        cx.notify();
    }

    fn close_bottom_panel(&mut self, cx: &mut Context<Self>) {
        if self.bottom_panel_open {
            self.bottom_panel_open = false;
            self.bottom_panel_add_menu_open = false;
            self.bottom_panel_keyboard_focus = false;
            self.bottom_panel_focus_pending = false;
            cx.notify();
        }
    }

    fn toggle_bottom_panel(&mut self, cx: &mut Context<Self>) {
        if self.bottom_panel_open {
            self.close_bottom_panel(cx);
        } else {
            self.open_bottom_panel(cx);
        }
    }

    fn close_bottom_panel_menu(&mut self, cx: &mut Context<Self>) {
        if self.bottom_panel_add_menu_open {
            self.bottom_panel_add_menu_open = false;
            self.bottom_panel_keyboard_focus = false;
            self.bottom_panel_focus_pending = false;
            cx.notify();
        }
    }

    fn toggle_bottom_panel_menu(&mut self, cx: &mut Context<Self>) {
        self.bottom_panel_add_menu_open = !self.bottom_panel_add_menu_open;
        self.bottom_panel_focused_item = 0;
        self.bottom_panel_keyboard_focus = false;
        self.bottom_panel_focus_pending = self.bottom_panel_add_menu_open;
        cx.notify();
    }

    pub fn open_bottom_panel_menu(&mut self, cx: &mut Context<Self>) {
        self.open_bottom_panel(cx);
        self.bottom_panel_add_menu_open = true;
        self.bottom_panel_focused_item = 0;
        self.bottom_panel_keyboard_focus = false;
        self.bottom_panel_focus_pending = true;
        cx.notify();
    }

    pub fn append_bottom_panel_item_for_capture(&mut self, name: &str, cx: &mut Context<Self>) {
        self.open_bottom_panel(cx);
        let index = match name {
            "review" => 0,
            "terminal" => 1,
            "browser" => 2,
            "files" => 3,
            "side-chat" => 4,
            _ => return,
        };
        self.select_bottom_panel_item(index, cx);
    }

    fn select_bottom_panel_item(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((mode, _, _, _)) = BOTTOM_PANEL_ITEMS.get(index) else {
            return;
        };
        self.bottom_panel_tabs.push(*mode);
        self.bottom_panel_active_tab = Some(self.bottom_panel_tabs.len() - 1);
        self.bottom_panel_hovered_tab = None;
        self.bottom_panel_add_menu_open = false;
        self.bottom_panel_keyboard_focus = false;
        self.bottom_panel_focus_pending = false;
        cx.notify();
    }

    fn activate_bottom_panel_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.bottom_panel_tabs.len() {
            self.bottom_panel_active_tab = Some(index);
            self.close_bottom_panel_menu(cx);
            cx.notify();
        }
    }

    fn close_bottom_panel_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.bottom_panel_tabs.len() {
            return;
        }
        self.bottom_panel_tabs.remove(index);
        self.bottom_panel_hovered_tab = None;
        self.bottom_panel_active_tab = match self.bottom_panel_active_tab {
            None => None,
            Some(_) if self.bottom_panel_tabs.is_empty() => None,
            Some(active) if active == index => Some(index.min(self.bottom_panel_tabs.len() - 1)),
            Some(active) if active > index => Some(active - 1),
            Some(active) => Some(active),
        };
        self.close_bottom_panel_menu(cx);
        cx.notify();
    }

    fn handle_bottom_panel_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.bottom_panel_add_menu_open {
            return;
        }
        match event.keystroke.key.as_str() {
            "down" => {
                self.bottom_panel_focused_item = if self.bottom_panel_keyboard_focus {
                    (self.bottom_panel_focused_item + 1) % BOTTOM_PANEL_ITEMS.len()
                } else {
                    0
                };
                self.bottom_panel_keyboard_focus = true;
            }
            "up" => {
                self.bottom_panel_focused_item = if self.bottom_panel_keyboard_focus {
                    (self.bottom_panel_focused_item + BOTTOM_PANEL_ITEMS.len() - 1)
                        % BOTTOM_PANEL_ITEMS.len()
                } else {
                    BOTTOM_PANEL_ITEMS.len() - 1
                };
                self.bottom_panel_keyboard_focus = true;
            }
            // Radix keeps focus inside the open dropdown and ignores Tab.
            "tab" => {
                cx.stop_propagation();
                return;
            }
            "home" => {
                self.bottom_panel_focused_item = 0;
                self.bottom_panel_keyboard_focus = true;
            }
            "end" => {
                self.bottom_panel_focused_item = BOTTOM_PANEL_ITEMS.len() - 1;
                self.bottom_panel_keyboard_focus = true;
            }
            "enter" | "space" => {
                self.select_bottom_panel_item(self.bottom_panel_focused_item, cx);
            }
            "escape" => self.close_bottom_panel_menu(cx),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub fn open_right_panel(&mut self, cx: &mut Context<Self>) {
        self.right_panel_open = true;
        self.right_panel_mode = None;
        self.right_panel_focused_item = 0;
        self.right_panel_keyboard_focus = false;
        self.right_panel_focus_pending = true;
        cx.notify();
    }

    fn close_right_panel(&mut self, cx: &mut Context<Self>) {
        if self.right_panel_open {
            self.right_panel_open = false;
            self.right_panel_mode = None;
            self.right_panel_keyboard_focus = false;
            self.right_panel_focus_pending = false;
            self.right_panel_resize_hovered = false;
            self.right_panel_resize_dragging = false;
            cx.notify();
        }
    }

    fn toggle_right_panel(&mut self, cx: &mut Context<Self>) {
        if self.right_panel_open {
            self.close_right_panel(cx);
        } else {
            self.open_right_panel(cx);
        }
    }

    fn select_right_panel_item(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((mode, _, _, _)) = RIGHT_PANEL_ITEMS.get(index) else {
            return;
        };
        self.right_panel_mode = Some(*mode);
        self.right_panel_keyboard_focus = false;
        cx.notify();
    }

    fn handle_right_panel_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.right_panel_open {
            return;
        }
        match event.keystroke.key.as_str() {
            "down" | "tab" if !event.keystroke.modifiers.shift => {
                self.right_panel_focused_item = if self.right_panel_keyboard_focus {
                    (self.right_panel_focused_item + 1) % RIGHT_PANEL_ITEMS.len()
                } else {
                    0
                };
                self.right_panel_keyboard_focus = true;
            }
            "up" => {
                self.right_panel_focused_item = if self.right_panel_keyboard_focus {
                    (self.right_panel_focused_item + RIGHT_PANEL_ITEMS.len() - 1)
                        % RIGHT_PANEL_ITEMS.len()
                } else {
                    RIGHT_PANEL_ITEMS.len() - 1
                };
                self.right_panel_keyboard_focus = true;
            }
            "tab" => {
                self.right_panel_focused_item = if self.right_panel_keyboard_focus {
                    (self.right_panel_focused_item + RIGHT_PANEL_ITEMS.len() - 1)
                        % RIGHT_PANEL_ITEMS.len()
                } else {
                    RIGHT_PANEL_ITEMS.len() - 1
                };
                self.right_panel_keyboard_focus = true;
            }
            "home" => {
                self.right_panel_focused_item = 0;
                self.right_panel_keyboard_focus = true;
            }
            "end" => {
                self.right_panel_focused_item = RIGHT_PANEL_ITEMS.len() - 1;
                self.right_panel_keyboard_focus = true;
            }
            "enter" | "space" if self.right_panel_mode.is_none() => {
                self.select_right_panel_item(self.right_panel_focused_item, cx);
            }
            // The docked panel is persistent. Escape belongs to the active
            // conversation/tool and must not hide the panel.
            "escape" => return,
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
            .font_family(UI_FONT_FAMILY)
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

    fn bottom_panel_menu_item(
        &self,
        index: usize,
        label: &'static str,
        shortcut: &'static str,
        glyph: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let focused = self.bottom_panel_keyboard_focus && self.bottom_panel_focused_item == index;
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
                    this.bottom_panel_focused_item = index;
                    this.bottom_panel_keyboard_focus = false;
                    this.bottom_panel_focus.focus(window, cx);
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

    fn bottom_panel_add_menu(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
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
            .track_focus(&self.bottom_panel_focus)
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

    fn bottom_panel_tab(
        &self,
        index: usize,
        mode: BottomPanelMode,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let active = self.bottom_panel_active_tab == Some(index);
        let hovered = self.bottom_panel_hovered_tab == Some(index);
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
                        if this.bottom_panel_hovered_tab != next {
                            this.bottom_panel_hovered_tab = next;
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

    fn bottom_panel(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
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
                        self.bottom_panel_tabs
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
                            .when(self.bottom_panel_add_menu_open, |button| {
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
                    .when(self.bottom_panel_add_menu_open, |anchor| {
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

    fn right_panel_menu_item(
        &self,
        index: usize,
        label: &'static str,
        shortcut: &'static str,
        glyph: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let focused = self.right_panel_keyboard_focus && self.right_panel_focused_item == index;
        div()
            .id(("right-panel-menu-item", index))
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
                    this.right_panel_focused_item = index;
                    this.right_panel_keyboard_focus = false;
                    this.right_panel_focus.focus(window, cx);
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

    fn right_panel_resize_handle(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let entity = cx.entity();
        let line_visible = self.right_panel_resize_hovered || self.right_panel_resize_dragging;
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
                        this.right_panel_resize_dragging = true;
                        this.right_panel_resize_hovered = true;
                        this.right_panel_resize_pointer_offset =
                            divider_x - f32::from(event.position.x);
                        // Resolve the responsive default to a persisted width as
                        // soon as the user starts dragging it.
                        if this.right_panel_width.is_none() {
                            this.right_panel_width = Some(current_width);
                        }
                        cx.notify();
                    });
                });

                let mouse_move_entity = entity.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, _, window, cx| {
                    let pointer_inside = bounds.contains(&event.position);
                    mouse_move_entity.update(cx, |this, cx| {
                        let mut changed = false;
                        if this.right_panel_resize_dragging {
                            let viewport_width = f32::from(window.viewport_size().width);
                            let revealed_sidebar_width =
                                this.sidebar.read(cx).width() * this.sidebar_reveal;
                            let divider_x = f32::from(event.position.x)
                                + this.right_panel_resize_pointer_offset;
                            let next_width = clamp_right_panel_width(
                                viewport_width - divider_x,
                                viewport_width,
                                revealed_sidebar_width,
                            );
                            if this
                                .right_panel_width
                                .is_none_or(|width| (width - next_width).abs() > f32::EPSILON)
                            {
                                this.right_panel_width = Some(next_width);
                                changed = true;
                            }
                        }
                        let next_hovered = pointer_inside || this.right_panel_resize_dragging;
                        if this.right_panel_resize_hovered != next_hovered {
                            this.right_panel_resize_hovered = next_hovered;
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
                        if !this.right_panel_resize_dragging {
                            return;
                        }
                        this.right_panel_resize_dragging = false;
                        this.right_panel_resize_hovered = bounds.contains(&event.position);
                        cx.notify();
                    });
                });
            },
        )
        .absolute()
        .inset_0();

        div()
            .id("right-panel-resize-handle")
            .absolute()
            .top_0()
            .bottom_0()
            .left(px(-8.0))
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

    fn right_panel(
        &self,
        panel_width: gpui::Pixels,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let toolbar = div()
            .h(px(46.0))
            .w_full()
            .flex_none()
            .px(px(8.0))
            .flex()
            .items_center()
            .when_some(self.right_panel_mode, |toolbar, mode| {
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
                                    this.right_panel_mode = None;
                                    this.right_panel_keyboard_focus = false;
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
            .track_focus(&self.right_panel_focus)
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
                    .when(self.right_panel_mode.is_none(), |body| {
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

impl Render for ChatApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_mode(self.mode);
        if self.project_creation_open && self.project_creation_focus_pending {
            self.project_creation_focus.focus(window, cx);
            self.project_creation_focus_pending = false;
        }
        if self.right_panel_open && self.right_panel_focus_pending {
            self.right_panel_focus.focus(window, cx);
            self.right_panel_focus_pending = false;
        }
        if self.bottom_panel_add_menu_open && self.bottom_panel_focus_pending {
            self.bottom_panel_focus.focus(window, cx);
            self.bottom_panel_focus_pending = false;
        }
        let sidebar_width = self.sidebar.read(cx).width();
        let sidebar_reveal = self.sidebar_reveal.clamp(0.0, 1.0);
        let revealed_sidebar_width = sidebar_width * sidebar_reveal;
        // CDP at both 2560×1410 and the project's 1440×900 target showed a
        // persisted 1418.21875 px panel, clamped to leave the main thread at
        // its measured 773.09375 px right edge on narrower windows.
        let viewport_width = f32::from(window.viewport_size().width);
        let default_right_panel_width = (window.viewport_size().width - px(773.09375))
            .min(px(1_418.218_8))
            .max(px(RIGHT_PANEL_MIN_WIDTH));
        let right_panel_width = self
            .right_panel_width
            .map_or(default_right_panel_width, |width| {
                px(clamp_right_panel_width(
                    width,
                    viewport_width,
                    revealed_sidebar_width,
                ))
            });
        div()
            .id(if self.showing_settings {
                "app-shell-settings"
            } else {
                "app-shell"
            })
            .size_full()
            // Apply the subtle theme underlay above the native blurred
            // material. The sidebar adds its measured tint on top, while the
            // main pane below is painted fully opaque in its own child.
            .bg(theme.surface_underlay)
            .relative()
            .flex()
            .font(ui_font())
            .on_click(cx.listener(|this, _, _, cx| {
                this.home.update(cx, |home, cx| home.close_model_picker(cx));
                this.sidebar
                    .update(cx, |sidebar, cx| sidebar.close_transient_menus(cx));
                this.close_bottom_panel_menu(cx);
            }))
            .on_key_down(cx.listener(Self::handle_project_creation_key))
            .on_action(cx.listener(|this, _: &DismissPermissionUi, _, cx| {
                if this.bottom_panel_add_menu_open {
                    this.close_bottom_panel_menu(cx);
                } else if this.project_creation_open {
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
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .h_full()
                                            // Keep the conversation surface away from both
                                            // workspace edges when a side panel narrows the main
                                            // column. Max-width content remains unchanged on wide
                                            // windows because HomeView still centers it internally.
                                            .px(px(MAIN_CONTENT_HORIZONTAL_GUTTER))
                                            .bg(theme.surface)
                                            .child(
                                                self.home.clone().cached(
                                                    StyleRefinement::default().size_full(),
                                                ),
                                            ),
                                    )
                                    .when(self.right_panel_open, |row| {
                                        row.child(self.right_panel(
                                            right_panel_width,
                                            theme,
                                            cx,
                                        ))
                                    }),
                            )
                            .when(self.bottom_panel_open, |workspace| {
                                workspace.child(self.bottom_panel(theme, cx))
                            }),
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
                                    self.bottom_panel_open,
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
                                    self.right_panel_open,
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

        // The control remains at left: 88px, centered at y=25px in both states.
        let toggle_center = point(px(102.0), px(25.0));
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
    fn right_panel_stays_open_until_its_titlebar_toggle_is_clicked_again() {
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
        let trigger = point(px(878.0), px(23.0));
        window.simulate_click(trigger, MouseButton::Left);
        assert!(window.read(|chat, _| chat.right_panel_open));

        window.draw();
        window.simulate_click(point(px(400.0), px(400.0)), MouseButton::Left);
        assert!(window.read(|chat, _| chat.right_panel_open));

        window.draw();
        window.simulate_keystroke("escape");
        assert!(window.read(|chat, _| chat.right_panel_open));

        window.draw();
        window.simulate_click(trigger, MouseButton::Left);
        assert!(!window.read(|chat, _| chat.right_panel_open));
    }

    #[test]
    fn bottom_panel_matches_the_persistent_native_titlebar_toggle() {
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
        let trigger = point(px(844.0), px(23.0));
        window.simulate_click(trigger, MouseButton::Left);
        assert!(window.read(|chat, _| chat.bottom_panel_open));
        assert_eq!(
            window.read(|chat, _| chat.bottom_panel_tabs.clone()),
            vec![super::BottomPanelMode::Terminal]
        );
        assert_eq!(window.read(|chat, _| chat.bottom_panel_active_tab), Some(0));

        window.draw();
        window.simulate_click(point(px(400.0), px(300.0)), MouseButton::Left);
        assert!(window.read(|chat, _| chat.bottom_panel_open));
        window.simulate_keystroke("escape");
        assert!(window.read(|chat, _| chat.bottom_panel_open));

        window.draw();
        window.simulate_click(trigger, MouseButton::Left);
        assert!(!window.read(|chat, _| chat.bottom_panel_open));
    }

    #[test]
    fn bottom_panel_add_menu_closes_on_toggle_outside_and_escape() {
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

        window.update(|chat, _, cx| {
            chat.open_bottom_panel(cx);
            chat.toggle_bottom_panel_menu(cx);
        });
        assert!(window.read(|chat, _| chat.bottom_panel_add_menu_open));

        window.update(|chat, _, cx| chat.toggle_bottom_panel_menu(cx));
        assert!(!window.read(|chat, _| chat.bottom_panel_add_menu_open));

        window.update(|chat, _, cx| chat.toggle_bottom_panel_menu(cx));
        window.draw();
        window.simulate_click(point(px(400.0), px(300.0)), MouseButton::Left);
        assert!(!window.read(|chat, _| chat.bottom_panel_add_menu_open));

        window.update(|chat, _, cx| chat.toggle_bottom_panel_menu(cx));
        window.draw();
        window.simulate_keystroke("escape");
        assert!(!window.read(|chat, _| chat.bottom_panel_add_menu_open));
        assert!(window.read(|chat, _| chat.bottom_panel_open));
    }

    #[test]
    fn bottom_panel_add_menu_supports_keyboard_selection() {
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

        window.update(|chat, _, cx| {
            chat.open_bottom_panel(cx);
            chat.toggle_bottom_panel_menu(cx);
        });
        window.draw();
        window.simulate_keystroke("down");
        window.simulate_keystroke("down");
        assert_eq!(window.read(|chat, _| chat.bottom_panel_focused_item), 1);
        window.simulate_keystroke("enter");
        assert_eq!(
            window.read(|chat, _| chat.bottom_panel_tabs.clone()),
            vec![
                super::BottomPanelMode::Terminal,
                super::BottomPanelMode::Terminal
            ]
        );
        assert_eq!(window.read(|chat, _| chat.bottom_panel_active_tab), Some(1));
        assert!(!window.read(|chat, _| chat.bottom_panel_add_menu_open));
    }

    #[test]
    fn bottom_panel_add_menu_appends_after_the_default_terminal() {
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

        window.update(|chat, _, cx| {
            chat.open_bottom_panel(cx);
            chat.select_bottom_panel_item(2, cx);
        });

        assert_eq!(
            window.read(|chat, _| chat.bottom_panel_tabs.clone()),
            vec![
                super::BottomPanelMode::Terminal,
                super::BottomPanelMode::Browser
            ]
        );
        assert_eq!(window.read(|chat, _| chat.bottom_panel_active_tab), Some(1));
        assert_eq!(
            super::bottom_panel_tab_spec(super::BottomPanelMode::Browser).0,
            "新标签页"
        );
    }

    #[test]
    fn right_panel_menu_supports_keyboard_selection() {
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

        window.update(|chat, _, cx| chat.open_right_panel(cx));
        window.draw();
        window.simulate_keystroke("down");
        window.simulate_keystroke("down");
        assert_eq!(window.read(|chat, _| chat.right_panel_focused_item), 1);
        window.simulate_keystroke("enter");
        assert_eq!(
            window.read(|chat, _| chat.right_panel_mode),
            Some(super::RightPanelMode::Browser)
        );
    }

    #[test]
    fn right_panel_resize_handle_matches_reference_limits_without_hiding_the_panel() {
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
        window.update(|chat, _, cx| chat.open_right_panel(cx));
        window.draw();

        // CDP: a 16 px hit area is centered on the one-pixel divider.
        window.simulate_mouse_move(point(px(773.09375), px(300.0)));
        assert!(window.read(|chat, _| chat.right_panel_resize_hovered));
        window.simulate_mouse_down(point(px(773.09375), px(300.0)), MouseButton::Left);
        window.simulate_mouse_move(point(px(1100.0), px(300.0)));
        window.simulate_mouse_up(point(px(1100.0), px(300.0)), MouseButton::Left);
        let narrow_width = window.read(|chat, _| chat.right_panel_width.unwrap());
        assert!((narrow_width - 339.09375).abs() < 0.2, "{narrow_width}");

        window.draw();
        window.simulate_mouse_down(point(px(1100.0), px(300.0)), MouseButton::Left);
        window.simulate_mouse_move(point(px(400.0), px(300.0)));
        window.simulate_mouse_up(point(px(400.0), px(300.0)), MouseButton::Left);
        let expected_max = 1440.0 - 256.125 - super::RIGHT_PANEL_MAIN_MIN_WIDTH;
        assert!(
            (window.read(|chat, _| chat.right_panel_width.unwrap()) - expected_max).abs() < 0.2
        );

        window.draw();
        let divider_x = 1440.0 - expected_max;
        window.simulate_mouse_down(point(px(divider_x), px(300.0)), MouseButton::Left);
        window.simulate_mouse_move(point(px(1300.0), px(300.0)));
        window.simulate_mouse_up(point(px(1300.0), px(300.0)), MouseButton::Left);
        assert!(window.read(|chat, _| chat.right_panel_open));
        assert_eq!(
            window.read(|chat, _| chat.right_panel_width),
            Some(super::RIGHT_PANEL_MIN_WIDTH)
        );
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
    fn project_menu_closes_when_the_main_surface_is_clicked() {
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
                app.open_project_menu_for_capture(0, cx);
                app
            },
        );

        window.draw();
        assert!(window.read(|app, cx| app.sidebar.read(cx).project_menu_is_open()));
        window.simulate_click(point(px(600.0), px(350.0)), MouseButton::Left);
        assert!(!window.read(|app, cx| app.sidebar.read(cx).project_menu_is_open()));
    }

    #[test]
    fn pinned_menu_closes_when_the_main_surface_is_clicked() {
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
        window.simulate_mouse_move(point(px(100.0), px(331.0)));
        window.draw();
        window.simulate_click(point(px(188.5), px(331.0)), MouseButton::Left);
        window.draw();
        window.simulate_mouse_move(point(px(219.0), px(267.0)));
        window.draw();
        window.simulate_click(point(px(219.0), px(267.0)), MouseButton::Left);
        assert!(window.read(|app, cx| app.sidebar.read(cx).pinned_menu_is_open()));
        window.simulate_click(point(px(600.0), px(350.0)), MouseButton::Left);
        assert!(!window.read(|app, cx| app.sidebar.read(cx).pinned_menu_is_open()));
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
