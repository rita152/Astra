//! Git settings presentation.

use gpui::{Context, IntoElement, div, prelude::*, px};

use super::{SettingsView, controls::CodingSettingRow};
use crate::{settings::PageSpec, theme::Theme};

impl SettingsView {
    pub(super) fn git_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
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
        let rows = page.sections[0].rows;
        let merge = div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .child(
                div()
                    .h(px(24.0))
                    .px(px(8.0))
                    .rounded_full()
                    .bg(theme.sidebar_hover)
                    .flex()
                    .items_center()
                    .text_size(px(13.0))
                    .child("合并"),
            )
            .child(
                div()
                    .h(px(24.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .text_size(px(13.0))
                    .text_color(secondary_text)
                    .child("压缩合并"),
            )
            .into_any_element();
        let review = div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .child(
                div()
                    .h(px(24.0))
                    .px(px(8.0))
                    .rounded_full()
                    .bg(theme.sidebar_hover)
                    .flex()
                    .items_center()
                    .text_size(px(13.0))
                    .child("内联"),
            )
            .child(
                div()
                    .h(px(24.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .text_size(px(13.0))
                    .text_color(secondary_text)
                    .child("单独"),
            )
            .into_any_element();
        let first_card = div()
            .w_full()
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel)
            .child(self.coding_setting_row(
                CodingSettingRow {
                    title: rows[0].title,
                    subtitle: rows[0].subtitle,
                    height: 60.5625,
                    content_phase: 0.0,
                    last: false,
                },
                self.coding_field("codex/", 224.0, 36.0, false, theme),
                theme,
            ))
            .child(self.coding_setting_row(
                CodingSettingRow {
                    title: rows[1].title,
                    subtitle: rows[1].subtitle,
                    height: 60.5625,
                    content_phase: 1.0,
                    last: false,
                },
                merge,
                theme,
            ))
            .child(
                self.coding_setting_row(
                    CodingSettingRow {
                        title: rows[2].title,
                        subtitle: rows[2].subtitle,
                        height: 60.5625,
                        content_phase: 1.0,
                        last: false,
                    },
                    self.switch_control(false, (page.slug, 0, 2), theme, cx)
                        .into_any_element(),
                    theme,
                ),
            )
            .child(
                self.coding_setting_row(
                    CodingSettingRow {
                        title: rows[3].title,
                        subtitle: rows[3].subtitle,
                        height: 60.5625,
                        content_phase: 2.0,
                        last: false,
                    },
                    self.switch_control(true, (page.slug, 0, 3), theme, cx)
                        .into_any_element(),
                    theme,
                ),
            )
            .child(self.coding_setting_row(
                CodingSettingRow {
                    title: rows[4].title,
                    subtitle: rows[4].subtitle,
                    height: 60.5625,
                    content_phase: 2.0,
                    last: true,
                },
                review,
                theme,
            ));

        let monitor = &page.sections[1];
        div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .pt(px(66.0))
            .pb(px(80.0))
            .text_color(primary_text)
            .child(
                div()
                    .text_size(px(24.0))
                    .line_height(px(28.8))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .text_color(theme.text)
                    .child(page.label),
            )
            .child(div().mt(px(32.0)).child(first_card))
            .child(
                div()
                    .mt(px(48.0))
                    .relative()
                    .top(px(-1.0))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child(monitor.title),
            )
            .child(
                div()
                    .mt(px(15.0))
                    .rounded(px(20.0))
                    .overflow_hidden()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.settings_panel)
                    .child(
                        self.coding_setting_row(
                            CodingSettingRow {
                                title: monitor.rows[0].title,
                                subtitle: "继续监控，直到 Pull Request 合并",
                                height: 60.5625,
                                content_phase: 0.0,
                                last: true,
                            },
                            self.switch_control(false, (page.slug, 1, 0), theme, cx)
                                .into_any_element(),
                            theme,
                        ),
                    ),
            )
            .child(div().mt(px(6.0)).child(self.coding_textarea(
                monitor.rows[1].subtitle,
                111.0,
                -3.0,
                -1.0,
                theme,
            )))
            .child(
                div()
                    .mt(px(40.0))
                    .relative()
                    .top(px(-1.0))
                    .text_size(px(16.0))
                    .line_height(px(25.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child(page.sections[2].title),
            )
            .child(
                div()
                    .mt(px(2.0))
                    .relative()
                    .top(px(-1.0))
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .text_color(secondary_text)
                    .child(page.sections[2].subtitle),
            )
            .child(div().mt(px(12.0)).child(self.coding_textarea(
                page.sections[2].rows[0].title,
                130.0,
                0.0,
                -1.0,
                theme,
            )))
            .child(
                div()
                    .mt(px(40.0))
                    .relative()
                    .top(px(-1.0))
                    .text_size(px(16.0))
                    .line_height(px(25.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child(page.sections[3].title),
            )
            .child(
                div()
                    .mt(px(2.0))
                    .relative()
                    .top(px(-1.0))
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .text_color(secondary_text)
                    .child(page.sections[3].subtitle),
            )
            .child(div().mt(px(12.0)).child(self.coding_textarea(
                page.sections[3].rows[0].title,
                130.0,
                0.0,
                -1.0,
                theme,
            )))
            .into_any_element()
    }
}
