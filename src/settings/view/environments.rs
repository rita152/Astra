//! Environments settings presentation.

use gpui::{IntoElement, div, prelude::*, px, svg};

use super::SettingsView;
use crate::{settings::PageSpec, theme::Theme};

impl SettingsView {
    pub(super) fn local_environments_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
    ) -> gpui::AnyElement {
        let rows = page.sections[0].rows;
        let mut cards = div().mt(px(12.0)).flex().flex_col().gap(px(12.0));
        for (index, row) in rows.iter().enumerate() {
            let subtitle = match index {
                2 => "",
                6 => "openai",
                _ => row.subtitle,
            };
            let expanded = index == 6;
            let row_height = if subtitle.is_empty() {
                52.0
            } else if matches!(index, 3 | 5 | 7) {
                61.5
            } else {
                60.5
            };
            let primary = div()
                .h(px(row_height))
                .flex_none()
                .px(px(16.0))
                .flex()
                .items_center()
                .gap(px(14.0))
                .child(
                    svg()
                        .path("icons/settings-local-project.svg")
                        .size(px(16.0))
                        .text_color(theme.text_tertiary),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .line_height(px(18.5))
                                .font_weight(gpui::FontWeight(500.0))
                                .child(row.title),
                        )
                        .when(!subtitle.is_empty(), |column| {
                            column.child(
                                div()
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .text_color(theme.text_tertiary)
                                    .child(subtitle),
                            )
                        }),
                )
                .child(self.coding_icon_button("icons/add.svg", true, theme));
            let card = div()
                .w_full()
                .rounded(px(20.0))
                .overflow_hidden()
                .border_1()
                .border_color(theme.border)
                .bg(theme.settings_panel)
                .child(primary)
                .when(expanded, |card| {
                    card.child(
                        div()
                            .h(px(61.5))
                            .flex_none()
                            .px(px(16.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .line_height(px(18.5))
                                            .font_weight(gpui::FontWeight(500.0))
                                            .child("codex"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .line_height(px(16.0))
                                            .text_color(theme.text_tertiary)
                                            .child("environment.toml"),
                                    ),
                            )
                            .child(
                                svg()
                                    .path("icons/settings-chevron-right.svg")
                                    .size(px(14.0))
                                    .text_color(theme.text_tertiary),
                            ),
                    )
                });
            cards = cards.child(card);
        }

        div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .pt(px(66.0))
            .pb(px(80.0))
            .child(
                div()
                    .text_size(px(24.0))
                    .line_height(px(28.8))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .child(page.label),
            )
            .child(
                div()
                    .mt(px(6.0))
                    .h(px(21.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .text_color(theme.text_tertiary)
                    .child("本地环境会告诉 ChatGPT 如何为项目设置工作树。")
                    .child(div().text_color(theme.settings_accent).child("了解更多。")),
            )
            .child(
                div()
                    .mt(px(38.0))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(21.0))
                            .font_weight(gpui::FontWeight(500.0))
                            .child(page.sections[0].title),
                    )
                    .child(self.coding_button("添加项目", 74.0, None, false, theme)),
            )
            .child(cards)
            .into_any_element()
    }
}
