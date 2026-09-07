//! Appearance settings presentation.

use gpui::{Context, IntoElement, WindowAppearance, div, prelude::*, px, svg};

use super::{ChangeTheme, SettingsView};
use crate::{
    settings::PageSpec,
    theme::{Theme, ThemeMode},
};

impl SettingsView {
    pub(super) fn appearance_preview(
        &self,
        mode: usize,
        selected: bool,
        theme: Theme,
    ) -> impl IntoElement {
        let (shell, asset) = match mode {
            1 => (gpui::rgba(0xf3f3f3ff), "icons/settings-theme-light.svg"),
            2 => (gpui::rgba(0x5d5d5dff), "icons/settings-theme-dark.svg"),
            _ => (gpui::rgba(0x9f9f9fff), "icons/settings-theme-system.svg"),
        };
        let mut preview = div()
            .w_full()
            .aspect_ratio(17.0 / 12.0)
            .rounded(px(12.5))
            .overflow_hidden()
            .when(selected, |node| node.border_2().border_color(theme.text))
            .when(!selected, |node| node.border_1().border_color(theme.border))
            .bg(shell)
            .relative();

        if mode == 0 {
            preview = preview.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .child(
                        div()
                            .h_full()
                            .flex_1()
                            .rounded_l(px(12.5))
                            .bg(gpui::rgba(0x9f9f9fff)),
                    )
                    .child(
                        div()
                            .h_full()
                            .flex_1()
                            .rounded_r(px(12.5))
                            .bg(gpui::rgba(0x5d5d5dff)),
                    ),
            );
        }
        preview.child(gpui::img(asset).size_full().rounded(px(12.5)))
    }
    pub(super) fn appearance_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let chevron = || {
            svg()
                .path("icons/chevron-down.svg")
                .size(px(12.0))
                .text_color(theme.text_tertiary)
        };
        let selector = |label: &'static str, width: f32, muted: bool| {
            let control_bg = if theme.surface == gpui::rgba(0x181818ff) {
                if width > 170.0 {
                    gpui::rgba(0x000000ff)
                } else if width > 80.0 {
                    gpui::rgba(0x262626ff)
                } else {
                    gpui::rgba(0x222222ff)
                }
            } else if !(80.0..=170.0).contains(&width) {
                gpui::rgba(0xf9f9f9ff)
            } else {
                gpui::rgba(0xf7f7f7ff)
            };
            div()
                .w(px(width))
                .h(px(28.0))
                .px(px(12.0))
                .rounded(px(12.5))
                .border_1()
                .border_color(theme.border)
                .bg(control_bg)
                .flex()
                .items_center()
                .justify_between()
                .gap(px(4.0))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(if muted {
                    theme.text_tertiary
                } else {
                    theme.text
                })
                .child(label)
                .child(chevron())
        };
        let color_field = |label: &'static str, fill: gpui::Rgba, ink: gpui::Rgba| {
            div()
                .w(px(136.0))
                .h(px(28.0))
                .px(px(9.0))
                .rounded(px(12.5))
                .bg(fill)
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(ink)
                .child(div().size(px(14.0)).rounded_full().border_1().border_color(
                    if label == "#FFFFFF" {
                        gpui::rgba(0x1a1c1f22)
                    } else {
                        gpui::rgba(0xffffff33)
                    },
                ))
                .child(label)
        };
        let theme_row =
            |title: &'static str, height: f32, last: bool, control: gpui::AnyElement| {
                div()
                    .h(px(height))
                    .px(px(16.0))
                    .relative()
                    .flex()
                    .items_center()
                    .justify_between()
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
                            .text_size(px(13.0))
                            .line_height(px(18.5714))
                            .font_weight(gpui::FontWeight(500.0))
                            .child(title),
                    )
                    .child(control)
            };
        let preference_row = |title: &'static str,
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
                        .child(
                            div()
                                .text_size(px(12.0))
                                .line_height(px(16.0))
                                .text_color(theme.text_tertiary)
                                .child(subtitle),
                        ),
                )
                .child(control)
        };

        let labels = ["系统", "浅色", "深色"];
        let mut previews = div().w_full().mt(px(18.5)).flex().gap(px(12.0));
        for (index, label) in labels.iter().enumerate() {
            let selected = self.appearance_theme == index;
            previews = previews.child(
                div()
                    .id(("appearance-theme", index))
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(13.0))
                    .line_height(px(18.5714))
                    .text_color(if selected {
                        theme.text
                    } else {
                        theme.text_secondary
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let mode = match index {
                            1 => ThemeMode::Light,
                            2 => ThemeMode::Dark,
                            _ => match window.appearance() {
                                WindowAppearance::Light | WindowAppearance::VibrantLight => {
                                    ThemeMode::Light
                                }
                                WindowAppearance::Dark | WindowAppearance::VibrantDark => {
                                    ThemeMode::Dark
                                }
                            },
                        };
                        this.mode = mode;
                        this.appearance_theme = index;
                        cx.emit(ChangeTheme(mode));
                        cx.notify();
                    }))
                    .child(self.appearance_preview(index, selected, theme))
                    .child(*label),
            );
        }

        let top_controls = div()
            .w(px(348.0))
            .h(px(28.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .w(px(46.0))
                    .h(px(28.0))
                    .rounded(px(8.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .text_color(theme.text_tertiary)
                    .child("导入"),
            )
            .child(
                div()
                    .w(px(74.0))
                    .h(px(28.0))
                    .rounded(px(12.5))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .text_color(theme.text_tertiary)
                    .child("复制主题"),
            )
            .child(
                div()
                    .size(px(28.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(gpui::rgba(0xffffff22))
                    .bg(gpui::rgba(0x181818ff))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.0))
                    .line_height(px(12.0))
                    .font_weight(gpui::FontWeight(600.0))
                    .text_color(gpui::rgba(0x339cffff))
                    .child("Aa"),
            )
            .child(selector("Codex", 176.0, false));
        let font_controls = || {
            div()
                .w(px(168.0))
                .flex()
                .gap(px(8.0))
                .child(selector("系统默认", 92.0, false))
                .child(selector("常规", 68.0, true))
        };
        let contrast = div()
            .w(px(192.0))
            .h(px(20.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(
                div()
                    .w(px(146.0))
                    .h(px(20.0))
                    .relative()
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .top(px(9.0))
                            .h(px(2.0))
                            .rounded_full()
                            .bg(theme.accent),
                    )
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top(px(9.0))
                            .w(px(88.0))
                            .h(px(2.0))
                            .rounded_full()
                            .bg(theme.text),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(78.0))
                            .top_0()
                            .size(px(20.0))
                            .rounded_full()
                            .bg(theme.text),
                    ),
            )
            .child(
                div()
                    .w(px(36.0))
                    .text_size(px(13.0))
                    .line_height(px(18.5714))
                    .flex()
                    .justify_end()
                    .child("60"),
            );

        let card = div()
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel)
            .child(theme_row(
                "深色主题",
                52.5625,
                false,
                top_controls.into_any_element(),
            ))
            .child(theme_row(
                "强调色",
                44.0,
                false,
                color_field("#339CFF", theme.settings_accent, gpui::rgba(0xffffffff))
                    .into_any_element(),
            ))
            .child(theme_row(
                "背景",
                44.0,
                false,
                color_field("#181818", gpui::rgba(0x181818ff), gpui::rgba(0xffffffff))
                    .into_any_element(),
            ))
            .child(theme_row(
                "前景",
                44.0,
                false,
                color_field("#FFFFFF", gpui::rgba(0xffffffff), gpui::rgba(0x181818ff))
                    .into_any_element(),
            ))
            .child(theme_row(
                "UI 字体",
                44.0,
                false,
                font_controls().into_any_element(),
            ))
            .child(theme_row(
                "代码字体",
                44.0,
                false,
                font_controls().into_any_element(),
            ))
            .child(theme_row(
                "半透明侧边栏",
                36.0,
                false,
                self.switch_control(true, (page.slug, 0, 7), theme, cx)
                    .into_any_element(),
            ))
            .child(theme_row(
                "对比度",
                53.4375,
                true,
                contrast.into_any_element(),
            ));

        let dock_icons = div()
            .w(px(104.0))
            .h(px(48.0))
            .flex()
            .gap(px(8.0))
            .child(
                div()
                    .size(px(48.0))
                    .rounded(px(15.0))
                    .border_1()
                    .border_color(theme.text)
                    .bg(theme.sidebar_hover)
                    .relative()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .size(px(27.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.surface)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                svg()
                                    .path("icons/settings-appshots.svg")
                                    .size(px(17.0))
                                    .text_color(theme.text),
                            ),
                    ),
            )
            .child(
                div()
                    .size(px(48.0))
                    .rounded(px(15.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .size(px(27.0))
                            .rounded(px(8.0))
                            .bg(gpui::rgba(0x181818ff))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight(600.0))
                            .text_color(gpui::white())
                            .child("C"),
                    ),
            );
        let reduced_motion = div().w(px(136.0)).h(px(24.0)).flex().gap(px(2.0));
        let mut reduced_motion = reduced_motion;
        for (index, label) in ["系统", "开启", "关闭"].iter().enumerate() {
            reduced_motion = reduced_motion.child(
                div()
                    .w(px(44.0))
                    .h(px(24.0))
                    .rounded_full()
                    .border_1()
                    .border_color(if index == 0 {
                        theme.border
                    } else {
                        gpui::rgba(0x00000000)
                    })
                    .when(index == 0, |node| node.bg(theme.sidebar_hover))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .text_color(if index == 0 {
                        theme.text
                    } else {
                        theme.text_tertiary
                    })
                    .child(*label),
            );
        }
        let number_control = |value: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(13.0))
                .line_height(px(18.5714))
                .child(
                    div()
                        .w(px(64.0))
                        .h(px(28.0))
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.settings_panel)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(value),
                )
                .child("px")
        };
        let diff_controls = div()
            .w(px(82.0))
            .h(px(24.0))
            .flex()
            .gap(px(2.0))
            .child(
                div()
                    .w(px(44.0))
                    .h(px(24.0))
                    .rounded_full()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.sidebar_hover)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .child("颜色"),
            )
            .child(
                div()
                    .w(px(36.0))
                    .h(px(24.0))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .text_color(theme.text_tertiary)
                    .child("+/-"),
            );
        let preferences = div()
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel)
            .child(preference_row(
                "使用指针光标",
                "悬停交互元素时切换为指针光标",
                60.5625,
                false,
                self.switch_control(false, (page.slug, 1, 0), theme, cx)
                    .into_any_element(),
            ))
            .child(preference_row(
                "Dock 图标",
                "选择应用在 Dock 中使用的图标",
                72.0,
                false,
                dock_icons.into_any_element(),
            ))
            .child(preference_row(
                "减少动态效果",
                "减少动画效果或匹配系统设置",
                60.5625,
                false,
                reduced_motion.into_any_element(),
            ))
            .child(preference_row(
                "UI 字号",
                "调整 ChatGPT 界面使用的基准字号",
                60.5625,
                false,
                number_control("14").into_any_element(),
            ))
            .child(preference_row(
                "代码字体大小",
                "调整聊天和差异视图中代码使用的基础字号",
                60.5625,
                false,
                number_control("12").into_any_element(),
            ))
            .child(preference_row(
                "差异标记",
                "使用颜色或 +/− 标记显示更改",
                60.5625,
                false,
                diff_controls.into_any_element(),
            ))
            .child(preference_row(
                "字体平滑",
                "使用 macOS 原生字体抗锯齿",
                60.5625,
                true,
                self.switch_control(true, (page.slug, 1, 6), theme, cx)
                    .into_any_element(),
            ));

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
                    .text_size(px(24.0))
                    .line_height(px(28.8))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .child(page.label),
            )
            .child(
                div()
                    .mt(px(41.5))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child("主题"),
            )
            .child(previews)
            .child(
                div()
                    .mt(px(15.0))
                    .h(px(2.0))
                    .rounded_full()
                    .border_1()
                    .border_color(theme.border),
            )
            .child(div().mt(px(16.0)).child(card))
            .child(
                div()
                    .mt(px(49.5))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child("偏好设置"),
            )
            .child(div().mt(px(15.5)).child(preferences))
            .into_any_element()
    }
}
