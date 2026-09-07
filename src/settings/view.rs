mod agent;
mod appearance;
mod appshots;
mod artwork;
mod browser;
mod chronicle;
mod computer_use;
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

pub struct SettingsView {
    mode: ThemeMode,
    selected: &'static str,
    nav_scroll: ScrollHandle,
    content_scroll: ScrollHandle,
    switch_overrides: HashMap<(&'static str, usize, usize), bool>,
    appearance_theme: usize,
}

impl EventEmitter<CloseSettings> for SettingsView {}
impl EventEmitter<ChangeTheme> for SettingsView {}

impl SettingsView {
    pub fn new(mode: ThemeMode) -> Self {
        Self {
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
            .size_full()
            .bg(theme.surface)
            .font_family(UI_FONT_FAMILY)
            .text_color(theme.text)
            .flex()
            .child(
                div()
                    .id("settings-sidebar")
                    .w(px(264.3125))
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
                    .pl(px(41.0))
                    .pr(px(40.0))
                    .child(self.content(selected, theme, viewport_width, cx)),
            )
    }
}
