//! Data controls settings presentation.

use gpui::{IntoElement, div, prelude::*, px, svg};

use super::SettingsView;
use crate::{
    settings::PageSpec,
    theme::{Theme, ThemeMode, UI_FONT_FAMILY},
};

impl SettingsView {
    pub(super) fn data_controls_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
    ) -> gpui::AnyElement {
        let search_border = match self.mode {
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
        let archive_text_overlay = match self.mode {
            ThemeMode::Light => "icons/settings-data-archive-text-light.svg",
            ThemeMode::Dark => "icons/settings-data-archive-text-dark.svg",
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
                    .font_weight(crate::theme::UI_BODY_FONT_WEIGHT)
                    .text_color(if danger { danger_text } else { theme.text })
                    .whitespace_nowrap();
                if let Some((path, size)) = icon {
                    node = node.child(svg().path(path).size(px(size)).text_color(if danger {
                        danger_text
                    } else {
                        theme.text
                    }));
                }
                node.child(label).into_any_element()
            };
        let icon_button = |path: &'static str, size: f32| {
            div()
                .size(px(28.0))
                .flex_none()
                .rounded(px(12.5))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    svg()
                        .path(path)
                        .size(px(size))
                        .text_color(theme.text_tertiary),
                )
                .into_any_element()
        };
        let search = div()
            .w(px(432.0))
            .h(px(32.0))
            .flex_none()
            .px(px(10.0))
            .rounded_full()
            .border_1()
            .border_color(search_border)
            .bg(theme.surface)
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(14.0))
            .line_height(px(18.0))
            .font_family(UI_FONT_FAMILY)
            .text_color(theme.settings_description)
            .child(
                svg()
                    .path("icons/settings-search-reference.svg")
                    .size(px(18.0))
                    .text_color(theme.settings_description),
            )
            .child("搜索已归档聊天");
        let scope = div()
            .w(px(144.0))
            .h(px(28.0))
            .flex_none()
            .px(px(12.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(card_border)
            .bg(theme.settings_panel)
            .font_family(UI_FONT_FAMILY)
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        svg()
                            .path("icons/settings-filter-reference.svg")
                            .size(px(16.0))
                            .text_color(theme.text),
                    )
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(18.0))
                            .child("全部聊天"),
                    ),
            )
            .child(
                svg()
                    .path("icons/settings-chevron-reference.svg")
                    .size(px(13.6))
                    .text_color(theme.text_tertiary),
            );
        let project = div()
            .w(px(176.0))
            .h(px(28.0))
            .flex_none()
            .px(px(12.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel)
            .font_family(UI_FONT_FAMILY)
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        svg()
                            .path("icons/settings-folder-reference.svg")
                            .size(px(16.0))
                            .text_color(theme.text),
                    )
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(18.0))
                            .child("所有项目"),
                    ),
            )
            .child(
                svg()
                    .path("icons/settings-chevron-reference.svg")
                    .size(px(13.6))
                    .text_color(theme.text_tertiary),
            );

        let mut archive = div()
            .w_full()
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel);
        let archived_rows = page.sections[1].rows;
        for (index, row) in archived_rows.iter().enumerate() {
            // GPUI snaps each 60.5625 px child to a full device pixel. Keep the
            // logical height (and therefore the native scrollbar) exact, then
            // compensate the accumulated first-screen raster phase explicitly.
            let content_phase = match index {
                0 => 0.0,
                1 | 2 => 1.0,
                3..=5 => 2.0,
                6 | 7 => 3.0,
                8..=10 => 4.0,
                _ => (index as f32 * 0.4375).round(),
            };
            let separator_phase = match index {
                0 => 0.0,
                1..=3 => 1.0,
                4 | 5 => 2.0,
                6 | 7 => 3.0,
                8..=10 => 4.0,
                _ => ((index + 1) as f32 * 0.4375).round(),
            };
            // CoreText and Chromium shape the mixed Chinese/Latin row strings
            // a few pixels differently at 1x. Preserve the shared leading
            // edge while optically centering the visible glyph runs.
            let content_x_adjust = match index {
                0 | 6 | 7 | 10 => -1.0,
                1 | 3 | 5 | 8 | 9 => 1.0,
                4 => -2.0,
                _ => 0.0,
            };
            let content_y_adjust = if index == 10 { -1.0 } else { 0.0 };
            let date_x_adjust = match index {
                4 => -1.0,
                _ => -2.0,
            };
            let date_y_adjust = match index {
                1 | 3 | 4 | 6 | 8 | 10 => 1.0,
                _ => 0.0,
            };
            let control_y_adjust = if matches!(index, 1 | 3 | 8) { 1.0 } else { 0.0 };
            let row_title = div()
                .text_size(px(13.0))
                .line_height(px(18.5714))
                .font_weight(gpui::FontWeight(500.0))
                .text_color(gpui::rgba(0x00000000))
                .child(row.title);
            archive = archive.child(
                div()
                    .h(px(60.5625))
                    .flex_none()
                    .px(px(16.0))
                    .relative()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(24.0))
                    .when(index + 1 < archived_rows.len(), |item| {
                        item.child(
                            div()
                                .absolute()
                                .bottom(px(separator_phase))
                                .left(px(16.0))
                                .right(px(16.0))
                                .h(px(1.0))
                                .bg(card_border),
                        )
                    })
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .relative()
                            .top(px(-content_phase - 1.0 + content_y_adjust))
                            .left(px(content_x_adjust))
                            .flex()
                            .flex_col()
                            .gap(px(0.0))
                            .child(row_title)
                            .child(
                                div()
                                    .relative()
                                    .left(px(date_x_adjust))
                                    .top(px(date_y_adjust))
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .text_color(gpui::rgba(0x00000000))
                                    .child(row.subtitle),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .relative()
                            .top(px(-content_phase + control_y_adjust))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(icon_button("icons/settings-trash.svg", 16.0))
                            .child(button("取消归档", 74.0, None, false)),
                    ),
            );
        }
        archive = archive.child(
            gpui::img(archive_text_overlay)
                .absolute()
                .left(px(15.0))
                .top(px(-1.0))
                .w(px(621.0))
                .h(px(647.0)),
        );

        div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .relative()
            .left(px(0.0))
            .pt(px(66.0))
            .pb(px(80.0))
            .font_family(".SystemUIFont")
            .child(
                div()
                    .h(px(29.0))
                    .flex()
                    .items_start()
                    .justify_between()
                    .child(
                        div()
                            .relative()
                            .top(px(0.0))
                            .left(px(0.0))
                            .font_family(UI_FONT_FAMILY)
                            .text_size(px(24.0))
                            .line_height(px(28.8))
                            .font_weight(gpui::FontWeight::NORMAL)
                            .child(page.label),
                    )
                    .child(button(
                        "全部删除",
                        94.0,
                        Some(("icons/settings-trash.svg", 16.0)),
                        true,
                    )),
            )
            .child(
                div()
                    .mt(px(32.0))
                    .h(px(60.0))
                    .pt(px(20.0))
                    .pb(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(search)
                    .child(scope)
                    .child(project),
            )
            .child(
                div()
                    .mt(px(26.0))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                svg()
                                    .path("icons/settings-folder-reference.svg")
                                    .size(px(16.0))
                                    .text_color(theme.text),
                            )
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .line_height(px(21.0))
                                    .font_weight(gpui::FontWeight(500.0))
                                    .child(page.sections[1].title),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .line_height(px(18.5714))
                                    .text_color(theme.settings_description)
                                    .child(page.sections[1].subtitle),
                            )
                            .child(icon_button("icons/more-horizontal.svg", 16.0)),
                    ),
            )
            .child(div().mt(px(12.0)).child(archive))
            .into_any_element()
    }
}
