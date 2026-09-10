//! Controls settings presentation.

use gpui::{Context, IntoElement, div, prelude::*, px, svg};

use super::SettingsView;
use crate::{
    settings::{ControlSpec, PageKind, PageSpec, RowSpec, SectionSpec},
    theme::{Theme, ThemeMode},
};

impl SettingsView {
    pub(super) fn agent_select(
        &self,
        label: &'static str,
        width: f32,
        theme: Theme,
    ) -> gpui::AnyElement {
        div()
            .w(px(width))
            .h(px(28.0))
            .flex_none()
            .px(px(12.0))
            .rounded(px(12.5))
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_control)
            .flex()
            .items_center()
            .justify_between()
            .gap(px(6.0))
            .text_size(px(14.0))
            .line_height(px(18.0))
            .whitespace_nowrap()
            .child(label)
            .child(
                svg()
                    .path("icons/chevron-down.svg")
                    .size(px(16.0))
                    .text_color(theme.text_tertiary),
            )
            .into_any_element()
    }

    pub(super) fn switch_control(
        &self,
        checked: bool,
        key: (&'static str, usize, usize),
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let checked = self.switch_overrides.get(&key).copied().unwrap_or(checked);
        div()
            .id(("settings-switch", key.1 * 1000 + key.2))
            .w(px(32.0))
            .h(px(20.0))
            .p(px(2.0))
            .rounded_full()
            .flex()
            .items_center()
            .when(checked, |track| {
                track.justify_end().bg(theme.settings_accent)
            })
            .when(!checked, |track| {
                track.justify_start().bg(theme.settings_switch_off)
            })
            .when(
                key.0 == "general-settings" && key.1 == 0 && key.2 == 0,
                |track| track.opacity(0.6),
            )
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.switch_overrides.insert(key, !checked);
                cx.notify();
            }))
            .child(
                div()
                    .size(px(16.0))
                    .rounded_full()
                    .bg(gpui::white())
                    .border_1()
                    .border_color(gpui::rgba(0x00000012)),
            )
    }
    pub(super) fn small_button(
        &self,
        label: &'static str,
        danger: bool,
        theme: Theme,
    ) -> impl IntoElement {
        div()
            .min_h(px(28.0))
            .px(px(8.0))
            .rounded(px(12.5))
            .border_1()
            .border_color(gpui::rgba(0x00000000))
            .bg(theme.settings_button)
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(14.0))
            .line_height(px(18.0))
            .text_color(if danger {
                gpui::rgba(0xff6b61ff)
            } else {
                theme.text
            })
            .whitespace_nowrap()
            .cursor_pointer()
            .hover(move |style| style.bg(theme.settings_switch_off))
            .child(label)
    }
    pub(super) fn control(
        &self,
        control: ControlSpec,
        key: (&'static str, usize, usize),
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        match control {
            ControlSpec::None => div().into_any_element(),
            ControlSpec::Switch(checked) => div()
                .flex()
                .items_center()
                .gap(px(if key.0 == "usage" && key.1 == 1 && key.2 == 1 {
                    8.0
                } else {
                    10.0
                }))
                .when(key.0 == "usage" && key.1 == 1 && key.2 == 1, |group| {
                    group.child(
                        div()
                            .h(px(24.0))
                            .px(px(10.0))
                            .rounded_full()
                            .bg(if theme.surface == gpui::rgba(0x181818ff) {
                                gpui::rgba(0x2d2934ff)
                            } else {
                                gpui::rgba(0xf0eafaff)
                            })
                            .flex()
                            .items_center()
                            .text_size(px(13.0))
                            .line_height(px(13.0))
                            .font_weight(gpui::FontWeight(500.0))
                            .text_color(theme.effort)
                            .child("最高可享 40% 折扣"),
                    )
                })
                .child(self.switch_control(checked, key, theme, cx))
                .into_any_element(),
            ControlSpec::Button(label) if key.0 == "voice" && key.1 == 3 && key.2 == 0 => div()
                .h(px(28.0))
                .px(px(8.0))
                .rounded(px(12.5))
                .border_1()
                .border_color(gpui::rgba(0x00000000))
                .bg(theme.settings_button)
                .flex()
                .items_center()
                .gap(px(4.0))
                .text_size(px(14.0))
                .line_height(px(18.0))
                .text_color(theme.text)
                .child(
                    svg()
                        .path("icons/add.svg")
                        .size(px(14.0))
                        .text_color(theme.text),
                )
                .child(label)
                .into_any_element(),
            ControlSpec::Button(label) => self.small_button(label, false, theme).into_any_element(),
            ControlSpec::Danger(label) => self.small_button(label, true, theme).into_any_element(),
            ControlSpec::Select(label) if key.0 == "voice" && key.1 == 1 && key.2 == 0 => div()
                .h(px(28.0))
                .px(px(8.0))
                .rounded(px(12.5))
                .border_1()
                .border_color(gpui::rgba(0x00000000))
                .bg(theme.settings_button)
                .flex()
                .items_center()
                .gap(px(4.0))
                .text_size(px(14.0))
                .line_height(px(18.0))
                .text_color(theme.text)
                .child(
                    div()
                        .size(px(12.0))
                        .rounded_full()
                        .bg(theme.settings_accent),
                )
                .child(label)
                .into_any_element(),
            ControlSpec::Select(label) => div()
                .min_h(px(28.0))
                .px(px(12.0))
                .rounded(px(12.5))
                .border_1()
                .border_color(theme.border)
                .bg(theme.settings_control)
                .flex()
                .items_center()
                .gap(px(7.0))
                .text_size(px(14.0))
                .line_height(px(18.0))
                .text_color(theme.text)
                .whitespace_nowrap()
                .when(
                    key.0 == "general-settings" && key.1 == 1 && key.2 == 1,
                    |control| {
                        control.child(
                            svg()
                                .path("icons/settings-cursor.svg")
                                .size(px(18.0))
                                .flex_none(),
                        )
                    },
                )
                .child(label)
                .child(
                    svg()
                        .path("icons/chevron-down.svg")
                        .size(px(12.0))
                        .text_color(theme.text_tertiary),
                )
                .into_any_element(),
            ControlSpec::Value(_) if key.0 == "general-settings" && key.1 == 1 && key.2 == 0 => {
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(if theme.surface == gpui::rgba(0x181818ff) {
                                gpui::rgba(0xdfdfdf80)
                            } else {
                                theme.text_tertiary
                            })
                            .whitespace_nowrap()
                            .child("/Users/zp/Documents/Codex"),
                    )
                    .child(self.small_button("更改", false, theme))
                    .into_any_element()
            }
            ControlSpec::Value(label) => div()
                .flex()
                .items_center()
                .gap(px(22.0))
                .when(label.starts_with("剩余 "), |group| {
                    let full = label.contains("100%");
                    group.child(
                        div()
                            .w(px(96.0))
                            .h(px(8.0))
                            .relative()
                            .left(px(if full { 6.0 } else { -2.0 }))
                            .rounded_full()
                            .bg(theme.settings_switch_off)
                            .child(
                                div()
                                    .h_full()
                                    .w(px(if full { 96.0 } else { 78.0 }))
                                    .rounded_full()
                                    .bg(theme.text),
                            ),
                    )
                })
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .text_color(theme.text_secondary)
                        .whitespace_nowrap()
                        .child(label),
                )
                .into_any_element(),
            ControlSpec::Shortcut(label)
                if key.0 == "voice"
                    && ((key.1 == 1 && key.2 == 1) || (key.1 == 2 && key.2 == 1)) =>
            {
                div()
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .text_size(px(13.0))
                    .line_height(px(18.5))
                    .text_color(theme.text_tertiary)
                    .child(label)
                    .child(
                        div()
                            .size(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                svg()
                                    .path("icons/settings-edit.svg")
                                    .size(px(16.0))
                                    .text_color(theme.text_tertiary),
                            ),
                    )
                    .into_any_element()
            }
            ControlSpec::Shortcut(label) if key.0 == "voice" && key.1 == 2 && key.2 == 0 => div()
                .h(px(32.0))
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(
                    div()
                        .h(px(20.0))
                        .px(px(8.0))
                        .rounded(px(5.0))
                        .bg(theme.sidebar_hover)
                        .flex()
                        .items_center()
                        .text_size(px(12.0))
                        .line_height(px(12.0))
                        .text_color(theme.text_tertiary)
                        .child(label),
                )
                .child(
                    div()
                        .size(px(28.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            svg()
                                .path("icons/settings-edit.svg")
                                .size(px(16.0))
                                .text_color(theme.text_tertiary),
                        ),
                )
                .child(
                    div()
                        .ml(px(4.0))
                        .size(px(28.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            svg()
                                .path("icons/settings-trash.svg")
                                .size(px(16.0))
                                .text_color(theme.text_tertiary),
                        ),
                )
                .into_any_element(),
            ControlSpec::Shortcut(label) => div()
                .min_h(px(20.0))
                .px(px(6.0))
                .rounded(px(5.0))
                .bg(theme.sidebar_hover)
                .flex()
                .items_center()
                .text_size(px(12.0))
                .text_color(theme.text_secondary)
                .whitespace_nowrap()
                .child(label)
                .into_any_element(),
            ControlSpec::Segmented(labels, selected) => {
                let mut group = div().flex().items_center().gap(px(2.0));
                for (index, label) in labels.iter().enumerate() {
                    group = group.child(
                        div()
                            .px(px(8.0))
                            .py(px(3.0))
                            .rounded_full()
                            .text_size(px(13.0))
                            .text_color(if index == selected {
                                theme.text
                            } else {
                                theme.text_tertiary
                            })
                            .when(index == selected, |item| item.bg(theme.settings_button))
                            .child(*label),
                    );
                }
                group.into_any_element()
            }
        }
    }
    pub(super) fn row(
        &self,
        location: SettingRowLocation,
        row: &'static RowSpec,
        last: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let SettingRowLocation {
            slug,
            section_index,
            row_index,
        } = location;
        div()
            .min_h(px(58.0))
            .when(slug == "general-settings", |node| {
                node.h(px(if section_index == 0 {
                    if row_index == 0 { 61.0 } else { 76.0 }
                } else if row_index < 2 || row_index % 2 == 1 {
                    61.0
                } else {
                    60.0
                }))
                .flex_none()
            })
            .when(slug == "usage", |node| {
                node.h(px(match section_index {
                    0 => 60.0,
                    2 | 4 => 61.0,
                    3 if row_index == 1 => 60.0,
                    3 => 61.0,
                    1 if row_index == 2 => 51.0,
                    1 => 61.0,
                    _ => 61.0,
                }))
                .flex_none()
            })
            .when(slug == "voice" && section_index == 1, |node| {
                node.h(px(61.0)).flex_none()
            })
            .when(slug == "voice" && section_index == 2, |node| {
                node.h(px(match row_index {
                    0 | 1 | 3 => 61.0,
                    _ => 60.0,
                }))
                .flex_none()
            })
            .when(
                slug == "usage" && section_index == 1 && row_index == 2,
                |node| node.min_h(px(0.0)),
            )
            .px(px(16.0))
            .py(px(12.0))
            .relative()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(24.0))
            .when(!last, |node| {
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
                    .when(
                        slug == "voice"
                            && ((section_index == 2 && row_index > 0)
                                || section_index == 3),
                        |column| column.relative().top(px(-1.0)),
                    )
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(18.5))
                            .font_weight(gpui::FontWeight(if matches!(
                                slug,
                                "general-settings" | "voice" | "usage"
                            ) {
                                500.0
                            } else {
                                300.0
                            }))
                            .text_color(theme.text)
                            .when(
                                slug == "general-settings"
                                    && section_index == 0
                                    && row_index == 1,
                                |title| title.relative().top(px(-1.0)),
                            )
                            .when(
                                slug == "general-settings"
                                    && section_index == 1
                                    && row_index == 0,
                                |title| title.relative().left(px(2.0)),
                            )
                            .when(
                                slug == "general-settings"
                                    && section_index == 1
                                    && matches!(row_index, 1 | 2),
                                |title| title.relative().top(px(-1.0)),
                            )
                            .child(row.title),
                    )
                    .when(
                        slug == "general-settings" && section_index == 0 && row_index == 1,
                        |column| {
                            column.child(
                                div()
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .text_color(if theme.surface == gpui::rgba(0x181818ff) {
                                        gpui::rgba(0xdfdfdf80)
                                    } else {
                                        theme.text_tertiary
                                    })
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .relative()
                                            .left(px(-1.0))
                                            .child(
                                                "当 ChatGPT 以完整访问权限运行时，它无需你的批准即可编辑你电脑上的任何文件，并运行可访问",
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .child(
                                                "网络的命令。这会显著增加数据丢失、泄露或意外行为的风险。",
                                            )
                                            .child(
                                                div()
                                                    .text_color(if theme.surface
                                                        == gpui::rgba(0x181818ff)
                                                    {
                                                        gpui::rgba(0x99ceffff)
                                                    } else {
                                                        gpui::rgba(0x339cffff)
                                                    })
                                                    .child("了解更多"),
                                            )
                                            .child("关于风险升高的信息。"),
                                    ),
                            )
                        },
                    )
                    .when(
                        !row.subtitle.is_empty()
                            && !(slug == "general-settings"
                                && section_index == 0
                                && row_index == 1),
                        |column| {
                        column.child(
                            div()
                                .text_size(px(12.0))
                                .line_height(px(16.0))
                                .text_color(if slug == "general-settings" {
                                    if theme.surface == gpui::rgba(0x181818ff) {
                                        gpui::rgba(0xdfdfdf80)
                                    } else {
                                        theme.text_tertiary
                                    }
                                } else if slug == "voice" || (slug == "usage"
                                    && theme.surface != gpui::rgba(0x181818ff))
                                {
                                    theme.text_tertiary
                                } else {
                                    theme.settings_description
                                })
                                .when(
                                    slug == "general-settings"
                                        && section_index == 1
                                        && row_index == 0,
                                    |subtitle| subtitle.relative().left(px(1.0)),
                                )
                                .when(
                                    slug == "general-settings"
                                        && section_index == 1
                                        && row_index == 7,
                                    |subtitle| subtitle.relative().left(px(-1.0)),
                                )
                                .child(
                                    if slug == "usage" && section_index == 1 && row_index == 1 {
                                        "达到上限后仍可继续工作"
                                    } else {
                                        row.subtitle
                                    },
                                ),
                        )
                    },
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .max_w(px(300.0))
                    .when(
                        slug == "voice" && matches!(section_index, 2 | 3),
                        |control| control.relative().top(px(-1.0)),
                    )
                    .when(slug == "usage" && section_index == 3, |control| {
                        control.relative().top(px(2.0))
                    })
                    .when(slug == "usage" && section_index == 0, |control| {
                        control.relative().left(px(1.0))
                    })
                    .when(slug == "usage" && section_index == 1, |control| {
                        control
                            .relative()
                            .left(px(1.0))
                            .top(px(if row_index == 0 { 1.0 } else { 0.0 }))
                    })
                    .when(slug == "usage" && section_index == 2, |control| {
                        control.relative().top(px(1.0))
                    })
                    .when(
                        slug == "general-settings" && section_index == 1,
                        |control| {
                            control.relative().left(px(match row_index {
                                0 => 1.0,
                                1 | 2 | 7 => 2.0,
                                5 => -1.0,
                                _ => 0.0,
                            }))
                        },
                    )
                    .when(
                        slug == "general-settings"
                            && section_index == 1
                            && matches!(row_index, 2 | 4),
                        |control| control.relative().top(px(-1.0)),
                    )
                    .child(self.control(
                        row.control,
                        (slug, section_index, row_index),
                        theme,
                        cx,
                    )),
            )
    }
    pub(super) fn section(
        &self,
        slug: &'static str,
        section_index: usize,
        section: &'static SectionSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut card = div()
            .w_full()
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel);
        for (row_index, row) in section.rows.iter().enumerate() {
            card = card.child(self.row(
                SettingRowLocation {
                    slug,
                    section_index,
                    row_index,
                },
                row,
                row_index + 1 == section.rows.len(),
                theme,
                cx,
            ));
        }
        let has_header = !section.title.is_empty() || !section.subtitle.is_empty();
        let usage_balance = slug == "usage" && section_index == 1;
        let voice_expanded_card = slug == "voice" && matches!(section_index, 1 | 2);
        div()
            .w_full()
            .flex()
            .flex_col()
            .when(has_header, |container| {
                container.gap(px(if usage_balance {
                    12.0
                } else if voice_expanded_card {
                    14.0
                } else {
                    16.0
                }))
            })
            .when(has_header, |container| {
                container.child(
                    div()
                        .min_h(px(32.0))
                        .when(usage_balance, |header| header.h(px(45.0)).flex_none())
                        .flex()
                        .flex_col()
                        .justify_end()
                        .gap(px(2.0))
                        .child(
                            div()
                                .relative()
                                .left(px(if slug == "usage" && section_index == 3 {
                                    -4.0
                                } else if matches!(
                                    slug,
                                    "general-settings" | "voice" | "usage"
                                ) {
                                    0.0
                                } else {
                                    1.0
                                }))
                                .top(px(if slug == "voice" && section_index == 2 {
                                    -2.0
                                } else if slug == "voice" && section_index > 0 {
                                    -1.0
                                } else if slug == "usage" && section_index == 2 {
                                    1.0
                                } else {
                                    0.0
                                }))
                                .text_size(px(if usage_balance { 16.0 } else { 14.0 }))
                                .line_height(px(if usage_balance { 24.875 } else { 21.0 }))
                                .font_weight(gpui::FontWeight(if usage_balance
                                    || matches!(
                                        slug,
                                        "general-settings" | "voice" | "usage"
                                    ) {
                                    500.0
                                } else {
                                    400.0
                                }))
                                .text_color(theme.text)
                                .child(section.title),
                        )
                        .when(!section.subtitle.is_empty(), |header| {
                            header.child(
                                div()
                                    .text_size(px(if usage_balance { 13.0 } else { 12.0 }))
                                    .line_height(px(if usage_balance { 18.0 } else { 16.0 }))
                                    .text_color(if matches!(slug, "general-settings" | "voice") || (slug == "usage"
                                        && theme.surface != gpui::rgba(0x181818ff))
                                    {
                                        theme.text_tertiary
                                    } else {
                                        theme.settings_description
                                    })
                                    .when(usage_balance, |subtitle| {
                                        subtitle.flex().child(
                                            "购买额度或启用自动充值，达到限额后仍可继续使用 Codex。",
                                        )
                                        .child(
                                            div()
                                                .ml(px(4.0))
                                                .text_color(if theme.surface
                                                    == gpui::rgba(0x181818ff)
                                                {
                                                    gpui::rgba(0x99ceffff)
                                                } else {
                                                    gpui::rgba(0x339cffff)
                                                })
                                                .child("了解更多"),
                                        )
                                    })
                                    .when(!usage_balance, |subtitle| {
                                        subtitle.child(section.subtitle)
                                    }),
                            )
                        }),
                )
            })
            .child(card)
    }
    pub(super) fn standard_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut content = div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .pt(px(64.0))
            .pb(px(96.0))
            .flex()
            .flex_col()
            .gap(px(30.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .relative()
                            .top(px(if page.kind == PageKind::Usage {
                                2.0
                            } else {
                                1.0
                            }))
                            .text_size(px(24.0))
                            .line_height(px(if page.kind == PageKind::Usage {
                                28.8
                            } else {
                                31.0
                            }))
                            .font_weight(gpui::FontWeight::NORMAL)
                            .text_color(theme.text)
                            .child(page.label),
                    )
                    .when(!page.intro.is_empty(), |header| {
                        header.child(
                            div()
                                .when(page.kind == PageKind::Usage, |intro| {
                                    intro.relative().top(px(2.0))
                                })
                                .text_size(px(if page.kind == PageKind::Usage {
                                    14.0
                                } else {
                                    13.0
                                }))
                                .line_height(px(if page.kind == PageKind::Usage {
                                    21.0
                                } else {
                                    19.0
                                }))
                                .text_color(if page.kind == PageKind::Usage {
                                    if theme.surface == gpui::rgba(0x181818ff) {
                                        theme.settings_description
                                    } else {
                                        theme.text_tertiary
                                    }
                                } else {
                                    theme.text_secondary
                                })
                                .when(page.kind == PageKind::Usage, |intro| {
                                    intro
                                        .flex()
                                        .child(
                                            "如需查看发票、更改付款方式或进行其他操作，请前往网页版",
                                        )
                                        .child(
                                            div()
                                                .text_color(if theme.surface
                                                    == gpui::rgba(0x181818ff)
                                                {
                                                    gpui::rgba(0x99ceffff)
                                                } else {
                                                    gpui::rgba(0x339cffff)
                                                })
                                                .child("设置"),
                                        )
                                })
                                .when(page.kind != PageKind::Usage, |intro| {
                                    intro.child(page.intro)
                                }),
                        )
                    }),
            );
        for (section_index, section) in page.sections.iter().enumerate() {
            content = content.child(
                div()
                    .when(section_index > 0, |item| {
                        item.mt(px(if page.slug == "general-settings" {
                            8.0
                        } else {
                            10.0
                        }))
                    })
                    .when(page.kind == PageKind::Usage && section_index == 1, |item| {
                        item.mt(px(12.0))
                    })
                    .when(page.kind == PageKind::Usage && section_index >= 2, |item| {
                        item.mt(px(8.0))
                    })
                    .when(page.kind == PageKind::Usage && section_index == 0, |item| {
                        item.relative().top(px(2.0))
                    })
                    .when(page.slug == "voice" && section_index == 3, |item| {
                        item.mt(px(-24.0))
                    })
                    .child(self.section(page.slug, section_index, section, theme, cx)),
            );
        }
        content
    }
    pub(super) fn coding_label(
        &self,
        title: &'static str,
        subtitle: &'static str,
        theme: Theme,
    ) -> gpui::AnyElement {
        let primary_text = if theme.surface == gpui::rgba(0x181818ff) {
            gpui::rgba(0xb4b4b4ff)
        } else {
            gpui::rgba(0x363636ff)
        };
        let secondary_text = if theme.surface == gpui::rgba(0x181818ff) {
            gpui::rgba(0x6e6e6eff)
        } else {
            gpui::rgba(0xa0a0a0ff)
        };
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
                    .text_color(primary_text)
                    .child(title),
            )
            .when(!subtitle.is_empty(), |column| {
                column.child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(secondary_text)
                        .child(subtitle),
                )
            })
            .into_any_element()
    }
    pub(super) fn coding_field(
        &self,
        value: &'static str,
        width: f32,
        height: f32,
        muted: bool,
        theme: Theme,
    ) -> gpui::AnyElement {
        let primary_text = if theme.surface == gpui::rgba(0x181818ff) {
            gpui::rgba(0xb4b4b4ff)
        } else {
            gpui::rgba(0x363636ff)
        };
        div()
            .w(px(width))
            .h(px(height))
            .flex_none()
            .px(px(if height <= 28.0 { 8.0 } else { 10.0 }))
            .rounded(px(10.0))
            .border_1()
            .border_color(theme.border)
            .flex()
            .items_center()
            .text_size(px(13.0))
            .line_height(px(18.0))
            .text_color(if muted {
                theme.text_tertiary
            } else {
                primary_text
            })
            .child(value)
            .into_any_element()
    }
    pub(super) fn coding_button(
        &self,
        label: &'static str,
        width: f32,
        icon: Option<&'static str>,
        danger: bool,
        theme: Theme,
    ) -> gpui::AnyElement {
        let danger_text = gpui::rgba(0xff625aff);
        let danger_fill = gpui::rgba(0xff625a1a);
        let mut button = div()
            .w(px(width))
            .h(px(28.0))
            .flex_none()
            .px(px(8.0))
            .rounded(px(10.0))
            .bg(if danger {
                danger_fill
            } else {
                theme.sidebar_hover
            })
            .flex()
            .items_center()
            .justify_center()
            .gap(px(4.0))
            .text_size(px(13.0))
            .line_height(px(18.0))
            .text_color(if danger { danger_text } else { theme.text })
            .whitespace_nowrap()
            .cursor_pointer()
            .hover(move |style| style.bg(theme.settings_switch_off));
        if let Some(path) = icon {
            button = button.child(svg().path(path).size(px(14.0)));
        }
        button.child(label).into_any_element()
    }
    pub(super) fn coding_icon_button(
        &self,
        path: &'static str,
        filled: bool,
        theme: Theme,
    ) -> gpui::AnyElement {
        div()
            .size(px(28.0))
            .flex_none()
            .rounded(px(10.0))
            .when(filled, |button| button.bg(theme.sidebar_hover))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(move |style| style.bg(theme.sidebar_hover))
            .child(
                svg()
                    .path(path)
                    .size(px(15.0))
                    .text_color(theme.text_tertiary),
            )
            .into_any_element()
    }
    pub(super) fn coding_setting_row(
        &self,
        setting: CodingSettingRow,
        right: gpui::AnyElement,
        theme: Theme,
    ) -> gpui::AnyElement {
        let CodingSettingRow {
            title,
            subtitle,
            height,
            content_phase,
            last,
        } = setting;
        div()
            .h(px(height))
            .flex_none()
            .px(px(16.0))
            .relative()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(24.0))
            .when(!last, |row| {
                row.child(
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
                    .relative()
                    .top(px(-content_phase))
                    .child(self.coding_label(title, subtitle, theme)),
            )
            .child(
                div()
                    .flex_none()
                    .relative()
                    .top(px(-content_phase))
                    .child(right),
            )
            .into_any_element()
    }
    pub(super) fn coding_textarea(
        &self,
        placeholder: &'static str,
        height: f32,
        x_nudge: f32,
        y_nudge: f32,
        theme: Theme,
    ) -> gpui::AnyElement {
        let secondary_text = if theme.surface == gpui::rgba(0x181818ff) {
            gpui::rgba(0x6e6e6eff)
        } else {
            gpui::rgba(0xa0a0a0ff)
        };
        div()
            .w_full()
            .h(px(height))
            .flex_none()
            .px(px(10.0))
            .py(px(8.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(theme.border)
            .text_size(px(13.0))
            .line_height(px(18.0))
            .text_color(secondary_text)
            .child(
                div()
                    .relative()
                    .left(px(x_nudge))
                    .top(px(y_nudge))
                    .child(placeholder),
            )
            .into_any_element()
    }
    pub(super) fn reference_switch_control(
        &self,
        checked: bool,
        key: (&'static str, usize, usize),
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let checked = self.switch_overrides.get(&key).copied().unwrap_or(checked);
        div()
            .id(("settings-reference-switch", key.1 * 1000 + key.2))
            .w(px(32.0))
            .h(px(20.0))
            .p(px(2.0))
            .rounded_full()
            .flex()
            .items_center()
            .when(checked, |track| {
                track.justify_end().bg(gpui::rgba(0x339cffff))
            })
            .when(!checked, |track| {
                track.justify_start().bg(theme.settings_switch_off)
            })
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.switch_overrides.insert(key, !checked);
                cx.notify();
            }))
            .child(
                div()
                    .size(px(16.0))
                    .rounded_full()
                    .bg(gpui::white())
                    .border_1()
                    .border_color(gpui::rgba(0x00000012)),
            )
            .into_any_element()
    }
    pub(super) fn reference_label(
        &self,
        title: &'static str,
        subtitle: &'static str,
        theme: Theme,
    ) -> gpui::AnyElement {
        div()
            .min_w(px(0.0))
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .line_height(px(18.5625))
                    .font_weight(gpui::FontWeight(500.0))
                    .text_color(theme.text)
                    .child(title),
            )
            .when(!subtitle.is_empty(), |column| {
                column.child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(theme.settings_description)
                        .child(subtitle),
                )
            })
            .into_any_element()
    }
    pub(super) fn reference_button(
        &self,
        label: &'static str,
        width: f32,
        icon: Option<(&'static str, f32)>,
        danger: bool,
        theme: Theme,
    ) -> gpui::AnyElement {
        let danger_text = match self.mode {
            ThemeMode::Light => gpui::rgba(0xe02e2aff),
            ThemeMode::Dark => gpui::rgba(0xff6764ff),
        };
        let danger_fill = match self.mode {
            ThemeMode::Light => gpui::rgba(0xe02e2a1a),
            ThemeMode::Dark => gpui::rgba(0xff67641a),
        };
        let mut button = div()
            .w(px(width))
            .h(px(28.0))
            .flex_none()
            .px(px(8.0))
            .rounded(px(12.5))
            .bg(if danger {
                danger_fill
            } else {
                theme.settings_button
            })
            .flex()
            .items_center()
            .justify_center()
            .gap(px(4.0))
            .text_size(px(14.0))
            .line_height(px(18.0))
            .text_color(if danger { danger_text } else { theme.text })
            .whitespace_nowrap()
            .cursor_pointer();
        if let Some((path, size)) = icon {
            button = button.child(svg().path(path).size(px(size)));
        }
        button.child(label).into_any_element()
    }
}

#[derive(Clone, Copy)]
pub(super) struct SettingRowLocation {
    pub(super) slug: &'static str,
    pub(super) section_index: usize,
    pub(super) row_index: usize,
}

#[derive(Clone, Copy)]
pub(super) struct CodingSettingRow {
    pub(super) title: &'static str,
    pub(super) subtitle: &'static str,
    pub(super) height: f32,
    pub(super) content_phase: f32,
    pub(super) last: bool,
}
