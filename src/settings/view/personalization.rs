//! Personalization settings presentation.

use gpui::{Context, IntoElement, div, prelude::*, px, svg};

use super::SettingsView;
use crate::{
    settings::PageSpec,
    theme::{Theme, ThemeMode},
};

impl SettingsView {
    pub(super) fn personalization_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let instruction = &page.sections[0].rows[0];
        let memory = &page.sections[1];
        let personality = &page.sections[2].rows[0];
        let link_color = match self.mode {
            ThemeMode::Light => gpui::rgba(0x339cffff),
            ThemeMode::Dark => gpui::rgba(0x99ceffff),
        };
        let textarea_border = match self.mode {
            ThemeMode::Light => gpui::rgba(0x1a1c1f1f),
            ThemeMode::Dark => gpui::rgba(0xffffff1f),
        };
        let danger_text = match self.mode {
            ThemeMode::Light => gpui::rgba(0xe02e2aff),
            ThemeMode::Dark => gpui::rgba(0xff6764ff),
        };
        let danger_fill = match self.mode {
            ThemeMode::Light => gpui::rgba(0xe02e2a1a),
            ThemeMode::Dark => gpui::rgba(0xff67641a),
        };
        let warning_fill = match self.mode {
            ThemeMode::Light => gpui::rgba(0xfffcfbff),
            ThemeMode::Dark => gpui::rgba(0x1c1613ff),
        };

        let mut memory_card = div()
            .w_full()
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel);
        for (index, row) in memory.rows.iter().enumerate() {
            let right = match index {
                0 => self.reference_switch_control(false, (page.slug, 1, 0), theme, cx),
                1 => self.reference_switch_control(true, (page.slug, 1, 1), theme, cx),
                _ => div()
                    .w(px(44.0))
                    .h(px(24.0))
                    .flex_none()
                    .rounded_full()
                    .bg(danger_fill)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(danger_text)
                    .child("删除")
                    .into_any_element(),
            };
            memory_card = memory_card.child(
                div()
                    .h(px(60.5625))
                    .flex_none()
                    .px(px(16.0))
                    .relative()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(24.0))
                    .when(index + 1 != memory.rows.len(), |item| {
                        item.child(
                            div()
                                .absolute()
                                .bottom_0()
                                .left(px(16.0))
                                .right(px(16.0))
                                .h(px(1.0))
                                .bg(theme.border),
                        )
                    })
                    .child(self.reference_label(row.title, row.subtitle, theme))
                    .child(right),
            );
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
                    .mt(px(32.0))
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
                                    .line_height(px(24.875))
                                    .font_weight(gpui::FontWeight(500.0))
                                    .child(instruction.title),
                            )
                            .child(
                                div()
                                    .mt(px(2.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .text_size(px(13.0))
                                    .line_height(px(18.0))
                                    .text_color(theme.settings_description)
                                    .child(
                                        "向 ChatGPT 提供适用于此主机上所有聊天的额外说明和上下文。",
                                    )
                                    .child(div().text_color(link_color).child("了解更多")),
                            ),
                    )
                    .child(
                        div()
                            .opacity(0.4)
                            .child(self.reference_button("保存", 46.0, None, false, theme)),
                    ),
            )
            .child(
                div()
                    .mt(px(12.0))
                    .h(px(147.9375))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(textarea_border)
                    .px(px(10.0))
                    .py(px(8.0))
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .text_color(theme.text_tertiary)
                    .child("添加自定义指令…"),
            )
            .child(
                div()
                    .mt(px(39.0))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(16.0))
                            .line_height(px(24.875))
                            .font_weight(gpui::FontWeight(500.0))
                            .child(memory.title),
                    )
                    .child(
                        div()
                            .mt(px(2.0))
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .text_color(theme.settings_description)
                            .child("设置在此电脑上如何收集、保留和整合本地记忆。")
                            .child(div().text_color(link_color).child("了解更多")),
                    ),
            )
            .child(div().mt(px(12.0)).child(memory_card))
            .child(
                div()
                    .mt(px(39.0))
                    .h(px(37.125))
                    .px(px(12.0))
                    .rounded(px(20.0))
                    .bg(warning_fill)
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        svg()
                            .path("icons/settings-warning.svg")
                            .size(px(20.0))
                            .flex_none()
                            .text_color(theme.warning),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .text_color(theme.text)
                            .child(page.sections[2].subtitle),
                    ),
            )
            .child(div().mt(px(6.0)).child(self.agent_card(
                vec![self.agent_row(
                    personality.title,
                    personality.subtitle,
                    self.agent_select("亲和", 72.0, theme),
                    true,
                    theme,
                )],
                theme,
            )))
            .into_any_element()
    }
}
