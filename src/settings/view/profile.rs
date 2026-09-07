//! Profile settings presentation.

use gpui::{IntoElement, div, prelude::*, px, svg};

use super::SettingsView;
use crate::theme::Theme;

impl SettingsView {
    pub(super) fn profile_content(&self, theme: Theme, viewport_width: f32) -> gpui::AnyElement {
        let is_dark = theme.surface == gpui::rgba(0x181818ff);
        let profile_border = if is_dark {
            gpui::rgba(0xffffff0a)
        } else {
            gpui::rgba(0x1a1c1f0c)
        };
        let profile_tertiary = if is_dark {
            gpui::rgba(0x7c7c7cff)
        } else {
            gpui::rgba(0x8d8e8fff)
        };
        let normal_weight = crate::theme::UI_BODY_FONT_WEIGHT;
        let activity_heading_offset = if is_dark { 10.0 } else { 9.0 };
        // The profile body is capped and centered, while its toolbar spans the
        // settings panel with 20px insets. Derive the toolbar geometry from the
        // viewport so it stays panel-aligned when the window is resized.
        let settings_sidebar_width = 264.3125;
        let content_padding_left = 41.0;
        let content_horizontal_padding = 81.0;
        let profile_width = 732.0;
        let available_content_width =
            (viewport_width - settings_sidebar_width - content_horizontal_padding).max(0.0);
        let rendered_profile_width = available_content_width.min(profile_width);
        let profile_left = settings_sidebar_width
            + content_padding_left
            + (available_content_width - rendered_profile_width) * 0.5;
        let toolbar_left = settings_sidebar_width + 20.0;
        let toolbar_offset = toolbar_left - profile_left;
        let toolbar_width = (viewport_width - settings_sidebar_width - 40.0).max(0.0);
        let mut stats = div()
            .w_full()
            .h(px(62.0))
            .rounded(px(20.0))
            .border_1()
            .border_color(profile_border)
            .relative()
            .top(px(8.0))
            .flex()
            .items_center();
        for (index, (value, label)) in [
            ("157亿", "累计 Token 数"),
            ("13.6亿", "峰值 Token 数"),
            ("10 小时 28 分", "最长聊天时长"),
            ("28 天", "当前连续天数"),
            ("28 天", "最长连续天数"),
        ]
        .iter()
        .enumerate()
        {
            if index > 0 {
                stats = stats.child(
                    div()
                        .w(px(1.0))
                        .h(px(36.0))
                        .flex_none()
                        .rounded(px(2.0))
                        .bg(profile_border),
                );
            }
            stats = stats.child(
                div()
                    .h(px(40.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .text_color(theme.text)
                    .child(*value)
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .text_color(theme.settings_description)
                            .child(*label),
                    ),
            );
        }

        // The reference renders the trailing 361 days (51 complete weeks and the
        // current four-day week), rather than a generated activity pattern.
        let activity_levels = [
            "0000000", "0000000", "0000000", "0000000", "0000000", "0000000", "0000000", "0000000",
            "0000000", "0000000", "0000000", "0000000", "0000000", "0000000", "0000000", "0000000",
            "0000000", "0000000", "0000000", "0000000", "0000000", "0000000", "0000000", "0000010",
            "0000000", "0000000", "0001000", "0000000", "0000000", "0000010", "0011100", "0000000",
            "0001110", "1111111", "0000000", "0000000", "0000000", "0000000", "0000000", "0000000",
            "0000000", "0000000", "0000000", "0000000", "0000000", "0000000", "0000000", "0004114",
            "2321212", "2213231", "1213212", "4310",
        ];
        let off_color = if is_dark {
            gpui::rgba(0x212121ff)
        } else {
            gpui::rgba(0xf4f4f4ff)
        };
        let activity_color = |level: char| match (is_dark, level) {
            (_, '0') => off_color,
            (false, '1') => gpui::rgba(0xd9e9fdff),
            (false, '2') => gpui::rgba(0xb7d5fcff),
            (false, '3') => gpui::rgba(0x8abafaff),
            (false, _) => gpui::rgba(0x539af8ff),
            (true, '1') => gpui::rgba(0x37404aff),
            (true, '2') => gpui::rgba(0x536477ff),
            (true, '3') => gpui::rgba(0x7793b2ff),
            (true, _) => gpui::rgba(0xa4cdfbff),
        };
        let mut heatmap = div().w_full().h(px(96.0)).flex().items_start().gap(px(3.0));
        for levels in activity_levels {
            let mut week = div().min_w(px(0.0)).flex_1().flex().flex_col().gap(px(3.0));
            for level in levels.chars() {
                week = week.child(
                    div()
                        .w_full()
                        .h(px(11.14))
                        .rounded(px(4.0))
                        .bg(activity_color(level)),
                );
            }
            heatmap = heatmap.child(week);
        }

        let insight_rows = [
            ("快速模式", "17%"),
            ("最常用的推理强度", "最高 · 91%"),
            ("已探索的技能", "53"),
            ("使用的技能总数", "962"),
            ("聊天总数", "1,806"),
        ];
        let plugin_rows = [
            ("$git-commit-message", "156 次运行"),
            ("$codebase-design", "127 次运行"),
            ("$openai-docs", "123 次运行"),
            ("$tdd", "68 次运行"),
            ("$ui-ux-pro-max", "64 次运行"),
        ];
        let list = |title: &'static str, rows: &[(&'static str, &'static str)], plugins: bool| {
            let mut row_list = div().flex().flex_col().gap(px(8.0));
            for (label, value) in rows {
                let leading = if plugins {
                    div()
                        .min_w(px(0.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .size(px(24.0))
                                .flex_none()
                                .rounded(px(8.0))
                                .border_1()
                                .border_color(profile_border)
                                .flex()
                                .items_center()
                                .justify_center()
                                .when(*label == "$openai-docs", |icon| {
                                    icon.child(
                                        svg()
                                            .path("icons/settings-account.svg")
                                            .size(px(21.0))
                                            .text_color(theme.text),
                                    )
                                })
                                .when(*label != "$openai-docs", |icon| {
                                    icon.child(
                                        gpui::img("icons/profile-plugin-cube.svg").size_full(),
                                    )
                                }),
                        )
                        .child(div().min_w(px(0.0)).text_color(theme.text).child(*label))
                } else {
                    div().text_color(theme.settings_description).child(*label)
                };
                row_list = row_list.child(
                    div()
                        .h(px(24.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(12.0))
                        .text_size(px(14.0))
                        .line_height(px(20.0))
                        .child(leading)
                        .child(
                            div()
                                .flex_none()
                                .text_color(if plugins {
                                    theme.settings_description
                                } else {
                                    theme.text
                                })
                                .child(*value),
                        ),
                );
            }
            div()
                .min_w(px(0.0))
                .flex_1()
                .pl(px(1.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .h(px(20.0))
                        .text_size(px(14.0))
                        .line_height(px(20.0))
                        .font_weight(gpui::FontWeight(500.0))
                        .child(title),
                )
                .child(row_list)
        };
        let header_action =
            |label: &'static str, path: &'static str, icon_size: f32, gap: f32, muted: bool| {
                div()
                    .h(px(28.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(gap))
                    .text_color(if muted { profile_tertiary } else { theme.text })
                    .child(svg().path(path).size(px(icon_size)).text_color(if muted {
                        profile_tertiary
                    } else {
                        theme.text
                    }))
                    .child(label)
            };

        div()
            .w_full()
            .max_w(px(732.0))
            .mx_auto()
            .pt(px(14.0))
            .pb(px(80.0))
            .font_family(".SystemUIFont")
            .font_weight(normal_weight)
            .flex()
            .flex_col()
            .child(
                div()
                    .ml(px(toolbar_offset))
                    .w(px(toolbar_width))
                    .h(px(24.0))
                    .relative()
                    .top(px(-2.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .child(div().relative().left(px(1.0)).child("个人资料"))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(header_action(
                                "邀请好友",
                                "icons/profile-invite.svg",
                                16.0,
                                4.0,
                                false,
                            ))
                            .child(header_action(
                                "分享",
                                "icons/profile-share.svg",
                                20.0,
                                4.0,
                                false,
                            ))
                            .child(header_action(
                                "私有",
                                "icons/profile-lock.svg",
                                18.0,
                                6.0,
                                true,
                            ))
                            .child(header_action(
                                "编辑",
                                "icons/settings-edit.svg",
                                21.0,
                                4.0,
                                false,
                            )),
                    ),
            )
            .child(
                div()
                    .mt(px(76.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        div()
                            .size(px(80.0))
                            .rounded_full()
                            .bg(gpui::rgba(0x98a5a6ff))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(27.0))
                            .text_color(gpui::white())
                            .child("RI"),
                    )
                    .child(
                        div()
                            .mt(px(12.0))
                            .h(px(32.0))
                            .relative()
                            .top(px(4.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(24.0))
                            .line_height(px(32.0))
                            .child("rita"),
                    )
                    .child(
                        div()
                            .mt(px(1.0))
                            .min_h(px(28.0))
                            .relative()
                            .top(px(7.0))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .text_color(profile_tertiary)
                            .child("@zb3242957365")
                            .child(
                                div()
                                    .text_color(theme.text_tertiary)
                                    .opacity(0.5)
                                    .child("·"),
                            )
                            .child(
                                div()
                                    .h(px(24.0))
                                    .px(px(5.0))
                                    .rounded(px(8.0))
                                    .border_1()
                                    .border_color(profile_border)
                                    .flex()
                                    .items_center()
                                    .text_size(px(12.0))
                                    .text_color(profile_tertiary)
                                    .child("Pro"),
                            ),
                    ),
            )
            .child(div().mt(px(39.0)).child(stats))
            .child(
                div()
                    .mt(px(38.0))
                    .relative()
                    .top(px(activity_heading_offset))
                    .flex()
                    .justify_between()
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child(div().relative().left(px(1.0)).child("Token 活动"))
                    .child(
                        div()
                            .flex()
                            .gap(px(12.0))
                            .font_weight(normal_weight)
                            .text_color(profile_tertiary)
                            .child(div().text_color(theme.text).child("每日"))
                            .child("每周")
                            .child("累计"),
                    ),
            )
            .child(div().mt(px(11.0)).relative().top(px(11.0)).child(heatmap))
            .child(
                div()
                    .mt(px(6.0))
                    .relative()
                    .top(px(11.0))
                    .flex()
                    .justify_between()
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(profile_tertiary)
                    .children([
                        "9月", "10月", "11月", "12月", "1月", "2月", "3月", "4月", "5月", "6月",
                        "7月", "8月",
                    ]),
            )
            .child(
                div()
                    .mt(px(39.0))
                    .relative()
                    .top(px(14.0))
                    .flex()
                    .gap(px(40.0))
                    .child(list("活动洞察", &insight_rows, false))
                    .child(list("最常用的插件", &plugin_rows, true)),
            )
            .into_any_element()
    }
}
