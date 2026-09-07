//! Appshots settings presentation.

use gpui::{Context, IntoElement, div, prelude::*, px, svg};

use super::SettingsView;
use crate::{settings::PageSpec, theme::Theme};

impl SettingsView {
    pub(super) fn appshots_illustration(&self, theme: Theme) -> impl IntoElement {
        div()
            .w(px(374.0))
            .h(px(454.09375))
            .rounded(px(20.0))
            .overflow_hidden()
            .relative()
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel)
            .child(
                div()
                    .absolute()
                    .left(px(0.0))
                    .right(px(0.0))
                    .top(px(1.0))
                    .bottom(px(1.0))
                    .rounded(px(19.0))
                    .overflow_hidden()
                    .child(
                        gpui::img("icons/settings-appshots-preview.svg")
                            .w(px(372.0))
                            .h(px(452.0)),
                    ),
            )
    }
    pub(super) fn appshots_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let section = &page.sections[0];
        let select_control = |label: &'static str, width: f32| {
            div()
                .w(px(width))
                .h(px(28.0))
                .px(px(12.0))
                .rounded(px(12.5))
                .border_1()
                .border_color(theme.border)
                .bg(theme.settings_panel)
                .flex()
                .items_center()
                .justify_between()
                .gap(px(4.0))
                .text_size(px(14.0))
                .line_height(px(18.0))
                .child(label)
                .child(
                    svg()
                        .path("icons/chevron-down.svg")
                        .size(px(12.0))
                        .text_color(theme.text_tertiary),
                )
        };
        let control_row = |title: &'static str,
                           subtitle: &'static str,
                           height: f32,
                           last: bool,
                           control: gpui::AnyElement| {
            div()
                .h(px(height))
                .px(px(16.0))
                .relative()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(20.0))
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
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .line_height(px(18.5714))
                                .font_weight(gpui::FontWeight(500.0))
                                .child(title),
                        )
                        .when(!subtitle.is_empty(), |node| {
                            node.child(
                                div()
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .text_color(theme.text_tertiary)
                                    .child(subtitle),
                            )
                        }),
                )
                .child(control)
        };
        let controls = div()
            .w(px(374.0))
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel)
            .child(control_row(
                "快捷键",
                "同时按下两个 ⌘ 键",
                60.5625,
                false,
                select_control("⌘ + ⌘", 85.671875).into_any_element(),
            ))
            .child(control_row(
                "Appshot 发送目标",
                "选择使用快捷键时将 appshots 发送到哪里",
                60.5625,
                false,
                select_control("自动", 72.0).into_any_element(),
            ))
            .child(control_row(
                "播放音效",
                "",
                44.0,
                true,
                self.switch_control(true, (page.slug, 0, 2), theme, cx)
                    .into_any_element(),
            ));
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
                    .h(px(64.0))
                    .px(px(20.0))
                    .rounded(px(20.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .flex()
                    .items_center()
                    .gap(px(16.0))
                    .child(
                        div()
                            .size(px(32.0))
                            .relative()
                            .flex_none()
                            .child(
                                div()
                                    .absolute()
                                    .left(px(4.0))
                                    .top(px(4.0))
                                    .size(px(24.0))
                                    .rounded(px(6.0))
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(gpui::white()),
                            )
                            .child(
                                svg()
                                    .path("icons/settings-appshots.svg")
                                    .size(px(32.0))
                                    .text_color(gpui::rgba(0x149bf3ff)),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .left(px(9.0))
                                    .top(px(9.0))
                                    .flex()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .size(px(3.0))
                                            .rounded_full()
                                            .bg(gpui::rgba(0xff5d55ff)),
                                    )
                                    .child(
                                        div()
                                            .size(px(3.0))
                                            .rounded_full()
                                            .bg(gpui::rgba(0xffbd2eff)),
                                    )
                                    .child(
                                        div()
                                            .size(px(3.0))
                                            .rounded_full()
                                            .bg(gpui::rgba(0x28c840ff)),
                                    ),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .left(px(9.0))
                                    .top(px(17.0))
                                    .w(px(14.0))
                                    .h(px(2.0))
                                    .bg(gpui::rgba(0x8f8f8fff)),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .left(px(9.0))
                                    .top(px(21.0))
                                    .w(px(11.0))
                                    .h(px(2.0))
                                    .bg(gpui::rgba(0xb1b1b1ff)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .font_family(".SystemUIFont")
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .line_height(px(21.0))
                                    .font_weight(gpui::FontWeight(500.0))
                                    .child(section.title),
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .line_height(px(16.25))
                                    .text_color(theme.text_tertiary)
                                    .child(section.subtitle),
                            ),
                    ),
            )
            .child(
                div()
                    .mt(px(20.0))
                    .flex()
                    .gap(px(20.0))
                    .items_start()
                    .child(controls)
                    .child(self.appshots_illustration(theme)),
            )
            .into_any_element()
    }
}
