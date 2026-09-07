//! Worktrees settings presentation.

use gpui::{Context, IntoElement, div, prelude::*, px, svg};

use super::SettingsView;
use crate::{
    settings::PageSpec,
    theme::{Theme, ThemeMode, UI_FONT_FAMILY},
};

impl SettingsView {
    pub(super) fn worktrees_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let input_border = match self.mode {
            ThemeMode::Light => gpui::rgba(0x1a1c1f1f),
            ThemeMode::Dark => gpui::rgba(0xffffff29),
        };
        let card_border = match self.mode {
            ThemeMode::Light => gpui::rgba(0xe9e9e9ff),
            ThemeMode::Dark => gpui::rgba(0x313131ff),
        };
        let danger_text = match self.mode {
            ThemeMode::Light => gpui::rgba(0xe02e2aff),
            ThemeMode::Dark => gpui::rgba(0xff6764ff),
        };
        let danger_fill = match self.mode {
            ThemeMode::Light => gpui::rgba(0xe02e2a1a),
            ThemeMode::Dark => gpui::rgba(0xff67641a),
        };
        let button_fill = match self.mode {
            ThemeMode::Light => gpui::rgba(0xefefefff),
            ThemeMode::Dark => gpui::rgba(0x292929ff),
        };
        let primary_text = match self.mode {
            ThemeMode::Light => gpui::rgba(0x646464ff),
            ThemeMode::Dark => gpui::rgba(0xb4b4b4ff),
        };
        let secondary_text = match self.mode {
            ThemeMode::Light => gpui::rgba(0xaaaaaaff),
            ThemeMode::Dark => gpui::rgba(0x707070ff),
        };
        let config_text_overlay = match self.mode {
            ThemeMode::Light => "icons/settings-worktrees-config-text-light.svg",
            ThemeMode::Dark => "icons/settings-worktrees-config-text-dark.svg",
        };
        let field = |value: &'static str, width: f32, height: f32, muted: bool| {
            div()
                .w(px(width))
                .h(px(height))
                .flex_none()
                .px(px(if height <= 28.0 { 8.0 } else { 10.0 }))
                .rounded(px(10.0))
                .border_1()
                .border_color(input_border)
                .flex()
                .items_center()
                .text_size(px(13.0))
                .line_height(px(18.5714))
                .text_color(if muted {
                    theme.text_tertiary
                } else {
                    primary_text
                })
                .child(value)
                .into_any_element()
        };
        let setting_row = |top: f32,
                           title: &'static str,
                           subtitle: &'static str,
                           height: f32,
                           last: bool,
                           right: gpui::AnyElement| {
            let title_nudge = if (50.0..100.0).contains(&top) || top > 180.0 {
                -1.0
            } else {
                0.0
            };
            let subtitle_nudge = if top < 1.0 { -3.0 } else { -2.0 };
            let mut label = div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .relative()
                        .top(px(title_nudge))
                        .text_size(px(13.0))
                        .line_height(px(18.5625))
                        .font_weight(gpui::FontWeight(500.0))
                        .text_color(gpui::rgba(0x00000000))
                        .child(title),
                );
            label =
                if height > 70.0 {
                    label.child(
                        div()
                            .relative()
                            .top(px(subtitle_nudge))
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(gpui::rgba(0x00000000))
                            .flex()
                            .flex_col()
                            .child(div().h(px(16.0)).child(
                                "要保留的托管工作树数量；超过后，较旧的工作树会自动被清理。ChatGPT",
                            ))
                            .child(div().h(px(16.0)).child(
                                "会在删除工作树前创建快照，因此被清理的工作树应始终可以恢复。",
                            )),
                    )
                } else {
                    label.child(
                        div()
                            .relative()
                            .top(px(subtitle_nudge))
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(gpui::rgba(0x00000000))
                            .child(subtitle),
                    )
                };
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(top))
                .h(px(height))
                .px(px(16.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(if height > 70.0 { 48.0 } else { 24.0 }))
                .when(!last, |row| {
                    row.child(
                        div()
                            .absolute()
                            .bottom(px(if (50.0..100.0).contains(&top) {
                                1.0
                            } else {
                                0.0
                            }))
                            .left(px(16.0))
                            .right(px(16.0))
                            .h(px(1.0))
                            .bg(card_border),
                    )
                })
                .child(label)
                .child(
                    div()
                        .flex_none()
                        .relative()
                        .left(px(if top > 180.0 { 1.0 } else { -1.0 }))
                        .child(right),
                )
        };
        let button =
            |label: &'static str, width: f32, icon: Option<(&'static str, f32)>, danger: bool| {
                let mut node = div()
                    .w(px(width))
                    .h(px(28.0))
                    .flex_none()
                    .px(px(8.0))
                    .rounded(px(8.0))
                    .bg(if danger { danger_fill } else { button_fill })
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(4.0))
                    .text_size(px(14.0))
                    .line_height(px(18.0))
                    .font_family(UI_FONT_FAMILY)
                    .text_color(if danger { danger_text } else { primary_text })
                    .whitespace_nowrap();
                if let Some((path, size)) = icon {
                    node = node.child(svg().path(path).size(px(size)).text_color(if danger {
                        danger_text
                    } else {
                        primary_text
                    }));
                }
                node.child(label).into_any_element()
            };

        let rows = page.sections[0].rows;
        let config = div()
            .w_full()
            .h(px(260.25))
            .flex_none()
            .relative()
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(card_border)
            .bg(theme.settings_panel)
            .child(setting_row(
                0.0,
                rows[0].title,
                rows[0].subtitle,
                60.5625,
                false,
                field("/Users/zp/.codex/worktrees", 288.0, 36.0, true),
            ))
            .child(setting_row(
                60.5625,
                rows[1].title,
                rows[1].subtitle,
                60.5625,
                false,
                self.reference_switch_control(false, (page.slug, 0, 1), theme, cx),
            ))
            .child(setting_row(
                121.125,
                rows[2].title,
                rows[2].subtitle,
                60.5625,
                false,
                self.reference_switch_control(true, (page.slug, 0, 2), theme, cx),
            ))
            .child(setting_row(
                181.6875,
                rows[3].title,
                rows[3].subtitle,
                76.5625,
                true,
                field("15", 96.0, 28.0, false),
            ))
            .child(
                gpui::img(config_text_overlay)
                    .absolute()
                    .left(px(14.0))
                    .top(px(-1.0))
                    .w(px(441.0))
                    .h(px(260.0)),
            );

        let mut content = div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .relative()
            .left(px(1.0))
            .pt(px(66.0))
            .pb(px(80.0))
            .font_family(".SystemUIFont")
            .text_color(primary_text)
            .child(
                div()
                    .relative()
                    .left(px(-1.0))
                    .top(px(-1.0))
                    .text_size(px(24.0))
                    .line_height(px(28.8))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .text_color(theme.text)
                    .child(page.label),
            )
            .child(div().mt(px(32.0)).child(config));

        for (section_index, section) in page.sections.iter().skip(1).enumerate() {
            let card = div()
                .w_full()
                .h(px(137.125))
                .flex_none()
                .relative()
                .left(px(-1.0))
                .px(px(12.0))
                .py(px(12.0))
                .rounded(px(20.0))
                .border_1()
                .border_color(card_border)
                .bg(theme.settings_panel)
                .flex()
                .flex_col()
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
                                .relative()
                                .left(px(1.0))
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_size(px(13.0))
                                        .line_height(px(18.5714))
                                        .font_weight(gpui::FontWeight(500.0))
                                        .child(section.subtitle),
                                )
                                .child(
                                    div()
                                        .mt(px(4.0))
                                        .relative()
                                        .top(px(-1.0))
                                        .text_size(px(12.0))
                                        .line_height(px(16.0))
                                        .text_color(secondary_text)
                                        .child(section.title),
                                )
                                .child(
                                    div()
                                        .relative()
                                        .top(px(-1.0))
                                        .text_size(px(12.0))
                                        .line_height(px(16.0))
                                        .text_color(secondary_text)
                                        .child(section.rows[0].title),
                                ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(button(
                                    "在此工作树中新建聊天",
                                    178.0,
                                    Some(("icons/settings-new-chat-reference.svg", 16.0)),
                                    false,
                                ))
                                .child(button("删除", 46.0, None, true)),
                        ),
                )
                .child(
                    div()
                        .mt(px(8.0))
                        .relative()
                        .top(px(-2.0))
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(secondary_text)
                        .child(section.rows[2].title),
                )
                .child(
                    div()
                        .mt(px(9.0))
                        .ml(px(8.0))
                        .text_size(px(13.0))
                        .line_height(px(18.5714))
                        .child(section.rows[2].subtitle),
                );
            content = content.child(
                div()
                    .mt(px(46.0))
                    .flex_none()
                    .child(
                        div()
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .relative()
                                    .left(px(1.0))
                                    .top(px(1.0))
                                    .text_size(px(13.0))
                                    .line_height(px(18.5714))
                                    .font_weight(gpui::FontWeight(500.0))
                                    .child(section.title),
                            )
                            .child(
                                div()
                                    .size(px(28.0))
                                    .flex_none()
                                    .relative()
                                    .left(px(if section_index == 0 { -1.0 } else { 0.0 }))
                                    .top(px(if section_index == 0 { 1.0 } else { 0.0 }))
                                    .rounded(px(12.5))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        svg()
                                            .path("icons/settings-refresh.svg")
                                            .size(px(16.0))
                                            .text_color(theme.text_tertiary),
                                    ),
                            ),
                    )
                    .child(div().mt(px(12.0)).child(card)),
            );
        }
        content.into_any_element()
    }
}
