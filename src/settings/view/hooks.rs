//! Hooks settings presentation.

use gpui::{IntoElement, div, prelude::*, px, svg};

use super::SettingsView;
use crate::{
    settings::PageSpec,
    theme::{Theme, ThemeMode, UI_FONT_FAMILY},
};

impl SettingsView {
    pub(super) fn hooks_content(&self, page: &'static PageSpec, theme: Theme) -> gpui::AnyElement {
        let empty = &page.sections[0];
        let link_color = match self.mode {
            ThemeMode::Light => gpui::rgba(0x339cffff),
            ThemeMode::Dark => gpui::rgba(0x99ceffff),
        };

        div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .pt(px(66.0))
            .pb(px(80.0))
            .font_family(".SystemUIFont")
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(16.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .relative()
                                    .top(px(-1.0))
                                    .font_family(UI_FONT_FAMILY)
                                    .text_size(px(24.0))
                                    .line_height(px(31.0))
                                    .font_weight(gpui::FontWeight::NORMAL)
                                    .child(page.label),
                            )
                            .child(
                                div()
                                    .relative()
                                    .left(px(1.0))
                                    .mt(px(3.8))
                                    .h(px(21.0))
                                    .flex()
                                    .items_center()
                                    .text_size(px(14.0))
                                    .line_height(px(21.0))
                                    .text_color(theme.settings_description)
                                    .child("通过配置和已启用的插件管理生命周期钩子。")
                                    .child(
                                        div().ml(px(8.0)).text_color(link_color).child("了解更多"),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .size(px(26.0))
                            .flex_none()
                            .rounded(px(10.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.sidebar_hover))
                            .child(
                                svg()
                                    .path("icons/settings-hooks-refresh.svg")
                                    .size(px(16.0))
                                    .text_color(theme.text_tertiary),
                            ),
                    ),
            )
            .child(
                div()
                    .mt(px(32.0))
                    .w_full()
                    .rounded(px(20.0))
                    .overflow_hidden()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.settings_panel)
                    .child(
                        div()
                            .h(px(60.5625))
                            .flex_none()
                            .px(px(16.0))
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .relative()
                                            .left(px(1.0))
                                            .text_size(px(13.0))
                                            .line_height(px(18.5625))
                                            .font_weight(gpui::FontWeight(500.0))
                                            .child(empty.title),
                                    )
                                    .child(
                                        div()
                                            .relative()
                                            .top(px(-2.0))
                                            .text_size(px(12.0))
                                            .line_height(px(16.0))
                                            .text_color(theme.settings_description)
                                            .child(empty.subtitle),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }
}
