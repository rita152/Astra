//! Import settings presentation.

use gpui::{Context, IntoElement, div, prelude::*, px, svg};

use super::SettingsView;
use crate::{
    settings::PageSpec,
    theme::{Theme, ThemeMode},
};

impl SettingsView {
    pub(super) fn import_source_row(
        &self,
        label: &'static str,
        icon: &'static str,
        orange: bool,
        last: bool,
        theme: Theme,
    ) -> gpui::AnyElement {
        let orange_fill = match self.mode {
            ThemeMode::Light => gpui::rgba(0xd25f28ff),
            ThemeMode::Dark => gpui::rgba(0xea733aff),
        };
        let orange_text = match self.mode {
            ThemeMode::Light => gpui::rgba(0xffffffff),
            ThemeMode::Dark => gpui::rgba(0x0d0d0dff),
        };
        div()
            .h(px(64.0))
            .flex_none()
            .px(px(16.0))
            .relative()
            .flex()
            .items_center()
            .justify_between()
            .when(!last, |row| {
                row.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left(px(16.0))
                        .right(px(16.0))
                        .h(px(0.5))
                        .bg(theme.border),
                )
            })
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .size(px(40.0))
                            .flex_none()
                            .rounded(px(15.0))
                            .when(orange, |mark| mark.bg(orange_fill))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(svg().path(icon).size(px(40.0)).text_color(if orange {
                                orange_text
                            } else {
                                theme.text
                            })),
                    )
                    .child(
                        div()
                            .relative()
                            .left(px(1.0))
                            .text_size(px(13.0))
                            .line_height(px(18.5625))
                            .font_weight(gpui::FontWeight(500.0))
                            .child(label),
                    ),
            )
            .child(self.reference_button("导入", 46.0, None, false, theme))
            .into_any_element()
    }
    pub(super) fn import_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let sync = div()
            .relative()
            .top(px(1.0))
            .w_full()
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
                    .justify_between()
                    .gap(px(24.0))
                    .child(self.reference_label(
                        "保持导入同步",
                        "自动同步已连接来源中的新增和更新内容",
                        theme,
                    ))
                    .child(self.reference_switch_control(false, (page.slug, 0, 0), theme, cx)),
            );
        let sources = div()
            .relative()
            .top(px(1.0))
            .w_full()
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel)
            .child(self.import_source_row(
                "Claude Code",
                "icons/settings-import-code.svg",
                true,
                false,
                theme,
            ))
            .child(self.import_source_row(
                "Claude Cowork",
                "icons/settings-import-cowork.svg",
                true,
                false,
                theme,
            ))
            .child(self.import_source_row(
                "Cursor",
                "icons/settings-import-cursor-mark.svg",
                false,
                true,
                theme,
            ));
        let status_green = match self.mode {
            ThemeMode::Light => gpui::rgba(0x00a240ff),
            ThemeMode::Dark => gpui::rgba(0x40c977ff),
        };
        let history = div()
            .relative()
            .top(px(1.0))
            .w_full()
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel)
            .child(
                div()
                    .h(px(64.0))
                    .flex_none()
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(24.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .size(px(40.0))
                                    .flex_none()
                                    .rounded(px(15.0))
                                    .bg(theme.settings_button)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        svg()
                                            .path("icons/settings-import.svg")
                                            .size(px(20.0))
                                            .text_color(theme.settings_description),
                                    ),
                            )
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
                                            .line_height(px(18.5625))
                                            .font_weight(gpui::FontWeight(500.0))
                                            .child("导入"),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .text_size(px(12.0))
                                            .line_height(px(16.0))
                                            .text_color(theme.settings_description)
                                            .child("2026年7月12日 14:08")
                                            .child("已导入 1 项"),
                                    ),
                            ),
                    )
                    .child(
                        svg()
                            .path("icons/settings-chevron-up.svg")
                            .size(px(20.0))
                            .text_color(theme.text_tertiary),
                    ),
            )
            .child(
                div()
                    .h(px(40.0))
                    .flex_none()
                    .px(px(16.0))
                    .relative()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left(px(16.0))
                            .right(px(16.0))
                            .h(px(0.5))
                            .bg(theme.border),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(18.5625))
                            .child("MCP 服务器"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .font_weight(gpui::FontWeight(500.0))
                                    .child(div().size(px(8.0)).rounded_full().bg(status_green))
                                    .child("已导入 1 项"),
                            )
                            .child(
                                svg()
                                    .path("icons/settings-chevron-next.svg")
                                    .size(px(20.0))
                                    .text_color(theme.text_tertiary),
                            ),
                    ),
            );

        div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .pt(px(66.0))
            .pb(px(80.0))
            .font_family(".SystemUIFont")
            .child(
                div()
                    .relative()
                    .left(px(1.0))
                    .top(px(-2.0))
                    .text_size(px(24.0))
                    .line_height(px(28.8))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .child(page.label),
            )
            .child(
                div()
                    .mt(px(6.0))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .text_color(theme.settings_description)
                    .child(page.intro),
            )
            .child(
                div()
                    .mt(px(41.5))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child("开启自动同步"),
            )
            .child(div().mt(px(15.5)).child(sync))
            .child(
                div()
                    .mt(px(40.0))
                    .text_size(px(16.0))
                    .line_height(px(24.875))
                    .font_weight(gpui::FontWeight(500.0))
                    .child("从其他 AI 应用导入"),
            )
            .child(
                div()
                    .mt(px(2.0))
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .text_color(theme.settings_description)
                    .child("检测到可添加到 ChatGPT 的配置"),
            )
            .child(div().mt(px(12.0)).child(sources))
            .child(
                div()
                    .mt(px(49.5))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child("导入历史"),
            )
            .child(div().mt(px(15.5)).child(history))
            .into_any_element()
    }
}
