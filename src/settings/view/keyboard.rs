//! Keyboard settings presentation.

use gpui::{Context, IntoElement, div, prelude::*, px, svg};

use super::SettingsView;
use crate::{
    settings::{ControlSpec, PageSpec},
    theme::Theme,
};

impl SettingsView {
    pub(super) fn keyboard_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        _cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let row_count: usize = page.sections.iter().map(|section| section.rows.len()).sum();
        let is_dark = theme.surface == gpui::rgba(0x181818ff);
        let reference_border = if is_dark {
            gpui::rgba(0x272727ff)
        } else {
            gpui::rgba(0xe9e9e9ff)
        };
        let search_border = if is_dark {
            gpui::rgba(0x3d3d3dff)
        } else {
            gpui::rgba(0xe3e3e4ff)
        };
        let reset_bg = if is_dark {
            gpui::rgba(0x222222ff)
        } else {
            gpui::rgba(0xf3f3f4ff)
        };
        let shortcut_bg = if is_dark {
            gpui::rgba(0xdfdfdf11)
        } else {
            gpui::rgba(0xedededff)
        };
        let mut card = div()
            .w_full()
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(reference_border)
            .bg(theme.settings_panel);
        let mut flat_index = 0;
        for section in page.sections {
            for row in section.rows {
                let row_height = if flat_index == 0 {
                    88.0
                } else {
                    let base = 295.796_88_f32;
                    let step = 60.5625_f32;
                    (base + flat_index as f32 * step).floor()
                        - (base + (flat_index - 1) as f32 * step).floor()
                };
                let mut bindings = div().w(px(384.0)).flex_none().flex().flex_col();
                if let ControlSpec::Shortcut(shortcuts) = row.control {
                    for shortcut in shortcuts.split(" · ") {
                        let assigned = shortcut != "未分配";
                        let binding = if assigned {
                            div()
                                .h(px(20.0))
                                .px(px(8.0))
                                .rounded(px(10.0))
                                .bg(shortcut_bg)
                                .flex()
                                .items_center()
                                .text_size(px(12.0))
                                .line_height(px(12.0))
                                .text_color(theme.settings_description)
                                .whitespace_nowrap()
                                .child(shortcut)
                        } else {
                            div()
                                .h(px(32.0))
                                .flex()
                                .items_center()
                                .text_size(px(13.0))
                                .line_height(px(18.5))
                                .text_color(theme.settings_description)
                                .whitespace_nowrap()
                                .child(shortcut)
                        };
                        bindings = bindings.child(
                            div()
                                .h(px(32.0))
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child(binding)
                                .child(
                                    div()
                                        .size(px(28.0))
                                        .flex_none()
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
                                .child(div().flex_1())
                                .when(assigned, |line| {
                                    line.child(
                                        div()
                                            .size(px(28.0))
                                            .flex_none()
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
                                }),
                        );
                    }
                }
                card = card.child(
                    div()
                        .h(px(row_height))
                        .flex_none()
                        .px(px(16.0))
                        .py(px(12.0))
                        .relative()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(24.0))
                        .when(flat_index + 1 != row_count, |node| {
                            node.child(
                                div()
                                    .absolute()
                                    .bottom_0()
                                    .left(px(16.0))
                                    .right(px(16.0))
                                    .h(px(0.5))
                                    .bg(reference_border),
                            )
                        })
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
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .line_height(px(16.0))
                                        .text_color(theme.settings_description)
                                        .child(row.subtitle),
                                ),
                        )
                        .child(bindings),
                );
                flat_index += 1;
            }
        }
        div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .pt(px(64.0))
            .pb(px(80.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(32.0))
                    .flex()
                    .items_start()
                    .justify_between()
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
                            .relative()
                            .left(px(1.0))
                            .top(px(2.0))
                            .min_h(px(28.0))
                            .px(px(8.0))
                            .rounded(px(12.5))
                            .border_1()
                            .border_color(gpui::rgba(0x00000000))
                            .bg(reset_bg)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(14.0))
                            .line_height(px(18.0))
                            .whitespace_nowrap()
                            .child("全部重置为默认值"),
                    ),
            )
            .child(
                div()
                    .mt(px(51.0))
                    .h(px(32.0))
                    .px(px(10.0))
                    .rounded(px(17.0))
                    .border_1()
                    .border_color(search_border)
                    .bg(theme.surface)
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(13.0))
                    .text_color(theme.text_tertiary)
                    .child(
                        svg()
                            .path("icons/search.svg")
                            .size(px(18.0))
                            .text_color(theme.settings_description),
                    )
                    .child(div().flex_1().child("搜索快捷键"))
                    .child(
                        div()
                            .size(px(28.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                svg()
                                    .path("icons/settings-shortcut-search.svg")
                                    .size(px(18.0))
                                    .text_color(theme.text_tertiary),
                            ),
                    ),
            )
            .child(div().mt(px(28.0)).child(card))
            .into_any_element()
    }
}
