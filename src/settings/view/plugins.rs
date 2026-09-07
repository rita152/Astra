//! Plugins settings presentation.

use gpui::{Context, IntoElement, div, prelude::*, px, svg};

use super::SettingsView;
use crate::{
    settings::PageSpec,
    theme::{Theme, ThemeMode},
};

impl SettingsView {
    pub(super) fn plugins_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let rows = page.sections[1].rows;
        let plugin_hover = match self.mode {
            ThemeMode::Dark => gpui::rgba(0x222222ff),
            ThemeMode::Light => gpui::rgba(0xf3f3f4ff),
        };
        let search_border = match self.mode {
            ThemeMode::Dark => gpui::rgba(0xffffff29),
            ThemeMode::Light => gpui::rgba(0x1a1c1f29),
        };
        let subtitle_weight = crate::theme::UI_BODY_FONT_WEIGHT;
        let subtitle_color = match self.mode {
            ThemeMode::Light => gpui::rgba(0xb0b1b2ff),
            ThemeMode::Dark => gpui::rgba(0x5a5a5aff),
        };
        let icon_paths = match self.mode {
            ThemeMode::Light => [
                "icons/settings-plugin-gmail-light.svg",
                "icons/settings-plugin-github-light.svg",
                "icons/settings-plugin-figma-light.svg",
                "icons/settings-plugin-zotero-light.svg",
                "icons/settings-plugin-templates-light.svg",
                "icons/settings-plugin-management-light.svg",
                "icons/settings-plugin-documents-light.svg",
                "icons/settings-plugin-pdf-light.svg",
                "icons/settings-plugin-spreadsheets-light.svg",
                "icons/settings-plugin-presentations-light.svg",
            ],
            ThemeMode::Dark => [
                "icons/settings-plugin-gmail-dark.svg",
                "icons/settings-plugin-github-dark.svg",
                "icons/settings-plugin-figma-dark.svg",
                "icons/settings-plugin-zotero-dark.svg",
                "icons/settings-plugin-templates-dark.svg",
                "icons/settings-plugin-management-dark.svg",
                "icons/settings-plugin-documents-dark.svg",
                "icons/settings-plugin-pdf-dark.svg",
                "icons/settings-plugin-spreadsheets-dark.svg",
                "icons/settings-plugin-presentations-dark.svg",
            ],
        };
        let mut list = div().mt(px(36.0)).flex().flex_col();
        for (index, row) in rows.iter().enumerate() {
            list = list.child(
                div()
                    .h(px(68.0625))
                    .pl(px(9.0))
                    .pr(px(8.0))
                    .rounded(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .size(px(40.0))
                            .rounded(px(10.0))
                            .overflow_hidden()
                            .flex()
                            .flex_col()
                            .items_start()
                            .when((6..10).contains(&index), |icon| {
                                icon.relative().top(px(1.0))
                            })
                            .when(index < 10, |icon| {
                                icon.child(gpui::img(icon_paths[index]).size(px(40.0)))
                            })
                            .when(index >= 10, |icon| {
                                icon.border_1()
                                    .border_color(theme.border)
                                    .child(
                                        gpui::img("icons/settings-plugin-broken.svg")
                                            .size(px(16.0))
                                            .flex_none(),
                                    )
                                    .child(
                                        div()
                                            .h(px(20.0))
                                            .flex_none()
                                            .text_size(px(14.0))
                                            .line_height(px(20.0))
                                            .font_weight(gpui::FontWeight::NORMAL)
                                            .child(row.title),
                                    )
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .relative()
                            .top(px(1.0))
                            .flex()
                            .flex_col()
                            .font_family(".SystemUIFont")
                            .gap(px(3.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .line_height(px(21.0))
                                    .font_weight(gpui::FontWeight(500.0))
                                    .child(row.title),
                            )
                            .child(
                                div()
                                    .relative()
                                    .top(px(-1.0))
                                    .text_size(px(13.0))
                                    .line_height(px(21.125))
                                    .font_weight(subtitle_weight)
                                    .text_color(subtitle_color)
                                    .child(row.subtitle),
                            ),
                    )
                    .child(self.switch_control(true, (page.slug, 1, index), theme, cx)),
            );
        }
        div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .pt(px(64.0))
            .pb(px(80.0))
            .child(
                div()
                    .relative()
                    .top(px(2.0))
                    .flex()
                    .items_start()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(5.0))
                            .child(
                                div()
                                    .relative()
                                    .top(px(-1.0))
                                    .text_size(px(24.0))
                                    .line_height(px(31.0))
                                    .font_weight(gpui::FontWeight::NORMAL)
                                    .child(page.label),
                            )
                            .child(
                                div()
                                    .relative()
                                    .text_size(px(14.0))
                                    .line_height(px(21.0))
                                    .text_color(theme.text_tertiary)
                                    .child(page.sections[0].title),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .w(px(74.0))
                                    .h(px(28.0))
                                    .rounded(px(12.5))
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(theme.settings_button)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(14.0))
                                    .line_height(px(18.0))
                                    .child("浏览目录"),
                            )
                            .child(
                                div()
                                    .w(px(66.0))
                                    .h(px(28.0))
                                    .px(px(8.0))
                                    .rounded(px(12.5))
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(theme.text)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .gap(px(4.0))
                                    .text_size(px(14.0))
                                    .line_height(px(18.0))
                                    .text_color(theme.surface)
                                    .child("添加")
                                    .child(svg().path("icons/chevron-down.svg").size(px(12.0))),
                            ),
                    ),
            )
            .child(
                div()
                    .relative()
                    .mt(px(33.0))
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.0))
                            .text_size(px(14.0))
                            .line_height(px(18.0))
                            .text_color(theme.text_tertiary)
                            .child(
                                div()
                                    .h(px(28.0))
                                    .w(px(67.203125))
                                    .pl(px(9.0))
                                    .pr(px(7.0))
                                    .rounded(px(12.5))
                                    .bg(plugin_hover)
                                    .flex()
                                    .items_center()
                                    .text_color(theme.text)
                                    .child("插件 14"),
                            )
                            .child(
                                div()
                                    .h(px(28.0))
                                    .w(px(60.796875))
                                    .pl(px(9.0))
                                    .pr(px(7.0))
                                    .flex()
                                    .items_center()
                                    .child("应用 8"),
                            )
                            .child(
                                div()
                                    .h(px(28.0))
                                    .w(px(63.5625))
                                    .pl(px(9.0))
                                    .pr(px(7.0))
                                    .flex()
                                    .items_center()
                                    .child("MCP 4"),
                            )
                            .child(
                                div()
                                    .h(px(28.0))
                                    .w(px(60.3125))
                                    .pl(px(9.0))
                                    .pr(px(7.0))
                                    .flex()
                                    .items_center()
                                    .child("技能 2"),
                            ),
                    )
                    .child(
                        div()
                            .w(px(224.0))
                            .h(px(32.0))
                            .px(px(10.0))
                            .rounded_full()
                            .border_1()
                            .border_color(search_border)
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(14.0))
                            .line_height(px(18.0))
                            .text_color(theme.text_tertiary)
                            .child(svg().path("icons/search.svg").size(px(15.0)))
                            .child("搜索插件"),
                    ),
            )
            .child(list)
            .into_any_element()
    }
}
