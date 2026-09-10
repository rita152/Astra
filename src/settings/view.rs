mod agent;
mod appearance;
mod appshots;
mod artwork;
mod browser;
mod chronicle;
mod computer_use;
mod configuration;
mod connections;
mod controls;
mod data_controls;
mod environments;
mod git;
mod hooks;
mod import;
mod keyboard;
mod navigation;
mod personalization;
mod pets;
mod plugins;
mod profile;
mod worktrees;

use std::collections::HashMap;

use gpui::{
    Context, EventEmitter, IntoElement, Render, ScrollHandle, Window, div, point, prelude::*, px,
    svg,
};

use super::{PageKind, PageSpec, page, pages};
use crate::theme::{Theme, ThemeMode, UI_FONT_FAMILY};

pub struct CloseSettings;
pub struct ChangeTheme(pub ThemeMode);
pub struct ConfigSaveFinished;

pub struct SettingsView {
    mode: ThemeMode,
    selected: &'static str,
    nav_scroll: ScrollHandle,
    content_scroll: ScrollHandle,
    switch_overrides: HashMap<(&'static str, usize, usize), bool>,
    appearance_theme: usize,
    backend: std::sync::Arc<dyn crate::agent::AgentBackend>,
    config_cwd: std::path::PathBuf,
    config_editor: crate::configuration::ConfigEditor,
    config_drafts: configuration::ConfigDrafts,
    config_choices: Vec<crate::agent::AgentConfigChoiceSet>,
    config_profiles: Vec<crate::agent::AgentPermissionProfile>,
    config_profiles_error: Option<String>,
    config_models: Vec<crate::agent::AgentModel>,
    config_menu: Option<String>,
    config_menu_index: usize,
    config_menu_scroll: ScrollHandle,
    config_control_bounds:
        std::rc::Rc<std::cell::RefCell<HashMap<String, gpui::Bounds<gpui::Pixels>>>>,
    config_menu_focus: gpui::FocusHandle,
    config_menu_focus_pending: bool,
    config_field_focus: HashMap<String, gpui::FocusHandle>,
    config_return_focus: Option<String>,
    config_sources_open: bool,
    config_advanced_open: bool,
    config_custom_key: Option<String>,
    config_input: gpui::Entity<crate::components::prompt_input::PromptInput>,
}

impl EventEmitter<CloseSettings> for SettingsView {}
impl EventEmitter<ChangeTheme> for SettingsView {}
impl EventEmitter<ConfigSaveFinished> for SettingsView {}

impl SettingsView {
    pub fn new(
        mode: ThemeMode,
        backend: std::sync::Arc<dyn crate::agent::AgentBackend>,
        cx: &mut Context<Self>,
    ) -> Self {
        let config_choices = backend.config_choices();
        let config_field_focus = config_choices
            .iter()
            .map(|field| field.key.clone())
            .chain(std::iter::once("source".into()))
            .map(|key| (key, cx.focus_handle().tab_stop(true)))
            .collect();
        let config_input = cx.new(|cx| {
            let mut input = crate::components::prompt_input::PromptInput::inline_other(
                mode,
                "输入配置值",
                false,
                cx,
            );
            input.set_accessible_name("配置值");
            input
        });
        cx.subscribe(
            &config_input,
            |this, _, event: &crate::components::prompt_input::PromptSubmitted, cx| {
                if let Some(key) = this.config_custom_key.clone() {
                    let value = event.0.trim();
                    if !value.is_empty() {
                        match this.config_editor.edit(&key, serde_json::json!(value)) {
                            Ok(()) => {
                                this.config_custom_key = None;
                                this.config_return_focus = Some(key.clone());
                            }
                            Err(error) => this.config_editor.feedback = Some(error),
                        }
                        cx.notify();
                    }
                }
            },
        )
        .detach();
        Self {
            config_choices,
            config_field_focus,
            config_return_focus: None,
            backend,
            config_cwd: std::env::current_dir().unwrap_or_default(),
            config_editor: Default::default(),
            config_drafts: Default::default(),
            config_profiles: Vec::new(),
            config_profiles_error: None,
            config_models: Vec::new(),
            config_menu: None,
            config_menu_index: 0,
            config_menu_scroll: ScrollHandle::new(),
            config_control_bounds: Default::default(),
            config_menu_focus: cx.focus_handle(),
            config_menu_focus_pending: false,
            config_sources_open: false,
            config_advanced_open: false,
            config_custom_key: None,
            config_input,
            mode,
            selected: "general-settings",
            nav_scroll: ScrollHandle::new(),
            content_scroll: ScrollHandle::new(),
            switch_overrides: HashMap::new(),
            appearance_theme: if mode == ThemeMode::Dark { 2 } else { 1 },
        }
    }

