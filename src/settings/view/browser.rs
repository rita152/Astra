//! Browser settings presentation.

use gpui::{Context, IntoElement, div, prelude::*, px, svg};

use super::SettingsView;
use crate::{
    settings::{ControlSpec, PageSpec},
    theme::{Theme, ThemeMode},
};

impl SettingsView {
    pub(super) fn browser_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let first = &page.sections[0].rows[0];
        let general = &page.sections[1];
        let link_color = gpui::rgba(0x539af8ff);
        let intro_color = match self.mode {
            ThemeMode::Light => gpui::rgba(0x8e8e8eff),
            ThemeMode::Dark => gpui::rgba(0x797979ff),
        };
        let browser_card_border = match self.mode {
            ThemeMode::Light => gpui::rgba(0xe9e9e9ff),
            ThemeMode::Dark => theme.border,
        };
        let browser_button_fill = match self.mode {
            ThemeMode::Light => gpui::rgba(0xefefefff),
            ThemeMode::Dark => theme.settings_button,
        };
        let browser_header_button_fill = match self.mode {
            ThemeMode::Light => gpui::rgba(0xf3f3f4ff),
            ThemeMode::Dark => gpui::rgba(0x222222ff),
        };
        let browser_primary = match self.mode {
            ThemeMode::Light => gpui::rgba(0x363636ff),
            ThemeMode::Dark => gpui::rgba(0xb4b4b4ff),
        };
        let browser_secondary = match self.mode {
            ThemeMode::Light => gpui::rgba(0xa0a0a0ff),
            ThemeMode::Dark => gpui::rgba(0x6e6e6eff),
        };
        let browser_label =
            |title: &'static str, subtitle: &'static str, title_nudge: f32, subtitle_nudge: f32| {
                div()
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
                            .text_color(browser_primary)
                            .child(title),
                    )
                    .when(!subtitle.is_empty(), |column| {
                        column.child(
                            div()
                                .relative()
                                .top(px(subtitle_nudge))
                                .text_size(px(12.0))
                                .line_height(px(16.0))
                                .text_color(browser_secondary)
                                .child(subtitle),
                        )
                    })
            };
        let browser_button = |label: &'static str, width: f32, header: bool| {
            div()
                .w(px(width))
                .h(px(28.0))
                .flex_none()
                .px(px(8.0))
                .rounded(px(12.5))
                .bg(if header {
                    browser_header_button_fill
                } else {
                    browser_button_fill
                })
                .flex()
                .items_center()
                .justify_center()
                .gap(px(4.0))
                .text_size(px(14.0))
                .line_height(px(18.0))
                .text_color(theme.text)
                .whitespace_nowrap()
                .cursor_pointer()
                .child(label)
                .into_any_element()
        };

        let mut general_card = div()
            .w_full()
            .h(px(304.8125))
            .relative()
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(browser_card_border)
            .bg(theme.settings_panel);
        for (index, row) in general.rows.iter().enumerate().skip(1) {
            let right = match row.control {
                ControlSpec::Button(label) => browser_button(
                    label,
                    if label == "清除浏览数据" {
                        102.0
                    } else {
                        46.0
                    },
                    false,
                ),
                ControlSpec::Select(label) => self.agent_select(
                    label,
                    match label {
                        "默认浏览器" => 114.0,
                        "ChatGPT" => 102.25,
                        "始终包含" => 168.0,
                        _ => 152.0,
                    },
                    theme,
                ),
                ControlSpec::Switch(checked) => {
                    self.reference_switch_control(checked, (page.slug, 1, index), theme, cx)
                }
                _ => div().into_any_element(),
            };
            general_card = general_card.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px((index - 1) as f32 * 60.5625))
                    .h(px(60.5625))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(24.0))
                    .when(index + 1 != general.rows.len(), |item| {
                        item.child(
                            div()
                                .absolute()
                                .bottom_0()
                                .left(px(16.0))
                                .right(px(16.0))
                                .h(px(1.0))
                                .bg(browser_card_border),
                        )
                    })
                    .child({
                        let phase = index - 1;
                        let title_nudge = [0.0, -1.0, 0.0, -1.0, 0.0][phase];
                        let subtitle_nudge = [-3.0, -3.0, -3.0, -3.0, -1.0][phase];
                        browser_label(row.title, row.subtitle, title_nudge, subtitle_nudge)
                    })
                    .child(
                        div()
                            .flex_none()
                            .relative()
                            .left(px(if matches!(index, 2 | 4) { -1.0 } else { 0.0 }))
                            .child(right),
                    ),
            );
        }

        let autofill = &page.sections[2];
        let mut autofill_card = div()
            .w_full()
            .h(px(123.125))
            .relative()
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(browser_card_border)
            .bg(theme.settings_panel);
        for (index, row) in autofill.rows.iter().enumerate() {
            autofill_card = autofill_card.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(index as f32 * 60.5625))
                    .h(px(60.5625))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(24.0))
                    .when(index + 1 != autofill.rows.len(), |item| {
                        item.child(
                            div()
                                .absolute()
                                .bottom_0()
                                .left(px(16.0))
                                .right(px(16.0))
                                .h(px(1.0))
                                .bg(browser_card_border),
                        )
                    })
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .relative()
                            .child(browser_label(
                                row.title,
                                row.subtitle,
                                if index == 0 { 1.0 } else { 0.0 },
                                -2.0,
                            )),
                    )
                    .child(
                        div()
                            .flex_none()
                            .relative()
                            .left(px(-1.0))
                            .top(px(if index == 0 { 1.0 } else { 0.0 }))
                            .child(browser_button("管理", 46.0, false)),
                    ),
            );
        }

        let download = &page.sections[3];
        let mut download_card = div()
            .w_full()
            .h(px(183.6875))
            .relative()
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel);
        for (index, row) in download.rows.iter().enumerate() {
            let right = match row.control {
                ControlSpec::Switch(checked) => {
                    self.reference_switch_control(checked, (page.slug, 3, index), theme, cx)
                }
                ControlSpec::Button(label) => {
                    self.reference_button(label, 46.0, None, false, theme)
                }
                _ => div().into_any_element(),
            };
            download_card = download_card.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(index as f32 * 60.5625))
                    .h(px(60.5625))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(24.0))
                    .when(index + 1 != download.rows.len(), |item| {
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
                    .child(browser_label(row.title, row.subtitle, 0.0, -2.0))
                    .child(right),
            );
        }

        let permissions = &page.sections[4];
        let mut permission_card = div()
            .w_full()
            .h(px(365.375))
            .relative()
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel);
        for (index, row) in permissions.rows.iter().take(6).enumerate() {
            let right = match row.control {
                ControlSpec::Switch(checked) => {
                    self.reference_switch_control(checked, (page.slug, 4, index), theme, cx)
                }
                ControlSpec::Button(label) => {
                    self.reference_button(label, 46.0, None, false, theme)
                }
                ControlSpec::Select(label) => self.agent_select(label, 152.0, theme),
                _ => div().into_any_element(),
            };
            permission_card = permission_card.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(index as f32 * 60.5625))
                    .h(px(60.5625))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(24.0))
                    .when(index != 5, |item| {
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

        let developer = &page.sections[5];
        let developer_row = &developer.rows[0];
        let browser_enabled_key = (page.slug, 0, 0);
        let browser_enabled = self
            .switch_overrides
            .get(&browser_enabled_key)
            .copied()
            .unwrap_or(true);

        div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .relative()
            .left(px(1.0))
            .pt(px(66.0))
            .pb(px(80.0))
            .font_family(".SystemUIFont")
            .child(
                div()
                    .relative()
                    .left(px(1.0))
                    .top(px(-1.0))
                    .text_size(px(24.0))
                    .line_height(px(28.8))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .child(page.label),
            )
            .child(
                div()
                    .mt(px(6.0))
                    .flex()
                    .items_center()
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .text_color(intro_color)
                    .child("管理内置浏览器。可在")
                    .child(div().text_color(link_color).child("计算机使用设置"))
                    .child("中设置浏览器扩展程序"),
            )
            .child(
                div()
                    .mt(px(32.0))
                    .h(px(66.0))
                    .px(px(16.0))
                    .rounded(px(20.0))
                    .border_1()
                    .border_color(browser_card_border)
                    .bg(theme.settings_panel)
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        svg()
                            .path("icons/settings-browser-card.svg")
                            .size(px(40.0))
                            .flex_none()
                            .relative()
                            .left(px(-1.0))
                            .text_color(theme.text),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .relative()
                            .left(px(-1.0))
                            .child(browser_label(first.title, first.subtitle, 1.0, -1.0)),
                    )
                    .child(
                        div().flex_none().relative().left(px(-1.0)).child(
                            div()
                                .id(("settings-reference-switch", 0usize))
                                .w(px(32.0))
                                .h(px(20.0))
                                .p(px(2.0))
                                .rounded_full()
                                .flex()
                                .items_center()
                                .when(browser_enabled, |track| track.justify_end().bg(link_color))
                                .when(!browser_enabled, |track| {
                                    track.justify_start().bg(theme.settings_switch_off)
                                })
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.switch_overrides
                                        .insert(browser_enabled_key, !browser_enabled);
                                    cx.notify();
                                }))
                                .child(
                                    div()
                                        .size(px(16.0))
                                        .rounded_full()
                                        .bg(gpui::white())
                                        .border_1()
                                        .border_color(gpui::rgba(0x00000012)),
                                ),
                        ),
                    ),
            )
            .child(
                div()
                    .mt(px(46.0))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child(div().child(general.title))
                    .child(
                        div()
                            .flex_none()
                            .relative()
                            .left(px(-1.0))
                            .child(browser_button("导入…", 57.125, true)),
                    ),
            )
            .child(div().mt(px(12.0)).child(general_card))
            .child(
                div()
                    .mt(px(49.5))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child(autofill.title),
            )
            .child(div().mt(px(15.5)).child(autofill_card))
            .child(
                div()
                    .mt(px(49.5))
                    .relative()
                    .left(px(-1.0))
                    .top(px(1.0))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child(download.title),
            )
            .child(div().mt(px(15.5)).child(download_card))
            .child(
                div()
                    .mt(px(49.5))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child(permissions.title),
            )
            .child(div().mt(px(15.5)).child(permission_card))
            .child(
                div()
                    .mt(px(40.0))
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
                                    .child(permissions.rows[6].title),
                            )
                            .child(
                                div()
                                    .mt(px(2.0))
                                    .text_size(px(13.0))
                                    .line_height(px(18.0))
                                    .text_color(theme.settings_description)
                                    .child(permissions.rows[6].subtitle),
                            ),
                    )
                    .child(self.reference_button("添加", 66.0, None, false, theme)),
            )
            .child(
                div()
                    .mt(px(12.0))
                    .h(px(66.0))
                    .rounded(px(20.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.settings_panel)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.0))
                    .line_height(px(18.5625))
                    .text_color(theme.settings_description)
                    .child(permissions.rows[7].title),
            )
            .child(
                div()
                    .mt(px(49.5))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child(developer.title),
            )
            .child(
                div()
                    .mt(px(15.5))
                    .h(px(101.125))
                    .px(px(16.0))
                    .rounded(px(20.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.settings_panel)
                    .flex()
                    .items_start()
                    .gap(px(24.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .mt(px(12.0))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .text_size(px(13.0))
                                    .line_height(px(18.5625))
                                    .font_weight(gpui::FontWeight(500.0))
                                    .text_color(theme.warning)
                                    .child(
                                        svg()
                                            .path("icons/settings-warning.svg")
                                            .size(px(16.0))
                                            .flex_none(),
                                    )
                                    .child(developer.subtitle),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(px(13.0))
                                    .line_height(px(18.5625))
                                    .font_weight(gpui::FontWeight(500.0))
                                    .child(developer_row.title),
                            )
                            .child(
                                div()
                                    .mt(px(2.0))
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .text_color(theme.settings_description)
                                    .child(developer_row.subtitle),
                            ),
                    )
                    .child(div().mt(px(39.5625)).child(self.reference_switch_control(
                        true,
                        (page.slug, 5, 0),
                        theme,
                        cx,
                    ))),
            )
            .into_any_element()
    }
}
