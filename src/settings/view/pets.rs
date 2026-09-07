//! Pets settings presentation.

use gpui::{Context, IntoElement, div, prelude::*, px, svg};

use super::SettingsView;
use crate::{settings::PageSpec, theme::Theme};

impl SettingsView {
    pub(super) fn pets_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let first = &page.sections[0];
        let pet_assets = [
            "icons/pet-codex.svg",
            "icons/pet-dewey.svg",
            "icons/pet-fireball.svg",
            "icons/pet-hoots.svg",
            "icons/pet-rocky.svg",
            "icons/pet-seedy.svg",
            "icons/pet-stacky.svg",
            "icons/pet-bsod.svg",
            "icons/pet-null-signal.svg",
        ];
        let mut card = div()
            .w_full()
            .rounded(px(16.0))
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel);
        for (index, row) in first.rows.iter().enumerate().skip(1) {
            card =
                card.child(
                    div()
                        .h(px(88.0))
                        .px(px(16.0))
                        .relative()
                        .flex()
                        .items_center()
                        .gap(px(24.0))
                        .when(index + 1 != first.rows.len(), |node| {
                            node.child(
                                div()
                                    .absolute()
                                    .bottom_0()
                                    .left(px(16.0))
                                    .right(px(16.0))
                                    .h(px(1.0))
                                    .bg(theme.border),
                            )
                        })
                        .child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .flex()
                                .items_center()
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .size(px(64.0))
                                        .flex_none()
                                        .overflow_hidden()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(
                                            div().w(px(48.0)).h(px(52.0)).flex_none().child(
                                                gpui::img(pet_assets[index - 1]).size_full(),
                                            ),
                                        ),
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
                                                .font_weight(gpui::FontWeight(500.0))
                                                .child(row.title),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .line_height(px(16.0))
                                                .text_color(theme.text_tertiary)
                                                .relative()
                                                .top(px(-1.0))
                                                .child(row.subtitle),
                                        ),
                                ),
                        )
                        .child(div().relative().top(px(-1.0)).child(self.control(
                            row.control,
                            (page.slug, 0, index),
                            theme,
                            cx,
                        ))),
                );
        }
        let mut content = div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .pt(px(64.0))
            .pb(px(80.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .relative()
                    .top(px(-3.0))
                    .text_size(px(24.0))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .child(page.label),
            )
            .child(
                div()
                    .mt(px(23.0))
                    .flex()
                    .items_start()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .font_weight(gpui::FontWeight(500.0))
                                    .child(first.title),
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(theme.text_tertiary)
                                    .child(first.subtitle),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .size(px(26.0))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(theme.text_tertiary)
                                    .child(
                                        svg()
                                            .path("icons/settings-refresh.svg")
                                            .size(px(16.0))
                                            .text_color(theme.text_tertiary),
                                    ),
                            )
                            .child(self.small_button("创建", false, theme))
                            .child(self.small_button("收起宠物", false, theme)),
                    ),
            )
            .child(div().mt(px(11.0)).child(card));
        for (index, section) in page.sections.iter().enumerate().skip(1) {
            content = content.child(
                div()
                    .mt(px(30.0))
                    .child(self.section(page.slug, index, section, theme, cx)),
            );
        }
        content.into_any_element()
    }
}