    pub fn select(&mut self, slug: &'static str, cx: &mut Context<Self>) {
        self.selected = slug;
        self.config_menu = None;
        if matches!(slug, "agent" | "personalization") && self.config_editor.snapshot.is_none() {
            self.set_config_context(self.config_cwd.clone(), cx);
        }
        self.content_scroll.set_offset(point(px(0.0), px(0.0)));
        cx.notify();
    }

    fn content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        viewport_width: f32,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        match page.kind {
            PageKind::Profile => self.profile_content(theme, viewport_width),
            PageKind::Pets => self.pets_content(page, theme, cx),
            PageKind::KeyboardShortcuts => self.keyboard_content(page, theme, cx),
            _ if page.slug == "appearance" => self.appearance_content(page, theme, cx),
            _ if page.slug == "appshots" => self.appshots_content(page, theme, cx),
            _ if page.slug == "computer-use" => self.computer_use_content(page, theme, cx),
            _ if page.slug == "personalization" => self.personalization_content(page, theme, cx),
            _ if page.slug == "chronicle" => self.chronicle_content(page, theme),
            _ if page.slug == "plugins-settings" => self.plugins_content(page, theme, cx),
            _ if page.slug == "hooks-settings" => self.hooks_content(page, theme),
            _ if page.slug == "connections" => self.connections_content(page, theme, cx),
            _ if page.slug == "browser-use" => self.browser_content(page, theme, cx),
            _ if page.slug == "import" => self.import_content(page, theme, cx),
            _ if page.slug == "agent" => self.agent_content(page, theme, cx),
            _ if page.slug == "git-settings" => self.git_content(page, theme, cx),
            _ if page.slug == "local-environments" => self.local_environments_content(page, theme),
            _ if page.slug == "worktrees" => self.worktrees_content(page, theme, cx),
            _ if page.slug == "data-controls" => self.data_controls_content(page, theme),
            PageKind::Standard | PageKind::Usage => {
                self.standard_content(page, theme, cx).into_any_element()
            }
        }
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.config_menu_focus_pending {
            if self.config_menu.is_some() {
                self.config_menu_focus.focus(window, cx);
            }
            self.config_menu_focus_pending = false;
        }
        if let Some(key) = self.config_return_focus.take()
            && let Some(focus) = self.config_field_focus.get(&key)
        {
            focus.focus(window, cx);
        }
        let viewport = window.viewport_size();
        let theme = Theme::for_window(
            self.mode,
            window.is_window_active(),
            f32::from(viewport.width),
            f32::from(viewport.height),
            window.scale_factor(),
        );
        let viewport_width = f32::from(window.viewport_size().width);
        let selected =
            page(self.selected).unwrap_or_else(|| pages().next().expect("settings pages"));
        let nav_scroll = self.nav_scroll.clone();
        let content_scroll = self.content_scroll.clone();

