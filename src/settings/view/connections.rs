//! Connections settings presentation.

use gpui::{Context, IntoElement, div, prelude::*, px, svg};

use super::SettingsView;
use crate::{
    settings::PageSpec,
    theme::{Theme, ThemeMode},
};

impl SettingsView {
    pub(super) fn connections_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let device = &page.sections[1];
        let other = &page.sections[2];
        let danger_text = match self.mode {
            ThemeMode::Light => gpui::rgba(0xe02e2aff),
            ThemeMode::Dark => gpui::rgba(0xff6764ff),
        };
        let danger_fill = match self.mode {
            ThemeMode::Light => gpui::rgba(0xe02e2a1a),
            ThemeMode::Dark => gpui::rgba(0xff67641a),
        };
        div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .pt(px(66.0))
            .pb(px(80.0))
            .font_family(".SystemUIFont")
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
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .text_size(px(14.0))
                    .line_height(px(18.0))
                    .text_color(theme.text_tertiary)
                    .child(
                        div()
                            .w(px(105.140625))
                            .h(px(28.0))
                            .rounded(px(12.5))
                            .bg(theme.settings_button)
                            .text_color(theme.text)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child("控制这台 Mac"),
                    )
                    .child(
                        div()
                            .w(px(102.0))
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child("控制其他设备"),
                    )
                    .child(
                        div()
                            .w(px(45.78125))
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child("SSH"),
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
                    .child(div().child(device.title))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .size(px(26.0))
                                    .rounded_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        svg()
                                            .path("icons/settings-refresh.svg")
                                            .size(px(16.0))
                                            .text_color(theme.text_tertiary),
                                    ),
                            )
                            .child(
                                div()
                                    .w(px(44.0))
                                    .h(px(24.0))
                                    .rounded_full()
                                    .bg(theme.text)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .font_weight(gpui::FontWeight(500.0))
                                    .text_color(theme.surface)
                                    .child("添加"),
                            ),
                    ),
            )
            .child(
                div()
                    .mt(px(12.0))
                    .rounded(px(20.0))
                    .overflow_hidden()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.settings_panel)
                    .child(
                        div()
                            .h(px(44.0))
                            .px(px(16.0))
                            .relative()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .line_height(px(18.5625))
                                    .font_weight(gpui::FontWeight(500.0))
                                    .child(device.rows[0].title),
                            )
                            .child(self.reference_switch_control(
                                true,
                                (page.slug, 1, 0),
                                theme,
                                cx,
                            ))
                            .child(
                                div()
                                    .absolute()
                                    .bottom_0()
                                    .left(px(16.0))
                                    .right(px(16.0))
                                    .h(px(0.5))
                                    .bg(theme.border),
                            ),
                    )
                    .child(
                        div()
                            .h(px(60.5625))
                            .px(px(16.0))
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                svg()
                                    .path("icons/settings-remote-device.svg")
                                    .size(px(20.0))
                                    .flex_none()
                                    .text_color(theme.text),
                            )
                            .child(self.reference_label(
                                device.rows[1].title,
                                device.rows[1].subtitle,
                                theme,
                            ))
                            .child(
                                div()
                                    .w(px(96.0))
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
                                    .child("撤销访问权限"),
                            ),
                    ),
            )
            .child(
                div()
                    .mt(px(49.5))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child(other.title),
            )
            .child(
                div()
                    .mt(px(15.5))
                    .rounded(px(20.0))
                    .overflow_hidden()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.settings_panel)
                    .child(
                        div()
                            .h(px(60.5625))
                            .px(px(16.0))
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                svg()
                                    .path("icons/settings-appearance.svg")
                                    .size(px(20.0))
                                    .flex_none()
                                    .text_color(theme.text),
                            )
                            .child(self.reference_label(
                                other.rows[0].title,
                                other.rows[0].subtitle,
                                theme,
                            ))
                            .child(self.reference_switch_control(
                                false,
                                (page.slug, 2, 0),
                                theme,
                                cx,
                            )),
                    ),
            )
            .into_any_element()
    }
}