        div()
            .id("settings-shell")
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "tab" {
                    if event.keystroke.modifiers.shift {
                        window.focus_prev(cx);
                    } else {
                        window.focus_next(cx);
                    }
                    cx.stop_propagation();
                } else if event.keystroke.modifiers.platform && event.keystroke.key == "s" {
                    this.save_config(cx);
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape" {
                    this.config_menu = None;
                    this.config_custom_key = None;
                    cx.notify();
                    cx.stop_propagation();
                } else {
                    cx.propagate();
                }
            }))
            .size_full()
            .bg(theme.surface)
            .font_family(UI_FONT_FAMILY)
            .text_color(theme.markdown_text)
            .flex()
            .child(
                div()
                    .id("settings-sidebar")
                    .w(px(240.0))
                    .h_full()
                    .flex_none()
                    .relative()
                    .bg(theme.sidebar_surface)
                    .border_r_1()
                    .border_color(theme.border)
                    .flex()
                    .flex_col()
                    .child(div().h(px(46.0)).flex_none())
                    .child(
                        div()
                            .id("settings-back")
                            .focus_visible(move |style| {
                                style.bg(theme.sidebar_hover).shadow(vec![
                                    gpui::BoxShadow::new(px(0.), px(0.), theme.accent.into())
                                        .spread_radius(px(2.)),
                                ])
                            })
                            .role(gpui::Role::Button)
                            .aria_label("返回应用")
                            .focusable()
                            .tab_stop(true)
                            .on_key_down(cx.listener(|_, event: &gpui::KeyDownEvent, _, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                    cx.emit(CloseSettings);
                                    cx.stop_propagation();
                                } else {
                                    cx.propagate();
                                }
                            }))
                            .mx(px(8.0))
                            .mb(px(8.0))
                            .h(px(31.0))
                            .px(px(8.0))
                            .rounded(px(12.5))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(14.0))
                            .line_height(px(21.0))
                            .text_color(theme.settings_description)
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.sidebar_hover))
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(CloseSettings)))
                            .child(
                                svg()
                                    .path("icons/back.svg")
                                    .size(px(16.0))
                                    .text_color(theme.settings_description),
                            )
                            .child("返回应用"),
                    )
                    .child(
                        div()
                            .mx(px(8.0))
                            .mb(px(10.0))
                            .h(px(29.0))
                            .px(px(8.0))
                            .rounded(px(12.5))
                            .bg(theme.settings_search)
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(14.0))
                            .line_height(px(18.0))
                            .text_color(theme.text_tertiary)
                            .child(
                                svg()
                                    .path("icons/search.svg")
                                    .size(px(18.0))
                                    .text_color(theme.text_tertiary),
                            )
                            .child("搜索设置…"),
                    )
                    .child(
                        div()
                            .id("settings-nav-scroll")
                            .min_h(px(0.0))
                            .flex_1()
                            .overflow_y_scroll()
                            .scrollbar_width(px(0.0))
                            .track_scroll(&nav_scroll)
                            .pl(px(8.0))
                            .pr(px(8.0))
                            .pt(px(1.0))
                            .pb(px(8.0))
                            .flex()
                            .flex_col()
                            .gap(px(11.0))
                            .child(self.nav_group(
                                "个人",
                                &[
                                    "general-settings",
                                    "import",
                                    "profile",
                                    "appearance",
                                    "voice",
                                    "agent",
                                    "personalization",
                                    "pets",
                                    "keyboard-shortcuts",
                                    "usage",
                                    "account",
                                ],
                                theme,
                                cx,
                            ))
                            .child(self.nav_group(
                                "集成",
                                &[
                                    "computer-use",
                                    "chronicle",
                                    "appshots",
                                    "plugins-settings",
                                    "browser-use",
                                ],
                                theme,
                                cx,
                            ))
                            .child(self.nav_group(
                                "编码",
                                &[
                                    "hooks-settings",
                                    "connections",
                                    "git-settings",
                                    "local-environments",
                                    "worktrees",
                                ],
                                theme,
                                cx,
                            ))
                            .child(self.nav_group("已归档", &["data-controls"], theme, cx)),
                    )
                    .child(Self::sidebar_edge_shade(theme)),
            )
            .child(
                div()
                    .id("settings-content-scroll")
                    .min_w(px(0.0))
                    .h_full()
                    .flex_1()
                    .bg(theme.surface)
                    .overflow_y_scroll()
                    .track_scroll(&content_scroll)
                    .pl(px(40.0))
                    .pr(px(55.0))
                    .child(self.content(selected, theme, viewport_width, cx)),
            )
    }
}
