use std::collections::HashMap;

use gpui::{
    Context, EventEmitter, IntoElement, Render, ScrollHandle, Window, WindowAppearance, div, point,
    prelude::*, px, svg,
};

use crate::theme::{Theme, ThemeMode};

use super::{ControlSpec, PageKind, PageSpec, RowSpec, SectionSpec, page, pages};

pub struct CloseSettings;
pub struct ChangeTheme(pub ThemeMode);

pub struct SettingsView {
    mode: ThemeMode,
    selected: &'static str,
    nav_scroll: ScrollHandle,
    content_scroll: ScrollHandle,
    switch_overrides: HashMap<(&'static str, usize, usize), bool>,
    appearance_theme: usize,
}

impl EventEmitter<CloseSettings> for SettingsView {}
impl EventEmitter<ChangeTheme> for SettingsView {}

impl SettingsView {
    pub fn new(mode: ThemeMode) -> Self {
        Self {
            mode,
            selected: "general-settings",
            nav_scroll: ScrollHandle::new(),
            content_scroll: ScrollHandle::new(),
            switch_overrides: HashMap::new(),
            appearance_theme: if mode == ThemeMode::Dark { 2 } else { 1 },
        }
    }

    pub fn select(&mut self, slug: &'static str, cx: &mut Context<Self>) {
        self.selected = slug;
        self.content_scroll.set_offset(point(px(0.0), px(0.0)));
        cx.notify();
    }

    fn nav_icon(slug: &'static str, theme: Theme) -> impl IntoElement {
        svg()
            .path(format!("icons/settings-{slug}.svg"))
            .size(px(16.0))
            .relative()
            .top(px(1.0))
            .text_color(theme.text)
    }

    fn sidebar_edge_shade(theme: Theme) -> gpui::AnyElement {
        let is_dark = theme.surface == gpui::rgba(0x181818ff);
        let alphas = [0, 0, 1, 1, 1, 1, 1, 1, 2, 2, 2, 3, 3, 3, 5, 5, 7, 7, 9];
        let mut shade = div()
            .absolute()
            .right_0()
            .top_0()
            .w(px(20.0))
            .h_full()
            .flex();
        for alpha in alphas {
            shade = shade.child(div().w(px(1.0)).h_full().flex_none().bg(gpui::rgba(alpha)));
        }
        shade
            .child(div().w(px(1.0)).h_full().flex_none().bg(if is_dark {
                gpui::rgba(0x282828ff)
            } else {
                gpui::rgba(0xe0e0e0ff)
            }))
            .into_any_element()
    }

    fn nav_row(
        &self,
        item: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = self.selected == item.slug;
        let slug = item.slug;
        let nav_text = if theme.surface == gpui::rgba(0x181818ff) {
            gpui::rgba(0xb4b4b4ff)
        } else {
            gpui::rgba(0x363636ff)
        };
        div()
            .id(slug)
            .h(px(29.0))
            .flex_none()
            .px(px(8.0))
            .rounded(px(12.5))
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(14.0))
            .line_height(px(21.0))
            .text_color(nav_text)
            .when(selected, |row| row.bg(theme.sidebar_hover))
            .cursor_pointer()
            .when(!cfg!(feature = "screenshot"), |row| {
                row.hover(move |style| style.bg(theme.sidebar_hover))
            })
            .on_click(cx.listener(move |this, _, _, cx| this.select(slug, cx)))
            .child(Self::nav_icon(slug, theme))
            .child(item.label)
    }

    fn nav_group(
        &self,
        title: &'static str,
        slugs: &'static [&'static str],
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut group = div().flex_none().flex().flex_col().gap(px(1.0)).child(
            div()
                .h(px(29.0))
                .px(px(8.0))
                .relative()
                .top(px(1.0))
                .flex()
                .items_center()
                .text_size(px(14.0))
                .line_height(px(21.0))
                .font_weight(gpui::FontWeight(500.0))
                .text_color(theme.text_tertiary)
                .child(title),
        );
        for slug in slugs {
            if let Some(item) = page(slug) {
                group = group.child(self.nav_row(item, theme, cx));
            } else if *slug == "account" {
                let nav_text = if theme.surface == gpui::rgba(0x181818ff) {
                    gpui::rgba(0xb4b4b4ff)
                } else {
                    gpui::rgba(0x363636ff)
                };
                group = group.child(
                    div()
                        .id("settings-account")
                        .h(px(29.0))
                        .flex_none()
                        .px(px(8.0))
                        .rounded(px(12.5))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .text_size(px(14.0))
                        .line_height(px(21.0))
                        .text_color(nav_text)
                        .when(!cfg!(feature = "screenshot"), |row| {
                            row.hover(move |style| style.bg(theme.sidebar_hover))
                        })
                        .child(
                            svg()
                                .path("icons/settings-account.svg")
                                .size(px(16.0))
                                .text_color(theme.text),
                        )
                        .child(div().flex_1().child("账户"))
                        .child(
                            svg()
                                .path("icons/settings-external.svg")
                                .size(px(12.0))
                                .text_color(theme.text_tertiary),
                        ),
                );
            }
        }
        group
    }

    fn switch_control(
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

    fn small_button(&self, label: &'static str, danger: bool, theme: Theme) -> impl IntoElement {
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

    fn control(
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

    fn row(
        &self,
        slug: &'static str,
        section_index: usize,
        row_index: usize,
        row: &'static RowSpec,
        last: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
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
                                } else if slug == "voice" {
                                    theme.text_tertiary
                                } else if slug == "usage"
                                    && theme.surface != gpui::rgba(0x181818ff)
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

    fn section(
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
                slug,
                section_index,
                row_index,
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
                                    .text_color(if matches!(slug, "general-settings" | "voice") {
                                        theme.text_tertiary
                                    } else if slug == "usage"
                                        && theme.surface != gpui::rgba(0x181818ff)
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

    fn standard_content(
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
                            .font_weight(gpui::FontWeight(300.0))
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

    fn profile_content(&self, theme: Theme, viewport_width: f32) -> gpui::AnyElement {
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
        let normal_weight = if is_dark {
            gpui::FontWeight(300.0)
        } else {
            gpui::FontWeight(350.0)
        };
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

    fn keyboard_content(
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

    fn pets_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let first = &page.sections[0];
        let pet_assets = [
            "icons/pet-codex.svg",
            "icons/pet-dewey.svg",
            "icons/pet-fireball.svg",
            "icons/pet-hoots.svg",
            "icons/pet-rocky.svg",
            "icons/pet-seedy.svg",
            "icons/pet-stacky.svg",
            "icons/pet-bsod.svg",
            "icons/pet-null-signal.svg",
        ];
        let mut card = div()
            .w_full()
            .rounded(px(16.0))
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel);
        for (index, row) in first.rows.iter().enumerate().skip(1) {
            card =
                card.child(
                    div()
                        .h(px(88.0))
                        .px(px(16.0))
                        .relative()
                        .flex()
                        .items_center()
                        .gap(px(24.0))
                        .when(index + 1 != first.rows.len(), |node| {
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
                                .items_center()
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .size(px(64.0))
                                        .flex_none()
                                        .overflow_hidden()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(
                                            div().w(px(48.0)).h(px(52.0)).flex_none().child(
                                                gpui::img(pet_assets[index - 1]).size_full(),
                                            ),
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
                                                .font_weight(gpui::FontWeight(500.0))
                                                .child(row.title),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .line_height(px(16.0))
                                                .text_color(theme.text_tertiary)
                                                .relative()
                                                .top(px(-1.0))
                                                .child(row.subtitle),
                                        ),
                                ),
                        )
                        .child(div().relative().top(px(-1.0)).child(self.control(
                            row.control,
                            (page.slug, 0, index),
                            theme,
                            cx,
                        ))),
                );
        }
        let mut content = div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .pt(px(64.0))
            .pb(px(80.0))
            .flex()
            .flex_col()
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
                    .mt(px(23.0))
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
                                    .font_weight(gpui::FontWeight(500.0))
                                    .child(first.title),
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(theme.text_tertiary)
                                    .child(first.subtitle),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .size(px(26.0))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(theme.text_tertiary)
                                    .child(
                                        svg()
                                            .path("icons/settings-refresh.svg")
                                            .size(px(16.0))
                                            .text_color(theme.text_tertiary),
                                    ),
                            )
                            .child(self.small_button("创建", false, theme))
                            .child(self.small_button("收起宠物", false, theme)),
                    ),
            )
            .child(div().mt(px(11.0)).child(card));
        for (index, section) in page.sections.iter().enumerate().skip(1) {
            content = content.child(
                div()
                    .mt(px(30.0))
                    .child(self.section(page.slug, index, section, theme, cx)),
            );
        }
        content.into_any_element()
    }

    fn appearance_preview(&self, mode: usize, selected: bool, theme: Theme) -> impl IntoElement {
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

    fn appearance_content(
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
            } else if width > 170.0 || width < 80.0 {
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

    fn appshots_illustration(&self, theme: Theme) -> impl IntoElement {
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

    fn appshots_content(
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
                    .font_weight(gpui::FontWeight(300.0))
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

    fn computer_use_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let rows = page.sections[0].rows;
        let extension_badge = || {
            div()
                .absolute()
                .right(px(-1.0))
                .bottom(px(-1.0))
                .size(px(16.0))
                .rounded(px(4.5))
                .border_1()
                .border_color(gpui::rgba(0xa6a8aaff))
                .bg(gpui::white())
                .child(
                    div()
                        .absolute()
                        .left(px(-2.0))
                        .top(px(5.0))
                        .size(px(5.0))
                        .rounded_full()
                        .border_1()
                        .border_color(gpui::rgba(0xa6a8aaff))
                        .bg(gpui::white()),
                )
        };
        let mut apps = div()
            .mt(px(16.5))
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel);
        for index in 0..4 {
            let row = &rows[index];
            let icon = match index {
                0 => div()
                    .size(px(40.0))
                    .rounded(px(8.0))
                    .overflow_hidden()
                    .relative()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.settings_panel)
                    .child(
                        div()
                            .absolute()
                            .left(px(2.0))
                            .top(px(2.0))
                            .size(px(16.0))
                            .rounded(px(1.5))
                            .border_1()
                            .border_color(gpui::rgba(0xa9a9a9ff))
                            .bg(gpui::rgba(0xf3f3f3ff))
                            .child(
                                div()
                                    .absolute()
                                    .left(px(3.0))
                                    .top(px(3.0))
                                    .size(px(3.0))
                                    .rounded_full()
                                    .bg(gpui::rgba(0x4e89d8ff)),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .left(px(2.0))
                                    .top(px(9.0))
                                    .w(px(11.0))
                                    .h(px(4.0))
                                    .rounded(px(1.0))
                                    .bg(gpui::rgba(0x6baa45ff)),
                            ),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(2.0))
                            .top(px(17.0))
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .font_weight(gpui::FontWeight::NORMAL)
                            .child("Compu"),
                    )
                    .into_any_element(),
                1 => div()
                    .size(px(40.0))
                    .rounded(px(8.0))
                    .overflow_hidden()
                    .relative()
                    .border_1()
                    .border_color(theme.border)
                    .child(gpui::img("icons/settings-computer-chrome.svg").size_full())
                    .child(extension_badge())
                    .into_any_element(),
                2 => div()
                    .size(px(40.0))
                    .rounded(px(8.0))
                    .overflow_hidden()
                    .border_1()
                    .border_color(theme.border)
                    .child(gpui::img("icons/settings-computer-edge.svg").size_full())
                    .into_any_element(),
                _ => div()
                    .size(px(40.0))
                    .rounded(px(8.0))
                    .overflow_hidden()
                    .relative()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.settings_panel)
                    .child(
                        div()
                            .absolute()
                            .left(px(14.0))
                            .top(px(5.0))
                            .w(px(20.0))
                            .h(px(30.0))
                            .rounded(px(2.0))
                            .border_1()
                            .border_color(gpui::rgba(0x7eb878ff))
                            .bg(gpui::white())
                            .child(
                                div()
                                    .absolute()
                                    .left(px(7.0))
                                    .top(px(1.0))
                                    .bottom(px(1.0))
                                    .w(px(1.0))
                                    .bg(gpui::rgba(0xb7d7b3ff)),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .left(px(1.0))
                                    .right(px(1.0))
                                    .top(px(10.0))
                                    .h(px(1.0))
                                    .bg(gpui::rgba(0xb7d7b3ff)),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .left(px(1.0))
                                    .right(px(1.0))
                                    .top(px(19.0))
                                    .h(px(1.0))
                                    .bg(gpui::rgba(0xb7d7b3ff)),
                            ),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(5.0))
                            .top(px(10.0))
                            .size(px(22.0))
                            .rounded(px(2.0))
                            .bg(gpui::rgba(0x1f7244ff))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight(600.0))
                            .text_color(gpui::white())
                            .child("X"),
                    )
                    .child(extension_badge())
                    .into_any_element(),
            };
            let right = match index {
                1 => div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(self.small_button("管理", false, theme))
                    .child(self.switch_control(true, (page.slug, 0, index), theme, cx))
                    .into_any_element(),
                2 => div()
                    .text_color(theme.text_tertiary)
                    .child("›")
                    .into_any_element(),
                _ => self.control(row.control, (page.slug, 0, index), theme, cx),
            };
            apps = apps.child(
                div()
                    .h(px(64.0))
                    .px(px(16.0))
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .when(index < 3, |node| {
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
                    .child(icon)
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
                    .child(right),
            );
        }
        let lock = &rows[4];
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
                    .font_weight(gpui::FontWeight(300.0))
                    .child(page.label),
            )
            .child(
                div()
                    .mt(px(6.0))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .when(self.mode == ThemeMode::Dark, |intro| {
                        intro.font_weight(gpui::FontWeight(300.0))
                    })
                    .text_color(theme.settings_description)
                    .child(page.intro),
            )
            .child(
                div()
                    .mt(px(41.5))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child("控制"),
            )
            .child(apps)
            .child(
                div()
                    .mt(px(6.0))
                    .h(px(66.0))
                    .px(px(16.0))
                    .rounded(px(20.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.settings_panel)
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .size(px(40.0))
                            .relative()
                            .child(
                                div()
                                    .absolute()
                                    .left(px(7.0))
                                    .top(px(8.0))
                                    .w(px(23.0))
                                    .h(px(16.0))
                                    .rounded(px(1.5))
                                    .border_1()
                                    .border_color(gpui::rgba(0x33414aff))
                                    .bg(gpui::rgba(0x68a9dcff)),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .left(px(5.0))
                                    .top(px(24.0))
                                    .w(px(27.0))
                                    .h(px(2.0))
                                    .rounded(px(1.0))
                                    .bg(gpui::rgba(0x485159ff)),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .left(px(25.0))
                                    .top(px(20.0))
                                    .w(px(10.0))
                                    .h(px(10.0))
                                    .rounded(px(1.5))
                                    .bg(gpui::rgba(0xc7a36bff))
                                    .child(
                                        div()
                                            .absolute()
                                            .left(px(2.0))
                                            .top(px(-5.0))
                                            .w(px(6.0))
                                            .h(px(7.0))
                                            .rounded_t(px(4.0))
                                            .border_1()
                                            .border_color(gpui::rgba(0x8f7654ff)),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .line_height(px(18.5714))
                                    .font_weight(gpui::FontWeight(500.0))
                                    .child(lock.title),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .text_color(theme.settings_description)
                                    .child(lock.subtitle),
                            ),
                    )
                    .child(self.switch_control(false, (page.slug, 0, 4), theme, cx)),
            )
            .child(
                div()
                    .mt(px(49.5))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child("始终允许的应用"),
            )
            .child(
                div()
                    .mt(px(15.5))
                    .h(px(66.0))
                    .rounded(px(20.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.settings_panel)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.0))
                    .line_height(px(18.5714))
                    .text_color(theme.settings_description)
                    .child("暂无"),
            )
            .into_any_element()
    }

    fn personalization_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let instruction = &page.sections[0].rows[0];
        let memory = &page.sections[1];
        let personality = &page.sections[2].rows[0];
        let link_color = match self.mode {
            ThemeMode::Light => gpui::rgba(0x339cffff),
            ThemeMode::Dark => gpui::rgba(0x99ceffff),
        };
        let textarea_border = match self.mode {
            ThemeMode::Light => gpui::rgba(0x1a1c1f1f),
            ThemeMode::Dark => gpui::rgba(0xffffff1f),
        };
        let danger_text = match self.mode {
            ThemeMode::Light => gpui::rgba(0xe02e2aff),
            ThemeMode::Dark => gpui::rgba(0xff6764ff),
        };
        let danger_fill = match self.mode {
            ThemeMode::Light => gpui::rgba(0xe02e2a1a),
            ThemeMode::Dark => gpui::rgba(0xff67641a),
        };
        let warning_fill = match self.mode {
            ThemeMode::Light => gpui::rgba(0xfffcfbff),
            ThemeMode::Dark => gpui::rgba(0x1c1613ff),
        };

        let mut memory_card = div()
            .w_full()
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_panel);
        for (index, row) in memory.rows.iter().enumerate() {
            let right = match index {
                0 => self.reference_switch_control(false, (page.slug, 1, 0), theme, cx),
                1 => self.reference_switch_control(true, (page.slug, 1, 1), theme, cx),
                _ => div()
                    .w(px(44.0))
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
                    .child("删除")
                    .into_any_element(),
            };
            memory_card = memory_card.child(
                div()
                    .h(px(60.5625))
                    .flex_none()
                    .px(px(16.0))
                    .relative()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(24.0))
                    .when(index + 1 != memory.rows.len(), |item| {
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
                                    .child(instruction.title),
                            )
                            .child(
                                div()
                                    .mt(px(2.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .text_size(px(13.0))
                                    .line_height(px(18.0))
                                    .text_color(theme.settings_description)
                                    .child(
                                        "向 ChatGPT 提供适用于此主机上所有聊天的额外说明和上下文。",
                                    )
                                    .child(div().text_color(link_color).child("了解更多")),
                            ),
                    )
                    .child(
                        div()
                            .opacity(0.4)
                            .child(self.reference_button("保存", 46.0, None, false, theme)),
                    ),
            )
            .child(
                div()
                    .mt(px(12.0))
                    .h(px(147.9375))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(textarea_border)
                    .px(px(10.0))
                    .py(px(8.0))
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .text_color(theme.text_tertiary)
                    .child("添加自定义指令…"),
            )
            .child(
                div()
                    .mt(px(39.0))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(16.0))
                            .line_height(px(24.875))
                            .font_weight(gpui::FontWeight(500.0))
                            .child(memory.title),
                    )
                    .child(
                        div()
                            .mt(px(2.0))
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .text_color(theme.settings_description)
                            .child("设置在此电脑上如何收集、保留和整合本地记忆。")
                            .child(div().text_color(link_color).child("了解更多")),
                    ),
            )
            .child(div().mt(px(12.0)).child(memory_card))
            .child(
                div()
                    .mt(px(39.0))
                    .h(px(37.125))
                    .px(px(12.0))
                    .rounded(px(20.0))
                    .bg(warning_fill)
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        svg()
                            .path("icons/settings-warning.svg")
                            .size(px(20.0))
                            .flex_none()
                            .text_color(theme.warning),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .text_color(theme.text)
                            .child(page.sections[2].subtitle),
                    ),
            )
            .child(div().mt(px(6.0)).child(self.agent_card(
                vec![self.agent_row(
                    personality.title,
                    personality.subtitle,
                    self.agent_select("亲和", 72.0, theme),
                    true,
                    theme,
                )],
                theme,
            )))
            .into_any_element()
    }

    fn chronicle_content(&self, page: &'static PageSpec, theme: Theme) -> gpui::AnyElement {
        let rows = page.sections[0].rows;
        const CHRONICLE_ART_TILES: &[(u16, u16, u16, u16, u32)] = &[
            (0, 0, 3, 3, 0xe9e9e9ff),
            (3, 0, 3, 3, 0x3d68e6ff),
            (6, 0, 3, 3, 0x416ae8ff),
            (9, 0, 3, 3, 0x436ce7ff),
            (12, 0, 3, 3, 0x426de7ff),
            (15, 0, 3, 3, 0x426ee9ff),
            (18, 0, 3, 3, 0x4270e9ff),
            (21, 0, 3, 3, 0x4673e9ff),
            (24, 0, 3, 3, 0x4472e9ff),
            (27, 0, 3, 3, 0x4271e8ff),
            (30, 0, 3, 3, 0x3c6ee7ff),
            (33, 0, 3, 3, 0x3a6de8ff),
            (36, 0, 3, 3, 0x366ce6ff),
            (39, 0, 3, 3, 0x3269e4ff),
            (42, 0, 3, 3, 0x2e68e3ff),
            (45, 0, 3, 3, 0x2e68e3ff),
            (48, 0, 3, 3, 0x3069e3ff),
            (51, 0, 3, 3, 0x3069e2ff),
            (54, 0, 3, 3, 0x316be3ff),
            (57, 0, 3, 3, 0x326be4ff),
            (60, 0, 3, 3, 0x356ce4ff),
            (63, 0, 3, 3, 0x346ce4ff),
            (66, 0, 3, 3, 0x2f6ae3ff),
            (69, 0, 3, 3, 0x2c67e1ff),
            (72, 0, 3, 3, 0x2b66e0ff),
            (75, 0, 3, 3, 0x2c67e0ff),
            (78, 0, 3, 3, 0x2b66e0ff),
            (81, 0, 3, 3, 0x2b65dfff),
            (84, 0, 3, 3, 0x2a63dfff),
            (87, 0, 3, 3, 0x2a63deff),
            (90, 0, 3, 3, 0x2962deff),
            (93, 0, 3, 3, 0x2a63dfff),
            (96, 0, 3, 3, 0x2962deff),
            (99, 0, 3, 3, 0x2962dfff),
            (102, 0, 3, 3, 0x2962e0ff),
            (105, 0, 3, 3, 0x2a64e1ff),
            (108, 0, 3, 3, 0x2b65e2ff),
            (111, 0, 3, 3, 0x2d66e2ff),
            (114, 0, 3, 3, 0x2e65e3ff),
            (117, 0, 3, 3, 0x2f65e4ff),
            (120, 0, 3, 3, 0x2c63e2ff),
            (123, 0, 3, 3, 0x2a62e1ff),
            (126, 0, 3, 3, 0x2e63e2ff),
            (129, 0, 3, 3, 0x3264e4ff),
            (132, 0, 3, 3, 0x3865e6ff),
            (135, 0, 3, 3, 0x3f67e8ff),
            (138, 0, 3, 3, 0x4f6ae9ff),
            (141, 0, 3, 3, 0x697ff0ff),
            (144, 0, 3, 3, 0x6e82f6ff),
            (147, 0, 3, 3, 0x7185f6ff),
            (150, 0, 3, 3, 0x7587f6ff),
            (153, 0, 3, 3, 0x7687f7ff),
            (156, 0, 3, 3, 0x7687f7ff),
            (159, 0, 3, 3, 0x7684f7ff),
            (162, 0, 3, 3, 0x7683f7ff),
            (165, 0, 3, 3, 0x7580f6ff),
            (168, 0, 3, 3, 0x747df6ff),
            (171, 0, 3, 3, 0x737af4ff),
            (174, 0, 3, 3, 0x7178f4ff),
            (177, 0, 3, 3, 0x6e74f2ff),
            (180, 0, 3, 3, 0x6e73f1ff),
            (183, 0, 3, 3, 0x6b70efff),
            (186, 0, 3, 3, 0x6e70efff),
            (189, 0, 3, 3, 0x6e70f0ff),
            (192, 0, 3, 3, 0x6d71efff),
            (195, 0, 3, 3, 0x6e71f0ff),
            (198, 0, 3, 3, 0x6e71efff),
            (201, 0, 3, 3, 0x6e73f0ff),
            (204, 0, 3, 3, 0x7074eeff),
            (207, 0, 3, 3, 0x7074efff),
            (210, 0, 3, 3, 0x7074ecff),
            (213, 0, 3, 3, 0x6c70ebff),
            (216, 0, 3, 3, 0x686eeaff),
            (219, 0, 3, 3, 0x646ceaff),
            (222, 0, 3, 3, 0x626beaff),
            (225, 0, 3, 3, 0x626be9ff),
            (228, 0, 3, 3, 0x6069e8ff),
            (231, 0, 3, 3, 0x5e69e7ff),
            (234, 0, 3, 3, 0x5e69e7ff),
            (237, 0, 3, 3, 0x5b68e6ff),
            (240, 0, 3, 3, 0x5967e5ff),
            (243, 0, 3, 3, 0x5566e3ff),
            (246, 0, 3, 3, 0x5263e1ff),
            (249, 0, 3, 3, 0x5061dfff),
            (252, 0, 3, 3, 0x4f5fdeff),
            (255, 0, 3, 3, 0x4b5cdbff),
            (258, 0, 3, 3, 0x485ad9ff),
            (261, 0, 3, 3, 0x4758d7ff),
            (264, 0, 3, 3, 0x4656d6ff),
            (267, 0, 3, 3, 0x4656d5ff),
            (270, 0, 3, 3, 0x4454d2ff),
            (273, 0, 3, 3, 0x4252d1ff),
            (276, 0, 3, 3, 0x4252cfff),
            (279, 0, 3, 3, 0x4551cdff),
            (282, 0, 3, 3, 0x4651ceff),
            (285, 0, 3, 3, 0x4450ccff),
            (288, 0, 3, 3, 0x434fccff),
            (291, 0, 3, 3, 0x444ecdff),
            (294, 0, 3, 3, 0x434ccbff),
            (297, 0, 3, 3, 0x414ac8ff),
            (300, 0, 3, 3, 0x4049c5ff),
            (303, 0, 3, 3, 0x4047c3ff),
            (306, 0, 3, 3, 0x3e46c2ff),
            (309, 0, 3, 3, 0x3d44c0ff),
            (312, 0, 3, 3, 0x363ebcff),
            (315, 0, 3, 3, 0x313ab9ff),
            (318, 0, 3, 3, 0x3039b7ff),
            (321, 0, 3, 3, 0x2c36b5ff),
            (324, 0, 3, 3, 0x2934b3ff),
            (327, 0, 3, 3, 0x2330aeff),
            (330, 0, 3, 3, 0x182dabff),
            (333, 0, 3, 3, 0x0c28a6ff),
            (336, 0, 3, 3, 0x0b25a3ff),
            (339, 0, 3, 3, 0x0a229fff),
            (342, 0, 3, 3, 0x081f9aff),
            (345, 0, 3, 3, 0x071c95ff),
            (348, 0, 3, 3, 0x071b90ff),
            (351, 0, 3, 3, 0x06198cff),
            (354, 0, 3, 3, 0x06188aff),
            (357, 0, 3, 3, 0x061888ff),
            (360, 0, 3, 3, 0x051788ff),
            (363, 0, 3, 3, 0x061889ff),
            (366, 0, 3, 3, 0x06188cff),
            (369, 0, 3, 3, 0x4250a7ff),
            (372, 0, 3, 3, 0xb4b9d6ff),
            (375, 0, 3, 3, 0xf4f4f4ff),
            (378, 0, 3, 3, 0xffffffff),
            (381, 0, 3, 3, 0xffffffff),
            (0, 3, 3, 3, 0x3e6ce8ff),
            (3, 3, 3, 3, 0x3d6be8ff),
            (6, 3, 3, 3, 0x3f6be7ff),
            (9, 3, 3, 3, 0x3f6de8ff),
            (12, 3, 3, 3, 0x3f6ce8ff),
            (15, 3, 3, 3, 0x3d6be7ff),
            (18, 3, 3, 3, 0x3c6be7ff),
            (21, 3, 3, 3, 0x3d6ce7ff),
            (24, 3, 3, 3, 0x3c6ee6ff),
            (27, 3, 3, 3, 0x3a6ee6ff),
            (30, 3, 3, 3, 0x366de6ff),
            (33, 3, 3, 3, 0x346ce5ff),
            (36, 3, 3, 3, 0x366de5ff),
            (39, 3, 3, 3, 0x336ae4ff),
            (42, 3, 3, 3, 0x316ae4ff),
            (45, 3, 3, 3, 0x2f68e2ff),
            (48, 3, 3, 3, 0x2e68e2ff),
            (51, 3, 3, 3, 0x2e68e2ff),
            (54, 3, 3, 3, 0x2f6ae3ff),
            (57, 3, 3, 3, 0x326ae4ff),
            (60, 3, 3, 3, 0x336ce4ff),
            (63, 3, 3, 3, 0x326be4ff),
            (66, 3, 3, 3, 0x316be3ff),
            (69, 3, 3, 3, 0x2c67e1ff),
            (72, 3, 3, 3, 0x2b66e0ff),
            (75, 3, 3, 3, 0x2c67e1ff),
            (78, 3, 3, 3, 0x2c67e1ff),
            (81, 3, 3, 3, 0x2b65e0ff),
            (84, 3, 3, 3, 0x2a64dfff),
            (87, 3, 3, 3, 0x2a63dfff),
            (90, 3, 3, 3, 0x2961dfff),
            (93, 3, 3, 3, 0x2961dfff),
            (96, 3, 3, 3, 0x2961dfff),
            (99, 3, 3, 3, 0x2a63e0ff),
            (102, 3, 3, 3, 0x2a64e0ff),
            (105, 3, 3, 3, 0x2a64e0ff),
            (108, 3, 3, 3, 0x2a64e1ff),
            (111, 3, 3, 3, 0x2c65e2ff),
            (114, 3, 3, 3, 0x2c65e2ff),
            (117, 3, 3, 3, 0x2e65e4ff),
            (120, 3, 3, 3, 0x3265e3ff),
            (123, 3, 3, 3, 0x3465e5ff),
            (126, 3, 3, 3, 0x3a66e7ff),
            (129, 3, 3, 3, 0x3c66e8ff),
            (132, 3, 3, 3, 0x3a65e8ff),
            (135, 3, 3, 3, 0x3f68e9ff),
            (138, 3, 3, 3, 0x506eefff),
            (141, 3, 3, 3, 0x687ef5ff),
            (144, 3, 3, 3, 0x6f83f6ff),
            (147, 3, 3, 3, 0x7085f6ff),
            (150, 3, 3, 3, 0x7386f6ff),
            (153, 3, 3, 3, 0x7586f7ff),
            (156, 3, 3, 3, 0x7384f6ff),
            (159, 3, 3, 3, 0x7482f6ff),
            (162, 3, 3, 3, 0x7582f7ff),
            (165, 3, 3, 3, 0x7580f7ff),
            (168, 3, 3, 3, 0x747ef5ff),
            (171, 3, 3, 3, 0x737bf5ff),
            (174, 3, 3, 3, 0x6e76f4ff),
            (177, 3, 3, 3, 0x6c72f1ff),
            (180, 3, 3, 3, 0x6a70f1ff),
            (183, 3, 3, 3, 0x676feeff),
            (186, 3, 3, 3, 0x686eefff),
            (189, 3, 3, 3, 0x696ff0ff),
            (192, 3, 3, 3, 0x6a6fefff),
            (195, 3, 3, 3, 0x686eeeff),
            (198, 3, 3, 3, 0x6a6fefff),
            (201, 3, 3, 3, 0x6b70eeff),
            (204, 3, 3, 3, 0x6c71edff),
            (207, 3, 3, 3, 0x6f72edff),
            (210, 3, 3, 3, 0x6d72edff),
            (213, 3, 3, 3, 0x6a6febff),
            (216, 3, 3, 3, 0x646ceaff),
            (219, 3, 3, 3, 0x606aeaff),
            (222, 3, 3, 3, 0x5e6ae9ff),
            (225, 3, 3, 3, 0x5d69e8ff),
            (228, 3, 3, 3, 0x5b67e7ff),
            (231, 3, 3, 3, 0x5765e5ff),
            (234, 3, 3, 3, 0x5664e4ff),
            (237, 3, 3, 3, 0x5465e3ff),
            (240, 3, 3, 3, 0x5464e2ff),
            (243, 3, 3, 3, 0x5263e1ff),
            (246, 3, 3, 3, 0x5162e0ff),
            (249, 3, 3, 3, 0x4f60deff),
            (252, 3, 3, 3, 0x4d5eddff),
            (255, 3, 3, 3, 0x4a5bdaff),
            (258, 3, 3, 3, 0x4659d8ff),
            (261, 3, 3, 3, 0x4556d6ff),
            (264, 3, 3, 3, 0x4455d5ff),
            (267, 3, 3, 3, 0x4454d4ff),
            (270, 3, 3, 3, 0x4152d2ff),
            (273, 3, 3, 3, 0x4151d0ff),
            (276, 3, 3, 3, 0x4151ceff),
            (279, 3, 3, 3, 0x4350cdff),
            (282, 3, 3, 3, 0x434fccff),
            (285, 3, 3, 3, 0x424ecbff),
            (288, 3, 3, 3, 0x434ecbff),
            (291, 3, 3, 3, 0x424ccbff),
            (294, 3, 3, 3, 0x3f4ac7ff),
            (297, 3, 3, 3, 0x3c47c4ff),
            (300, 3, 3, 3, 0x3c46c2ff),
            (303, 3, 3, 3, 0x3c45c1ff),
            (306, 3, 3, 3, 0x3c44c0ff),
            (309, 3, 3, 3, 0x3a41bfff),
            (312, 3, 3, 3, 0x353ebbff),
            (315, 3, 3, 3, 0x303ab9ff),
            (318, 3, 3, 3, 0x2d37b5ff),
            (321, 3, 3, 3, 0x2b35b3ff),
            (324, 3, 3, 3, 0x2632b0ff),
            (327, 3, 3, 3, 0x212facff),
            (330, 3, 3, 3, 0x102aa9ff),
            (333, 3, 3, 3, 0x0c27a4ff),
            (336, 3, 3, 3, 0x0b24a1ff),
            (339, 3, 3, 3, 0x09219dff),
            (342, 3, 3, 3, 0x081f99ff),
            (345, 3, 3, 3, 0x071c95ff),
            (348, 3, 3, 3, 0x071c92ff),
            (351, 3, 3, 3, 0x061a8dff),
            (354, 3, 3, 3, 0x06188bff),
            (357, 3, 3, 3, 0x061889ff),
            (360, 3, 3, 3, 0x061888ff),
            (363, 3, 3, 3, 0x061888ff),
            (366, 3, 3, 3, 0x06188aff),
            (369, 3, 3, 3, 0x06198bff),
            (372, 3, 3, 3, 0x06198dff),
            (375, 3, 3, 3, 0x061a8eff),
            (378, 3, 3, 3, 0xb4bad7ff),
            (381, 3, 3, 3, 0xffffffff),
            (0, 6, 3, 3, 0x3f6feaff),
            (3, 6, 3, 3, 0x3c6de9ff),
            (6, 6, 3, 3, 0x3d6ee9ff),
            (9, 6, 3, 3, 0x3f70e9ff),
            (12, 6, 3, 3, 0x3d6fe9ff),
            (15, 6, 3, 3, 0x3a6de8ff),
            (18, 6, 3, 3, 0x3769e7ff),
            (21, 6, 3, 3, 0x3469e7ff),
            (24, 6, 3, 3, 0x3069e5ff),
            (27, 6, 3, 3, 0x2e69e5ff),
            (30, 6, 3, 3, 0x3069e5ff),
            (33, 6, 3, 3, 0x336be5ff),
            (36, 6, 3, 3, 0x316ae4ff),
            (39, 6, 3, 3, 0x2e69e3ff),
            (42, 6, 3, 3, 0x2c68e3ff),
            (45, 6, 3, 3, 0x2c67e2ff),
            (48, 6, 3, 3, 0x2d69e4ff),
            (51, 6, 3, 3, 0x2e69e3ff),
            (54, 6, 3, 3, 0x2e68e3ff),
            (57, 6, 3, 3, 0x2e69e4ff),
            (60, 6, 3, 3, 0x2d68e3ff),
            (63, 6, 3, 3, 0x2c68e3ff),
            (66, 6, 3, 3, 0x2c68e3ff),
            (69, 6, 3, 3, 0x2c67e2ff),
            (72, 6, 3, 3, 0x2b66e1ff),
            (75, 6, 3, 3, 0x2b65e1ff),
            (78, 6, 3, 3, 0x2b65e1ff),
            (81, 6, 3, 3, 0x2b65e2ff),
            (84, 6, 3, 3, 0x2a64e1ff),
            (87, 6, 3, 3, 0x2a64e1ff),
            (90, 6, 3, 3, 0x2962e1ff),
            (93, 6, 3, 3, 0x2961e1ff),
            (96, 6, 3, 3, 0x2961e1ff),
            (99, 6, 3, 3, 0x2962e1ff),
            (102, 6, 3, 3, 0x2a63e0ff),
            (105, 6, 3, 3, 0x2b65e3ff),
            (108, 6, 3, 3, 0x2b66e2ff),
            (111, 6, 3, 3, 0x2c66e2ff),
            (114, 6, 3, 3, 0x2e65e4ff),
            (117, 6, 3, 3, 0x3567e7ff),
            (120, 6, 3, 3, 0x3a68e7ff),
            (123, 6, 3, 3, 0x3b67e9ff),
            (126, 6, 3, 3, 0x3d68e9ff),
            (129, 6, 3, 3, 0x4069eaff),
            (132, 6, 3, 3, 0x3f69ebff),
            (135, 6, 3, 3, 0x426aebff),
            (138, 6, 3, 3, 0x5370f0ff),
            (141, 6, 3, 3, 0x6980f6ff),
            (144, 6, 3, 3, 0x6e84f7ff),
            (147, 6, 3, 3, 0x6e84f7ff),
            (150, 6, 3, 3, 0x7286f6ff),
            (153, 6, 3, 3, 0x7385f6ff),
            (156, 6, 3, 3, 0x7283f7ff),
            (159, 6, 3, 3, 0x7281f7ff),
            (162, 6, 3, 3, 0x7380f7ff),
            (165, 6, 3, 3, 0x727ff7ff),
            (168, 6, 3, 3, 0x707cf6ff),
            (171, 6, 3, 3, 0x6e79f6ff),
            (174, 6, 3, 3, 0x6b76f6ff),
            (177, 6, 3, 3, 0x6871f3ff),
            (180, 6, 3, 3, 0x656ff0ff),
            (183, 6, 3, 3, 0x646defff),
            (186, 6, 3, 3, 0x636cefff),
            (189, 6, 3, 3, 0x646cefff),
            (192, 6, 3, 3, 0x646deeff),
            (195, 6, 3, 3, 0x646cedff),
            (198, 6, 3, 3, 0x656eefff),
            (201, 6, 3, 3, 0x666dedff),
            (204, 6, 3, 3, 0x656decff),
            (207, 6, 3, 3, 0x656cedff),
            (210, 6, 3, 3, 0x656decff),
            (213, 6, 3, 3, 0x646debff),
            (216, 6, 3, 3, 0x616bebff),
            (219, 6, 3, 3, 0x5c69e9ff),
            (222, 6, 3, 3, 0x5c69e9ff),
            (225, 6, 3, 3, 0x5b68e7ff),
            (228, 6, 3, 3, 0x5867e7ff),
            (231, 6, 3, 3, 0x5464e5ff),
            (234, 6, 3, 3, 0x5362e4ff),
            (237, 6, 3, 3, 0x5262e3ff),
            (240, 6, 3, 3, 0x4f61e1ff),
            (243, 6, 3, 3, 0x4e60e0ff),
            (246, 6, 3, 3, 0x4d5edfff),
            (249, 6, 3, 3, 0x4b5ddeff),
            (252, 6, 3, 3, 0x4a5bdcff),
            (255, 6, 3, 3, 0x495adbff),
            (258, 6, 3, 3, 0x4659d8ff),
            (261, 6, 3, 3, 0x4556d6ff),
            (264, 6, 3, 3, 0x4254d4ff),
            (267, 6, 3, 3, 0x4254d3ff),
            (270, 6, 3, 3, 0x4153d2ff),
            (273, 6, 3, 3, 0x4050cfff),
            (276, 6, 3, 3, 0x3f4fceff),
            (279, 6, 3, 3, 0x3f4fcbff),
            (282, 6, 3, 3, 0x3f4ccaff),
            (285, 6, 3, 3, 0x3e4cc9ff),
            (288, 6, 3, 3, 0x3e4bc9ff),
            (291, 6, 3, 3, 0x3d49c8ff),
            (294, 6, 3, 3, 0x3b47c6ff),
            (297, 6, 3, 3, 0x3945c3ff),
            (300, 6, 3, 3, 0x3844c1ff),
            (303, 6, 3, 3, 0x3742c0ff),
            (306, 6, 3, 3, 0x3641bdff),
            (309, 6, 3, 3, 0x3640bdff),
            (312, 6, 3, 3, 0x323cbaff),
            (315, 6, 3, 3, 0x2e3ab8ff),
            (318, 6, 3, 3, 0x2c37b5ff),
            (321, 6, 3, 3, 0x2935b2ff),
            (324, 6, 3, 3, 0x2330b0ff),
            (327, 6, 3, 3, 0x1f2daaff),
            (330, 6, 3, 3, 0x162aa8ff),
            (333, 6, 3, 3, 0x0c27a5ff),
            (336, 6, 3, 3, 0x0b24a2ff),
            (339, 6, 3, 3, 0x0a229dff),
            (342, 6, 3, 3, 0x081f9aff),
            (345, 6, 3, 3, 0x081d96ff),
            (348, 6, 3, 3, 0x071c94ff),
            (351, 6, 3, 3, 0x071c90ff),
            (354, 6, 3, 3, 0x071b8dff),
            (357, 6, 3, 3, 0x061989ff),
            (360, 6, 3, 3, 0x061988ff),
            (363, 6, 3, 3, 0x061989ff),
            (366, 6, 3, 3, 0x06198aff),
            (369, 6, 3, 3, 0x061a8aff),
            (372, 6, 3, 3, 0x061a8bff),
            (375, 6, 3, 3, 0x061a8eff),
            (378, 6, 3, 3, 0x071b8fff),
            (381, 6, 3, 3, 0xefefefff),
            (0, 9, 3, 3, 0x356ce9ff),
            (3, 9, 3, 3, 0x346be8ff),
            (6, 9, 3, 3, 0x376de8ff),
            (9, 9, 3, 3, 0x3b70e9ff),
            (12, 9, 3, 3, 0x386de9ff),
            (15, 9, 3, 3, 0x386ce9ff),
            (18, 9, 3, 3, 0x356ae7ff),
            (21, 9, 3, 3, 0x3068e6ff),
            (24, 9, 3, 3, 0x2b65e4ff),
            (27, 9, 3, 3, 0x2b65e4ff),
            (30, 9, 3, 3, 0x2b65e3ff),
            (33, 9, 3, 3, 0x2b65e1ff),
            (36, 9, 3, 3, 0x2b66e3ff),
            (39, 9, 3, 3, 0x2b66e2ff),
            (42, 9, 3, 3, 0x2b66e3ff),
            (45, 9, 3, 3, 0x2c67e4ff),
            (48, 9, 3, 3, 0x2c68e4ff),
            (51, 9, 3, 3, 0x2d69e4ff),
            (54, 9, 3, 3, 0x2d68e3ff),
            (57, 9, 3, 3, 0x2c67e3ff),
            (60, 9, 3, 3, 0x2b66e4ff),
            (63, 9, 3, 3, 0x2b66e4ff),
            (66, 9, 3, 3, 0x2b65e3ff),
            (69, 9, 3, 3, 0x2b65e3ff),
            (72, 9, 3, 3, 0x2a65e3ff),
            (75, 9, 3, 3, 0x2b65e2ff),
            (78, 9, 3, 3, 0x2d66e2ff),
            (81, 9, 3, 3, 0x2c66e3ff),
            (84, 9, 3, 3, 0x2b65e2ff),
            (87, 9, 3, 3, 0x2b65e2ff),
            (90, 9, 3, 3, 0x2a63e1ff),
            (93, 9, 3, 3, 0x2961e1ff),
            (96, 9, 3, 3, 0x2962e1ff),
            (99, 9, 3, 3, 0x2b65e3ff),
            (102, 9, 3, 3, 0x2b65e3ff),
            (105, 9, 3, 3, 0x2a64e2ff),
            (108, 9, 3, 3, 0x2b65e3ff),
            (111, 9, 3, 3, 0x3268e5ff),
            (114, 9, 3, 3, 0x396be9ff),
            (117, 9, 3, 3, 0x386ae9ff),
            (120, 9, 3, 3, 0x3d6be9ff),
            (123, 9, 3, 3, 0x426ceaff),
            (126, 9, 3, 3, 0x3e6aebff),
            (129, 9, 3, 3, 0x3d69ebff),
            (132, 9, 3, 3, 0x3e6aebff),
            (135, 9, 3, 3, 0x436bedff),
            (138, 9, 3, 3, 0x5372f1ff),
            (141, 9, 3, 3, 0x677ef5ff),
            (144, 9, 3, 3, 0x6a81f7ff),
            (147, 9, 3, 3, 0x6d84f7ff),
            (150, 9, 3, 3, 0x7185f7ff),
            (153, 9, 3, 3, 0x7486f7ff),
            (156, 9, 3, 3, 0x7384f7ff),
            (159, 9, 3, 3, 0x7282f7ff),
            (162, 9, 3, 3, 0x7281f7ff),
            (165, 9, 3, 3, 0x717ef6ff),
            (168, 9, 3, 3, 0x6d7af5ff),
            (171, 9, 3, 3, 0x6a76f6ff),
            (174, 9, 3, 3, 0x6873f5ff),
            (177, 9, 3, 3, 0x6670f2ff),
            (180, 9, 3, 3, 0x656ef1ff),
            (183, 9, 3, 3, 0x636cf0ff),
            (186, 9, 3, 3, 0x616bf0ff),
            (189, 9, 3, 3, 0x606aeeff),
            (192, 9, 3, 3, 0x6069edff),
            (195, 9, 3, 3, 0x6069edff),
            (198, 9, 3, 3, 0x5f68ecff),
            (201, 9, 3, 3, 0x5e69ebff),
            (204, 9, 3, 3, 0x5e69eaff),
            (207, 9, 3, 3, 0x5f69eaff),
            (210, 9, 3, 3, 0x5e68eaff),
            (213, 9, 3, 3, 0x5d67eaff),
            (216, 9, 3, 3, 0x5a67e9ff),
            (219, 9, 3, 3, 0x5a68e8ff),
            (222, 9, 3, 3, 0x5a68e9ff),
            (225, 9, 3, 3, 0x5a68e8ff),
            (228, 9, 3, 3, 0x5967e7ff),
            (231, 9, 3, 3, 0x5664e6ff),
            (234, 9, 3, 3, 0x5362e3ff),
            (237, 9, 3, 3, 0x505fe2ff),
            (240, 9, 3, 3, 0x4b5de0ff),
            (243, 9, 3, 3, 0x485cdfff),
            (246, 9, 3, 3, 0x485bdfff),
            (249, 9, 3, 3, 0x485bdeff),
            (252, 9, 3, 3, 0x485adcff),
            (255, 9, 3, 3, 0x4659dbff),
            (258, 9, 3, 3, 0x4659d9ff),
            (261, 9, 3, 3, 0x4658d8ff),
            (264, 9, 3, 3, 0x4456d5ff),
            (267, 9, 3, 3, 0x4254d3ff),
            (270, 9, 3, 3, 0x4052d2ff),
            (273, 9, 3, 3, 0x3e50d0ff),
            (276, 9, 3, 3, 0x3d4ecdff),
            (279, 9, 3, 3, 0x3a4ccbff),
            (282, 9, 3, 3, 0x3b4ac9ff),
            (285, 9, 3, 3, 0x3949c8ff),
            (288, 9, 3, 3, 0x3a48c7ff),
            (291, 9, 3, 3, 0x3a47c7ff),
            (294, 9, 3, 3, 0x3946c4ff),
            (297, 9, 3, 3, 0x3644c3ff),
            (300, 9, 3, 3, 0x3541c1ff),
            (303, 9, 3, 3, 0x323fbdff),
            (306, 9, 3, 3, 0x303dbbff),
            (309, 9, 3, 3, 0x2e3bbaff),
            (312, 9, 3, 3, 0x2d39b8ff),
            (315, 9, 3, 3, 0x2c39b6ff),
            (318, 9, 3, 3, 0x2c37b5ff),
            (321, 9, 3, 3, 0x2934b2ff),
            (324, 9, 3, 3, 0x2230aeff),
            (327, 9, 3, 3, 0x1c2cabff),
            (330, 9, 3, 3, 0x1429a8ff),
            (333, 9, 3, 3, 0x0d27a5ff),
            (336, 9, 3, 3, 0x0b24a2ff),
            (339, 9, 3, 3, 0x0a22a0ff),
            (342, 9, 3, 3, 0x09209aff),
            (345, 9, 3, 3, 0x081e96ff),
            (348, 9, 3, 3, 0x081d95ff),
            (351, 9, 3, 3, 0x071c92ff),
            (354, 9, 3, 3, 0x071b8fff),
            (357, 9, 3, 3, 0x071b8bff),
            (360, 9, 3, 3, 0x061a88ff),
            (363, 9, 3, 3, 0x061a88ff),
            (366, 9, 3, 3, 0x071b8aff),
            (369, 9, 3, 3, 0x071b8bff),
            (372, 9, 3, 3, 0x071b8cff),
            (375, 9, 3, 3, 0x071b8dff),
            (378, 9, 3, 3, 0x071c8eff),
            (381, 9, 3, 3, 0x4454aaff),
            (0, 12, 3, 3, 0x3369e7ff),
            (3, 12, 3, 3, 0x3169e7ff),
            (6, 12, 3, 3, 0x3069e8ff),
            (9, 12, 3, 3, 0x2d69e9ff),
            (12, 12, 3, 3, 0x2e69e7ff),
            (15, 12, 3, 3, 0x2d68e6ff),
            (18, 12, 3, 3, 0x2d68e5ff),
            (21, 12, 3, 3, 0x2c67e4ff),
            (24, 12, 3, 3, 0x2b66e3ff),
            (27, 12, 3, 3, 0x2b66e3ff),
            (30, 12, 3, 3, 0x2c67e2ff),
            (33, 12, 3, 3, 0x2b66e2ff),
            (36, 12, 3, 3, 0x2b65e3ff),
            (39, 12, 3, 3, 0x2b65e3ff),
            (42, 12, 3, 3, 0x2a64e2ff),
            (45, 12, 3, 3, 0x2b65e3ff),
            (48, 12, 3, 3, 0x2c67e4ff),
            (51, 12, 3, 3, 0x2c67e4ff),
            (54, 12, 3, 3, 0x2c67e4ff),
            (57, 12, 3, 3, 0x2c68e3ff),
            (60, 12, 3, 3, 0x2b66e3ff),
            (63, 12, 3, 3, 0x2c67e4ff),
            (66, 12, 3, 3, 0x2b66e3ff),
            (69, 12, 3, 3, 0x2b66e3ff),
            (72, 12, 3, 3, 0x2b65e3ff),
            (75, 12, 3, 3, 0x2b65e3ff),
            (78, 12, 3, 3, 0x2b66e3ff),
            (81, 12, 3, 3, 0x2c66e4ff),
            (84, 12, 3, 3, 0x2e66e4ff),
            (87, 12, 3, 3, 0x2e66e4ff),
            (90, 12, 3, 3, 0x2c65e4ff),
            (93, 12, 3, 3, 0x2e66e4ff),
            (96, 12, 3, 3, 0x2c65e3ff),
            (99, 12, 3, 3, 0x2c67e4ff),
            (102, 12, 3, 3, 0x2d68e5ff),
            (105, 12, 3, 3, 0x3069e6ff),
            (108, 12, 3, 3, 0x356ae8ff),
            (111, 12, 3, 3, 0x3d6ce9ff),
            (114, 12, 3, 3, 0x3f6eeaff),
            (117, 12, 3, 3, 0x3b6beaff),
            (120, 12, 3, 3, 0x3c6beaff),
            (123, 12, 3, 3, 0x3f6cebff),
            (126, 12, 3, 3, 0x416eecff),
            (129, 12, 3, 3, 0x446fedff),
            (132, 12, 3, 3, 0x446fefff),
            (135, 12, 3, 3, 0x4a6fefff),
            (138, 12, 3, 3, 0x5673f1ff),
            (141, 12, 3, 3, 0x667ef5ff),
            (144, 12, 3, 3, 0x6a81f7ff),
            (147, 12, 3, 3, 0x6c83f7ff),
            (150, 12, 3, 3, 0x6f84f7ff),
            (153, 12, 3, 3, 0x6f82f7ff),
            (156, 12, 3, 3, 0x6f81f7ff),
            (159, 12, 3, 3, 0x6f80f7ff),
            (162, 12, 3, 3, 0x6e7ff7ff),
            (165, 12, 3, 3, 0x6e7df7ff),
            (168, 12, 3, 3, 0x6a77f6ff),
            (171, 12, 3, 3, 0x6874f5ff),
            (174, 12, 3, 3, 0x6873f5ff),
            (177, 12, 3, 3, 0x6771f3ff),
            (180, 12, 3, 3, 0x656ef2ff),
            (183, 12, 3, 3, 0x636df0ff),
            (186, 12, 3, 3, 0x626cf0ff),
            (189, 12, 3, 3, 0x5f6aefff),
            (192, 12, 3, 3, 0x5d69eeff),
            (195, 12, 3, 3, 0x5c68edff),
            (198, 12, 3, 3, 0x5a66ecff),
            (201, 12, 3, 3, 0x5966eaff),
            (204, 12, 3, 3, 0x5865e9ff),
            (207, 12, 3, 3, 0x5865eaff),
            (210, 12, 3, 3, 0x5864e9ff),
            (213, 12, 3, 3, 0x5763e9ff),
            (216, 12, 3, 3, 0x5663e8ff),
            (219, 12, 3, 3, 0x5663e7ff),
            (222, 12, 3, 3, 0x5765e7ff),
            (225, 12, 3, 3, 0x5765e7ff),
            (228, 12, 3, 3, 0x5765e7ff),
            (231, 12, 3, 3, 0x5563e6ff),
            (234, 12, 3, 3, 0x5261e3ff),
            (237, 12, 3, 3, 0x4d5ee2ff),
            (240, 12, 3, 3, 0x485ce0ff),
            (243, 12, 3, 3, 0x475bdfff),
            (246, 12, 3, 3, 0x475addff),
            (249, 12, 3, 3, 0x495bdeff),
            (252, 12, 3, 3, 0x495cdcff),
            (255, 12, 3, 3, 0x475adbff),
            (258, 12, 3, 3, 0x4658d9ff),
            (261, 12, 3, 3, 0x4557d8ff),
            (264, 12, 3, 3, 0x4355d6ff),
            (267, 12, 3, 3, 0x4052d3ff),
            (270, 12, 3, 3, 0x3e4fd1ff),
            (273, 12, 3, 3, 0x3c4ecfff),
            (276, 12, 3, 3, 0x3c4dcdff),
            (279, 12, 3, 3, 0x394ccbff),
            (282, 12, 3, 3, 0x3748c9ff),
            (285, 12, 3, 3, 0x3646c8ff),
            (288, 12, 3, 3, 0x3645c7ff),
            (291, 12, 3, 3, 0x3645c5ff),
            (294, 12, 3, 3, 0x3544c2ff),
            (297, 12, 3, 3, 0x3441c0ff),
            (300, 12, 3, 3, 0x313ebeff),
            (303, 12, 3, 3, 0x2d3bbcff),
            (306, 12, 3, 3, 0x2b3abaff),
            (309, 12, 3, 3, 0x2a39b8ff),
            (312, 12, 3, 3, 0x2b38b7ff),
            (315, 12, 3, 3, 0x2a37b4ff),
            (318, 12, 3, 3, 0x2834b2ff),
            (321, 12, 3, 3, 0x2633b1ff),
            (324, 12, 3, 3, 0x2230aeff),
            (327, 12, 3, 3, 0x1a2cabff),
            (330, 12, 3, 3, 0x0f29a8ff),
            (333, 12, 3, 3, 0x0c26a5ff),
            (336, 12, 3, 3, 0x0b25a2ff),
            (339, 12, 3, 3, 0x0a23a0ff),
            (342, 12, 3, 3, 0x09219cff),
            (345, 12, 3, 3, 0x081e99ff),
            (348, 12, 3, 3, 0x081d97ff),
            (351, 12, 3, 3, 0x071c94ff),
            (354, 12, 3, 3, 0x071b8fff),
            (357, 12, 3, 3, 0x071b8dff),
            (360, 12, 3, 3, 0x061a8bff),
            (363, 12, 3, 3, 0x061a8aff),
            (366, 12, 3, 3, 0x071b8aff),
            (369, 12, 3, 3, 0x071b8bff),
            (372, 12, 3, 3, 0x071b8cff),
            (375, 12, 3, 3, 0x071b8cff),
            (378, 12, 3, 3, 0x071c8cff),
            (381, 12, 3, 3, 0x081d8dff),
            (0, 15, 3, 3, 0x3368e7ff),
            (3, 15, 3, 3, 0x2e66e7ff),
            (6, 15, 3, 3, 0x2c66e7ff),
            (9, 15, 3, 3, 0x2d66e6ff),
            (12, 15, 3, 3, 0x2d67e5ff),
            (15, 15, 3, 3, 0x2b66e4ff),
            (18, 15, 3, 3, 0x2a63e3ff),
            (21, 15, 3, 3, 0x2a64e3ff),
            (24, 15, 3, 3, 0x2a64e1ff),
            (27, 15, 3, 3, 0x2b65e2ff),
            (30, 15, 3, 3, 0x2c67e3ff),
            (33, 15, 3, 3, 0x2c68e3ff),
            (36, 15, 3, 3, 0x2c67e3ff),
            (39, 15, 3, 3, 0x2c67e3ff),
            (42, 15, 3, 3, 0x2b66e2ff),
            (45, 15, 3, 3, 0x2b66e2ff),
            (48, 15, 3, 3, 0x2c67e2ff),
            (51, 15, 3, 3, 0x2c67e3ff),
            (54, 15, 3, 3, 0x2c68e5ff),
            (57, 15, 3, 3, 0x2c68e4ff),
            (60, 15, 3, 3, 0x2c68e4ff),
            (63, 15, 3, 3, 0x2c68e4ff),
            (66, 15, 3, 3, 0x2d68e3ff),
            (69, 15, 3, 3, 0x2d69e4ff),
            (72, 15, 3, 3, 0x2c67e5ff),
            (75, 15, 3, 3, 0x2c67e4ff),
            (78, 15, 3, 3, 0x2c67e4ff),
            (81, 15, 3, 3, 0x2c67e4ff),
            (84, 15, 3, 3, 0x3167e5ff),
            (87, 15, 3, 3, 0x3368e6ff),
            (90, 15, 3, 3, 0x3569e7ff),
            (93, 15, 3, 3, 0x3569e8ff),
            (96, 15, 3, 3, 0x396be9ff),
            (99, 15, 3, 3, 0x3b6ce9ff),
            (102, 15, 3, 3, 0x3b6ce9ff),
            (105, 15, 3, 3, 0x3b6beaff),
            (108, 15, 3, 3, 0x406debff),
            (111, 15, 3, 3, 0x416debff),
            (114, 15, 3, 3, 0x436fecff),
            (117, 15, 3, 3, 0x456dedff),
            (120, 15, 3, 3, 0x466eedff),
            (123, 15, 3, 3, 0x426dedff),
            (126, 15, 3, 3, 0x446eeeff),
            (129, 15, 3, 3, 0x4771efff),
            (132, 15, 3, 3, 0x4a72f0ff),
            (135, 15, 3, 3, 0x4f72f1ff),
            (138, 15, 3, 3, 0x5875f2ff),
            (141, 15, 3, 3, 0x647df6ff),
            (144, 15, 3, 3, 0x687ff7ff),
            (147, 15, 3, 3, 0x697ff7ff),
            (150, 15, 3, 3, 0x6a7ff7ff),
            (153, 15, 3, 3, 0x6b7ef7ff),
            (156, 15, 3, 3, 0x6a7cf6ff),
            (159, 15, 3, 3, 0x697bf6ff),
            (162, 15, 3, 3, 0x6a7bf6ff),
            (165, 15, 3, 3, 0x6676f5ff),
            (168, 15, 3, 3, 0x6474f4ff),
            (171, 15, 3, 3, 0x6573f4ff),
            (174, 15, 3, 3, 0x6772f4ff),
            (177, 15, 3, 3, 0x6771f3ff),
            (180, 15, 3, 3, 0x656ff2ff),
            (183, 15, 3, 3, 0x636df0ff),
            (186, 15, 3, 3, 0x606aefff),
            (189, 15, 3, 3, 0x5b67eeff),
            (192, 15, 3, 3, 0x5a65ecff),
            (195, 15, 3, 3, 0x5864ebff),
            (198, 15, 3, 3, 0x5764ebff),
            (201, 15, 3, 3, 0x5763eaff),
            (204, 15, 3, 3, 0x5763eaff),
            (207, 15, 3, 3, 0x5663e9ff),
            (210, 15, 3, 3, 0x5663e9ff),
            (213, 15, 3, 3, 0x5562e8ff),
            (216, 15, 3, 3, 0x5562e8ff),
            (219, 15, 3, 3, 0x5462e7ff),
            (222, 15, 3, 3, 0x5562e7ff),
            (225, 15, 3, 3, 0x5361e6ff),
            (228, 15, 3, 3, 0x5060e4ff),
            (231, 15, 3, 3, 0x4f60e4ff),
            (234, 15, 3, 3, 0x4d5fe2ff),
            (237, 15, 3, 3, 0x495ce1ff),
            (240, 15, 3, 3, 0x485bdfff),
            (243, 15, 3, 3, 0x485adeff),
            (246, 15, 3, 3, 0x475addff),
            (249, 15, 3, 3, 0x485adcff),
            (252, 15, 3, 3, 0x4558dbff),
            (255, 15, 3, 3, 0x4256d8ff),
            (258, 15, 3, 3, 0x4255d7ff),
            (261, 15, 3, 3, 0x4254d6ff),
            (264, 15, 3, 3, 0x4053d6ff),
            (267, 15, 3, 3, 0x3e4fd2ff),
            (270, 15, 3, 3, 0x3b4ed0ff),
            (273, 15, 3, 3, 0x3a4dcdff),
            (276, 15, 3, 3, 0x394bcbff),
            (279, 15, 3, 3, 0x3649c9ff),
            (282, 15, 3, 3, 0x3446c8ff),
            (285, 15, 3, 3, 0x3243c6ff),
            (288, 15, 3, 3, 0x3242c4ff),
            (291, 15, 3, 3, 0x3241c2ff),
            (294, 15, 3, 3, 0x3141c1ff),
            (297, 15, 3, 3, 0x303ebfff),
            (300, 15, 3, 3, 0x2e3dbdff),
            (303, 15, 3, 3, 0x2b3abcff),
            (306, 15, 3, 3, 0x2839b9ff),
            (309, 15, 3, 3, 0x2938b8ff),
            (312, 15, 3, 3, 0x2937b5ff),
            (315, 15, 3, 3, 0x2835b2ff),
            (318, 15, 3, 3, 0x2632b0ff),
            (321, 15, 3, 3, 0x2431afff),
            (324, 15, 3, 3, 0x2330aeff),
            (327, 15, 3, 3, 0x1f2eabff),
            (330, 15, 3, 3, 0x182aa8ff),
            (333, 15, 3, 3, 0x1026a4ff),
            (336, 15, 3, 3, 0x0b25a3ff),
            (339, 15, 3, 3, 0x0a23a1ff),
            (342, 15, 3, 3, 0x0a229dff),
            (345, 15, 3, 3, 0x081f9aff),
            (348, 15, 3, 3, 0x081e99ff),
            (351, 15, 3, 3, 0x081d96ff),
            (354, 15, 3, 3, 0x071c93ff),
            (357, 15, 3, 3, 0x071b90ff),
            (360, 15, 3, 3, 0x071b8eff),
            (363, 15, 3, 3, 0x061a8bff),
            (366, 15, 3, 3, 0x061a8cff),
            (369, 15, 3, 3, 0x061a8cff),
            (372, 15, 3, 3, 0x061a8cff),
            (375, 15, 3, 3, 0x071b8aff),
            (378, 15, 3, 3, 0x071b8bff),
            (381, 15, 3, 3, 0x071c8cff),
            (0, 18, 3, 3, 0x3366e7ff),
            (3, 18, 3, 3, 0x2f66e6ff),
            (6, 18, 3, 3, 0x2c65e5ff),
            (9, 18, 3, 3, 0x2a64e4ff),
            (12, 18, 3, 3, 0x2a63e3ff),
            (15, 18, 3, 3, 0x2a63e3ff),
            (18, 18, 3, 3, 0x2962e3ff),
            (21, 18, 3, 3, 0x2a63e3ff),
            (24, 18, 3, 3, 0x2a63e2ff),
            (27, 18, 3, 3, 0x2a64e2ff),
            (30, 18, 3, 3, 0x2b65e1ff),
            (33, 18, 3, 3, 0x2b65e2ff),
            (36, 18, 3, 3, 0x2b66e2ff),
            (39, 18, 3, 3, 0x2b66e2ff),
            (42, 18, 3, 3, 0x2c67e3ff),
            (45, 18, 3, 3, 0x2c68e3ff),
            (48, 18, 3, 3, 0x2e69e4ff),
            (51, 18, 3, 3, 0x2d69e4ff),
            (54, 18, 3, 3, 0x2c68e5ff),
            (57, 18, 3, 3, 0x2d69e6ff),
            (60, 18, 3, 3, 0x2c68e5ff),
            (63, 18, 3, 3, 0x2d68e5ff),
            (66, 18, 3, 3, 0x2e69e5ff),
            (69, 18, 3, 3, 0x2e6ae6ff),
            (72, 18, 3, 3, 0x2e69e6ff),
            (75, 18, 3, 3, 0x2e69e5ff),
            (78, 18, 3, 3, 0x2d68e6ff),
            (81, 18, 3, 3, 0x3068e6ff),
            (84, 18, 3, 3, 0x356ae6ff),
            (87, 18, 3, 3, 0x376be6ff),
            (90, 18, 3, 3, 0x376be8ff),
            (93, 18, 3, 3, 0x396ce9ff),
            (96, 18, 3, 3, 0x3f70ecff),
            (99, 18, 3, 3, 0x4271ecff),
            (102, 18, 3, 3, 0x4471edff),
            (105, 18, 3, 3, 0x4771eeff),
            (108, 18, 3, 3, 0x4b71edff),
            (111, 18, 3, 3, 0x4e73eeff),
            (114, 18, 3, 3, 0x5277efff),
            (117, 18, 3, 3, 0x5679f1ff),
            (120, 18, 3, 3, 0x5b7bf2ff),
            (123, 18, 3, 3, 0x577cf3ff),
            (126, 18, 3, 3, 0x567cf4ff),
            (129, 18, 3, 3, 0x557af3ff),
            (132, 18, 3, 3, 0x5578f2ff),
            (135, 18, 3, 3, 0x5677f3ff),
            (138, 18, 3, 3, 0x5a77f4ff),
            (141, 18, 3, 3, 0x637cf6ff),
            (144, 18, 3, 3, 0x667df6ff),
            (147, 18, 3, 3, 0x667cf7ff),
            (150, 18, 3, 3, 0x677bf7ff),
            (153, 18, 3, 3, 0x697cf7ff),
            (156, 18, 3, 3, 0x687af6ff),
            (159, 18, 3, 3, 0x6578f6ff),
            (162, 18, 3, 3, 0x6476f5ff),
            (165, 18, 3, 3, 0x6372f4ff),
            (168, 18, 3, 3, 0x6170f3ff),
            (171, 18, 3, 3, 0x626ff3ff),
            (174, 18, 3, 3, 0x636ef3ff),
            (177, 18, 3, 3, 0x646ef3ff),
            (180, 18, 3, 3, 0x626cf2ff),
            (183, 18, 3, 3, 0x6069efff),
            (186, 18, 3, 3, 0x5f67eeff),
            (189, 18, 3, 3, 0x5a65edff),
            (192, 18, 3, 3, 0x5763ecff),
            (195, 18, 3, 3, 0x5562ebff),
            (198, 18, 3, 3, 0x5662eaff),
            (201, 18, 3, 3, 0x5562eaff),
            (204, 18, 3, 3, 0x5562eaff),
            (207, 18, 3, 3, 0x5562eaff),
            (210, 18, 3, 3, 0x5764e9ff),
            (213, 18, 3, 3, 0x5663e8ff),
            (216, 18, 3, 3, 0x5663e8ff),
            (219, 18, 3, 3, 0x5562e7ff),
            (222, 18, 3, 3, 0x5361e6ff),
            (225, 18, 3, 3, 0x4f5fe5ff),
            (228, 18, 3, 3, 0x4d5de4ff),
            (231, 18, 3, 3, 0x4c5de2ff),
            (234, 18, 3, 3, 0x4a5ce0ff),
            (237, 18, 3, 3, 0x475be0ff),
            (240, 18, 3, 3, 0x485adfff),
            (243, 18, 3, 3, 0x475adeff),
            (246, 18, 3, 3, 0x4458dcff),
            (249, 18, 3, 3, 0x4357dbff),
            (252, 18, 3, 3, 0x4255dbff),
            (255, 18, 3, 3, 0x4053d9ff),
            (258, 18, 3, 3, 0x3e52d7ff),
            (261, 18, 3, 3, 0x3d50d4ff),
            (264, 18, 3, 3, 0x3c4fd3ff),
            (267, 18, 3, 3, 0x3c4ed2ff),
            (270, 18, 3, 3, 0x3a4dd0ff),
            (273, 18, 3, 3, 0x384bceff),
            (276, 18, 3, 3, 0x3649cbff),
            (279, 18, 3, 3, 0x3547c9ff),
            (282, 18, 3, 3, 0x3344c6ff),
            (285, 18, 3, 3, 0x2f42c5ff),
            (288, 18, 3, 3, 0x2e41c2ff),
            (291, 18, 3, 3, 0x2e40c2ff),
            (294, 18, 3, 3, 0x2e3fc2ff),
            (297, 18, 3, 3, 0x2f3ebfff),
            (300, 18, 3, 3, 0x2c3cbdff),
            (303, 18, 3, 3, 0x2a3abcff),
            (306, 18, 3, 3, 0x2939baff),
            (309, 18, 3, 3, 0x2938b6ff),
            (312, 18, 3, 3, 0x2836b5ff),
            (315, 18, 3, 3, 0x2734b3ff),
            (318, 18, 3, 3, 0x2533b1ff),
            (321, 18, 3, 3, 0x2431afff),
            (324, 18, 3, 3, 0x2230adff),
            (327, 18, 3, 3, 0x1e2dabff),
            (330, 18, 3, 3, 0x1b2aa8ff),
            (333, 18, 3, 3, 0x1328a5ff),
            (336, 18, 3, 3, 0x0c25a3ff),
            (339, 18, 3, 3, 0x0a23a0ff),
            (342, 18, 3, 3, 0x09219dff),
            (345, 18, 3, 3, 0x081f9cff),
            (348, 18, 3, 3, 0x081e9bff),
            (351, 18, 3, 3, 0x081e99ff),
            (354, 18, 3, 3, 0x071c95ff),
            (357, 18, 3, 3, 0x071c95ff),
            (360, 18, 3, 3, 0x071b92ff),
            (363, 18, 3, 3, 0x071b91ff),
            (366, 18, 3, 3, 0x071b90ff),
            (369, 18, 3, 3, 0x071b8fff),
            (372, 18, 3, 3, 0x061a8eff),
            (375, 18, 3, 3, 0x071b8eff),
            (378, 18, 3, 3, 0x071b8eff),
            (381, 18, 3, 3, 0x081d90ff),
            (0, 21, 3, 3, 0x3366e5ff),
            (3, 21, 3, 3, 0x2d65e5ff),
            (6, 21, 3, 3, 0x2a64e4ff),
            (9, 21, 3, 3, 0x2b64e3ff),
            (12, 21, 3, 3, 0x2a63e2ff),
            (15, 21, 3, 3, 0x2a64e2ff),
            (18, 21, 3, 3, 0x2a63e3ff),
            (363, 21, 3, 3, 0x071c94ff),
            (366, 21, 3, 3, 0x071c93ff),
            (369, 21, 3, 3, 0x071c92ff),
            (372, 21, 3, 3, 0x071c91ff),
            (375, 21, 3, 3, 0x071c93ff),
            (378, 21, 3, 3, 0x081d94ff),
            (381, 21, 3, 3, 0x0a1f97ff),
            (0, 24, 3, 3, 0x2a62e3ff),
            (3, 24, 3, 3, 0x2961e2ff),
            (6, 24, 3, 3, 0x2b63e2ff),
            (9, 24, 3, 3, 0x2b64e3ff),
            (12, 24, 3, 3, 0x2a63e2ff),
            (15, 24, 3, 3, 0x2a63e2ff),
            (18, 24, 3, 3, 0x2962e2ff),
            (363, 24, 3, 3, 0x071c96ff),
            (366, 24, 3, 3, 0x071c94ff),
            (369, 24, 3, 3, 0x071c94ff),
            (372, 24, 3, 3, 0x081d94ff),
            (375, 24, 3, 3, 0x081e95ff),
            (378, 24, 3, 3, 0x081f97ff),
            (381, 24, 3, 3, 0x0d219cff),
            (0, 27, 3, 3, 0x2c61e1ff),
            (3, 27, 3, 3, 0x2b60e1ff),
            (6, 27, 3, 3, 0x2a61e2ff),
            (9, 27, 3, 3, 0x2961e2ff),
            (12, 27, 3, 3, 0x2961e2ff),
            (15, 27, 3, 3, 0x2961e1ff),
            (18, 27, 3, 3, 0x2961e2ff),
            (363, 27, 3, 3, 0x071b98ff),
            (366, 27, 3, 3, 0x071c98ff),
            (369, 27, 3, 3, 0x071c98ff),
            (372, 27, 3, 3, 0x081e98ff),
            (375, 27, 3, 3, 0x081f9aff),
            (378, 27, 3, 3, 0x09219eff),
            (381, 27, 3, 3, 0x0c24a4ff),
            (0, 30, 3, 3, 0x2e62e2ff),
            (3, 30, 3, 3, 0x2c61e2ff),
            (6, 30, 3, 3, 0x2c62e2ff),
            (9, 30, 3, 3, 0x2a62e3ff),
            (12, 30, 3, 3, 0x2962e2ff),
            (15, 30, 3, 3, 0x2963e2ff),
            (18, 30, 3, 3, 0x2a63e2ff),
            (363, 30, 3, 3, 0x071c98ff),
            (366, 30, 3, 3, 0x081d9aff),
            (369, 30, 3, 3, 0x081e9bff),
            (372, 30, 3, 3, 0x081f9dff),
            (375, 30, 3, 3, 0x0a22a0ff),
            (378, 30, 3, 3, 0x0b24a5ff),
            (381, 30, 3, 3, 0x0d29afff),
            (0, 33, 3, 3, 0x2b61e3ff),
            (3, 33, 3, 3, 0x2a62e2ff),
            (6, 33, 3, 3, 0x2c62e3ff),
            (9, 33, 3, 3, 0x2a62e3ff),
            (12, 33, 3, 3, 0x2962e2ff),
            (15, 33, 3, 3, 0x2962e2ff),
            (18, 33, 3, 3, 0x2962e2ff),
            (363, 33, 3, 3, 0x081e9dff),
            (366, 33, 3, 3, 0x081e9dff),
            (369, 33, 3, 3, 0x081f9fff),
            (372, 33, 3, 3, 0x0921a3ff),
            (375, 33, 3, 3, 0x0b24a7ff),
            (378, 33, 3, 3, 0x0c27aeff),
            (381, 33, 3, 3, 0x0e2bb4ff),
            (0, 36, 3, 3, 0x275fe1ff),
            (3, 36, 3, 3, 0x275ee1ff),
            (6, 36, 3, 3, 0x2860e1ff),
            (9, 36, 3, 3, 0x285fe2ff),
            (12, 36, 3, 3, 0x2961e2ff),
            (15, 36, 3, 3, 0x2961e1ff),
            (18, 36, 3, 3, 0x2a63e2ff),
            (363, 36, 3, 3, 0x081fa0ff),
            (366, 36, 3, 3, 0x081fa0ff),
            (369, 36, 3, 3, 0x0921a3ff),
            (372, 36, 3, 3, 0x0a23a8ff),
            (375, 36, 3, 3, 0x0b25aaff),
            (378, 36, 3, 3, 0x0c28b1ff),
            (381, 36, 3, 3, 0x0e2cb5ff),
            (0, 39, 3, 3, 0x265be0ff),
            (3, 39, 3, 3, 0x275de0ff),
            (6, 39, 3, 3, 0x275ee0ff),
            (9, 39, 3, 3, 0x265ce0ff),
            (12, 39, 3, 3, 0x275de0ff),
            (15, 39, 3, 3, 0x285fe1ff),
            (18, 39, 3, 3, 0x285fe1ff),
            (363, 39, 3, 3, 0x0920a3ff),
            (366, 39, 3, 3, 0x0920a5ff),
            (369, 39, 3, 3, 0x0a22a9ff),
            (372, 39, 3, 3, 0x0b24a9ff),
            (375, 39, 3, 3, 0x0b26acff),
            (378, 39, 3, 3, 0x0d29b0ff),
            (381, 39, 3, 3, 0x0e2bb6ff),
            (0, 42, 3, 3, 0x275ee0ff),
            (3, 42, 3, 3, 0x275ee0ff),
            (6, 42, 3, 3, 0x275ddfff),
            (9, 42, 3, 3, 0x275edfff),
            (12, 42, 3, 3, 0x285fe1ff),
            (15, 42, 3, 3, 0x2860e2ff),
            (18, 42, 3, 3, 0x285fe1ff),
            (363, 42, 3, 3, 0x0a22a6ff),
            (366, 42, 3, 3, 0x0a22a7ff),
            (369, 42, 3, 3, 0x0a23a9ff),
            (372, 42, 3, 3, 0x0b24aaff),
            (375, 42, 3, 3, 0x0c26aeff),
            (378, 42, 3, 3, 0x0d2ab1ff),
            (381, 42, 3, 3, 0x0e2db7ff),
            (0, 45, 3, 3, 0x2961e1ff),
            (3, 45, 3, 3, 0x285fe0ff),
            (6, 45, 3, 3, 0x275edfff),
            (9, 45, 3, 3, 0x275ddfff),
            (12, 45, 3, 3, 0x275ee0ff),
            (15, 45, 3, 3, 0x285fe1ff),
            (18, 45, 3, 3, 0x285fe1ff),
            (363, 45, 3, 3, 0x0a23a7ff),
            (366, 45, 3, 3, 0x0a23a8ff),
            (369, 45, 3, 3, 0x0b24acff),
            (372, 45, 3, 3, 0x0b26aeff),
            (375, 45, 3, 3, 0x0c28b1ff),
            (378, 45, 3, 3, 0x0e2bb5ff),
            (381, 45, 3, 3, 0x0f2eb8ff),
            (0, 48, 3, 3, 0x2a63e2ff),
            (3, 48, 3, 3, 0x2961e0ff),
            (6, 48, 3, 3, 0x2860e0ff),
            (9, 48, 3, 3, 0x285fe0ff),
            (12, 48, 3, 3, 0x285fe0ff),
            (15, 48, 3, 3, 0x285fe0ff),
            (18, 48, 3, 3, 0x275ee1ff),
            (363, 48, 3, 3, 0x0b24a9ff),
            (366, 48, 3, 3, 0x0b25abff),
            (369, 48, 3, 3, 0x0c27afff),
            (372, 48, 3, 3, 0x0d29b1ff),
            (375, 48, 3, 3, 0x0e2bb4ff),
            (378, 48, 3, 3, 0x0f2db7ff),
            (381, 48, 3, 3, 0x102fbaff),
            (0, 51, 3, 3, 0x2961e1ff),
            (3, 51, 3, 3, 0x2961e0ff),
            (6, 51, 3, 3, 0x2860dfff),
            (9, 51, 3, 3, 0x2860e0ff),
            (12, 51, 3, 3, 0x2860e0ff),
            (15, 51, 3, 3, 0x2961e1ff),
            (18, 51, 3, 3, 0x2961e3ff),
            (363, 51, 3, 3, 0x0b26acff),
            (366, 51, 3, 3, 0x0c28aeff),
            (369, 51, 3, 3, 0x0d29b1ff),
            (372, 51, 3, 3, 0x0e2cb5ff),
            (375, 51, 3, 3, 0x0f2db8ff),
            (378, 51, 3, 3, 0x102fbbff),
            (381, 51, 3, 3, 0x1232bfff),
            (0, 54, 3, 3, 0x2a64e4ff),
            (3, 54, 3, 3, 0x2a64e3ff),
            (6, 54, 3, 3, 0x2a63e3ff),
            (9, 54, 3, 3, 0x2a63e4ff),
            (12, 54, 3, 3, 0x2a64e5ff),
            (15, 54, 3, 3, 0x2a64e4ff),
            (18, 54, 3, 3, 0x2962e3ff),
            (363, 54, 3, 3, 0x0d2ab1ff),
            (366, 54, 3, 3, 0x0e2bb2ff),
            (369, 54, 3, 3, 0x0e2cb4ff),
            (372, 54, 3, 3, 0x0f2db8ff),
            (375, 54, 3, 3, 0x102fbcff),
            (378, 54, 3, 3, 0x1131bdff),
            (381, 54, 3, 3, 0x1634c0ff),
            (0, 57, 3, 3, 0x2a65e4ff),
            (3, 57, 3, 3, 0x2a63e4ff),
            (6, 57, 3, 3, 0x2a63e5ff),
            (9, 57, 3, 3, 0x2a63e4ff),
            (12, 57, 3, 3, 0x2b65e5ff),
            (15, 57, 3, 3, 0x2a64e4ff),
            (18, 57, 3, 3, 0x2b65e6ff),
            (363, 57, 3, 3, 0x0e2bb4ff),
            (366, 57, 3, 3, 0x0f2db6ff),
            (369, 57, 3, 3, 0x0f2eb9ff),
            (372, 57, 3, 3, 0x1030bbff),
            (375, 57, 3, 3, 0x1132beff),
            (378, 57, 3, 3, 0x1433bfff),
            (381, 57, 3, 3, 0x1835c2ff),
            (0, 60, 3, 3, 0x2a64e6ff),
            (3, 60, 3, 3, 0x2d66e6ff),
            (6, 60, 3, 3, 0x2b66e5ff),
            (9, 60, 3, 3, 0x2c66e6ff),
            (12, 60, 3, 3, 0x2e68e7ff),
            (15, 60, 3, 3, 0x2e68e8ff),
            (18, 60, 3, 3, 0x3069e8ff),
            (363, 60, 3, 3, 0x0f2eb9ff),
            (366, 60, 3, 3, 0x0f2ebaff),
            (369, 60, 3, 3, 0x0f2ebbff),
            (372, 60, 3, 3, 0x1131beff),
            (375, 60, 3, 3, 0x1434c0ff),
            (378, 60, 3, 3, 0x1c36c3ff),
            (381, 60, 3, 3, 0x2037c5ff),
            (0, 63, 3, 3, 0x396beaff),
            (3, 63, 3, 3, 0x3067e8ff),
            (6, 63, 3, 3, 0x3166e7ff),
            (9, 63, 3, 3, 0x3068e8ff),
            (12, 63, 3, 3, 0x2f67e7ff),
            (15, 63, 3, 3, 0x3068e8ff),
            (18, 63, 3, 3, 0x3268e7ff),
            (363, 63, 3, 3, 0x1132beff),
            (366, 63, 3, 3, 0x1131bdff),
            (369, 63, 3, 3, 0x1232beff),
            (372, 63, 3, 3, 0x1535c0ff),
            (375, 63, 3, 3, 0x1c37c4ff),
            (378, 63, 3, 3, 0x243ac7ff),
            (381, 63, 3, 3, 0x273dcbff),
            (0, 66, 3, 3, 0x426eecff),
            (3, 66, 3, 3, 0x3e6deaff),
            (6, 66, 3, 3, 0x3c6ceaff),
            (9, 66, 3, 3, 0x3e6deaff),
            (12, 66, 3, 3, 0x3d6debff),
            (15, 66, 3, 3, 0x3c6eeaff),
            (18, 66, 3, 3, 0x3d6feaff),
            (363, 66, 3, 3, 0x1636c1ff),
            (366, 66, 3, 3, 0x1435c0ff),
            (369, 66, 3, 3, 0x1836c2ff),
            (372, 66, 3, 3, 0x1e38c4ff),
            (375, 66, 3, 3, 0x223ac7ff),
            (378, 66, 3, 3, 0x283ecbff),
            (381, 66, 3, 3, 0x2b40ceff),
            (0, 69, 3, 3, 0x436eecff),
            (3, 69, 3, 3, 0x416eebff),
            (6, 69, 3, 3, 0x416debff),
            (9, 69, 3, 3, 0x406eebff),
            (12, 69, 3, 3, 0x436febff),
            (15, 69, 3, 3, 0x416eebff),
            (18, 69, 3, 3, 0x406febff),
            (363, 69, 3, 3, 0x1b38c4ff),
            (366, 69, 3, 3, 0x1b38c4ff),
            (369, 69, 3, 3, 0x1a39c6ff),
            (372, 69, 3, 3, 0x1f3bc7ff),
            (375, 69, 3, 3, 0x233ecbff),
            (378, 69, 3, 3, 0x2a40ceff),
            (381, 69, 3, 3, 0x2e42d0ff),
            (0, 72, 3, 3, 0x3d69e9ff),
            (3, 72, 3, 3, 0x3d69e8ff),
            (6, 72, 3, 3, 0x3d69e8ff),
            (9, 72, 3, 3, 0x3f6aeaff),
            (12, 72, 3, 3, 0x426ceaff),
            (15, 72, 3, 3, 0x436debff),
            (18, 72, 3, 3, 0x446decff),
            (363, 72, 3, 3, 0x203bc8ff),
            (366, 72, 3, 3, 0x233ccaff),
            (369, 72, 3, 3, 0x223cccff),
            (372, 72, 3, 3, 0x243dccff),
            (375, 72, 3, 3, 0x273fcdff),
            (378, 72, 3, 3, 0x2e41d0ff),
            (381, 72, 3, 3, 0x3445d3ff),
            (0, 75, 3, 3, 0x3e6ae8ff),
            (3, 75, 3, 3, 0x426ce9ff),
            (6, 75, 3, 3, 0x436deaff),
            (9, 75, 3, 3, 0x4870ebff),
            (12, 75, 3, 3, 0x4970ebff),
            (15, 75, 3, 3, 0x496fecff),
            (18, 75, 3, 3, 0x486fedff),
            (363, 75, 3, 3, 0x273ecbff),
            (366, 75, 3, 3, 0x293fcdff),
            (369, 75, 3, 3, 0x273fcdff),
            (372, 75, 3, 3, 0x2b41d0ff),
            (375, 75, 3, 3, 0x3043d1ff),
            (378, 75, 3, 3, 0x3544d4ff),
            (381, 75, 3, 3, 0x3a48d7ff),
            (0, 78, 3, 3, 0x496febff),
            (3, 78, 3, 3, 0x4e73ecff),
            (6, 78, 3, 3, 0x5176efff),
            (9, 78, 3, 3, 0x5276eeff),
            (12, 78, 3, 3, 0x5579eeff),
            (15, 78, 3, 3, 0x577bf0ff),
            (18, 78, 3, 3, 0x587af1ff),
            (363, 78, 3, 3, 0x2b40ceff),
            (366, 78, 3, 3, 0x2f42d1ff),
            (369, 78, 3, 3, 0x2d43d2ff),
            (372, 78, 3, 3, 0x2f44d3ff),
            (375, 78, 3, 3, 0x3546d5ff),
            (378, 78, 3, 3, 0x3a48d7ff),
            (381, 78, 3, 3, 0x3f4cdaff),
            (0, 81, 3, 3, 0x5073edff),
            (3, 81, 3, 3, 0x5374edff),
            (6, 81, 3, 3, 0x4c71eeff),
            (9, 81, 3, 3, 0x4f75efff),
            (12, 81, 3, 3, 0x5378f0ff),
            (15, 81, 3, 3, 0x567af1ff),
            (18, 81, 3, 3, 0x5a79f2ff),
            (363, 81, 3, 3, 0x3043d2ff),
            (366, 81, 3, 3, 0x3345d4ff),
            (369, 81, 3, 3, 0x3247d6ff),
            (372, 81, 3, 3, 0x3447d6ff),
            (375, 81, 3, 3, 0x3a4ad8ff),
            (378, 81, 3, 3, 0x404cdaff),
            (381, 81, 3, 3, 0x4550deff),
            (0, 84, 3, 3, 0x456be8ff),
            (3, 84, 3, 3, 0x436beaff),
            (6, 84, 3, 3, 0x466debff),
            (9, 84, 3, 3, 0x496feeff),
            (12, 84, 3, 3, 0x4d72eeff),
            (15, 84, 3, 3, 0x5175efff),
            (18, 84, 3, 3, 0x5779f2ff),
            (363, 84, 3, 3, 0x3446d4ff),
            (366, 84, 3, 3, 0x3848d7ff),
            (369, 84, 3, 3, 0x3c4ad8ff),
            (372, 84, 3, 3, 0x3d4bd9ff),
            (375, 84, 3, 3, 0x414edbff),
            (378, 84, 3, 3, 0x4751ddff),
            (381, 84, 3, 3, 0x4c57e1ff),
            (0, 87, 3, 3, 0x416ae9ff),
            (3, 87, 3, 3, 0x426beaff),
            (6, 87, 3, 3, 0x436beaff),
            (9, 87, 3, 3, 0x496eedff),
            (12, 87, 3, 3, 0x4f73efff),
            (15, 87, 3, 3, 0x5679f0ff),
            (18, 87, 3, 3, 0x5b7df1ff),
            (363, 87, 3, 3, 0x364ad9ff),
            (366, 87, 3, 3, 0x3a4bdaff),
            (369, 87, 3, 3, 0x3e4ddcff),
            (372, 87, 3, 3, 0x4350deff),
            (375, 87, 3, 3, 0x4953e0ff),
            (378, 87, 3, 3, 0x4e57e2ff),
            (381, 87, 3, 3, 0x505ce4ff),
            (0, 90, 3, 3, 0x3f6ae9ff),
            (3, 90, 3, 3, 0x406aeaff),
            (6, 90, 3, 3, 0x456cebff),
            (9, 90, 3, 3, 0x4e72efff),
            (12, 90, 3, 3, 0x5678f0ff),
            (15, 90, 3, 3, 0x5f7df2ff),
            (18, 90, 3, 3, 0x607ff3ff),
            (363, 90, 3, 3, 0x394ddcff),
            (366, 90, 3, 3, 0x3b4eddff),
            (369, 90, 3, 3, 0x3d50deff),
            (372, 90, 3, 3, 0x4553dfff),
            (375, 90, 3, 3, 0x4a56e1ff),
            (378, 90, 3, 3, 0x4d59e3ff),
            (381, 90, 3, 3, 0x515fe5ff),
            (0, 93, 3, 3, 0x3e67eaff),
            (3, 93, 3, 3, 0x3f67e9ff),
            (6, 93, 3, 3, 0x456becff),
            (9, 93, 3, 3, 0x4c6eeeff),
            (12, 93, 3, 3, 0x5575f0ff),
            (15, 93, 3, 3, 0x5c79f2ff),
            (18, 93, 3, 3, 0x5f7cf3ff),
            (363, 93, 3, 3, 0x3d52e1ff),
            (366, 93, 3, 3, 0x3e52e1ff),
            (369, 93, 3, 3, 0x4155e3ff),
            (372, 93, 3, 3, 0x4857e3ff),
            (375, 93, 3, 3, 0x4957e3ff),
            (378, 93, 3, 3, 0x4c59e5ff),
            (381, 93, 3, 3, 0x515de6ff),
            (0, 96, 3, 3, 0x446aecff),
            (3, 96, 3, 3, 0x4269ebff),
            (6, 96, 3, 3, 0x486bedff),
            (9, 96, 3, 3, 0x5473f0ff),
            (12, 96, 3, 3, 0x607bf2ff),
            (15, 96, 3, 3, 0x5f7bf3ff),
            (18, 96, 3, 3, 0x627ef5ff),
            (363, 96, 3, 3, 0x4456e3ff),
            (366, 96, 3, 3, 0x4858e4ff),
            (369, 96, 3, 3, 0x4e5ce7ff),
            (372, 96, 3, 3, 0x515ce7ff),
            (375, 96, 3, 3, 0x505be6ff),
            (378, 96, 3, 3, 0x4e57e3ff),
            (381, 96, 3, 3, 0x4e55dfff),
            (0, 99, 3, 3, 0x446aecff),
            (3, 99, 3, 3, 0x486cedff),
            (6, 99, 3, 3, 0x5070efff),
            (9, 99, 3, 3, 0x5774f0ff),
            (12, 99, 3, 3, 0x5d79f2ff),
            (15, 99, 3, 3, 0x607cf4ff),
            (18, 99, 3, 3, 0x6985f7ff),
            (363, 99, 3, 3, 0x4c5be6ff),
            (366, 99, 3, 3, 0x535ee6ff),
            (369, 99, 3, 3, 0x5b63e8ff),
            (372, 99, 3, 3, 0x585fe7ff),
            (375, 99, 3, 3, 0x5155dfff),
            (378, 99, 3, 3, 0x494ad6ff),
            (381, 99, 3, 3, 0x4543cdff),
            (0, 102, 3, 3, 0x466bebff),
            (3, 102, 3, 3, 0x4c6feeff),
            (6, 102, 3, 3, 0x5472efff),
            (9, 102, 3, 3, 0x5d79f2ff),
            (12, 102, 3, 3, 0x6681f5ff),
            (15, 102, 3, 3, 0x6f88f7ff),
            (18, 102, 3, 3, 0x758cf8ff),
            (363, 102, 3, 3, 0x5760e7ff),
            (366, 102, 3, 3, 0x6267eaff),
            (369, 102, 3, 3, 0x6569ebff),
            (372, 102, 3, 3, 0x5858e0ff),
            (375, 102, 3, 3, 0x4640cbff),
            (378, 102, 3, 3, 0x3b34c0ff),
            (381, 102, 3, 3, 0x3a32beff),
            (0, 105, 3, 3, 0x4d6eecff),
            (3, 105, 3, 3, 0x5573f0ff),
            (6, 105, 3, 3, 0x5a76f1ff),
            (9, 105, 3, 3, 0x677ff3ff),
            (12, 105, 3, 3, 0x6f86f7ff),
            (15, 105, 3, 3, 0x778cf8ff),
            (18, 105, 3, 3, 0x7c91f8ff),
            (363, 105, 3, 3, 0x6b6decff),
            (366, 105, 3, 3, 0x6b6cebff),
            (369, 105, 3, 3, 0x6664e9ff),
            (372, 105, 3, 3, 0x4a3ec9ff),
            (375, 105, 3, 3, 0x3730bfff),
            (378, 105, 3, 3, 0x3931bfff),
            (381, 105, 3, 3, 0x3b32c0ff),
            (0, 108, 3, 3, 0x5a75f2ff),
            (3, 108, 3, 3, 0x5e78f2ff),
            (6, 108, 3, 3, 0x657df3ff),
            (9, 108, 3, 3, 0x7689f7ff),
            (12, 108, 3, 3, 0x7d8ff8ff),
            (15, 108, 3, 3, 0x7d90f8ff),
            (18, 108, 3, 3, 0x8294f8ff),
            (363, 108, 3, 3, 0x7572ecff),
            (366, 108, 3, 3, 0x6d69ebff),
            (369, 108, 3, 3, 0x5546d1ff),
            (372, 108, 3, 3, 0x3b33c2ff),
            (375, 108, 3, 3, 0x3830c0ff),
            (378, 108, 3, 3, 0x3b31c2ff),
            (381, 108, 3, 3, 0x4135c5ff),
            (0, 111, 3, 3, 0x647bf4ff),
            (3, 111, 3, 3, 0x6a80f4ff),
            (6, 111, 3, 3, 0x6d83f5ff),
            (9, 111, 3, 3, 0x798cf7ff),
            (12, 111, 3, 3, 0x7a8df8ff),
            (15, 111, 3, 3, 0x7f91f8ff),
            (18, 111, 3, 3, 0x8495f8ff),
            (363, 111, 3, 3, 0x7773ecff),
            (366, 111, 3, 3, 0x6253ddff),
            (369, 111, 3, 3, 0x4438ccff),
            (372, 111, 3, 3, 0x3c33c5ff),
            (375, 111, 3, 3, 0x3c33c5ff),
            (378, 111, 3, 3, 0x4235c8ff),
            (381, 111, 3, 3, 0x4739cdff),
            (0, 114, 3, 3, 0x667df4ff),
            (3, 114, 3, 3, 0x6b80f5ff),
            (6, 114, 3, 3, 0x6c81f5ff),
            (9, 114, 3, 3, 0x798af7ff),
            (12, 114, 3, 3, 0x7d8ff8ff),
            (15, 114, 3, 3, 0x8292f8ff),
            (18, 114, 3, 3, 0x8796f8ff),
            (363, 114, 3, 3, 0x7160e4ff),
            (366, 114, 3, 3, 0x4e3fd5ff),
            (369, 114, 3, 3, 0x4739d0ff),
            (372, 114, 3, 3, 0x4235cdff),
            (375, 114, 3, 3, 0x4436cdff),
            (378, 114, 3, 3, 0x4739d0ff),
            (381, 114, 3, 3, 0x4d3cd4ff),
            (0, 117, 3, 3, 0x6a80f5ff),
            (3, 117, 3, 3, 0x6e82f6ff),
            (6, 117, 3, 3, 0x7184f7ff),
            (9, 117, 3, 3, 0x7588f7ff),
            (12, 117, 3, 3, 0x7c8df8ff),
            (15, 117, 3, 3, 0x8594f8ff),
            (18, 117, 3, 3, 0x8c9af8ff),
            (363, 117, 3, 3, 0x5744d9ff),
            (366, 117, 3, 3, 0x4f3fd5ff),
            (369, 117, 3, 3, 0x4a3cd4ff),
            (372, 117, 3, 3, 0x483ad2ff),
            (375, 117, 3, 3, 0x483ad1ff),
            (378, 117, 3, 3, 0x4b3bd4ff),
            (381, 117, 3, 3, 0x523fd8ff),
            (0, 120, 3, 3, 0x697df5ff),
            (3, 120, 3, 3, 0x6d81f6ff),
            (6, 120, 3, 3, 0x7184f7ff),
            (9, 120, 3, 3, 0x7a8bf8ff),
            (12, 120, 3, 3, 0x8994f8ff),
            (15, 120, 3, 3, 0x909bf8ff),
            (18, 120, 3, 3, 0xa1abf9ff),
            (363, 120, 3, 3, 0x5744daff),
            (366, 120, 3, 3, 0x5240d8ff),
            (369, 120, 3, 3, 0x4e3ed6ff),
            (372, 120, 3, 3, 0x4d3ed4ff),
            (375, 120, 3, 3, 0x4f3ed5ff),
            (378, 120, 3, 3, 0x523fd7ff),
            (381, 120, 3, 3, 0x5943dbff),
            (0, 123, 3, 3, 0x7283f7ff),
            (3, 123, 3, 3, 0x7888f7ff),
            (6, 123, 3, 3, 0x7e8cf8ff),
            (9, 123, 3, 3, 0x8a96f8ff),
            (12, 123, 3, 3, 0x949ef8ff),
            (15, 123, 3, 3, 0xa1aaf9ff),
            (18, 123, 3, 3, 0xa5aef9ff),
            (363, 123, 3, 3, 0x5844dbff),
            (366, 123, 3, 3, 0x5642d9ff),
            (369, 123, 3, 3, 0x5440d8ff),
            (372, 123, 3, 3, 0x533fd8ff),
            (375, 123, 3, 3, 0x543fd9ff),
            (378, 123, 3, 3, 0x5843dbff),
            (381, 123, 3, 3, 0x5f47dfff),
            (0, 126, 3, 3, 0x7a89f7ff),
            (3, 126, 3, 3, 0x8491f8ff),
            (6, 126, 3, 3, 0x8693f8ff),
            (9, 126, 3, 3, 0x919cf8ff),
            (12, 126, 3, 3, 0x9da7f9ff),
            (15, 126, 3, 3, 0xa4adf9ff),
            (18, 126, 3, 3, 0xa7b0f9ff),
            (363, 126, 3, 3, 0x5b46ddff),
            (366, 126, 3, 3, 0x5844dcff),
            (369, 126, 3, 3, 0x5743dbff),
            (372, 126, 3, 3, 0x5842dbff),
            (375, 126, 3, 3, 0x5a44ddff),
            (378, 126, 3, 3, 0x5c46dfff),
            (381, 126, 3, 3, 0x6349e3ff),
            (0, 129, 3, 3, 0x818ff8ff),
            (3, 129, 3, 3, 0x8592f8ff),
            (6, 129, 3, 3, 0x8e9af8ff),
            (9, 129, 3, 3, 0x96a1f8ff),
            (12, 129, 3, 3, 0xa0aaf9ff),
            (15, 129, 3, 3, 0xa6b0f9ff),
            (18, 129, 3, 3, 0xabb4faff),
            (363, 129, 3, 3, 0x6047ddff),
            (366, 129, 3, 3, 0x5d47ddff),
            (369, 129, 3, 3, 0x5c45ddff),
            (372, 129, 3, 3, 0x5d46dfff),
            (375, 129, 3, 3, 0x5f48e0ff),
            (378, 129, 3, 3, 0x6249e2ff),
            (381, 129, 3, 3, 0x694fe6ff),
            (0, 132, 3, 3, 0x8996f8ff),
            (3, 132, 3, 3, 0x8b98f8ff),
            (6, 132, 3, 3, 0x94a0f8ff),
            (9, 132, 3, 3, 0x9ca6f9ff),
            (12, 132, 3, 3, 0xa2acf9ff),
            (15, 132, 3, 3, 0xabb4faff),
            (18, 132, 3, 3, 0xb0b8faff),
            (363, 132, 3, 3, 0x634adfff),
            (366, 132, 3, 3, 0x6148deff),
            (369, 132, 3, 3, 0x5f46deff),
            (372, 132, 3, 3, 0x6148e0ff),
            (375, 132, 3, 3, 0x654be2ff),
            (378, 132, 3, 3, 0x6a4ee5ff),
            (381, 132, 3, 3, 0x6d52e7ff),
            (0, 135, 3, 3, 0x8c9af8ff),
            (3, 135, 3, 3, 0x909ef8ff),
            (6, 135, 3, 3, 0x97a2f9ff),
            (9, 135, 3, 3, 0x9ba5f9ff),
            (12, 135, 3, 3, 0xa3adf9ff),
            (15, 135, 3, 3, 0xadb6faff),
            (18, 135, 3, 3, 0xb3bafaff),
            (363, 135, 3, 3, 0x664ce1ff),
            (366, 135, 3, 3, 0x664ae0ff),
            (369, 135, 3, 3, 0x6549e0ff),
            (372, 135, 3, 3, 0x674be2ff),
            (375, 135, 3, 3, 0x6c4ee5ff),
            (378, 135, 3, 3, 0x7053e8ff),
            (381, 135, 3, 3, 0x7356eaff),
            (0, 138, 3, 3, 0x8c9bf8ff),
            (3, 138, 3, 3, 0x8f9df8ff),
            (6, 138, 3, 3, 0x9ba8f9ff),
            (9, 138, 3, 3, 0x9fa9f9ff),
            (12, 138, 3, 3, 0xa4aef9ff),
            (15, 138, 3, 3, 0xacb5faff),
            (18, 138, 3, 3, 0xb3bafaff),
            (363, 138, 3, 3, 0x6a50e2ff),
            (366, 138, 3, 3, 0x6a4de3ff),
            (369, 138, 3, 3, 0x6a4ce2ff),
            (372, 138, 3, 3, 0x6c4ee5ff),
            (375, 138, 3, 3, 0x7052e8ff),
            (378, 138, 3, 3, 0x7455eaff),
            (381, 138, 3, 3, 0x7758ecff),
            (0, 141, 3, 3, 0x8d9df8ff),
            (3, 141, 3, 3, 0x92a1f9ff),
            (6, 141, 3, 3, 0x98a5f9ff),
            (9, 141, 3, 3, 0xa2adf9ff),
            (12, 141, 3, 3, 0xa7b1f9ff),
            (15, 141, 3, 3, 0xaab4faff),
            (18, 141, 3, 3, 0xb0b8faff),
            (363, 141, 3, 3, 0x6f54e5ff),
            (366, 141, 3, 3, 0x6d50e4ff),
            (369, 141, 3, 3, 0x6f52e5ff),
            (372, 141, 3, 3, 0x7051e7ff),
            (375, 141, 3, 3, 0x7455e9ff),
            (378, 141, 3, 3, 0x7959ebff),
            (381, 141, 3, 3, 0x7b5ceeff),
            (0, 144, 3, 3, 0x8c9cf8ff),
            (3, 144, 3, 3, 0x97a4f9ff),
            (6, 144, 3, 3, 0xa2aef9ff),
            (9, 144, 3, 3, 0xa4aef9ff),
            (12, 144, 3, 3, 0xa9b3f9ff),
            (15, 144, 3, 3, 0xaab4faff),
            (18, 144, 3, 3, 0xb4bafaff),
            (363, 144, 3, 3, 0x7357e8ff),
            (366, 144, 3, 3, 0x7456e8ff),
            (369, 144, 3, 3, 0x7355e7ff),
            (372, 144, 3, 3, 0x7454e9ff),
            (375, 144, 3, 3, 0x7859ebff),
            (378, 144, 3, 3, 0x7c5dedff),
            (381, 144, 3, 3, 0x8161f0ff),
            (0, 147, 3, 3, 0x96a2f9ff),
            (3, 147, 3, 3, 0x99a4f9ff),
            (6, 147, 3, 3, 0x9fa9f9ff),
            (9, 147, 3, 3, 0xa4aef9ff),
            (12, 147, 3, 3, 0xa7b1f9ff),
            (15, 147, 3, 3, 0xafb8faff),
            (18, 147, 3, 3, 0xb9bffaff),
            (363, 147, 3, 3, 0x7859eaff),
            (366, 147, 3, 3, 0x7859e9ff),
            (369, 147, 3, 3, 0x795be9ff),
            (372, 147, 3, 3, 0x795aeaff),
            (375, 147, 3, 3, 0x7c5cedff),
            (378, 147, 3, 3, 0x8061efff),
            (381, 147, 3, 3, 0x8767f1ff),
            (0, 150, 3, 3, 0x959ff8ff),
            (3, 150, 3, 3, 0x9caaf9ff),
            (6, 150, 3, 3, 0x9ea8f9ff),
            (9, 150, 3, 3, 0xa1abf9ff),
            (12, 150, 3, 3, 0xa8b1f9ff),
            (15, 150, 3, 3, 0xb1b8faff),
            (18, 150, 3, 3, 0xbbc0faff),
            (363, 150, 3, 3, 0x7b5decff),
            (366, 150, 3, 3, 0x7c5eecff),
            (369, 150, 3, 3, 0x7e5fecff),
            (372, 150, 3, 3, 0x7f60edff),
            (375, 150, 3, 3, 0x8061efff),
            (378, 150, 3, 3, 0x8566f0ff),
            (381, 150, 3, 3, 0x8b6bf3ff),
            (0, 153, 3, 3, 0x94a0f8ff),
            (3, 153, 3, 3, 0x99a5f9ff),
            (6, 153, 3, 3, 0x9ca6f9ff),
            (9, 153, 3, 3, 0xa4adf9ff),
            (12, 153, 3, 3, 0xacb4faff),
            (15, 153, 3, 3, 0xb8bdfaff),
            (18, 153, 3, 3, 0xc4c9fbff),
            (363, 153, 3, 3, 0x7e5eecff),
            (366, 153, 3, 3, 0x7f5fecff),
            (369, 153, 3, 3, 0x8262efff),
            (372, 153, 3, 3, 0x8465f0ff),
            (375, 153, 3, 3, 0x8667f1ff),
            (378, 153, 3, 3, 0x896af2ff),
            (381, 153, 3, 3, 0x9070f5ff),
            (0, 156, 3, 3, 0x939ff8ff),
            (3, 156, 3, 3, 0x99a5f9ff),
            (6, 156, 3, 3, 0xa4adf9ff),
            (9, 156, 3, 3, 0xabb3faff),
            (12, 156, 3, 3, 0xb4bafaff),
            (15, 156, 3, 3, 0xbdc2faff),
            (18, 156, 3, 3, 0xc1c5fbff),
            (363, 156, 3, 3, 0x8263edff),
            (366, 156, 3, 3, 0x8363efff),
            (369, 156, 3, 3, 0x8766f0ff),
            (372, 156, 3, 3, 0x8a69f2ff),
            (375, 156, 3, 3, 0x8c6cf2ff),
            (378, 156, 3, 3, 0x8e70f5ff),
            (381, 156, 3, 3, 0x9374f6ff),
            (0, 159, 3, 3, 0x96a2f8ff),
            (3, 159, 3, 3, 0xa2acf9ff),
            (6, 159, 3, 3, 0xaab2f9ff),
            (9, 159, 3, 3, 0xb1b7faff),
            (12, 159, 3, 3, 0xb8bdfaff),
            (15, 159, 3, 3, 0xbec3faff),
            (18, 159, 3, 3, 0xc2c5fbff),
            (363, 159, 3, 3, 0x8668f0ff),
            (366, 159, 3, 3, 0x896af1ff),
            (369, 159, 3, 3, 0x8b6bf2ff),
            (372, 159, 3, 3, 0x8e6ef3ff),
            (375, 159, 3, 3, 0x9273f6ff),
            (378, 159, 3, 3, 0x9476f6ff),
            (381, 159, 3, 3, 0x987af6ff),
            (0, 162, 3, 3, 0xa6b0faff),
            (3, 162, 3, 3, 0xacb5faff),
            (6, 162, 3, 3, 0xabb3faff),
            (9, 162, 3, 3, 0xb5bcfaff),
            (12, 162, 3, 3, 0xbbc0faff),
            (15, 162, 3, 3, 0xbcc0faff),
            (18, 162, 3, 3, 0xbfc2fbff),
            (363, 162, 3, 3, 0x8c6ef1ff),
            (366, 162, 3, 3, 0x8f70f3ff),
            (369, 162, 3, 3, 0x9172f4ff),
            (372, 162, 3, 3, 0x9577f5ff),
            (375, 162, 3, 3, 0x9a7bf7ff),
            (378, 162, 3, 3, 0x9c7ef7ff),
            (381, 162, 3, 3, 0x9f81f7ff),
            (0, 165, 3, 3, 0xacb7faff),
            (3, 165, 3, 3, 0xa5aef9ff),
            (6, 165, 3, 3, 0xaab3f9ff),
            (9, 165, 3, 3, 0xb4bbfaff),
            (12, 165, 3, 3, 0xb8bdfaff),
            (15, 165, 3, 3, 0xbabffaff),
            (18, 165, 3, 3, 0xbfc3fbff),
            (363, 165, 3, 3, 0x9075f3ff),
            (366, 165, 3, 3, 0x9377f5ff),
            (369, 165, 3, 3, 0x9679f5ff),
            (372, 165, 3, 3, 0x9b7df7ff),
            (375, 165, 3, 3, 0xa081f8ff),
            (378, 165, 3, 3, 0xa485f8ff),
            (381, 165, 3, 3, 0xa789f8ff),
            (0, 168, 3, 3, 0xa1acf9ff),
            (3, 168, 3, 3, 0xa1abf9ff),
            (6, 168, 3, 3, 0xa7aff9ff),
            (9, 168, 3, 3, 0xabb2faff),
            (12, 168, 3, 3, 0xb7bdfaff),
            (15, 168, 3, 3, 0xb7bcfaff),
            (18, 168, 3, 3, 0xbabffaff),
            (363, 168, 3, 3, 0x9379f5ff),
            (366, 168, 3, 3, 0x977cf6ff),
            (369, 168, 3, 3, 0x9d81f7ff),
            (372, 168, 3, 3, 0xa284f8ff),
            (375, 168, 3, 3, 0xa486f8ff),
            (378, 168, 3, 3, 0xaa8bf8ff),
            (381, 168, 3, 3, 0xae91f8ff),
            (0, 171, 3, 3, 0x9fa9f9ff),
            (3, 171, 3, 3, 0xa0a7f9ff),
            (6, 171, 3, 3, 0xa5acf9ff),
            (9, 171, 3, 3, 0xb1b8faff),
            (12, 171, 3, 3, 0xb1b8faff),
            (15, 171, 3, 3, 0xb6bcfaff),
            (18, 171, 3, 3, 0xc0c6fbff),
            (363, 171, 3, 3, 0x977ff6ff),
            (366, 171, 3, 3, 0x9d82f8ff),
            (369, 171, 3, 3, 0xa387f8ff),
            (372, 171, 3, 3, 0xa78bf8ff),
            (375, 171, 3, 3, 0xa98bf8ff),
            (378, 171, 3, 3, 0xae91f8ff),
            (381, 171, 3, 3, 0xb397f8ff),
            (0, 174, 3, 3, 0x9ca8f9ff),
            (3, 174, 3, 3, 0x9ea8f9ff),
            (6, 174, 3, 3, 0xa8b1f9ff),
            (9, 174, 3, 3, 0xadb6faff),
            (12, 174, 3, 3, 0xb3bcfaff),
            (15, 174, 3, 3, 0xb9c1faff),
            (18, 174, 3, 3, 0xc1c9fbff),
            (363, 174, 3, 3, 0x9e86f8ff),
            (366, 174, 3, 3, 0xa189f8ff),
            (369, 174, 3, 3, 0xa68bf8ff),
            (372, 174, 3, 3, 0xaa8ef8ff),
            (375, 174, 3, 3, 0xad91f8ff),
            (378, 174, 3, 3, 0xb295f8ff),
            (381, 174, 3, 3, 0xb79cf9ff),
            (0, 177, 3, 3, 0x99a5f9ff),
            (3, 177, 3, 3, 0x9ea6f9ff),
            (6, 177, 3, 3, 0xa8b2f9ff),
            (9, 177, 3, 3, 0xaeb8faff),
            (12, 177, 3, 3, 0xb0bbfaff),
            (15, 177, 3, 3, 0xb6c0faff),
            (18, 177, 3, 3, 0xbcc5fbff),
            (363, 177, 3, 3, 0xa58ef8ff),
            (366, 177, 3, 3, 0xa891f8ff),
            (369, 177, 3, 3, 0xab93f8ff),
            (372, 177, 3, 3, 0xad94f8ff),
            (375, 177, 3, 3, 0xb297f9ff),
            (378, 177, 3, 3, 0xb79bf9ff),
            (381, 177, 3, 3, 0xbba2f9ff),
            (0, 180, 3, 3, 0x929ff8ff),
            (3, 180, 3, 3, 0x9ba6f9ff),
            (6, 180, 3, 3, 0xa1acf9ff),
            (9, 180, 3, 3, 0xa6b2f9ff),
            (12, 180, 3, 3, 0xabb7faff),
            (15, 180, 3, 3, 0xb2bcfaff),
            (18, 180, 3, 3, 0xbdc7fbff),
            (363, 180, 3, 3, 0xab97f8ff),
            (366, 180, 3, 3, 0xad98f8ff),
            (369, 180, 3, 3, 0xb19cf9ff),
            (372, 180, 3, 3, 0xb69ef9ff),
            (375, 180, 3, 3, 0xb89ff9ff),
            (378, 180, 3, 3, 0xbaa1f9ff),
            (381, 180, 3, 3, 0xc0a7f9ff),
            (0, 183, 3, 3, 0x8e9cf8ff),
            (3, 183, 3, 3, 0x92a1f8ff),
            (6, 183, 3, 3, 0x9caaf9ff),
            (9, 183, 3, 3, 0xa4b1f9ff),
            (12, 183, 3, 3, 0xa6b3f9ff),
            (15, 183, 3, 3, 0xafbafaff),
            (18, 183, 3, 3, 0xb7c2faff),
            (363, 183, 3, 3, 0xb19df9ff),
            (366, 183, 3, 3, 0xb49ff9ff),
            (369, 183, 3, 3, 0xb6a3f9ff),
            (372, 183, 3, 3, 0xbaa5f9ff),
            (375, 183, 3, 3, 0xbda5f9ff),
            (378, 183, 3, 3, 0xbea6f9ff),
            (381, 183, 3, 3, 0xc6aefaff),
            (0, 186, 3, 3, 0x8598f8ff),
            (3, 186, 3, 3, 0x8fa0f8ff),
            (6, 186, 3, 3, 0x99aaf9ff),
            (9, 186, 3, 3, 0xa1aff9ff),
            (12, 186, 3, 3, 0xa6b4f9ff),
            (15, 186, 3, 3, 0xaebcfaff),
            (18, 186, 3, 3, 0xafbcfaff),
            (363, 186, 3, 3, 0xb5a5f9ff),
            (366, 186, 3, 3, 0xb8a7f9ff),
            (369, 186, 3, 3, 0xbba8f9ff),
            (372, 186, 3, 3, 0xbeabf9ff),
            (375, 186, 3, 3, 0xc1abf9ff),
            (378, 186, 3, 3, 0xc4acfaff),
            (381, 186, 3, 3, 0xc9b3faff),
            (0, 189, 3, 3, 0x8699f8ff),
            (3, 189, 3, 3, 0x99abf9ff),
            (6, 189, 3, 3, 0x99aaf9ff),
            (9, 189, 3, 3, 0xa8b6faff),
            (12, 189, 3, 3, 0xaebbfaff),
            (15, 189, 3, 3, 0xadbafaff),
            (18, 189, 3, 3, 0xa8b8faff),
            (363, 189, 3, 3, 0xb8abf9ff),
            (366, 189, 3, 3, 0xbaadf9ff),
            (369, 189, 3, 3, 0xbeaef9ff),
            (372, 189, 3, 3, 0xc1affaff),
            (375, 189, 3, 3, 0xc3b1faff),
            (378, 189, 3, 3, 0xc7b2faff),
            (381, 189, 3, 3, 0xcdb7faff),
            (0, 192, 3, 3, 0x7e93f8ff),
            (3, 192, 3, 3, 0x8fa1f8ff),
            (6, 192, 3, 3, 0x9daaf9ff),
            (9, 192, 3, 3, 0xb0bafaff),
            (12, 192, 3, 3, 0xa8b4f9ff),
            (15, 192, 3, 3, 0xa1b0f9ff),
            (18, 192, 3, 3, 0xa1b2f9ff),
            (363, 192, 3, 3, 0xbcaff9ff),
            (366, 192, 3, 3, 0xbeb1faff),
            (369, 192, 3, 3, 0xc1b4faff),
            (372, 192, 3, 3, 0xc5b4faff),
            (375, 192, 3, 3, 0xc8b5faff),
            (378, 192, 3, 3, 0xcbb7faff),
            (381, 192, 3, 3, 0xcebbfaff),
            (0, 195, 3, 3, 0x5d77edff),
            (3, 195, 3, 3, 0x6078f0ff),
            (6, 195, 3, 3, 0x8695f8ff),
            (9, 195, 3, 3, 0x9aa6f9ff),
            (12, 195, 3, 3, 0x9fabf9ff),
            (15, 195, 3, 3, 0x9faef9ff),
            (18, 195, 3, 3, 0x9caef9ff),
            (363, 195, 3, 3, 0xbfb2faff),
            (366, 195, 3, 3, 0xc1b4faff),
            (369, 195, 3, 3, 0xc3b7faff),
            (372, 195, 3, 3, 0xc9b8faff),
            (375, 195, 3, 3, 0xcbbafaff),
            (378, 195, 3, 3, 0xcdbcfaff),
            (381, 195, 3, 3, 0xd1bffbff),
            (0, 198, 3, 3, 0x4f6febff),
            (3, 198, 3, 3, 0x526fedff),
            (6, 198, 3, 3, 0x5a73f0ff),
            (9, 198, 3, 3, 0x7383f3ff),
            (12, 198, 3, 3, 0x909ff8ff),
            (15, 198, 3, 3, 0x94a5f9ff),
            (18, 198, 3, 3, 0x9babf9ff),
            (363, 198, 3, 3, 0xc1b4faff),
            (366, 198, 3, 3, 0xc4b7faff),
            (369, 198, 3, 3, 0xc4b9faff),
            (372, 198, 3, 3, 0xc9bbfaff),
            (375, 198, 3, 3, 0xccbcfaff),
            (378, 198, 3, 3, 0xcfbffbff),
            (381, 198, 3, 3, 0xd3c2fbff),
            (0, 201, 3, 3, 0x4968e9ff),
            (3, 201, 3, 3, 0x4969e9ff),
            (6, 201, 3, 3, 0x4f6cecff),
            (9, 201, 3, 3, 0x5770eeff),
            (12, 201, 3, 3, 0x5f76efff),
            (15, 201, 3, 3, 0x8391f7ff),
            (18, 201, 3, 3, 0xa1a8f9ff),
            (363, 201, 3, 3, 0xc3b8faff),
            (366, 201, 3, 3, 0xc5bafaff),
            (369, 201, 3, 3, 0xc6bdfaff),
            (372, 201, 3, 3, 0xcbbefaff),
            (375, 201, 3, 3, 0xcebefaff),
            (378, 201, 3, 3, 0xd0c0fbff),
            (381, 201, 3, 3, 0xd4c5fbff),
            (0, 204, 3, 3, 0x4064e6ff),
            (3, 204, 3, 3, 0x4164e6ff),
            (6, 204, 3, 3, 0x4666e8ff),
            (9, 204, 3, 3, 0x4b68eaff),
            (12, 204, 3, 3, 0x516cedff),
            (15, 204, 3, 3, 0x5a71f0ff),
            (18, 204, 3, 3, 0x9aa7f8ff),
            (363, 204, 3, 3, 0xc5bafaff),
            (366, 204, 3, 3, 0xc7bcfaff),
            (369, 204, 3, 3, 0xcabffaff),
            (372, 204, 3, 3, 0xcdc0fbff),
            (375, 204, 3, 3, 0xcfc0fbff),
            (378, 204, 3, 3, 0xd2c1fbff),
            (381, 204, 3, 3, 0xd5c8fbff),
            (0, 207, 3, 3, 0x3a5fe3ff),
            (3, 207, 3, 3, 0x385fe3ff),
            (6, 207, 3, 3, 0x3a60e4ff),
            (9, 207, 3, 3, 0x3f61e6ff),
            (12, 207, 3, 3, 0x4262e9ff),
            (15, 207, 3, 3, 0x4a66ecff),
            (18, 207, 3, 3, 0x5b73f0ff),
            (363, 207, 3, 3, 0xc6bafaff),
            (366, 207, 3, 3, 0xc8bffaff),
            (369, 207, 3, 3, 0xccc3fbff),
            (372, 207, 3, 3, 0xcec3fbff),
            (375, 207, 3, 3, 0xd0c3fbff),
            (378, 207, 3, 3, 0xd2c4fbff),
            (381, 207, 3, 3, 0xd6cafbff),
            (0, 210, 3, 3, 0x335ae0ff),
            (3, 210, 3, 3, 0x315ae0ff),
            (6, 210, 3, 3, 0x3159e1ff),
            (9, 210, 3, 3, 0x3359e1ff),
            (12, 210, 3, 3, 0x355ae2ff),
            (15, 210, 3, 3, 0x3e5fe7ff),
            (18, 210, 3, 3, 0x4464ebff),
            (363, 210, 3, 3, 0xc6bcfaff),
            (366, 210, 3, 3, 0xcac0faff),
            (369, 210, 3, 3, 0xcdc5fbff),
            (372, 210, 3, 3, 0xcfc5fbff),
            (375, 210, 3, 3, 0xd1c3fbff),
            (378, 210, 3, 3, 0xd3c4fbff),
            (381, 210, 3, 3, 0xd7cbfbff),
            (0, 213, 3, 3, 0x2c57dcff),
            (3, 213, 3, 3, 0x2856deff),
            (6, 213, 3, 3, 0x2755dfff),
            (9, 213, 3, 3, 0x2a55dfff),
            (12, 213, 3, 3, 0x2f56e1ff),
            (15, 213, 3, 3, 0x3359e3ff),
            (18, 213, 3, 3, 0x395de6ff),
            (363, 213, 3, 3, 0xc7bbfaff),
            (366, 213, 3, 3, 0xcbc0faff),
            (369, 213, 3, 3, 0xcec5fbff),
            (372, 213, 3, 3, 0xcfc6fbff),
            (375, 213, 3, 3, 0xd1c5fbff),
            (378, 213, 3, 3, 0xd2c5fbff),
            (381, 213, 3, 3, 0xd6c9fbff),
            (0, 216, 3, 3, 0x2253daff),
            (3, 216, 3, 3, 0x2152daff),
            (6, 216, 3, 3, 0x2151daff),
            (9, 216, 3, 3, 0x2253dcff),
            (12, 216, 3, 3, 0x2454deff),
            (15, 216, 3, 3, 0x2654dfff),
            (18, 216, 3, 3, 0x2b56e0ff),
            (363, 216, 3, 3, 0xc8bdfaff),
            (366, 216, 3, 3, 0xcac0faff),
            (369, 216, 3, 3, 0xcec5fbff),
            (372, 216, 3, 3, 0xd0c8fbff),
            (375, 216, 3, 3, 0xd2c7fbff),
            (378, 216, 3, 3, 0xd4c7fbff),
            (381, 216, 3, 3, 0xd7cbfbff),
            (0, 219, 3, 3, 0x2050d7ff),
            (3, 219, 3, 3, 0x1f4ed7ff),
            (6, 219, 3, 3, 0x1f4dd7ff),
            (9, 219, 3, 3, 0x204fd9ff),
            (12, 219, 3, 3, 0x2050daff),
            (15, 219, 3, 3, 0x2151dbff),
            (18, 219, 3, 3, 0x2152dcff),
            (363, 219, 3, 3, 0xc8bdfaff),
            (366, 219, 3, 3, 0xcbc0faff),
            (369, 219, 3, 3, 0xcec5fbff),
            (372, 219, 3, 3, 0xd1c8fbff),
            (375, 219, 3, 3, 0xd2c9fbff),
            (378, 219, 3, 3, 0xd5c9fbff),
            (381, 219, 3, 3, 0xd7ccfbff),
            (0, 222, 3, 3, 0x1e4cd3ff),
            (3, 222, 3, 3, 0x1e4bd3ff),
            (6, 222, 3, 3, 0x1e4bd4ff),
            (9, 222, 3, 3, 0x1e4bd5ff),
            (12, 222, 3, 3, 0x1e4cd6ff),
            (15, 222, 3, 3, 0x1e4dd7ff),
            (18, 222, 3, 3, 0x1f4ed9ff),
            (363, 222, 3, 3, 0xc7bffaff),
            (366, 222, 3, 3, 0xcac2fbff),
            (369, 222, 3, 3, 0xcec6fbff),
            (372, 222, 3, 3, 0xd2cafbff),
            (375, 222, 3, 3, 0xd2cafbff),
            (378, 222, 3, 3, 0xd5cafbff),
            (381, 222, 3, 3, 0xd7cdfbff),
            (0, 225, 3, 3, 0x1d4ad2ff),
            (3, 225, 3, 3, 0x1d4ad2ff),
            (6, 225, 3, 3, 0x1d4ad2ff),
            (9, 225, 3, 3, 0x1d49d3ff),
            (12, 225, 3, 3, 0x1d49d3ff),
            (15, 225, 3, 3, 0x1e4ad7ff),
            (18, 225, 3, 3, 0x1e4bd6ff),
            (363, 225, 3, 3, 0xc9c1fbff),
            (366, 225, 3, 3, 0xcac2fbff),
            (369, 225, 3, 3, 0xcec7fbff),
            (372, 225, 3, 3, 0xd2cafbff),
            (375, 225, 3, 3, 0xd4cafbff),
            (378, 225, 3, 3, 0xd5cafbff),
            (381, 225, 3, 3, 0xd7d2fcff),
            (0, 228, 3, 3, 0x1c49d0ff),
            (3, 228, 3, 3, 0x1d49d1ff),
            (6, 228, 3, 3, 0x1d49d1ff),
            (9, 228, 3, 3, 0x1c48d1ff),
            (12, 228, 3, 3, 0x1c48d1ff),
            (15, 228, 3, 3, 0x1d48d3ff),
            (18, 228, 3, 3, 0x1d49d4ff),
            (363, 228, 3, 3, 0xcac2fbff),
            (366, 228, 3, 3, 0xcbc3fbff),
            (369, 228, 3, 3, 0xcec7fbff),
            (372, 228, 3, 3, 0xd3cbfbff),
            (375, 228, 3, 3, 0xd4cbfbff),
            (378, 228, 3, 3, 0xd4ccfbff),
            (381, 228, 3, 3, 0xdad5fcff),
            (0, 231, 3, 3, 0x1c48d0ff),
            (3, 231, 3, 3, 0x1c47cfff),
            (6, 231, 3, 3, 0x1b47cfff),
            (9, 231, 3, 3, 0x1c47cfff),
            (12, 231, 3, 3, 0x1c47cfff),
            (15, 231, 3, 3, 0x1b45d0ff),
            (18, 231, 3, 3, 0x1b46d1ff),
            (363, 231, 3, 3, 0xccc4fbff),
            (366, 231, 3, 3, 0xcbc3fbff),
            (369, 231, 3, 3, 0xcec7fbff),
            (372, 231, 3, 3, 0xd3cbfbff),
            (375, 231, 3, 3, 0xd4ccfbff),
            (378, 231, 3, 3, 0xd5cefbff),
            (381, 231, 3, 3, 0xdad6fcff),
            (0, 234, 3, 3, 0x1c47ceff),
            (3, 234, 3, 3, 0x1b45ccff),
            (6, 234, 3, 3, 0x1b45cdff),
            (9, 234, 3, 3, 0x1b45ccff),
            (12, 234, 3, 3, 0x1b45ceff),
            (15, 234, 3, 3, 0x1a44cfff),
            (18, 234, 3, 3, 0x1a44d0ff),
            (363, 234, 3, 3, 0xccc6fbff),
            (366, 234, 3, 3, 0xcac5fbff),
            (369, 234, 3, 3, 0xccc7fbff),
            (372, 234, 3, 3, 0xd1cbfbff),
            (375, 234, 3, 3, 0xd6cefbff),
            (378, 234, 3, 3, 0xd6d1fcff),
            (381, 234, 3, 3, 0xd7d4fcff),
            (0, 237, 3, 3, 0x1b45ccff),
            (3, 237, 3, 3, 0x1a44caff),
            (6, 237, 3, 3, 0x1a44caff),
            (9, 237, 3, 3, 0x1a44cbff),
            (12, 237, 3, 3, 0x1a44ccff),
            (15, 237, 3, 3, 0x1a43cdff),
            (18, 237, 3, 3, 0x1a43ceff),
            (363, 237, 3, 3, 0xcbc6fbff),
            (366, 237, 3, 3, 0xcac6fbff),
            (369, 237, 3, 3, 0xccc8fbff),
            (372, 237, 3, 3, 0xd0ccfbff),
            (375, 237, 3, 3, 0xd5cffbff),
            (378, 237, 3, 3, 0xd9d5fcff),
            (381, 237, 3, 3, 0xd3d2fcff),
            (0, 240, 3, 3, 0x1a44caff),
            (3, 240, 3, 3, 0x1a44caff),
            (6, 240, 3, 3, 0x1a43caff),
            (9, 240, 3, 3, 0x1a43cbff),
            (12, 240, 3, 3, 0x1a43caff),
            (15, 240, 3, 3, 0x1942cbff),
            (18, 240, 3, 3, 0x1941ccff),
            (363, 240, 3, 3, 0xcbc7fbff),
            (366, 240, 3, 3, 0xcbc7fbff),
            (369, 240, 3, 3, 0xccc8fbff),
            (372, 240, 3, 3, 0xd0ccfbff),
            (375, 240, 3, 3, 0xd7d1fcff),
            (378, 240, 3, 3, 0xd6d4fcff),
            (381, 240, 3, 3, 0xd1cffcff),
            (0, 243, 3, 3, 0x1a44caff),
            (3, 243, 3, 3, 0x1a43caff),
            (6, 243, 3, 3, 0x1942caff),
            (9, 243, 3, 3, 0x1942caff),
            (12, 243, 3, 3, 0x1942caff),
            (15, 243, 3, 3, 0x1942caff),
            (18, 243, 3, 3, 0x1941cbff),
            (363, 243, 3, 3, 0xcbc7fbff),
            (366, 243, 3, 3, 0xcbc7fbff),
            (369, 243, 3, 3, 0xccc8fbff),
            (372, 243, 3, 3, 0xd0cdfbff),
            (375, 243, 3, 3, 0xd7d5fcff),
            (378, 243, 3, 3, 0xd5d4fcff),
            (381, 243, 3, 3, 0xcfcbfbff),
            (0, 246, 3, 3, 0x1a43caff),
            (3, 246, 3, 3, 0x1942c9ff),
            (6, 246, 3, 3, 0x1941c8ff),
            (9, 246, 3, 3, 0x1941c9ff),
            (12, 246, 3, 3, 0x1941caff),
            (15, 246, 3, 3, 0x1941caff),
            (18, 246, 3, 3, 0x1941caff),
            (363, 246, 3, 3, 0xcbc7fbff),
            (366, 246, 3, 3, 0xccc8fbff),
            (369, 246, 3, 3, 0xccc9fbff),
            (372, 246, 3, 3, 0xd5d2fcff),
            (375, 246, 3, 3, 0xd6d6fcff),
            (378, 246, 3, 3, 0xd1d1fcff),
            (381, 246, 3, 3, 0xcfcbfbff),
            (0, 249, 3, 3, 0x1942c7ff),
            (3, 249, 3, 3, 0x1942c8ff),
            (6, 249, 3, 3, 0x1941c9ff),
            (9, 249, 3, 3, 0x1941c9ff),
            (12, 249, 3, 3, 0x1941caff),
            (15, 249, 3, 3, 0x1941caff),
            (18, 249, 3, 3, 0x1942caff),
            (363, 249, 3, 3, 0xcdc9fbff),
            (366, 249, 3, 3, 0xcdcafbff),
            (369, 249, 3, 3, 0xcecbfbff),
            (372, 249, 3, 3, 0xd7d6fcff),
            (375, 249, 3, 3, 0xd5d7fcff),
            (378, 249, 3, 3, 0xd2cffbff),
            (381, 249, 3, 3, 0xd0cafbff),
            (0, 252, 3, 3, 0x1941c7ff),
            (3, 252, 3, 3, 0x1941c7ff),
            (6, 252, 3, 3, 0x1941c9ff),
            (9, 252, 3, 3, 0x1941c8ff),
            (12, 252, 3, 3, 0x1941c9ff),
            (15, 252, 3, 3, 0x1941caff),
            (18, 252, 3, 3, 0x1942c9ff),
            (363, 252, 3, 3, 0xcecbfbff),
            (366, 252, 3, 3, 0xcccafbff),
            (369, 252, 3, 3, 0xd6d5fcff),
            (372, 252, 3, 3, 0xd8dafcff),
            (375, 252, 3, 3, 0xd4d7fcff),
            (378, 252, 3, 3, 0xd2cffbff),
            (381, 252, 3, 3, 0xd0c9fbff),
            (0, 255, 3, 3, 0x1840c6ff),
            (3, 255, 3, 3, 0x1840c6ff),
            (6, 255, 3, 3, 0x1840c7ff),
            (9, 255, 3, 3, 0x1840c6ff),
            (12, 255, 3, 3, 0x1840c6ff),
            (15, 255, 3, 3, 0x1840c7ff),
            (18, 255, 3, 3, 0x1941c8ff),
            (363, 255, 3, 3, 0xceccfbff),
            (366, 255, 3, 3, 0xd0d0fbff),
            (369, 255, 3, 3, 0xdbdbfcff),
            (372, 255, 3, 3, 0xd8dbfcff),
            (375, 255, 3, 3, 0xd6d8fcff),
            (378, 255, 3, 3, 0xd3cdfbff),
            (381, 255, 3, 3, 0xccc5fbff),
            (0, 258, 3, 3, 0x1941c5ff),
            (3, 258, 3, 3, 0x1840c6ff),
            (6, 258, 3, 3, 0x1840c6ff),
            (9, 258, 3, 3, 0x1840c5ff),
            (12, 258, 3, 3, 0x183fc6ff),
            (15, 258, 3, 3, 0x1941c6ff),
            (18, 258, 3, 3, 0x1941c7ff),
            (363, 258, 3, 3, 0xcfcffbff),
            (366, 258, 3, 3, 0xdbd9fcff),
            (369, 258, 3, 3, 0xdcddfcff),
            (372, 258, 3, 3, 0xd9dbfcff),
            (375, 258, 3, 3, 0xd5d4fcff),
            (378, 258, 3, 3, 0xcfcafbff),
            (381, 258, 3, 3, 0xc8c0faff),
            (0, 261, 3, 3, 0x1942c5ff),
            (3, 261, 3, 3, 0x1942c5ff),
            (6, 261, 3, 3, 0x1942c5ff),
            (9, 261, 3, 3, 0x1942c5ff),
            (12, 261, 3, 3, 0x1a43c6ff),
            (15, 261, 3, 3, 0x1941c6ff),
            (18, 261, 3, 3, 0x1942c5ff),
            (363, 261, 3, 3, 0xdedcfcff),
            (366, 261, 3, 3, 0xdeddfdff),
            (369, 261, 3, 3, 0xdbddfcff),
            (372, 261, 3, 3, 0xd6dafcff),
            (375, 261, 3, 3, 0xd1cffbff),
            (378, 261, 3, 3, 0xcbc6fbff),
            (381, 261, 3, 3, 0xc4befaff),
            (0, 264, 3, 3, 0x1942c5ff),
            (3, 264, 3, 3, 0x1942c4ff),
            (6, 264, 3, 3, 0x1942c4ff),
            (9, 264, 3, 3, 0x1942c5ff),
            (12, 264, 3, 3, 0x1a43c5ff),
            (15, 264, 3, 3, 0x1a43c6ff),
            (18, 264, 3, 3, 0x1a43c5ff),
            (363, 264, 3, 3, 0xe2dffdff),
            (366, 264, 3, 3, 0xdeddfcff),
            (369, 264, 3, 3, 0xd9dbfcff),
            (372, 264, 3, 3, 0xd1d4fcff),
            (375, 264, 3, 3, 0xcdc9fbff),
            (378, 264, 3, 3, 0xc7c1fbff),
            (381, 264, 3, 3, 0xc0bcfaff),
            (0, 267, 3, 3, 0x1942c4ff),
            (3, 267, 3, 3, 0x1942c4ff),
            (6, 267, 3, 3, 0x1941c4ff),
            (9, 267, 3, 3, 0x1942c5ff),
            (12, 267, 3, 3, 0x1942c5ff),
            (15, 267, 3, 3, 0x1a43c5ff),
            (18, 267, 3, 3, 0x1a43c5ff),
            (363, 267, 3, 3, 0xe0dffdff),
            (366, 267, 3, 3, 0xd9dbfcff),
            (369, 267, 3, 3, 0xd1d6fcff),
            (372, 267, 3, 3, 0xcccefbff),
            (375, 267, 3, 3, 0xcac8fbff),
            (378, 267, 3, 3, 0xc5bffaff),
            (381, 267, 3, 3, 0xbfbbfaff),
            (0, 270, 3, 3, 0x1941c3ff),
            (3, 270, 3, 3, 0x1941c3ff),
            (6, 270, 3, 3, 0x1941c4ff),
            (9, 270, 3, 3, 0x1840c4ff),
            (12, 270, 3, 3, 0x1941c4ff),
            (15, 270, 3, 3, 0x1941c5ff),
            (18, 270, 3, 3, 0x1942c4ff),
            (363, 270, 3, 3, 0xdcddfcff),
            (366, 270, 3, 3, 0xd3d8fcff),
            (369, 270, 3, 3, 0xced3fcff),
            (372, 270, 3, 3, 0xc9cbfbff),
            (375, 270, 3, 3, 0xc6c5fbff),
            (378, 270, 3, 3, 0xc2bdfaff),
            (381, 270, 3, 3, 0xbebafaff),
            (0, 273, 3, 3, 0x1840c2ff),
            (3, 273, 3, 3, 0x1840c3ff),
            (6, 273, 3, 3, 0x1840c3ff),
            (9, 273, 3, 3, 0x1840c3ff),
            (12, 273, 3, 3, 0x1941c3ff),
            (15, 273, 3, 3, 0x1840c3ff),
            (18, 273, 3, 3, 0x1941c4ff),
            (363, 273, 3, 3, 0xd8dbfcff),
            (366, 273, 3, 3, 0xd0d5fcff),
            (369, 273, 3, 3, 0xcccefbff),
            (372, 273, 3, 3, 0xc6c7fbff),
            (375, 273, 3, 3, 0xc5c2fbff),
            (378, 273, 3, 3, 0xc0bafaff),
            (381, 273, 3, 3, 0xbab6faff),
            (0, 276, 3, 3, 0x1840c4ff),
            (3, 276, 3, 3, 0x1840c3ff),
            (6, 276, 3, 3, 0x1840c2ff),
            (9, 276, 3, 3, 0x1840c2ff),
            (12, 276, 3, 3, 0x1941c2ff),
            (15, 276, 3, 3, 0x1941c3ff),
            (18, 276, 3, 3, 0x1941c3ff),
            (363, 276, 3, 3, 0xd1d7fcff),
            (366, 276, 3, 3, 0xcfd4fcff),
            (369, 276, 3, 3, 0xccccfbff),
            (372, 276, 3, 3, 0xc6c5fbff),
            (375, 276, 3, 3, 0xc2c0faff),
            (378, 276, 3, 3, 0xbdbafaff),
            (381, 276, 3, 3, 0xb7b3faff),
            (0, 279, 3, 3, 0x1840c2ff),
            (3, 279, 3, 3, 0x183fc1ff),
            (6, 279, 3, 3, 0x1840c1ff),
            (9, 279, 3, 3, 0x1941c1ff),
            (12, 279, 3, 3, 0x1941c2ff),
            (15, 279, 3, 3, 0x1941c3ff),
            (18, 279, 3, 3, 0x1840c1ff),
            (363, 279, 3, 3, 0xd1d5fcff),
            (366, 279, 3, 3, 0xcdd1fbff),
            (369, 279, 3, 3, 0xc8c8fbff),
            (372, 279, 3, 3, 0xc3c3fbff),
            (375, 279, 3, 3, 0xc0befaff),
            (378, 279, 3, 3, 0xbab7faff),
            (381, 279, 3, 3, 0xb4b0faff),
            (0, 282, 3, 3, 0x1840c1ff),
            (3, 282, 3, 3, 0x1840c0ff),
            (6, 282, 3, 3, 0x183fc1ff),
            (9, 282, 3, 3, 0x1941c1ff),
            (12, 282, 3, 3, 0x1840c0ff),
            (15, 282, 3, 3, 0x1941c0ff),
            (18, 282, 3, 3, 0x1942c0ff),
            (363, 282, 3, 3, 0xd2d6fcff),
            (366, 282, 3, 3, 0xcacdfbff),
            (369, 282, 3, 3, 0xc4c5fbff),
            (372, 282, 3, 3, 0xc0c0faff),
            (375, 282, 3, 3, 0xbcbbfaff),
            (378, 282, 3, 3, 0xb7b4faff),
            (381, 282, 3, 3, 0xb1aff9ff),
            (0, 285, 3, 3, 0x1840c1ff),
            (3, 285, 3, 3, 0x1840c0ff),
            (6, 285, 3, 3, 0x183fc1ff),
            (9, 285, 3, 3, 0x1840c0ff),
            (12, 285, 3, 3, 0x1840bfff),
            (15, 285, 3, 3, 0x1942c1ff),
            (18, 285, 3, 3, 0x1942c0ff),
            (363, 285, 3, 3, 0xcbd1fbff),
            (366, 285, 3, 3, 0xc6cafbff),
            (369, 285, 3, 3, 0xbfc2fbff),
            (372, 285, 3, 3, 0xbdbdfaff),
            (375, 285, 3, 3, 0xb8b8faff),
            (378, 285, 3, 3, 0xb3b3faff),
            (381, 285, 3, 3, 0xadacf9ff),
            (0, 288, 3, 3, 0x1840bfff),
            (3, 288, 3, 3, 0x1840c0ff),
            (6, 288, 3, 3, 0x1840c0ff),
            (9, 288, 3, 3, 0x1840bfff),
            (12, 288, 3, 3, 0x1941c0ff),
            (15, 288, 3, 3, 0x1941c0ff),
            (18, 288, 3, 3, 0x1942c0ff),
            (363, 288, 3, 3, 0xc6cbfbff),
            (366, 288, 3, 3, 0xc1c6fbff),
            (369, 288, 3, 3, 0xbcbffaff),
            (372, 288, 3, 3, 0xbabbfaff),
            (375, 288, 3, 3, 0xb5b5faff),
            (378, 288, 3, 3, 0xaeaff9ff),
            (381, 288, 3, 3, 0xabaaf9ff),
            (0, 291, 3, 3, 0x1840c0ff),
            (3, 291, 3, 3, 0x1941c1ff),
            (6, 291, 3, 3, 0x1941c0ff),
            (9, 291, 3, 3, 0x1942c1ff),
            (12, 291, 3, 3, 0x1942c0ff),
            (15, 291, 3, 3, 0x1941c0ff),
            (18, 291, 3, 3, 0x1a42c1ff),
            (363, 291, 3, 3, 0xc2c7fbff),
            (366, 291, 3, 3, 0xbdc0faff),
            (369, 291, 3, 3, 0xb9bbfaff),
            (372, 291, 3, 3, 0xb8b9faff),
            (375, 291, 3, 3, 0xb2b3faff),
            (378, 291, 3, 3, 0xabacf9ff),
            (381, 291, 3, 3, 0xa7a8f9ff),
            (0, 294, 3, 3, 0x1941c0ff),
            (3, 294, 3, 3, 0x1942c1ff),
            (6, 294, 3, 3, 0x1941c1ff),
            (9, 294, 3, 3, 0x1a43c0ff),
            (12, 294, 3, 3, 0x1a43bfff),
            (15, 294, 3, 3, 0x1a43c0ff),
            (18, 294, 3, 3, 0x1a43bfff),
            (363, 294, 3, 3, 0xbdc2faff),
            (366, 294, 3, 3, 0xb7bcfaff),
            (369, 294, 3, 3, 0xb6bafaff),
            (372, 294, 3, 3, 0xb2b4faff),
            (375, 294, 3, 3, 0xaeaef9ff),
            (378, 294, 3, 3, 0xa6a7f9ff),
            (381, 294, 3, 3, 0xa5a5f9ff),
            (0, 297, 3, 3, 0x1941c0ff),
            (3, 297, 3, 3, 0x1942c1ff),
            (6, 297, 3, 3, 0x1a44c2ff),
            (9, 297, 3, 3, 0x1a44c0ff),
            (12, 297, 3, 3, 0x1a44c0ff),
            (15, 297, 3, 3, 0x1a44bfff),
            (18, 297, 3, 3, 0x1a44bfff),
            (363, 297, 3, 3, 0xb5bffaff),
            (366, 297, 3, 3, 0xb3bafaff),
            (369, 297, 3, 3, 0xb1b6faff),
            (372, 297, 3, 3, 0xaeb0f9ff),
            (375, 297, 3, 3, 0xa8abf9ff),
            (378, 297, 3, 3, 0xa3a6f9ff),
            (381, 297, 3, 3, 0xa0a2f9ff),
            (0, 300, 3, 3, 0x1a43c0ff),
            (3, 300, 3, 3, 0x1a43c1ff),
            (6, 300, 3, 3, 0x1b45c1ff),
            (9, 300, 3, 3, 0x1b45c1ff),
            (12, 300, 3, 3, 0x1b45c0ff),
            (15, 300, 3, 3, 0x1b46c0ff),
            (18, 300, 3, 3, 0x1b45c0ff),
            (363, 300, 3, 3, 0xb2bbfaff),
            (366, 300, 3, 3, 0xafb5faff),
            (369, 300, 3, 3, 0xacb0f9ff),
            (372, 300, 3, 3, 0xa9acf9ff),
            (375, 300, 3, 3, 0xa3a7f9ff),
            (378, 300, 3, 3, 0x9fa2f9ff),
            (381, 300, 3, 3, 0x9d9ef8ff),
            (0, 303, 3, 3, 0x1942c0ff),
            (3, 303, 3, 3, 0x1a43c1ff),
            (6, 303, 3, 3, 0x1b45c3ff),
            (9, 303, 3, 3, 0x1b45c2ff),
            (12, 303, 3, 3, 0x1b45c1ff),
            (15, 303, 3, 3, 0x1b46c1ff),
            (18, 303, 3, 3, 0x1b46c0ff),
            (363, 303, 3, 3, 0xadb6faff),
            (366, 303, 3, 3, 0xabb0f9ff),
            (369, 303, 3, 3, 0xa7acf9ff),
            (372, 303, 3, 3, 0xa4a8f9ff),
            (375, 303, 3, 3, 0xa0a5f9ff),
            (378, 303, 3, 3, 0x9c9ff9ff),
            (381, 303, 3, 3, 0x9a9cf8ff),
            (0, 306, 3, 3, 0x1a43c1ff),
            (3, 306, 3, 3, 0x1b45c2ff),
            (6, 306, 3, 3, 0x1b46c2ff),
            (9, 306, 3, 3, 0x1b46c1ff),
            (12, 306, 3, 3, 0x1b46c1ff),
            (15, 306, 3, 3, 0x1c47c2ff),
            (18, 306, 3, 3, 0x1c47c1ff),
            (363, 306, 3, 3, 0xa9b3f9ff),
            (366, 306, 3, 3, 0xa6adf9ff),
            (369, 306, 3, 3, 0xa4a9f9ff),
            (372, 306, 3, 3, 0xa0a5f9ff),
            (375, 306, 3, 3, 0x9ca1f9ff),
            (378, 306, 3, 3, 0x979cf8ff),
            (381, 306, 3, 3, 0x9496f8ff),
            (0, 309, 3, 3, 0x1a44c1ff),
            (3, 309, 3, 3, 0x1b45c2ff),
            (6, 309, 3, 3, 0x1b46c2ff),
            (9, 309, 3, 3, 0x1b46c1ff),
            (12, 309, 3, 3, 0x1b46c2ff),
            (15, 309, 3, 3, 0x1c48c3ff),
            (18, 309, 3, 3, 0x1d49c1ff),
            (363, 309, 3, 3, 0xa6b0f9ff),
            (366, 309, 3, 3, 0xa3adf9ff),
            (369, 309, 3, 3, 0x9fa7f9ff),
            (372, 309, 3, 3, 0x9ba2f9ff),
            (375, 309, 3, 3, 0x989ef8ff),
            (378, 309, 3, 3, 0x9497f8ff),
            (381, 309, 3, 3, 0x8e91f8ff),
            (0, 312, 3, 3, 0x1a43c1ff),
            (3, 312, 3, 3, 0x1b45c2ff),
            (6, 312, 3, 3, 0x1c47c2ff),
            (9, 312, 3, 3, 0x1c47c4ff),
            (12, 312, 3, 3, 0x1c47c4ff),
            (15, 312, 3, 3, 0x1d49c3ff),
            (18, 312, 3, 3, 0x1d49c2ff),
            (363, 312, 3, 3, 0x9faaf9ff),
            (366, 312, 3, 3, 0xa0a7f9ff),
            (369, 312, 3, 3, 0x9da4f9ff),
            (372, 312, 3, 3, 0x989ef8ff),
            (375, 312, 3, 3, 0x939af8ff),
            (378, 312, 3, 3, 0x9093f8ff),
            (381, 312, 3, 3, 0x898df8ff),
            (0, 315, 3, 3, 0x1a44c1ff),
            (3, 315, 3, 3, 0x1b45c3ff),
            (6, 315, 3, 3, 0x1c47c4ff),
            (9, 315, 3, 3, 0x1c48c4ff),
            (12, 315, 3, 3, 0x1d4ac4ff),
            (15, 315, 3, 3, 0x1d4ac4ff),
            (18, 315, 3, 3, 0x1d4ac3ff),
            (363, 315, 3, 3, 0x9aa7f9ff),
            (366, 315, 3, 3, 0x9aa3f9ff),
            (369, 315, 3, 3, 0x979ff8ff),
            (372, 315, 3, 3, 0x939bf8ff),
            (375, 315, 3, 3, 0x8e96f8ff),
            (378, 315, 3, 3, 0x8990f8ff),
            (381, 315, 3, 3, 0x858af8ff),
            (0, 318, 3, 3, 0x1b46c2ff),
            (3, 318, 3, 3, 0x1c48c3ff),
            (6, 318, 3, 3, 0x1d49c4ff),
            (9, 318, 3, 3, 0x1d4ac5ff),
            (12, 318, 3, 3, 0x1e4bc4ff),
            (15, 318, 3, 3, 0x1e4bc4ff),
            (18, 318, 3, 3, 0x1e4cc4ff),
            (21, 318, 3, 3, 0x1e4cc3ff),
            (24, 318, 3, 3, 0x1f4dc3ff),
            (27, 318, 3, 3, 0xe2e5eaff),
            (30, 318, 3, 3, 0xf0f2f8ff),
            (33, 318, 3, 3, 0xf4f6fcff),
            (36, 318, 3, 3, 0xf4f6fcff),
            (39, 318, 3, 3, 0xf4f6fbff),
            (42, 318, 3, 3, 0xf4f6fbff),
            (45, 318, 3, 3, 0xf3f6fbff),
            (48, 318, 3, 3, 0xf3f6fbff),
            (51, 318, 3, 3, 0xf3f6fbff),
            (54, 318, 3, 3, 0xf3f6fbff),
            (57, 318, 3, 3, 0xf3f6fbff),
            (60, 318, 3, 3, 0xf3f6fbff),
            (63, 318, 3, 3, 0xf3f5fbff),
            (66, 318, 3, 3, 0xf3f5faff),
            (69, 318, 3, 3, 0xf3f5faff),
            (72, 318, 3, 3, 0xf3f5faff),
            (75, 318, 3, 3, 0xf3f5faff),
            (78, 318, 3, 3, 0xf3f5faff),
            (81, 318, 3, 3, 0xf3f5faff),
            (84, 318, 3, 3, 0xf3f5faff),
            (87, 318, 3, 3, 0xf3f5faff),
            (90, 318, 3, 3, 0xf3f5fbff),
            (93, 318, 3, 3, 0xf3f5fbff),
            (96, 318, 3, 3, 0xf3f6fbff),
            (99, 318, 3, 3, 0xf3f6fcff),
            (102, 318, 3, 3, 0xf4f6fdff),
            (105, 318, 3, 3, 0xf5f6fdff),
            (108, 318, 3, 3, 0xf5f6feff),
            (111, 318, 3, 3, 0xf6f6feff),
            (114, 318, 3, 3, 0xf6f6feff),
            (117, 318, 3, 3, 0xf6f6feff),
            (120, 318, 3, 3, 0xf6f6feff),
            (123, 318, 3, 3, 0xf6f6feff),
            (126, 318, 3, 3, 0xf6f6feff),
            (129, 318, 3, 3, 0xf6f6feff),
            (132, 318, 3, 3, 0xf6f6feff),
            (135, 318, 3, 3, 0xf6f6feff),
            (138, 318, 3, 3, 0xf6f6feff),
            (141, 318, 3, 3, 0xf6f6feff),
            (144, 318, 3, 3, 0xf6f6feff),
            (147, 318, 3, 3, 0xf6f6feff),
            (150, 318, 3, 3, 0xf7f6feff),
            (153, 318, 3, 3, 0xf7f6feff),
            (156, 318, 3, 3, 0xf7f6feff),
            (159, 318, 3, 3, 0xf7f7feff),
            (162, 318, 3, 3, 0xf7f7feff),
            (165, 318, 3, 3, 0xf7f7feff),
            (168, 318, 3, 3, 0xf7f7feff),
            (171, 318, 3, 3, 0xf7f7feff),
            (174, 318, 3, 3, 0xf8f7feff),
            (177, 318, 3, 3, 0xf8f7feff),
            (180, 318, 3, 3, 0xf8f7ffff),
            (183, 318, 3, 3, 0xf8f8ffff),
            (186, 318, 3, 3, 0xf8f8ffff),
            (189, 318, 3, 3, 0xf8f8ffff),
            (192, 318, 3, 3, 0xf9f9ffff),
            (195, 318, 3, 3, 0xf9f9ffff),
            (198, 318, 3, 3, 0xfaf9ffff),
            (201, 318, 3, 3, 0xfbfaffff),
            (204, 318, 3, 3, 0xfbfbffff),
            (207, 318, 3, 3, 0xfbfbffff),
            (210, 318, 3, 3, 0xfbfbffff),
            (213, 318, 3, 3, 0xfbfbffff),
            (216, 318, 3, 3, 0xfbfbffff),
            (219, 318, 3, 3, 0xfbfbffff),
            (222, 318, 3, 3, 0xfbfbffff),
            (225, 318, 3, 3, 0xfafaffff),
            (228, 318, 3, 3, 0xfafaffff),
            (231, 318, 3, 3, 0xf9f9ffff),
            (234, 318, 3, 3, 0xf9f9ffff),
            (237, 318, 3, 3, 0xf9f8ffff),
            (240, 318, 3, 3, 0xf8f8ffff),
            (243, 318, 3, 3, 0xf8f8ffff),
            (246, 318, 3, 3, 0xf8f8ffff),
            (249, 318, 3, 3, 0xf8f7feff),
            (252, 318, 3, 3, 0xf7f7feff),
            (255, 318, 3, 3, 0xf7f7feff),
            (258, 318, 3, 3, 0xf7f7feff),
            (261, 318, 3, 3, 0xf7f7feff),
            (264, 318, 3, 3, 0xf7f6feff),
            (267, 318, 3, 3, 0xf7f6feff),
            (270, 318, 3, 3, 0xf7f6feff),
            (273, 318, 3, 3, 0xf7f6feff),
            (276, 318, 3, 3, 0xf7f6feff),
            (279, 318, 3, 3, 0xf7f6feff),
            (282, 318, 3, 3, 0xf7f6feff),
            (285, 318, 3, 3, 0xf7f6feff),
            (288, 318, 3, 3, 0xf7f7feff),
            (291, 318, 3, 3, 0xf7f7feff),
            (294, 318, 3, 3, 0xf7f7feff),
            (297, 318, 3, 3, 0xf7f7feff),
            (300, 318, 3, 3, 0xf7f7feff),
            (303, 318, 3, 3, 0xf7f7feff),
            (306, 318, 3, 3, 0xf8f7feff),
            (309, 318, 3, 3, 0xf8f8ffff),
            (312, 318, 3, 3, 0xf8f8ffff),
            (315, 318, 3, 3, 0xf9f9ffff),
            (318, 318, 3, 3, 0xf9f9ffff),
            (321, 318, 3, 3, 0xf9f9ffff),
            (324, 318, 3, 3, 0xf9f9ffff),
            (327, 318, 3, 3, 0xfcfcffff),
            (330, 318, 3, 3, 0xfcfcffff),
            (333, 318, 3, 3, 0xfcfcffff),
            (336, 318, 3, 3, 0xfcfcffff),
            (339, 318, 3, 3, 0xfbfcffff),
            (342, 318, 3, 3, 0xfbfcffff),
            (345, 318, 3, 3, 0xfbfbffff),
            (348, 318, 3, 3, 0xfbfbffff),
            (351, 318, 3, 3, 0xfbfbffff),
            (354, 318, 3, 3, 0xededf6ff),
            (357, 318, 3, 3, 0xaeb8f9ff),
            (360, 318, 3, 3, 0x97a5f9ff),
            (363, 318, 3, 3, 0x98a3f9ff),
            (366, 318, 3, 3, 0x949ef8ff),
            (369, 318, 3, 3, 0x919af8ff),
            (372, 318, 3, 3, 0x8c96f8ff),
            (375, 318, 3, 3, 0x8993f8ff),
            (378, 318, 3, 3, 0x848cf8ff),
            (381, 318, 3, 3, 0x8187f7ff),
            (0, 321, 3, 3, 0x1c48c5ff),
            (3, 321, 3, 3, 0x1d49c5ff),
            (6, 321, 3, 3, 0x1e4bc6ff),
            (9, 321, 3, 3, 0x1f4dc7ff),
            (12, 321, 3, 3, 0x1f4dc6ff),
            (15, 321, 3, 3, 0x1f4dc5ff),
            (18, 321, 3, 3, 0x1f4ec6ff),
            (21, 321, 3, 3, 0x1f4dc4ff),
            (24, 321, 3, 3, 0x1f4dc3ff),
            (27, 321, 3, 3, 0x1f4ec3ff),
            (30, 321, 3, 3, 0x1f4ec0ff),
            (33, 321, 3, 3, 0x1f4dbcff),
            (36, 321, 3, 3, 0x1e4cbcff),
            (39, 321, 3, 3, 0x1e4cbbff),
            (42, 321, 3, 3, 0x1e4bb9ff),
            (45, 321, 3, 3, 0x1e4bb8ff),
            (48, 321, 3, 3, 0x1d4ab5ff),
            (51, 321, 3, 3, 0x1c48b2ff),
            (54, 321, 3, 3, 0x1c47aeff),
            (57, 321, 3, 3, 0x1b46abff),
            (60, 321, 3, 3, 0x1b46a8ff),
            (63, 321, 3, 3, 0x1b46a6ff),
            (66, 321, 3, 3, 0x1b46a6ff),
            (69, 321, 3, 3, 0x1b45a4ff),
            (72, 321, 3, 3, 0x1a44a3ff),
            (75, 321, 3, 3, 0x1a43a2ff),
            (78, 321, 3, 3, 0x1a43a3ff),
            (81, 321, 3, 3, 0x1942a4ff),
            (84, 321, 3, 3, 0x1942a5ff),
            (87, 321, 3, 3, 0x1942a6ff),
            (90, 321, 3, 3, 0x1942a8ff),
            (93, 321, 3, 3, 0x1a44acff),
            (96, 321, 3, 3, 0x1b45b1ff),
            (99, 321, 3, 3, 0x1c47c5ff),
            (102, 321, 3, 3, 0x334ad8ff),
            (105, 321, 3, 3, 0x3d4fdfff),
            (108, 321, 3, 3, 0x4351e2ff),
            (111, 321, 3, 3, 0x4853e5ff),
            (114, 321, 3, 3, 0x4c54e7ff),
            (117, 321, 3, 3, 0x4e54e8ff),
            (120, 321, 3, 3, 0x4f52e8ff),
            (123, 321, 3, 3, 0x5151e7ff),
            (126, 321, 3, 3, 0x514fe6ff),
            (129, 321, 3, 3, 0x504fe8ff),
            (132, 321, 3, 3, 0x5550e9ff),
            (135, 321, 3, 3, 0x554feaff),
            (138, 321, 3, 3, 0x5651e9ff),
            (141, 321, 3, 3, 0x5651e9ff),
            (144, 321, 3, 3, 0x5752eaff),
            (147, 321, 3, 3, 0x5754ebff),
            (150, 321, 3, 3, 0x5755ebff),
            (153, 321, 3, 3, 0x5856ecff),
            (156, 321, 3, 3, 0x5b59eeff),
            (159, 321, 3, 3, 0x5f5defff),
            (162, 321, 3, 3, 0x6461f1ff),
            (165, 321, 3, 3, 0x6764f2ff),
            (168, 321, 3, 3, 0x6765f4ff),
            (171, 321, 3, 3, 0x6767f4ff),
            (174, 321, 3, 3, 0x6968f4ff),
            (177, 321, 3, 3, 0x6b68f4ff),
            (180, 321, 3, 3, 0x6e6af5ff),
            (183, 321, 3, 3, 0x716df7ff),
            (186, 321, 3, 3, 0x7774f7ff),
            (189, 321, 3, 3, 0x817df7ff),
            (192, 321, 3, 3, 0x8b86f7ff),
            (195, 321, 3, 3, 0x948ef8ff),
            (198, 321, 3, 3, 0xa19bf8ff),
            (201, 321, 3, 3, 0xaca5f9ff),
            (204, 321, 3, 3, 0xb1a9f9ff),
            (207, 321, 3, 3, 0xb3adf9ff),
            (210, 321, 3, 3, 0xb4aff9ff),
            (213, 321, 3, 3, 0xb5b0faff),
            (216, 321, 3, 3, 0xb3aff9ff),
            (219, 321, 3, 3, 0xb1adf9ff),
            (222, 321, 3, 3, 0xaaaaf9ff),
            (225, 321, 3, 3, 0xa5a4f9ff),
            (228, 321, 3, 3, 0x9f9df9ff),
            (231, 321, 3, 3, 0x9693f8ff),
            (234, 321, 3, 3, 0x8c88f8ff),
            (237, 321, 3, 3, 0x8480f7ff),
            (240, 321, 3, 3, 0x7975f7ff),
            (243, 321, 3, 3, 0x7672f6ff),
            (246, 321, 3, 3, 0x7571f6ff),
            (249, 321, 3, 3, 0x6e6bf4ff),
            (252, 321, 3, 3, 0x6966f2ff),
            (255, 321, 3, 3, 0x6663f2ff),
            (258, 321, 3, 3, 0x615ff0ff),
            (261, 321, 3, 3, 0x5f5beeff),
            (264, 321, 3, 3, 0x5d59eeff),
            (267, 321, 3, 3, 0x5a55ecff),
            (270, 321, 3, 3, 0x5955ebff),
            (273, 321, 3, 3, 0x5b56ecff),
            (276, 321, 3, 3, 0x5b56ecff),
            (279, 321, 3, 3, 0x5d57edff),
            (282, 321, 3, 3, 0x5c57edff),
            (285, 321, 3, 3, 0x5c59edff),
            (288, 321, 3, 3, 0x5d5aecff),
            (291, 321, 3, 3, 0x5f5becff),
            (294, 321, 3, 3, 0x615eecff),
            (297, 321, 3, 3, 0x6460eeff),
            (300, 321, 3, 3, 0x6763f1ff),
            (303, 321, 3, 3, 0x6c69f4ff),
            (306, 321, 3, 3, 0x6e6cf4ff),
            (309, 321, 3, 3, 0x7773f6ff),
            (312, 321, 3, 3, 0x7c78f7ff),
            (315, 321, 3, 3, 0x817ff7ff),
            (318, 321, 3, 3, 0x8684f7ff),
            (321, 321, 3, 3, 0x898af8ff),
            (324, 321, 3, 3, 0xb5b1f9ff),
            (327, 321, 3, 3, 0xcbc7fbff),
            (330, 321, 3, 3, 0xc8c7fbff),
            (333, 321, 3, 3, 0xc2c5fbff),
            (336, 321, 3, 3, 0xbbc0faff),
            (339, 321, 3, 3, 0xb7befaff),
            (342, 321, 3, 3, 0xaeb8faff),
            (345, 321, 3, 3, 0xa7b3f9ff),
            (348, 321, 3, 3, 0xa7b3f9ff),
            (351, 321, 3, 3, 0xa2aff9ff),
            (354, 321, 3, 3, 0x9ba9f9ff),
            (357, 321, 3, 3, 0x96a5f9ff),
            (360, 321, 3, 3, 0x94a2f9ff),
            (363, 321, 3, 3, 0x94a0f8ff),
            (366, 321, 3, 3, 0x909bf8ff),
            (369, 321, 3, 3, 0x8c97f8ff),
            (372, 321, 3, 3, 0x8994f8ff),
            (375, 321, 3, 3, 0x858ff8ff),
            (378, 321, 3, 3, 0x8189f7ff),
            (381, 321, 3, 3, 0x7c82f6ff),
            (0, 324, 3, 3, 0x1d4ac6ff),
            (3, 324, 3, 3, 0x1e4cc6ff),
            (6, 324, 3, 3, 0x1f4dc7ff),
            (9, 324, 3, 3, 0x1f4dc8ff),
            (12, 324, 3, 3, 0x1f4ec7ff),
            (15, 324, 3, 3, 0x2050c7ff),
            (18, 324, 3, 3, 0x2151c6ff),
            (21, 324, 3, 3, 0x2050c6ff),
            (24, 324, 3, 3, 0x204fc5ff),
            (27, 324, 3, 3, 0x204fc3ff),
            (30, 324, 3, 3, 0x204fc1ff),
            (33, 324, 3, 3, 0x1f4ebeff),
            (36, 324, 3, 3, 0x1f4ebdff),
            (39, 324, 3, 3, 0x1f4ebcff),
            (42, 324, 3, 3, 0x1e4dbaff),
            (45, 324, 3, 3, 0x1e4cb8ff),
            (48, 324, 3, 3, 0x1e4bb5ff),
            (51, 324, 3, 3, 0x1d4ab2ff),
            (54, 324, 3, 3, 0x1d49afff),
            (57, 324, 3, 3, 0x1c47abff),
            (60, 324, 3, 3, 0x1c47a8ff),
            (63, 324, 3, 3, 0x1b46a6ff),
            (66, 324, 3, 3, 0x1b46a5ff),
            (69, 324, 3, 3, 0x1b46a5ff),
            (72, 324, 3, 3, 0x1a44a3ff),
            (75, 324, 3, 3, 0x1a44a3ff),
            (78, 324, 3, 3, 0x1a44a3ff),
            (81, 324, 3, 3, 0x1a43a3ff),
            (84, 324, 3, 3, 0x1a43a5ff),
            (87, 324, 3, 3, 0x1a43a5ff),
            (90, 324, 3, 3, 0x1a43a8ff),
            (93, 324, 3, 3, 0x1b45adff),
            (96, 324, 3, 3, 0x1b46b2ff),
            (99, 324, 3, 3, 0x1c47c2ff),
            (102, 324, 3, 3, 0x314ad7ff),
            (105, 324, 3, 3, 0x3d4eddff),
            (108, 324, 3, 3, 0x4553e2ff),
            (111, 324, 3, 3, 0x4b55e6ff),
            (114, 324, 3, 3, 0x4e55e8ff),
            (117, 324, 3, 3, 0x4e55e8ff),
            (120, 324, 3, 3, 0x5254e8ff),
            (123, 324, 3, 3, 0x5152e8ff),
            (126, 324, 3, 3, 0x5151e8ff),
            (129, 324, 3, 3, 0x5151e9ff),
            (132, 324, 3, 3, 0x5352e9ff),
            (135, 324, 3, 3, 0x5552e9ff),
            (138, 324, 3, 3, 0x5552e9ff),
            (141, 324, 3, 3, 0x5552e9ff),
            (144, 324, 3, 3, 0x5453eaff),
            (147, 324, 3, 3, 0x5453eaff),
            (150, 324, 3, 3, 0x5553eaff),
            (153, 324, 3, 3, 0x5555eaff),
            (156, 324, 3, 3, 0x5958edff),
            (159, 324, 3, 3, 0x5f5df0ff),
            (162, 324, 3, 3, 0x6261f1ff),
            (165, 324, 3, 3, 0x6463f1ff),
            (168, 324, 3, 3, 0x6464f1ff),
            (171, 324, 3, 3, 0x6465f2ff),
            (174, 324, 3, 3, 0x6566f3ff),
            (177, 324, 3, 3, 0x6867f4ff),
            (180, 324, 3, 3, 0x6b69f5ff),
            (183, 324, 3, 3, 0x706df6ff),
            (186, 324, 3, 3, 0x7673f7ff),
            (189, 324, 3, 3, 0x837ff7ff),
            (192, 324, 3, 3, 0x8c88f8ff),
            (195, 324, 3, 3, 0x9994f8ff),
            (198, 324, 3, 3, 0xa59ff9ff),
            (201, 324, 3, 3, 0xaea8f9ff),
            (204, 324, 3, 3, 0xb1abf9ff),
            (207, 324, 3, 3, 0xb3adf9ff),
            (210, 324, 3, 3, 0xb5b0faff),
            (213, 324, 3, 3, 0xb5b0faff),
            (216, 324, 3, 3, 0xb4b0f9ff),
            (219, 324, 3, 3, 0xb1aef9ff),
            (222, 324, 3, 3, 0xaaaaf9ff),
            (225, 324, 3, 3, 0xa5a3f9ff),
            (228, 324, 3, 3, 0x9d9df9ff),
            (231, 324, 3, 3, 0x9494f8ff),
            (234, 324, 3, 3, 0x8a89f8ff),
            (237, 324, 3, 3, 0x837ff7ff),
            (240, 324, 3, 3, 0x7874f7ff),
            (243, 324, 3, 3, 0x726ff5ff),
            (246, 324, 3, 3, 0x726df4ff),
            (249, 324, 3, 3, 0x6d69f4ff),
            (252, 324, 3, 3, 0x6764f2ff),
            (255, 324, 3, 3, 0x6362f1ff),
            (258, 324, 3, 3, 0x605defff),
            (261, 324, 3, 3, 0x5e5aeeff),
            (264, 324, 3, 3, 0x5c59edff),
            (267, 324, 3, 3, 0x5a55ecff),
            (270, 324, 3, 3, 0x5955ebff),
            (273, 324, 3, 3, 0x5a56ecff),
            (276, 324, 3, 3, 0x5a56ebff),
            (279, 324, 3, 3, 0x5b57ebff),
            (282, 324, 3, 3, 0x5a57ebff),
            (285, 324, 3, 3, 0x5a59ebff),
            (288, 324, 3, 3, 0x5d5aebff),
            (291, 324, 3, 3, 0x5e5becff),
            (294, 324, 3, 3, 0x5f5dedff),
            (297, 324, 3, 3, 0x6260eeff),
            (300, 324, 3, 3, 0x6664f1ff),
            (303, 324, 3, 3, 0x6c69f5ff),
            (306, 324, 3, 3, 0x6e6cf5ff),
            (309, 324, 3, 3, 0x7673f6ff),
            (312, 324, 3, 3, 0x7977f7ff),
            (315, 324, 3, 3, 0x7c7cf7ff),
            (318, 324, 3, 3, 0x8181f7ff),
            (321, 324, 3, 3, 0x8889f8ff),
            (324, 324, 3, 3, 0xc6c3fbff),
            (327, 324, 3, 3, 0xc7c5fbff),
            (330, 324, 3, 3, 0xc4c5fbff),
            (333, 324, 3, 3, 0xbfc1faff),
            (336, 324, 3, 3, 0xbabffaff),
            (339, 324, 3, 3, 0xb0b8faff),
            (342, 324, 3, 3, 0xa7b2f9ff),
            (345, 324, 3, 3, 0xa5b1f9ff),
            (348, 324, 3, 3, 0xa1adf9ff),
            (351, 324, 3, 3, 0x9da9f9ff),
            (354, 324, 3, 3, 0x97a5f9ff),
            (357, 324, 3, 3, 0x919ff8ff),
            (360, 324, 3, 3, 0x919ff8ff),
            (363, 324, 3, 3, 0x909df8ff),
            (366, 324, 3, 3, 0x8c98f8ff),
            (369, 324, 3, 3, 0x8794f8ff),
            (372, 324, 3, 3, 0x8691f8ff),
            (375, 324, 3, 3, 0x818df8ff),
            (378, 324, 3, 3, 0x7d84f7ff),
            (381, 324, 3, 3, 0x767ff5ff),
            (0, 327, 3, 3, 0x1e4cc6ff),
            (3, 327, 3, 3, 0x1f4dc7ff),
            (6, 327, 3, 3, 0x1f4ec7ff),
            (9, 327, 3, 3, 0x204fc7ff),
            (12, 327, 3, 3, 0x2050c7ff),
            (15, 327, 3, 3, 0x2151c8ff),
            (18, 327, 3, 3, 0x2152c9ff),
            (21, 327, 3, 3, 0x2151c6ff),
            (24, 327, 3, 3, 0x2050c6ff),
            (27, 327, 3, 3, 0x2050c3ff),
            (30, 327, 3, 3, 0x2150c2ff),
            (33, 327, 3, 3, 0x2050c0ff),
            (36, 327, 3, 3, 0x2050bfff),
            (39, 327, 3, 3, 0x204fbdff),
            (42, 327, 3, 3, 0x204fbcff),
            (45, 327, 3, 3, 0x1f4db9ff),
            (48, 327, 3, 3, 0x1e4cb5ff),
            (51, 327, 3, 3, 0x1e4bb4ff),
            (54, 327, 3, 3, 0x1d4ab0ff),
            (57, 327, 3, 3, 0x1d49adff),
            (60, 327, 3, 3, 0x1c48a8ff),
            (63, 327, 3, 3, 0x1c47a6ff),
            (66, 327, 3, 3, 0x1b46a5ff),
            (69, 327, 3, 3, 0x1b46a4ff),
            (72, 327, 3, 3, 0x1b45a3ff),
            (75, 327, 3, 3, 0x1b45a2ff),
            (78, 327, 3, 3, 0x1b45a3ff),
            (81, 327, 3, 3, 0x1b45a3ff),
            (84, 327, 3, 3, 0x1a44a3ff),
            (87, 327, 3, 3, 0x1a44a4ff),
            (90, 327, 3, 3, 0x1b45a7ff),
            (93, 327, 3, 3, 0x1b46acff),
            (96, 327, 3, 3, 0x1c47b1ff),
            (99, 327, 3, 3, 0x1c47bdff),
            (102, 327, 3, 3, 0x2d4ad3ff),
            (105, 327, 3, 3, 0x3d4edeff),
            (108, 327, 3, 3, 0x4754e3ff),
            (111, 327, 3, 3, 0x4d56e6ff),
            (114, 327, 3, 3, 0x4e57e8ff),
            (117, 327, 3, 3, 0x5056e9ff),
            (120, 327, 3, 3, 0x5256e9ff),
            (123, 327, 3, 3, 0x5355e9ff),
            (126, 327, 3, 3, 0x5252e9ff),
            (129, 327, 3, 3, 0x5353e8ff),
            (132, 327, 3, 3, 0x5353e8ff),
            (135, 327, 3, 3, 0x5352e8ff),
            (138, 327, 3, 3, 0x5452e9ff),
            (141, 327, 3, 3, 0x5252e9ff),
            (144, 327, 3, 3, 0x5352e9ff),
            (147, 327, 3, 3, 0x5152e8ff),
            (150, 327, 3, 3, 0x5152e9ff),
            (153, 327, 3, 3, 0x5455ebff),
            (156, 327, 3, 3, 0x595bedff),
            (159, 327, 3, 3, 0x6060f0ff),
            (162, 327, 3, 3, 0x6062f1ff),
            (165, 327, 3, 3, 0x6162f1ff),
            (168, 327, 3, 3, 0x6162f0ff),
            (171, 327, 3, 3, 0x6263f1ff),
            (174, 327, 3, 3, 0x6264f2ff),
            (177, 327, 3, 3, 0xc9cafbff),
            (180, 327, 3, 3, 0x696af4ff),
            (183, 327, 3, 3, 0x6e6ef5ff),
            (186, 327, 3, 3, 0xa6a5faff),
            (189, 327, 3, 3, 0x8481f7ff),
            (192, 327, 3, 3, 0x8e89f8ff),
            (195, 327, 3, 3, 0xbfbbfbff),
            (198, 327, 3, 3, 0xa7a2f9ff),
            (201, 327, 3, 3, 0xaea8f9ff),
            (204, 327, 3, 3, 0xcbc7fbff),
            (207, 327, 3, 3, 0xb2adf9ff),
            (210, 327, 3, 3, 0xb4b0faff),
            (213, 327, 3, 3, 0xb5b1faff),
            (216, 327, 3, 3, 0xb4b0faff),
            (219, 327, 3, 3, 0xafaef9ff),
            (222, 327, 3, 3, 0xaaaaf9ff),
            (225, 327, 3, 3, 0xa3a4f9ff),
            (228, 327, 3, 3, 0x9b9cf8ff),
            (231, 327, 3, 3, 0x9393f8ff),
            (234, 327, 3, 3, 0x8889f8ff),
            (237, 327, 3, 3, 0x807ff7ff),
            (240, 327, 3, 3, 0x7674f7ff),
            (243, 327, 3, 3, 0x716ef5ff),
            (246, 327, 3, 3, 0x6f6cf3ff),
            (249, 327, 3, 3, 0x6b68f3ff),
            (252, 327, 3, 3, 0x6463f1ff),
            (255, 327, 3, 3, 0x6361f0ff),
            (258, 327, 3, 3, 0x615deeff),
            (261, 327, 3, 3, 0x5c58edff),
            (264, 327, 3, 3, 0x5a56ebff),
            (267, 327, 3, 3, 0x5955ebff),
            (270, 327, 3, 3, 0x5855ebff),
            (273, 327, 3, 3, 0x5956ecff),
            (276, 327, 3, 3, 0x5a56e9ff),
            (279, 327, 3, 3, 0x5856e9ff),
            (282, 327, 3, 3, 0x5956e9ff),
            (285, 327, 3, 3, 0x5a58eaff),
            (288, 327, 3, 3, 0x5b5aebff),
            (291, 327, 3, 3, 0x5d5becff),
            (294, 327, 3, 3, 0x5d5dedff),
            (297, 327, 3, 3, 0x605eedff),
            (300, 327, 3, 3, 0x6664f0ff),
            (303, 327, 3, 3, 0x6c6af3ff),
            (306, 327, 3, 3, 0x706ef6ff),
            (309, 327, 3, 3, 0x7472f6ff),
            (312, 327, 3, 3, 0x7776f7ff),
            (315, 327, 3, 3, 0x7a7bf7ff),
            (318, 327, 3, 3, 0x7f82f7ff),
            (321, 327, 3, 3, 0xa4a4f9ff),
            (324, 327, 3, 3, 0xc4c1faff),
            (327, 327, 3, 3, 0xc5c2fbff),
            (330, 327, 3, 3, 0xbfc0faff),
            (333, 327, 3, 3, 0xbcbffaff),
            (336, 327, 3, 3, 0xb4b8faff),
            (339, 327, 3, 3, 0xa9b3faff),
            (342, 327, 3, 3, 0xa4b0f9ff),
            (345, 327, 3, 3, 0xa0acf9ff),
            (348, 327, 3, 3, 0x9ba9f9ff),
            (351, 327, 3, 3, 0x96a5f9ff),
            (354, 327, 3, 3, 0x92a1f8ff),
            (357, 327, 3, 3, 0x8f9df8ff),
            (360, 327, 3, 3, 0x8e9bf8ff),
            (363, 327, 3, 3, 0x8b99f8ff),
            (366, 327, 3, 3, 0x8895f8ff),
            (369, 327, 3, 3, 0x8492f8ff),
            (372, 327, 3, 3, 0x828df8ff),
            (375, 327, 3, 3, 0x7d89f7ff),
            (378, 327, 3, 3, 0x7882f5ff),
            (381, 327, 3, 3, 0x747ff4ff),
            (0, 330, 3, 3, 0x1f4ec7ff),
            (3, 330, 3, 3, 0x1f4dc7ff),
            (6, 330, 3, 3, 0x1f4ec7ff),
            (9, 330, 3, 3, 0x2050c7ff),
            (12, 330, 3, 3, 0x2151c7ff),
            (15, 330, 3, 3, 0x2152c8ff),
            (18, 330, 3, 3, 0x2152c8ff),
            (21, 330, 3, 3, 0x2152c7ff),
            (24, 330, 3, 3, 0x2151c6ff),
            (27, 330, 3, 3, 0x2152c4ff),
            (30, 330, 3, 3, 0x2152c4ff),
            (33, 330, 3, 3, 0x2151c2ff),
            (36, 330, 3, 3, 0x2151bfff),
            (39, 330, 3, 3, 0x2151bdff),
            (42, 330, 3, 3, 0x2151bcff),
            (45, 330, 3, 3, 0x204fbaff),
            (48, 330, 3, 3, 0x1f4db6ff),
            (51, 330, 3, 3, 0x1e4cb4ff),
            (54, 330, 3, 3, 0x1e4bb0ff),
            (57, 330, 3, 3, 0x1d49adff),
            (60, 330, 3, 3, 0x1d49abff),
            (63, 330, 3, 3, 0x1c48a7ff),
            (66, 330, 3, 3, 0x1b46a5ff),
            (69, 330, 3, 3, 0x1b46a3ff),
            (72, 330, 3, 3, 0x1b45a3ff),
            (75, 330, 3, 3, 0x1b46a3ff),
            (78, 330, 3, 3, 0x1b45a2ff),
            (81, 330, 3, 3, 0x1b45a2ff),
            (84, 330, 3, 3, 0x1b45a3ff),
            (87, 330, 3, 3, 0x1b46a4ff),
            (90, 330, 3, 3, 0x1b46a7ff),
            (93, 330, 3, 3, 0x1b47acff),
            (96, 330, 3, 3, 0x1c48b0ff),
            (99, 330, 3, 3, 0x1c47b8ff),
            (102, 330, 3, 3, 0x264ad0ff),
            (105, 330, 3, 3, 0x3b4eddff),
            (108, 330, 3, 3, 0x4553e1ff),
            (111, 330, 3, 3, 0x4c57e6ff),
            (114, 330, 3, 3, 0x4f57e8ff),
            (117, 330, 3, 3, 0x5158e9ff),
            (120, 330, 3, 3, 0x5257e9ff),
            (123, 330, 3, 3, 0x5356e9ff),
            (126, 330, 3, 3, 0x5354e8ff),
            (129, 330, 3, 3, 0x5354e8ff),
            (132, 330, 3, 3, 0x5354e8ff),
            (135, 330, 3, 3, 0x5253e9ff),
            (138, 330, 3, 3, 0x5154eaff),
            (141, 330, 3, 3, 0x5255e9ff),
            (144, 330, 3, 3, 0x5054e9ff),
            (147, 330, 3, 3, 0x5054e8ff),
            (150, 330, 3, 3, 0x5155eaff),
            (153, 330, 3, 3, 0x5559ecff),
            (156, 330, 3, 3, 0x5a5eedff),
            (159, 330, 3, 3, 0x5d62f0ff),
            (162, 330, 3, 3, 0x5e62f0ff),
            (165, 330, 3, 3, 0x5d61efff),
            (168, 330, 3, 3, 0x5c61efff),
            (171, 330, 3, 3, 0x5d62f0ff),
            (174, 330, 3, 3, 0x6063f0ff),
            (177, 330, 3, 3, 0xc9cafbff),
            (180, 330, 3, 3, 0x686af4ff),
            (183, 330, 3, 3, 0x6e6ff5ff),
            (186, 330, 3, 3, 0xa8a5faff),
            (189, 330, 3, 3, 0x8581f7ff),
            (192, 330, 3, 3, 0x908cf8ff),
            (195, 330, 3, 3, 0xbfbdfbff),
            (198, 330, 3, 3, 0xa7a3f9ff),
            (201, 330, 3, 3, 0xaea9f9ff),
            (204, 330, 3, 3, 0xcbc7fbff),
            (207, 330, 3, 3, 0xb2aef9ff),
            (210, 330, 3, 3, 0xb5b1faff),
            (213, 330, 3, 3, 0xb5b1faff),
            (216, 330, 3, 3, 0xb3b1faff),
            (219, 330, 3, 3, 0xafaef9ff),
            (222, 330, 3, 3, 0xabaaf9ff),
            (225, 330, 3, 3, 0xa3a4f9ff),
            (228, 330, 3, 3, 0x9a9bf8ff),
            (231, 330, 3, 3, 0x9092f8ff),
            (234, 330, 3, 3, 0x8889f8ff),
            (237, 330, 3, 3, 0x7d7ef7ff),
            (240, 330, 3, 3, 0x7373f7ff),
            (243, 330, 3, 3, 0x706ef6ff),
            (246, 330, 3, 3, 0x6e6cf5ff),
            (249, 330, 3, 3, 0x6967f3ff),
            (252, 330, 3, 3, 0x6562f1ff),
            (255, 330, 3, 3, 0x625eeeff),
            (258, 330, 3, 3, 0x5f5ceeff),
            (261, 330, 3, 3, 0x5b58ecff),
            (264, 330, 3, 3, 0x5856eaff),
            (267, 330, 3, 3, 0x5655eaff),
            (270, 330, 3, 3, 0x5656eaff),
            (273, 330, 3, 3, 0x5758ebff),
            (276, 330, 3, 3, 0x5958eaff),
            (279, 330, 3, 3, 0x5655e9ff),
            (282, 330, 3, 3, 0x5857eaff),
            (285, 330, 3, 3, 0x5756eaff),
            (288, 330, 3, 3, 0x5a59ebff),
            (291, 330, 3, 3, 0x5a5aecff),
            (294, 330, 3, 3, 0x5b5cecff),
            (297, 330, 3, 3, 0x5f60efff),
            (300, 330, 3, 3, 0x6766f1ff),
            (303, 330, 3, 3, 0x6e6af3ff),
            (306, 330, 3, 3, 0x726ff6ff),
            (309, 330, 3, 3, 0x7371f6ff),
            (312, 330, 3, 3, 0x7576f7ff),
            (315, 330, 3, 3, 0x787af7ff),
            (318, 330, 3, 3, 0x8185f7ff),
            (321, 330, 3, 3, 0xc0bffaff),
            (324, 330, 3, 3, 0xc2befaff),
            (327, 330, 3, 3, 0xc1c1fbff),
            (330, 330, 3, 3, 0xbcbefaff),
            (333, 330, 3, 3, 0xb6b9faff),
            (336, 330, 3, 3, 0xabb1f9ff),
            (339, 330, 3, 3, 0xa2acf9ff),
            (342, 330, 3, 3, 0x9dabf9ff),
            (345, 330, 3, 3, 0x9aa7f9ff),
            (348, 330, 3, 3, 0x95a3f9ff),
            (351, 330, 3, 3, 0x8ea0f8ff),
            (354, 330, 3, 3, 0x8c9df8ff),
            (357, 330, 3, 3, 0x8c9af8ff),
            (360, 330, 3, 3, 0x8b99f8ff),
            (363, 330, 3, 3, 0x8794f8ff),
            (366, 330, 3, 3, 0x8290f8ff),
            (369, 330, 3, 3, 0x818ff8ff),
            (372, 330, 3, 3, 0x7e89f7ff),
            (375, 330, 3, 3, 0x7a86f5ff),
            (378, 330, 3, 3, 0x7480f4ff),
            (381, 330, 3, 3, 0x727df2ff),
            (0, 333, 3, 3, 0x1f4ec8ff),
            (3, 333, 3, 3, 0x1f4ec8ff),
            (6, 333, 3, 3, 0x1f4ec6ff),
            (9, 333, 3, 3, 0x2050c7ff),
            (12, 333, 3, 3, 0x2050c8ff),
            (15, 333, 3, 3, 0x2253c9ff),
            (18, 333, 3, 3, 0x2253c9ff),
            (21, 333, 3, 3, 0x2253c8ff),
            (24, 333, 3, 3, 0x2152c7ff),
            (27, 333, 3, 3, 0x2253c5ff),
            (30, 333, 3, 3, 0x2254c5ff),
            (33, 333, 3, 3, 0x2253c3ff),
            (36, 333, 3, 3, 0x2152c0ff),
            (39, 333, 3, 3, 0x2152beff),
            (42, 333, 3, 3, 0x2151beff),
            (45, 333, 3, 3, 0x2151bbff),
            (48, 333, 3, 3, 0x2050b6ff),
            (51, 333, 3, 3, 0x1f4db4ff),
            (54, 333, 3, 3, 0x1e4bb1ff),
            (57, 333, 3, 3, 0x1e4baeff),
            (60, 333, 3, 3, 0x1d49aaff),
            (63, 333, 3, 3, 0x1c48a8ff),
            (66, 333, 3, 3, 0x1c47a6ff),
            (69, 333, 3, 3, 0x1b46a4ff),
            (72, 333, 3, 3, 0x1b46a3ff),
            (75, 333, 3, 3, 0x1b46a3ff),
            (78, 333, 3, 3, 0x1b46a1ff),
            (81, 333, 3, 3, 0x1b46a1ff),
            (84, 333, 3, 3, 0x1b45a2ff),
            (87, 333, 3, 3, 0x1b46a4ff),
            (90, 333, 3, 3, 0x1b45a7ff),
            (93, 333, 3, 3, 0x1c47aaff),
            (96, 333, 3, 3, 0x1c47aeff),
            (99, 333, 3, 3, 0x1c48b7ff),
            (102, 333, 3, 3, 0x244bcdff),
            (105, 333, 3, 3, 0x3b4fddff),
            (108, 333, 3, 3, 0x4554e2ff),
            (111, 333, 3, 3, 0x4c59e5ff),
            (114, 333, 3, 3, 0x4f59e7ff),
            (117, 333, 3, 3, 0x5259e8ff),
            (120, 333, 3, 3, 0x5359eaff),
            (123, 333, 3, 3, 0x5358e9ff),
            (126, 333, 3, 3, 0x5356eaff),
            (129, 333, 3, 3, 0x5355e9ff),
            (132, 333, 3, 3, 0x5256eaff),
            (135, 333, 3, 3, 0x5256eaff),
            (138, 333, 3, 3, 0x5257e9ff),
            (141, 333, 3, 3, 0x5257eaff),
            (144, 333, 3, 3, 0x5056eaff),
            (147, 333, 3, 3, 0x5156e9ff),
            (150, 333, 3, 3, 0x5358ebff),
            (153, 333, 3, 3, 0x575cecff),
            (156, 333, 3, 3, 0x5b60eeff),
            (159, 333, 3, 3, 0x5d62efff),
            (162, 333, 3, 3, 0x5d62f0ff),
            (165, 333, 3, 3, 0x5a60eeff),
            (168, 333, 3, 3, 0x5b60eeff),
            (171, 333, 3, 3, 0x5c62efff),
            (174, 333, 3, 3, 0x5e63f0ff),
            (177, 333, 3, 3, 0x6065f1ff),
            (180, 333, 3, 3, 0x6669f4ff),
            (183, 333, 3, 3, 0x6d6ef5ff),
            (186, 333, 3, 3, 0x7877f7ff),
            (189, 333, 3, 3, 0x8281f7ff),
            (192, 333, 3, 3, 0x908df8ff),
            (195, 333, 3, 3, 0x9f9af8ff),
            (198, 333, 3, 3, 0xa7a3f9ff),
            (201, 333, 3, 3, 0xada9f9ff),
            (204, 333, 3, 3, 0xb1acf9ff),
            (207, 333, 3, 3, 0xb3aef9ff),
            (210, 333, 3, 3, 0xb5b2faff),
            (213, 333, 3, 3, 0xb6b3faff),
            (216, 333, 3, 3, 0xb2b2faff),
            (219, 333, 3, 3, 0xaeaff9ff),
            (222, 333, 3, 3, 0xabacf9ff),
            (225, 333, 3, 3, 0xa2a5f9ff),
            (228, 333, 3, 3, 0x969af8ff),
            (231, 333, 3, 3, 0x8d91f8ff),
            (234, 333, 3, 3, 0x8587f7ff),
            (237, 333, 3, 3, 0x7a7af7ff),
            (240, 333, 3, 3, 0x7271f6ff),
            (243, 333, 3, 3, 0x6f6ef6ff),
            (246, 333, 3, 3, 0x6d6bf6ff),
            (249, 333, 3, 3, 0x6666f2ff),
            (252, 333, 3, 3, 0x6362f0ff),
            (255, 333, 3, 3, 0x605fefff),
            (258, 333, 3, 3, 0x5b5aedff),
            (261, 333, 3, 3, 0x5957ebff),
            (264, 333, 3, 3, 0x5656eaff),
            (267, 333, 3, 3, 0x5455e9ff),
            (270, 333, 3, 3, 0x5455e9ff),
            (273, 333, 3, 3, 0x5657eaff),
            (276, 333, 3, 3, 0x5757ebff),
            (279, 333, 3, 3, 0x5857eaff),
            (282, 333, 3, 3, 0x5757eaff),
            (285, 333, 3, 3, 0x5758eaff),
            (288, 333, 3, 3, 0x5859eaff),
            (291, 333, 3, 3, 0x585bebff),
            (294, 333, 3, 3, 0x5a5decff),
            (297, 333, 3, 3, 0x6061efff),
            (300, 333, 3, 3, 0x6b68f1ff),
            (303, 333, 3, 3, 0x726df4ff),
            (306, 333, 3, 3, 0x7370f5ff),
            (309, 333, 3, 3, 0x7271f5ff),
            (312, 333, 3, 3, 0x7373f6ff),
            (315, 333, 3, 3, 0x787bf7ff),
            (318, 333, 3, 3, 0x9694f8ff),
            (321, 333, 3, 3, 0xbfbcfaff),
            (324, 333, 3, 3, 0xbdb9faff),
            (327, 333, 3, 3, 0xbcbdfaff),
            (330, 333, 3, 3, 0xb9bcfaff),
            (333, 333, 3, 3, 0xacb1f9ff),
            (336, 333, 3, 3, 0xa2abf9ff),
            (339, 333, 3, 3, 0x9eaaf9ff),
            (342, 333, 3, 3, 0x98a6f9ff),
            (345, 333, 3, 3, 0x94a3f9ff),
            (348, 333, 3, 3, 0x8d9ff8ff),
            (351, 333, 3, 3, 0x899af8ff),
            (354, 333, 3, 3, 0x8a9af8ff),
            (357, 333, 3, 3, 0x8796f8ff),
            (360, 333, 3, 3, 0x8694f8ff),
            (363, 333, 3, 3, 0x818ff8ff),
            (366, 333, 3, 3, 0x7f8ef8ff),
            (369, 333, 3, 3, 0x7e8cf7ff),
            (372, 333, 3, 3, 0x7985f3ff),
            (375, 333, 3, 3, 0x7481f2ff),
            (378, 333, 3, 3, 0x707cf1ff),
            (381, 333, 3, 3, 0xe9e9f1ff),
            (0, 336, 3, 3, 0x1f4ec9ff),
            (3, 336, 3, 3, 0x204fc8ff),
            (6, 336, 3, 3, 0x204fc8ff),
            (9, 336, 3, 3, 0x2050c9ff),
            (12, 336, 3, 3, 0x2151c9ff),
            (15, 336, 3, 3, 0x2253caff),
            (18, 336, 3, 3, 0x2354caff),
            (21, 336, 3, 3, 0x2355caff),
            (24, 336, 3, 3, 0x2356caff),
            (27, 336, 3, 3, 0x2356c8ff),
            (30, 336, 3, 3, 0x2254c6ff),
            (33, 336, 3, 3, 0x2254c4ff),
            (36, 336, 3, 3, 0x2253c1ff),
            (39, 336, 3, 3, 0x2152beff),
            (42, 336, 3, 3, 0x2152bdff),
            (45, 336, 3, 3, 0x2152bbff),
            (48, 336, 3, 3, 0x2050b8ff),
            (51, 336, 3, 3, 0x204fb6ff),
            (54, 336, 3, 3, 0x1f4db2ff),
            (57, 336, 3, 3, 0x1e4cafff),
            (60, 336, 3, 3, 0x1d4aabff),
            (63, 336, 3, 3, 0x1d49a7ff),
            (66, 336, 3, 3, 0x1c48a6ff),
            (69, 336, 3, 3, 0x1c47a4ff),
            (72, 336, 3, 3, 0x1b46a2ff),
            (75, 336, 3, 3, 0x1b46a2ff),
            (78, 336, 3, 3, 0x1b46a1ff),
            (81, 336, 3, 3, 0x1b46a2ff),
            (84, 336, 3, 3, 0x1b46a3ff),
            (87, 336, 3, 3, 0x1c47a4ff),
            (90, 336, 3, 3, 0x1c47a5ff),
            (93, 336, 3, 3, 0x1c47aaff),
            (96, 336, 3, 3, 0x1c47adff),
            (99, 336, 3, 3, 0x1d4ab7ff),
            (102, 336, 3, 3, 0x224cccff),
            (105, 336, 3, 3, 0x3b4fdcff),
            (108, 336, 3, 3, 0x4655e2ff),
            (111, 336, 3, 3, 0x4c5ae5ff),
            (114, 336, 3, 3, 0x505ae7ff),
            (117, 336, 3, 3, 0x515ae8ff),
            (120, 336, 3, 3, 0x525be9ff),
            (123, 336, 3, 3, 0x535bebff),
            (126, 336, 3, 3, 0x545aeaff),
            (129, 336, 3, 3, 0x5359eaff),
            (132, 336, 3, 3, 0x5257eaff),
            (135, 336, 3, 3, 0x5257e9ff),
            (138, 336, 3, 3, 0x5257e9ff),
            (141, 336, 3, 3, 0x5258e9ff),
            (144, 336, 3, 3, 0x5258ebff),
            (147, 336, 3, 3, 0x5359ebff),
            (150, 336, 3, 3, 0x565cecff),
            (153, 336, 3, 3, 0x5960eeff),
            (156, 336, 3, 3, 0x5b61efff),
            (159, 336, 3, 3, 0x5a61eeff),
            (162, 336, 3, 3, 0x585fedff),
            (165, 336, 3, 3, 0x565eedff),
            (168, 336, 3, 3, 0x575fedff),
            (171, 336, 3, 3, 0x5961eeff),
            (174, 336, 3, 3, 0x5b63f0ff),
            (177, 336, 3, 3, 0x5e65f0ff),
            (180, 336, 3, 3, 0x6369f3ff),
            (183, 336, 3, 3, 0x6c6ef6ff),
            (186, 336, 3, 3, 0x7778f7ff),
            (189, 336, 3, 3, 0x8384f7ff),
            (192, 336, 3, 3, 0x9290f8ff),
            (195, 336, 3, 3, 0x9f9bf8ff),
            (198, 336, 3, 3, 0xa6a2f9ff),
            (201, 336, 3, 3, 0xaca8f9ff),
            (204, 336, 3, 3, 0xb1acf9ff),
            (207, 336, 3, 3, 0xb4affaff),
            (210, 336, 3, 3, 0xb5b2faff),
            (213, 336, 3, 3, 0xb5b3faff),
            (216, 336, 3, 3, 0xb3b4faff),
            (219, 336, 3, 3, 0xafb0f9ff),
            (222, 336, 3, 3, 0xa9abf9ff),
            (225, 336, 3, 3, 0xa1a3f9ff),
            (228, 336, 3, 3, 0x9599f8ff),
            (231, 336, 3, 3, 0x8b8ff8ff),
            (234, 336, 3, 3, 0x8285f7ff),
            (237, 336, 3, 3, 0x7978f7ff),
            (240, 336, 3, 3, 0x706ff6ff),
            (243, 336, 3, 3, 0x6e6ef5ff),
            (246, 336, 3, 3, 0x6a69f4ff),
            (249, 336, 3, 3, 0x6565f2ff),
            (252, 336, 3, 3, 0x6061efff),
            (255, 336, 3, 3, 0x5d5eeeff),
            (258, 336, 3, 3, 0x5759ecff),
            (261, 336, 3, 3, 0x5557eaff),
            (264, 336, 3, 3, 0x5355e9ff),
            (267, 336, 3, 3, 0x5254e9ff),
            (270, 336, 3, 3, 0x5053e8ff),
            (273, 336, 3, 3, 0x5255e9ff),
            (276, 336, 3, 3, 0x5758eaff),
            (279, 336, 3, 3, 0x5859ebff),
            (282, 336, 3, 3, 0x5658eaff),
            (285, 336, 3, 3, 0x5859ebff),
            (288, 336, 3, 3, 0x585aebff),
            (291, 336, 3, 3, 0x575cebff),
            (294, 336, 3, 3, 0x595decff),
            (297, 336, 3, 3, 0x5f61efff),
            (300, 336, 3, 3, 0x6a69f2ff),
            (303, 336, 3, 3, 0x6f6bf3ff),
            (306, 336, 3, 3, 0x706df4ff),
            (309, 336, 3, 3, 0x6e6ef4ff),
            (312, 336, 3, 3, 0x7273f7ff),
            (315, 336, 3, 3, 0x8080f7ff),
            (318, 336, 3, 3, 0xa7a5f9ff),
            (321, 336, 3, 3, 0xb7b2faff),
            (324, 336, 3, 3, 0xb7b4faff),
            (327, 336, 3, 3, 0xb2b4faff),
            (330, 336, 3, 3, 0xacb0f9ff),
            (333, 336, 3, 3, 0xa5abf9ff),
            (336, 336, 3, 3, 0x9ea8f9ff),
            (339, 336, 3, 3, 0x99a7f9ff),
            (342, 336, 3, 3, 0x94a3f9ff),
            (345, 336, 3, 3, 0x8c9ef8ff),
            (348, 336, 3, 3, 0x899bf8ff),
            (351, 336, 3, 3, 0x8699f8ff),
            (354, 336, 3, 3, 0x8395f8ff),
            (357, 336, 3, 3, 0x8394f7ff),
            (360, 336, 3, 3, 0x818ff7ff),
            (363, 336, 3, 3, 0x7e8cf6ff),
            (366, 336, 3, 3, 0x7e8cf6ff),
            (369, 336, 3, 3, 0x7b89f6ff),
            (372, 336, 3, 3, 0x7583f3ff),
            (375, 336, 3, 3, 0x707cf1ff),
            (378, 336, 3, 3, 0xcdd1efff),
            (381, 336, 3, 3, 0xffffffff),
            (0, 339, 3, 3, 0xe9e9e9ff),
            (3, 339, 3, 3, 0x2050c9ff),
            (6, 339, 3, 3, 0x2151c9ff),
            (9, 339, 3, 3, 0x2152caff),
            (12, 339, 3, 3, 0x2253caff),
            (15, 339, 3, 3, 0x2355cbff),
            (18, 339, 3, 3, 0x2457cbff),
            (21, 339, 3, 3, 0x2457caff),
            (24, 339, 3, 3, 0x2559cbff),
            (27, 339, 3, 3, 0x2458caff),
            (30, 339, 3, 3, 0x2457c8ff),
            (33, 339, 3, 3, 0x2355c4ff),
            (36, 339, 3, 3, 0x2355c1ff),
            (39, 339, 3, 3, 0x2253bdff),
            (42, 339, 3, 3, 0x2152bcff),
            (45, 339, 3, 3, 0x2253bbff),
            (48, 339, 3, 3, 0x2152b9ff),
            (51, 339, 3, 3, 0x2151b7ff),
            (54, 339, 3, 3, 0x1f4eb3ff),
            (57, 339, 3, 3, 0x1f4db1ff),
            (60, 339, 3, 3, 0x1e4babff),
            (63, 339, 3, 3, 0x1d4aa9ff),
            (66, 339, 3, 3, 0x1c48a6ff),
            (69, 339, 3, 3, 0x1c48a5ff),
            (72, 339, 3, 3, 0x1c47a3ff),
            (75, 339, 3, 3, 0x1c47a2ff),
            (78, 339, 3, 3, 0x1c47a0ff),
            (81, 339, 3, 3, 0x1b46a2ff),
            (84, 339, 3, 3, 0x1c47a3ff),
            (87, 339, 3, 3, 0x1c48a4ff),
            (90, 339, 3, 3, 0x1c48a7ff),
            (93, 339, 3, 3, 0x1d49a9ff),
            (96, 339, 3, 3, 0x1c48afff),
            (99, 339, 3, 3, 0x1e4cbbff),
            (102, 339, 3, 3, 0x274dceff),
            (105, 339, 3, 3, 0x3b51daff),
            (108, 339, 3, 3, 0x4759e1ff),
            (111, 339, 3, 3, 0x4c5ae5ff),
            (114, 339, 3, 3, 0x4f5ae7ff),
            (117, 339, 3, 3, 0x515ae8ff),
            (120, 339, 3, 3, 0x525ce9ff),
            (123, 339, 3, 3, 0x545cebff),
            (126, 339, 3, 3, 0x545cebff),
            (129, 339, 3, 3, 0x525be9ff),
            (132, 339, 3, 3, 0x5259e9ff),
            (135, 339, 3, 3, 0x5258e9ff),
            (138, 339, 3, 3, 0x5258e9ff),
            (141, 339, 3, 3, 0x5159e9ff),
            (144, 339, 3, 3, 0x525bebff),
            (147, 339, 3, 3, 0x545debff),
            (150, 339, 3, 3, 0x565eecff),
            (153, 339, 3, 3, 0x585fecff),
            (156, 339, 3, 3, 0x5960edff),
            (159, 339, 3, 3, 0x575fecff),
            (162, 339, 3, 3, 0x565decff),
            (165, 339, 3, 3, 0x555debff),
            (168, 339, 3, 3, 0x5760edff),
            (171, 339, 3, 3, 0x5860eeff),
            (174, 339, 3, 3, 0x5961ecff),
            (177, 339, 3, 3, 0x5e66eeff),
            (180, 339, 3, 3, 0x666bf0ff),
            (183, 339, 3, 3, 0x6f73f4ff),
            (186, 339, 3, 3, 0x7b7df7ff),
            (189, 339, 3, 3, 0x8989f7ff),
            (192, 339, 3, 3, 0x9794f8ff),
            (195, 339, 3, 3, 0xa09df8ff),
            (198, 339, 3, 3, 0xa6a3f9ff),
            (201, 339, 3, 3, 0xada9f9ff),
            (204, 339, 3, 3, 0xb1acf9ff),
            (207, 339, 3, 3, 0xb3b0f9ff),
            (210, 339, 3, 3, 0xb4b1faff),
            (213, 339, 3, 3, 0xb4b3faff),
            (216, 339, 3, 3, 0xb2b5faff),
            (219, 339, 3, 3, 0xafb2f9ff),
            (222, 339, 3, 3, 0xabacf9ff),
            (225, 339, 3, 3, 0xa2a4f8ff),
            (228, 339, 3, 3, 0x999cf8ff),
            (231, 339, 3, 3, 0x8d91f8ff),
            (234, 339, 3, 3, 0x8287f7ff),
            (237, 339, 3, 3, 0x7a7cf7ff),
            (240, 339, 3, 3, 0x7170f5ff),
            (243, 339, 3, 3, 0x6e6df3ff),
            (246, 339, 3, 3, 0x6b6af2ff),
            (249, 339, 3, 3, 0x6565f0ff),
            (252, 339, 3, 3, 0x6061efff),
            (255, 339, 3, 3, 0x5d5fecff),
            (258, 339, 3, 3, 0x575beaff),
            (261, 339, 3, 3, 0x5456e9ff),
            (264, 339, 3, 3, 0x5254e9ff),
            (267, 339, 3, 3, 0x5354e9ff),
            (270, 339, 3, 3, 0x5053e7ff),
            (273, 339, 3, 3, 0x5354e8ff),
            (276, 339, 3, 3, 0x5859e9ff),
            (279, 339, 3, 3, 0x5b5becff),
            (282, 339, 3, 3, 0x595aeaff),
            (285, 339, 3, 3, 0x585aebff),
            (288, 339, 3, 3, 0x595bebff),
            (291, 339, 3, 3, 0x575beaff),
            (294, 339, 3, 3, 0x595eeaff),
            (297, 339, 3, 3, 0x6063eeff),
            (300, 339, 3, 3, 0x6768f0ff),
            (303, 339, 3, 3, 0x6c6af2ff),
            (306, 339, 3, 3, 0x6c6bf3ff),
            (309, 339, 3, 3, 0x6d6ef3ff),
            (312, 339, 3, 3, 0x7b7af6ff),
            (315, 339, 3, 3, 0x8c88f7ff),
            (318, 339, 3, 3, 0xb0b0f9ff),
            (321, 339, 3, 3, 0xb2aff9ff),
            (324, 339, 3, 3, 0xb3b4f9ff),
            (327, 339, 3, 3, 0xaeb1f9ff),
            (330, 339, 3, 3, 0xaaaff9ff),
            (333, 339, 3, 3, 0xa3a8f9ff),
            (336, 339, 3, 3, 0x9aa7f9ff),
            (339, 339, 3, 3, 0x97a5f9ff),
            (342, 339, 3, 3, 0x94a3f8ff),
            (345, 339, 3, 3, 0x8a9df8ff),
            (348, 339, 3, 3, 0x879af8ff),
            (351, 339, 3, 3, 0x8799f8ff),
            (354, 339, 3, 3, 0x8194f7ff),
            (357, 339, 3, 3, 0x8091f6ff),
            (360, 339, 3, 3, 0x7f8ef6ff),
            (363, 339, 3, 3, 0x7d8bf5ff),
            (366, 339, 3, 3, 0x7c8bf5ff),
            (369, 339, 3, 3, 0x99a4f5ff),
            (372, 339, 3, 3, 0xd0d3f2ff),
            (375, 339, 3, 3, 0xfafafaff),
            (378, 339, 3, 3, 0xffffffff),
            (381, 339, 3, 3, 0xffffffff),
        ];
        const CHRONICLE_INNER_LIGHT_TILES: &[(u16, u16, u16, u16, u32)] = &[
            (0, 0, 20, 20, 0xe8eefbff),
            (20, 0, 20, 20, 0xf4f7fdff),
            (40, 0, 20, 20, 0xf4f7fdff),
            (60, 0, 20, 20, 0xf5f8fdff),
            (80, 0, 20, 20, 0xf6f8feff),
            (100, 0, 20, 20, 0xf7f8feff),
            (120, 0, 20, 20, 0xf7f8feff),
            (140, 0, 20, 20, 0xf6f7fdff),
            (160, 0, 20, 20, 0xf6f6fdff),
            (180, 0, 20, 20, 0xf5f6fdff),
            (200, 0, 20, 20, 0xf5f6fdff),
            (220, 0, 20, 20, 0xf4f5fcff),
            (240, 0, 20, 20, 0xf4f5fcff),
            (260, 0, 20, 20, 0xf3f4fbff),
            (280, 0, 20, 20, 0xf3f4faff),
            (300, 0, 20, 20, 0xf2f3faff),
            (320, 0, 20, 20, 0xe8eaf5ff),
            (340, 0, 2, 20, 0x929bceff),
            (0, 20, 20, 20, 0xf4f7feff),
            (20, 20, 20, 20, 0xf5f8feff),
            (40, 20, 20, 20, 0xf6f9feff),
            (60, 20, 20, 20, 0xf8faffff),
            (80, 20, 20, 20, 0xfafbffff),
            (100, 20, 20, 20, 0xfbfbffff),
            (120, 20, 20, 20, 0xf9faffff),
            (140, 20, 20, 20, 0xf8f8ffff),
            (160, 20, 20, 20, 0xf7f8ffff),
            (180, 20, 20, 20, 0xf6f7feff),
            (200, 20, 20, 20, 0xf5f6fdff),
            (220, 20, 20, 20, 0xf4f5fdff),
            (240, 20, 20, 20, 0xf3f5fcff),
            (260, 20, 20, 20, 0xf3f5fcff),
            (280, 20, 20, 20, 0xf3f4fbff),
            (300, 20, 20, 20, 0xf3f4fbff),
            (320, 20, 20, 20, 0xf2f4faff),
            (340, 20, 2, 20, 0xeaecf2ff),
            (0, 40, 20, 20, 0xf6f8feff),
            (20, 40, 20, 20, 0xf7f9ffff),
            (40, 40, 20, 20, 0xf9fbffff),
            (60, 40, 20, 20, 0xfbfcffff),
            (80, 40, 20, 20, 0xfdfdffff),
            (100, 40, 20, 20, 0xfdfdffff),
            (120, 40, 20, 20, 0xfbfbffff),
            (140, 40, 20, 20, 0xfafaffff),
            (160, 40, 20, 20, 0xf8f8ffff),
            (180, 40, 20, 20, 0xf6f7feff),
            (200, 40, 20, 20, 0xf5f6fdff),
            (220, 40, 20, 20, 0xf4f5fdff),
            (240, 40, 20, 20, 0xf5f6fdff),
            (260, 40, 20, 20, 0xf4f5fdff),
            (280, 40, 20, 20, 0xf3f5fcff),
            (300, 40, 20, 20, 0xf3f4fcff),
            (320, 40, 20, 20, 0xf3f4fbff),
            (340, 40, 2, 20, 0xebecf3ff),
            (0, 60, 20, 20, 0xf8faffff),
            (20, 60, 20, 20, 0xfafbffff),
            (40, 60, 20, 20, 0xfcfdffff),
            (60, 60, 20, 20, 0xfdfdffff),
            (80, 60, 20, 20, 0xfefeffff),
            (100, 60, 20, 20, 0xfefeffff),
            (120, 60, 20, 20, 0xfcfcffff),
            (140, 60, 20, 20, 0xfafaffff),
            (160, 60, 20, 20, 0xf7f7feff),
            (180, 60, 20, 20, 0xf5f6feff),
            (200, 60, 20, 20, 0xf5f6feff),
            (220, 60, 20, 20, 0xf6f6feff),
            (240, 60, 20, 20, 0xf6f6feff),
            (260, 60, 20, 20, 0xf5f6fdff),
            (280, 60, 20, 20, 0xf4f5fdff),
            (300, 60, 20, 20, 0xf4f5fdff),
            (320, 60, 20, 20, 0xf4f5fdff),
            (340, 60, 2, 20, 0xecedf4ff),
            (0, 80, 20, 20, 0xfafbffff),
            (20, 80, 20, 20, 0xfcfcffff),
            (40, 80, 20, 20, 0xfdfdffff),
            (60, 80, 20, 20, 0xfdfeffff),
            (80, 80, 20, 20, 0xfefeffff),
            (100, 80, 20, 20, 0xfdfdffff),
            (120, 80, 20, 20, 0xfafbffff),
            (140, 80, 20, 20, 0xf8f9feff),
            (160, 80, 20, 20, 0xf7f7feff),
            (180, 80, 20, 20, 0xf7f7feff),
            (200, 80, 20, 20, 0xf7f7feff),
            (220, 80, 20, 20, 0xf6f6feff),
            (240, 80, 20, 20, 0xf6f7feff),
            (260, 80, 20, 20, 0xf5f6fdff),
            (280, 80, 20, 20, 0xf5f6fdff),
            (300, 80, 20, 20, 0xf5f6fdff),
            (320, 80, 20, 20, 0xf6f6feff),
            (340, 80, 2, 20, 0xefeef5ff),
            (0, 100, 20, 20, 0xfcfcffff),
            (20, 100, 20, 20, 0xfdfdffff),
            (40, 100, 20, 20, 0xfdfdffff),
            (60, 100, 20, 20, 0xfdfdffff),
            (80, 100, 20, 20, 0xfcfdffff),
            (100, 100, 20, 20, 0xfbfcffff),
            (120, 100, 20, 20, 0xf8f9ffff),
            (140, 100, 20, 20, 0xf7f8feff),
            (160, 100, 20, 20, 0xf6f6feff),
            (180, 100, 20, 20, 0xf5f6fdff),
            (200, 100, 20, 20, 0xf5f5fdff),
            (220, 100, 20, 20, 0xf5f5fcff),
            (240, 100, 20, 20, 0xf5f5fcff),
            (260, 100, 20, 20, 0xf5f5fdff),
            (280, 100, 20, 20, 0xf5f5fdff),
            (300, 100, 20, 20, 0xf6f6feff),
            (320, 100, 20, 20, 0xf7f7feff),
            (340, 100, 2, 20, 0xeeedf4ff),
            (0, 120, 20, 20, 0xfcfdffff),
            (20, 120, 20, 20, 0xfdfdffff),
            (40, 120, 20, 20, 0xfcfdffff),
            (60, 120, 20, 20, 0xfbfcffff),
            (80, 120, 20, 20, 0xfafbffff),
            (100, 120, 20, 20, 0xf8f9feff),
            (120, 120, 20, 20, 0xf7f8feff),
            (140, 120, 20, 20, 0xf6f7feff),
            (160, 120, 20, 20, 0xf6f6fdff),
            (180, 120, 20, 20, 0xf5f5fdff),
            (200, 120, 20, 20, 0xf5f5fdff),
            (220, 120, 20, 20, 0xf5f5fcff),
            (240, 120, 20, 20, 0xf5f4fcff),
            (260, 120, 20, 20, 0xf5f4fcff),
            (280, 120, 20, 20, 0xf6f5fcff),
            (300, 120, 20, 20, 0xf7f6fdff),
            (320, 120, 20, 20, 0xf8f6feff),
            (340, 120, 2, 20, 0xefeef5ff),
            (0, 140, 20, 20, 0xfcfcffff),
            (20, 140, 20, 20, 0xfbfcffff),
            (40, 140, 20, 20, 0xfafcffff),
            (60, 140, 20, 20, 0xf9fbffff),
            (80, 140, 20, 20, 0xf7f9feff),
            (100, 140, 20, 20, 0xf7f8feff),
            (120, 140, 20, 20, 0xf6f8feff),
            (140, 140, 20, 20, 0xf6f7feff),
            (160, 140, 20, 20, 0xf6f6fdff),
            (180, 140, 20, 20, 0xf5f5fdff),
            (200, 140, 20, 20, 0xf5f5fcff),
            (220, 140, 20, 20, 0xf5f4fcff),
            (240, 140, 20, 20, 0xf5f4fcff),
            (260, 140, 20, 20, 0xf6f5fcff),
            (280, 140, 20, 20, 0xf7f6fdff),
            (300, 140, 20, 20, 0xf8f6feff),
            (320, 140, 20, 20, 0xf9f7feff),
            (340, 140, 2, 20, 0xf1eff6ff),
            (0, 160, 20, 20, 0xfbfcffff),
            (20, 160, 20, 20, 0xfafbffff),
            (40, 160, 20, 20, 0xf9faffff),
            (60, 160, 20, 20, 0xf7f9feff),
            (80, 160, 20, 20, 0xf6f8feff),
            (100, 160, 20, 20, 0xf6f8feff),
            (120, 160, 20, 20, 0xf6f7feff),
            (140, 160, 20, 20, 0xf6f6fdff),
            (160, 160, 20, 20, 0xf5f5fdff),
            (180, 160, 20, 20, 0xf4f5fcff),
            (200, 160, 20, 20, 0xf5f5fcff),
            (220, 160, 20, 20, 0xf6f5fdff),
            (240, 160, 20, 20, 0xf6f5fdff),
            (260, 160, 20, 20, 0xf8f7feff),
            (280, 160, 20, 20, 0xf8f6feff),
            (300, 160, 20, 20, 0xf8f7feff),
            (320, 160, 20, 20, 0xfaf9ffff),
            (340, 160, 2, 20, 0xf2f1f6ff),
            (0, 180, 20, 20, 0xf9faffff),
            (20, 180, 20, 20, 0xf8faffff),
            (40, 180, 20, 20, 0xf7f9feff),
            (60, 180, 20, 20, 0xf6f8feff),
            (80, 180, 20, 20, 0xf6f8feff),
            (100, 180, 20, 20, 0xf7f8feff),
            (120, 180, 20, 20, 0xf6f6feff),
            (140, 180, 20, 20, 0xf5f5fdff),
            (160, 180, 20, 20, 0xf4f5fdff),
            (180, 180, 20, 20, 0xf4f5fcff),
            (200, 180, 20, 20, 0xf5f5fdff),
            (220, 180, 20, 20, 0xf6f5fdff),
            (240, 180, 20, 20, 0xf8f7feff),
            (260, 180, 20, 20, 0xf8f7feff),
            (280, 180, 20, 20, 0xf8f7feff),
            (300, 180, 20, 20, 0xf9f8ffff),
            (320, 180, 20, 20, 0xfbfaffff),
            (340, 180, 2, 20, 0xf3f2f6ff),
            (0, 200, 20, 20, 0xf4f6fdff),
            (20, 200, 20, 20, 0xf6f8feff),
            (40, 200, 20, 20, 0xf6f7feff),
            (60, 200, 20, 20, 0xf6f7feff),
            (80, 200, 20, 20, 0xf6f7feff),
            (100, 200, 20, 20, 0xf6f7feff),
            (120, 200, 20, 20, 0xf5f6feff),
            (140, 200, 20, 20, 0xf4f5fdff),
            (160, 200, 20, 20, 0xf4f5fdff),
            (180, 200, 20, 20, 0xf5f5fdff),
            (200, 200, 20, 20, 0xf6f6fdff),
            (220, 200, 20, 20, 0xf8f7feff),
            (240, 200, 20, 20, 0xf9f8ffff),
            (260, 200, 20, 20, 0xf8f7feff),
            (280, 200, 20, 20, 0xf8f7feff),
            (300, 200, 20, 20, 0xf9f8ffff),
            (320, 200, 20, 20, 0xfcfbffff),
            (340, 200, 2, 20, 0xf3f3f6ff),
            (0, 220, 20, 20, 0xf3f5fcff),
            (20, 220, 20, 20, 0xf3f6fcff),
            (40, 220, 20, 20, 0xf4f6fcff),
            (60, 220, 20, 20, 0xf4f6fdff),
            (80, 220, 20, 20, 0xf5f7feff),
            (100, 220, 20, 20, 0xf6f6feff),
            (120, 220, 20, 20, 0xf5f6fdff),
            (140, 220, 20, 20, 0xf4f5fdff),
            (160, 220, 20, 20, 0xf5f5fdff),
            (180, 220, 20, 20, 0xf6f6fdff),
            (200, 220, 20, 20, 0xf8f7feff),
            (220, 220, 20, 20, 0xfaf9ffff),
            (240, 220, 20, 20, 0xf9f8ffff),
            (260, 220, 20, 20, 0xf8f7feff),
            (280, 220, 20, 20, 0xf8f7feff),
            (300, 220, 20, 20, 0xf9f8ffff),
            (320, 220, 20, 20, 0xfcfbffff),
            (340, 220, 2, 20, 0xf4f3f6ff),
            (0, 240, 20, 20, 0xf3f5fcff),
            (20, 240, 20, 20, 0xf3f5fcff),
            (40, 240, 20, 20, 0xf3f6fcff),
            (60, 240, 20, 20, 0xf3f6fcff),
            (80, 240, 20, 20, 0xf4f6fdff),
            (100, 240, 20, 20, 0xf4f5fdff),
            (120, 240, 20, 20, 0xf4f5fdff),
            (140, 240, 20, 20, 0xf5f5fdff),
            (160, 240, 20, 20, 0xf6f6fdff),
            (180, 240, 20, 20, 0xf8f7feff),
            (200, 240, 20, 20, 0xfaf9ffff),
            (220, 240, 20, 20, 0xf9f8ffff),
            (240, 240, 20, 20, 0xf8f8ffff),
            (260, 240, 20, 20, 0xf8f7feff),
            (280, 240, 20, 20, 0xf8f7feff),
            (300, 240, 20, 20, 0xf9f8ffff),
            (320, 240, 20, 20, 0xfcfcffff),
            (340, 240, 2, 20, 0xf4f4f6ff),
            (0, 260, 20, 20, 0xf3f5fcff),
            (20, 260, 20, 20, 0xf3f5fbff),
            (40, 260, 20, 20, 0xf3f5fbff),
            (60, 260, 20, 20, 0xf3f5fbff),
            (80, 260, 20, 20, 0xf4f5fdff),
            (100, 260, 20, 20, 0xf5f5fdff),
            (120, 260, 20, 20, 0xf5f5fdff),
            (140, 260, 20, 20, 0xf6f6feff),
            (160, 260, 20, 20, 0xf8f7feff),
            (180, 260, 20, 20, 0xfaf9ffff),
            (200, 260, 20, 20, 0xfafaffff),
            (220, 260, 20, 20, 0xf9f8ffff),
            (240, 260, 20, 20, 0xf7f7feff),
            (260, 260, 20, 20, 0xf7f7feff),
            (280, 260, 20, 20, 0xf8f7feff),
            (300, 260, 20, 20, 0xfaf9ffff),
            (320, 260, 20, 20, 0xfdfdffff),
            (340, 260, 2, 20, 0xf3f4f6ff),
            (0, 280, 20, 20, 0xe7ecf8ff),
            (20, 280, 20, 20, 0xf2f5faff),
            (40, 280, 20, 20, 0xf2f4faff),
            (60, 280, 20, 20, 0xf2f4faff),
            (80, 280, 20, 20, 0xf4f5fdff),
            (100, 280, 20, 20, 0xf5f5fdff),
            (120, 280, 20, 20, 0xf5f5fdff),
            (140, 280, 20, 20, 0xf7f6fdff),
            (160, 280, 20, 20, 0xf8f7feff),
            (180, 280, 20, 20, 0xfaf9feff),
            (200, 280, 20, 20, 0xf9f9feff),
            (220, 280, 20, 20, 0xf7f7feff),
            (240, 280, 20, 20, 0xf6f6fdff),
            (260, 280, 20, 20, 0xf6f6fdff),
            (280, 280, 20, 20, 0xf7f7feff),
            (300, 280, 20, 20, 0xfafafeff),
            (320, 280, 20, 20, 0xf7f8feff),
            (340, 280, 2, 20, 0xd0d6f6ff),
        ];
        const CHRONICLE_INNER_DARK_TILES: &[(u16, u16, u16, u16, u32)] = &[
            (0, 0, 20, 20, 0x1b212eff),
            (20, 0, 20, 20, 0x1a1d24ff),
            (40, 0, 20, 20, 0x1b1e24ff),
            (60, 0, 20, 20, 0x1c1e24ff),
            (80, 0, 20, 20, 0x1d1f24ff),
            (100, 0, 20, 20, 0x1e1f25ff),
            (120, 0, 20, 20, 0x1e1e25ff),
            (140, 0, 20, 20, 0x1d1e24ff),
            (160, 0, 20, 20, 0x1d1d24ff),
            (180, 0, 20, 20, 0x1c1d24ff),
            (200, 0, 20, 20, 0x1c1c23ff),
            (220, 0, 20, 20, 0x1b1c23ff),
            (240, 0, 20, 20, 0x1b1c22ff),
            (260, 0, 20, 20, 0x1a1b22ff),
            (280, 0, 20, 20, 0x191a21ff),
            (300, 0, 20, 20, 0x191a20ff),
            (320, 0, 20, 20, 0x171a25ff),
            (340, 0, 2, 20, 0x182053ff),
            (0, 20, 20, 20, 0x191c23ff),
            (20, 20, 20, 20, 0x1a1d23ff),
            (40, 20, 20, 20, 0x1b1e23ff),
            (60, 20, 20, 20, 0x1d1f24ff),
            (80, 20, 20, 20, 0x1f2024ff),
            (100, 20, 20, 20, 0x202024ff),
            (120, 20, 20, 20, 0x1e1f24ff),
            (140, 20, 20, 20, 0x1d1d24ff),
            (160, 20, 20, 20, 0x1c1d24ff),
            (180, 20, 20, 20, 0x1b1c23ff),
            (200, 20, 20, 20, 0x1a1b22ff),
            (220, 20, 20, 20, 0x191a22ff),
            (240, 20, 20, 20, 0x181a21ff),
            (260, 20, 20, 20, 0x181a21ff),
            (280, 20, 20, 20, 0x181920ff),
            (300, 20, 20, 20, 0x181920ff),
            (320, 20, 20, 20, 0x17191fff),
            (340, 20, 2, 20, 0x212229ff),
            (0, 40, 20, 20, 0x1b1d23ff),
            (20, 40, 20, 20, 0x1c1e24ff),
            (40, 40, 20, 20, 0x1e2024ff),
            (60, 40, 20, 20, 0x202124ff),
            (80, 40, 20, 20, 0x222224ff),
            (100, 40, 20, 20, 0x222224ff),
            (120, 40, 20, 20, 0x202024ff),
            (140, 40, 20, 20, 0x1f1f24ff),
            (160, 40, 20, 20, 0x1d1d24ff),
            (180, 40, 20, 20, 0x1b1c23ff),
            (200, 40, 20, 20, 0x1a1b22ff),
            (220, 40, 20, 20, 0x191a22ff),
            (240, 40, 20, 20, 0x1a1b22ff),
            (260, 40, 20, 20, 0x191a22ff),
            (280, 40, 20, 20, 0x181a21ff),
            (300, 40, 20, 20, 0x181920ff),
            (320, 40, 20, 20, 0x181920ff),
            (340, 40, 2, 20, 0x22232aff),
            (0, 60, 20, 20, 0x1d1f24ff),
            (20, 60, 20, 20, 0x1f2024ff),
            (40, 60, 20, 20, 0x212224ff),
            (60, 60, 20, 20, 0x222224ff),
            (80, 60, 20, 20, 0x232324ff),
            (100, 60, 20, 20, 0x232324ff),
            (120, 60, 20, 20, 0x212124ff),
            (140, 60, 20, 20, 0x1f1f24ff),
            (160, 60, 20, 20, 0x1c1c23ff),
            (180, 60, 20, 20, 0x1a1b23ff),
            (200, 60, 20, 20, 0x1a1b23ff),
            (220, 60, 20, 20, 0x1b1b23ff),
            (240, 60, 20, 20, 0x1b1b23ff),
            (260, 60, 20, 20, 0x1a1b22ff),
            (280, 60, 20, 20, 0x191a22ff),
            (300, 60, 20, 20, 0x191a22ff),
            (320, 60, 20, 20, 0x191a22ff),
            (340, 60, 2, 20, 0x23242bff),
            (0, 80, 20, 20, 0x1f2024ff),
            (20, 80, 20, 20, 0x212124ff),
            (40, 80, 20, 20, 0x222224ff),
            (60, 80, 20, 20, 0x222324ff),
            (80, 80, 20, 20, 0x232324ff),
            (100, 80, 20, 20, 0x222224ff),
            (120, 80, 20, 20, 0x1f2024ff),
            (140, 80, 20, 20, 0x1d1e23ff),
            (160, 80, 20, 20, 0x1c1c23ff),
            (180, 80, 20, 20, 0x1c1c23ff),
            (200, 80, 20, 20, 0x1c1c23ff),
            (220, 80, 20, 20, 0x1b1b23ff),
            (240, 80, 20, 20, 0x1b1c23ff),
            (260, 80, 20, 20, 0x1a1b22ff),
            (280, 80, 20, 20, 0x1a1b22ff),
            (300, 80, 20, 20, 0x1a1b22ff),
            (320, 80, 20, 20, 0x1b1b23ff),
            (340, 80, 2, 20, 0x26262cff),
            (0, 100, 20, 20, 0x212124ff),
            (20, 100, 20, 20, 0x222224ff),
            (40, 100, 20, 20, 0x222224ff),
            (60, 100, 20, 20, 0x222224ff),
            (80, 100, 20, 20, 0x212224ff),
            (100, 100, 20, 20, 0x202124ff),
            (120, 100, 20, 20, 0x1d1e24ff),
            (140, 100, 20, 20, 0x1c1d23ff),
            (160, 100, 20, 20, 0x1b1b23ff),
            (180, 100, 20, 20, 0x1a1b22ff),
            (200, 100, 20, 20, 0x1a1a22ff),
            (220, 100, 20, 20, 0x1a1a21ff),
            (240, 100, 20, 20, 0x1a1a21ff),
            (260, 100, 20, 20, 0x1a1a22ff),
            (280, 100, 20, 20, 0x1a1a22ff),
            (300, 100, 20, 20, 0x1b1b23ff),
            (320, 100, 20, 20, 0x1c1c23ff),
            (340, 100, 2, 20, 0x25242bff),
            (0, 120, 20, 20, 0x212224ff),
            (20, 120, 20, 20, 0x222224ff),
            (40, 120, 20, 20, 0x212224ff),
            (60, 120, 20, 20, 0x202124ff),
            (80, 120, 20, 20, 0x1f2024ff),
            (100, 120, 20, 20, 0x1d1e24ff),
            (120, 120, 20, 20, 0x1c1d23ff),
            (140, 120, 20, 20, 0x1b1c23ff),
            (160, 120, 20, 20, 0x1b1b22ff),
            (180, 120, 20, 20, 0x1a1a22ff),
            (200, 120, 20, 20, 0x1a1a22ff),
            (220, 120, 20, 20, 0x1a1a21ff),
            (240, 120, 20, 20, 0x1a1921ff),
            (260, 120, 20, 20, 0x1a1921ff),
            (280, 120, 20, 20, 0x1b1a21ff),
            (300, 120, 20, 20, 0x1c1b22ff),
            (320, 120, 20, 20, 0x1d1b23ff),
            (340, 120, 2, 20, 0x26252cff),
            (0, 140, 20, 20, 0x212124ff),
            (20, 140, 20, 20, 0x202124ff),
            (40, 140, 20, 20, 0x1f2124ff),
            (60, 140, 20, 20, 0x1e2024ff),
            (80, 140, 20, 20, 0x1c1e23ff),
            (100, 140, 20, 20, 0x1c1d23ff),
            (120, 140, 20, 20, 0x1b1d23ff),
            (140, 140, 20, 20, 0x1b1c23ff),
            (160, 140, 20, 20, 0x1b1b22ff),
            (180, 140, 20, 20, 0x1a1a22ff),
            (200, 140, 20, 20, 0x1a1a21ff),
            (220, 140, 20, 20, 0x1a1921ff),
            (240, 140, 20, 20, 0x1a1921ff),
            (260, 140, 20, 20, 0x1b1a21ff),
            (280, 140, 20, 20, 0x1c1b22ff),
            (300, 140, 20, 20, 0x1d1b23ff),
            (320, 140, 20, 20, 0x1e1c23ff),
            (340, 140, 2, 20, 0x28262dff),
            (0, 160, 20, 20, 0x202124ff),
            (20, 160, 20, 20, 0x1f2024ff),
            (40, 160, 20, 20, 0x1e1f24ff),
            (60, 160, 20, 20, 0x1c1e23ff),
            (80, 160, 20, 20, 0x1b1d23ff),
            (100, 160, 20, 20, 0x1b1d23ff),
            (120, 160, 20, 20, 0x1b1c23ff),
            (140, 160, 20, 20, 0x1b1b22ff),
            (160, 160, 20, 20, 0x1a1a22ff),
            (180, 160, 20, 20, 0x191a21ff),
            (200, 160, 20, 20, 0x1a1a21ff),
            (220, 160, 20, 20, 0x1b1a22ff),
            (240, 160, 20, 20, 0x1b1a22ff),
            (260, 160, 20, 20, 0x1d1c23ff),
            (280, 160, 20, 20, 0x1d1b23ff),
            (300, 160, 20, 20, 0x1d1c23ff),
            (320, 160, 20, 20, 0x1f1e24ff),
            (340, 160, 2, 20, 0x29282dff),
            (0, 180, 20, 20, 0x1e1f24ff),
            (20, 180, 20, 20, 0x1d1f24ff),
            (40, 180, 20, 20, 0x1c1e23ff),
            (60, 180, 20, 20, 0x1b1d23ff),
            (80, 180, 20, 20, 0x1b1d23ff),
            (100, 180, 20, 20, 0x1c1d23ff),
            (120, 180, 20, 20, 0x1b1b23ff),
            (140, 180, 20, 20, 0x1a1a22ff),
            (160, 180, 20, 20, 0x191a22ff),
            (180, 180, 20, 20, 0x191a21ff),
            (200, 180, 20, 20, 0x1a1a22ff),
            (220, 180, 20, 20, 0x1b1a22ff),
            (240, 180, 20, 20, 0x1d1c23ff),
            (260, 180, 20, 20, 0x1d1c23ff),
            (280, 180, 20, 20, 0x1d1c23ff),
            (300, 180, 20, 20, 0x1e1d24ff),
            (320, 180, 20, 20, 0x201f24ff),
            (340, 180, 2, 20, 0x2a2a2dff),
            (0, 200, 20, 20, 0x191b22ff),
            (20, 200, 20, 20, 0x1b1d23ff),
            (40, 200, 20, 20, 0x1b1c23ff),
            (60, 200, 20, 20, 0x1b1c23ff),
            (80, 200, 20, 20, 0x1b1c23ff),
            (100, 200, 20, 20, 0x1b1c23ff),
            (120, 200, 20, 20, 0x1a1b23ff),
            (140, 200, 20, 20, 0x191a22ff),
            (160, 200, 20, 20, 0x191a22ff),
            (180, 200, 20, 20, 0x1a1a22ff),
            (200, 200, 20, 20, 0x1b1b22ff),
            (220, 200, 20, 20, 0x1d1c23ff),
            (240, 200, 20, 20, 0x1e1d24ff),
            (260, 200, 20, 20, 0x1d1c23ff),
            (280, 200, 20, 20, 0x1d1c23ff),
            (300, 200, 20, 20, 0x1e1d24ff),
            (320, 200, 20, 20, 0x212024ff),
            (340, 200, 2, 20, 0x2a2a2dff),
            (0, 220, 20, 20, 0x181a21ff),
            (20, 220, 20, 20, 0x181b21ff),
            (40, 220, 20, 20, 0x191b21ff),
            (60, 220, 20, 20, 0x191b22ff),
            (80, 220, 20, 20, 0x1a1c23ff),
            (100, 220, 20, 20, 0x1b1b23ff),
            (120, 220, 20, 20, 0x1a1b22ff),
            (140, 220, 20, 20, 0x191a22ff),
            (160, 220, 20, 20, 0x1a1a22ff),
            (180, 220, 20, 20, 0x1b1b22ff),
            (200, 220, 20, 20, 0x1d1c23ff),
            (220, 220, 20, 20, 0x1f1e24ff),
            (240, 220, 20, 20, 0x1e1d24ff),
            (260, 220, 20, 20, 0x1d1c23ff),
            (280, 220, 20, 20, 0x1d1c23ff),
            (300, 220, 20, 20, 0x1e1d24ff),
            (320, 220, 20, 20, 0x212024ff),
            (340, 220, 2, 20, 0x2b2a2dff),
            (0, 240, 20, 20, 0x181a21ff),
            (20, 240, 20, 20, 0x181a21ff),
            (40, 240, 20, 20, 0x181b21ff),
            (60, 240, 20, 20, 0x181b21ff),
            (80, 240, 20, 20, 0x191b22ff),
            (100, 240, 20, 20, 0x191a22ff),
            (120, 240, 20, 20, 0x191a22ff),
            (140, 240, 20, 20, 0x1a1a22ff),
            (160, 240, 20, 20, 0x1b1b22ff),
            (180, 240, 20, 20, 0x1d1c23ff),
            (200, 240, 20, 20, 0x1f1e24ff),
            (220, 240, 20, 20, 0x1e1d24ff),
            (240, 240, 20, 20, 0x1d1d24ff),
            (260, 240, 20, 20, 0x1d1c23ff),
            (280, 240, 20, 20, 0x1d1c23ff),
            (300, 240, 20, 20, 0x1e1d24ff),
            (320, 240, 20, 20, 0x212124ff),
            (340, 240, 2, 20, 0x2b2b2dff),
            (0, 260, 20, 20, 0x181a21ff),
            (20, 260, 20, 20, 0x181a20ff),
            (40, 260, 20, 20, 0x181a20ff),
            (60, 260, 20, 20, 0x181a20ff),
            (80, 260, 20, 20, 0x191a22ff),
            (100, 260, 20, 20, 0x1a1a22ff),
            (120, 260, 20, 20, 0x1a1a22ff),
            (140, 260, 20, 20, 0x1b1b23ff),
            (160, 260, 20, 20, 0x1d1c23ff),
            (180, 260, 20, 20, 0x1f1e24ff),
            (200, 260, 20, 20, 0x1f1f24ff),
            (220, 260, 20, 20, 0x1e1d24ff),
            (240, 260, 20, 20, 0x1c1c23ff),
            (260, 260, 20, 20, 0x1c1c23ff),
            (280, 260, 20, 20, 0x1d1c23ff),
            (300, 260, 20, 20, 0x1f1e24ff),
            (320, 260, 20, 20, 0x222224ff),
            (340, 260, 2, 20, 0x2a2b2dff),
            (0, 280, 20, 20, 0x1a1f2bff),
            (20, 280, 20, 20, 0x191c21ff),
            (40, 280, 20, 20, 0x191b21ff),
            (60, 280, 20, 20, 0x191b21ff),
            (80, 280, 20, 20, 0x1b1c23ff),
            (100, 280, 20, 20, 0x1c1c24ff),
            (120, 280, 20, 20, 0x1c1c24ff),
            (140, 280, 20, 20, 0x1d1d24ff),
            (160, 280, 20, 20, 0x1e1e25ff),
            (180, 280, 20, 20, 0x212025ff),
            (200, 280, 20, 20, 0x202025ff),
            (220, 280, 20, 20, 0x1e1e24ff),
            (240, 280, 20, 20, 0x1d1d24ff),
            (260, 280, 20, 20, 0x1d1d24ff),
            (280, 280, 20, 20, 0x1e1e25ff),
            (300, 280, 20, 20, 0x212125ff),
            (320, 280, 20, 20, 0x26272dff),
            (340, 280, 2, 20, 0x545a7aff),
        ];
        let link_color = match self.mode {
            ThemeMode::Light => gpui::rgba(0x339cffff),
            ThemeMode::Dark => gpui::rgba(0x99ceffff),
        };
        let inner_fill = match self.mode {
            ThemeMode::Light => gpui::rgba(0xf7f7feff),
            ThemeMode::Dark => gpui::rgba(0x1c1c23ff),
        };
        let inner_tiles = match self.mode {
            ThemeMode::Light => CHRONICLE_INNER_LIGHT_TILES,
            ThemeMode::Dark => CHRONICLE_INNER_DARK_TILES,
        };
        let chronicle_text_weight = match self.mode {
            ThemeMode::Light => gpui::FontWeight(350.0),
            ThemeMode::Dark => gpui::FontWeight(200.0),
        };
        let (active_dot, inactive_dot) = match self.mode {
            ThemeMode::Light => (gpui::rgba(0xffffffff), gpui::rgba(0xffffff80)),
            ThemeMode::Dark => (gpui::rgba(0x181818ff), gpui::rgba(0x18181880)),
        };
        let art = div()
            .w(px(384.0))
            .h_full()
            .flex_none()
            .relative()
            .left(px(-1.0))
            .overflow_hidden()
            .child(
                gpui::canvas(
                    |bounds, _, _| bounds,
                    |bounds, _, window, _| {
                        for &(x, y, width, height, color) in CHRONICLE_ART_TILES {
                            let color = gpui::rgba(color);
                            window.paint_quad(gpui::quad(
                                gpui::Bounds {
                                    origin: point(
                                        bounds.origin.x + px(x as f32),
                                        bounds.origin.y + px(y as f32),
                                    ),
                                    size: gpui::size(px(width as f32), px(height as f32)),
                                },
                                px(0.0),
                                color,
                                px(0.0),
                                color,
                                Default::default(),
                            ));
                        }
                    },
                )
                .absolute()
                .inset_0()
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .left(px(20.0))
                    .right(px(20.0))
                    .top(px(20.0))
                    .bottom(px(20.0))
                    .rounded(px(15.0))
                    .overflow_hidden()
                    .border_1()
                    .border_color(theme.border)
                    .bg(inner_fill)
                    .child(
                        gpui::canvas(
                            |bounds, _, _| bounds,
                            move |bounds, _, window, _| {
                                for &(x, y, width, height, color) in inner_tiles {
                                    let color = gpui::rgba(color);
                                    window.paint_quad(gpui::quad(
                                        gpui::Bounds {
                                            origin: point(
                                                bounds.origin.x + px(x as f32),
                                                bounds.origin.y + px(y as f32),
                                            ),
                                            size: gpui::size(px(width as f32), px(height as f32)),
                                        },
                                        px(0.0),
                                        color,
                                        px(0.0),
                                        color,
                                        Default::default(),
                                    ));
                                }
                            },
                        )
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(px(342.0))
                        .h(px(300.0)),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(px(176.0))
                    .top(px(327.0))
                    .flex()
                    .gap(px(6.0))
                    .children((0..4).map(|index| {
                        div().size(px(4.0)).rounded_full().bg(if index == 0 {
                            active_dot
                        } else {
                            inactive_dot
                        })
                    })),
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
                    .text_size(px(24.0))
                    .line_height(px(28.8))
                    .font_weight(chronicle_text_weight)
                    .child(page.label),
            )
            .child(
                div()
                    .mt(px(32.0))
                    .h(px(342.0))
                    .rounded(px(20.0))
                    .overflow_hidden()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.settings_panel)
                    .flex()
                    .child(
                        div()
                            .w(px(384.0))
                            .h_full()
                            .flex_none()
                            .relative()
                            .child(
                                div()
                                    .absolute()
                                    .left(px(37.0))
                                    .top(px(90.5))
                                    .text_size(px(14.0))
                                    .line_height(px(24.0))
                                    .font_weight(gpui::FontWeight(400.0))
                                    .child(page.sections[0].title),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .left(px(37.0))
                                    .top(px(119.5))
                                    .w(px(300.0))
                                    .text_size(px(13.0))
                                    .line_height(px(18.0))
                                    .font_weight(chronicle_text_weight)
                                    .text_color(theme.settings_description)
                                    .child(page.sections[0].subtitle),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .left(px(37.0))
                                    .top(px(167.5))
                                    .w(px(300.0))
                                    .text_size(px(13.0))
                                    .line_height(px(18.0))
                                    .font_weight(chronicle_text_weight)
                                    .text_color(theme.settings_description)
                                    .child(rows[0].subtitle),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .left(px(37.0))
                                    .top(px(219.5))
                                    .w(px(52.0))
                                    .h(px(30.0))
                                    .rounded(px(12.5))
                                    .bg(theme.text)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(13.0))
                                    .line_height(px(18.0))
                                    .font_weight(gpui::FontWeight(400.0))
                                    .text_color(theme.surface)
                                    .child("开启"),
                            ),
                    )
                    .child(art),
            )
            .child(
                div()
                    .relative()
                    .left(px(3.0))
                    .mt(px(18.0))
                    .text_size(px(13.0))
                    .line_height(px(20.0))
                    .font_weight(chronicle_text_weight)
                    .text_color(theme.settings_description)
                    .child(
                        "开启后，ChatGPT 会保存你在允许的应用和网站中的活动文本摘要，可能包括通信内容。音频和私密模式网页浏览绝不会包含在内；",
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(
                                "你可以随时暂停或清除历史记录，并管理包含的内容。此功能会增加 Token 用量。",
                            )
                            .child(div().text_color(link_color).child("了解更多")),
                    ),
            )
            .into_any_element()
    }

    fn hooks_content(&self, page: &'static PageSpec, theme: Theme) -> gpui::AnyElement {
        let empty = &page.sections[0];
        let link_color = match self.mode {
            ThemeMode::Light => gpui::rgba(0x339cffff),
            ThemeMode::Dark => gpui::rgba(0x99ceffff),
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
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(16.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .relative()
                                    .top(px(-1.0))
                                    .font_family("PingFang SC")
                                    .text_size(px(24.0))
                                    .line_height(px(31.0))
                                    .font_weight(gpui::FontWeight(300.0))
                                    .child(page.label),
                            )
                            .child(
                                div()
                                    .relative()
                                    .left(px(1.0))
                                    .mt(px(3.8))
                                    .h(px(21.0))
                                    .flex()
                                    .items_center()
                                    .text_size(px(14.0))
                                    .line_height(px(21.0))
                                    .text_color(theme.settings_description)
                                    .child("通过配置和已启用的插件管理生命周期钩子。")
                                    .child(
                                        div().ml(px(8.0)).text_color(link_color).child("了解更多"),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .size(px(26.0))
                            .flex_none()
                            .rounded(px(10.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.sidebar_hover))
                            .child(
                                svg()
                                    .path("icons/settings-hooks-refresh.svg")
                                    .size(px(16.0))
                                    .text_color(theme.text_tertiary),
                            ),
                    ),
            )
            .child(
                div()
                    .mt(px(32.0))
                    .w_full()
                    .rounded(px(20.0))
                    .overflow_hidden()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.settings_panel)
                    .child(
                        div()
                            .h(px(60.5625))
                            .flex_none()
                            .px(px(16.0))
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .relative()
                                            .left(px(1.0))
                                            .text_size(px(13.0))
                                            .line_height(px(18.5625))
                                            .font_weight(gpui::FontWeight(500.0))
                                            .child(empty.title),
                                    )
                                    .child(
                                        div()
                                            .relative()
                                            .top(px(-2.0))
                                            .text_size(px(12.0))
                                            .line_height(px(16.0))
                                            .text_color(theme.settings_description)
                                            .child(empty.subtitle),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn connections_content(
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

    fn browser_content(
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
                                if index == 0 { -2.0 } else { -2.0 },
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

    fn plugins_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let rows = page.sections[1].rows;
        let plugin_hover = match self.mode {
            ThemeMode::Dark => gpui::rgba(0x222222ff),
            ThemeMode::Light => gpui::rgba(0xf3f3f4ff),
        };
        let search_border = match self.mode {
            ThemeMode::Dark => gpui::rgba(0xffffff29),
            ThemeMode::Light => gpui::rgba(0x1a1c1f29),
        };
        let subtitle_weight = match self.mode {
            ThemeMode::Light => gpui::FontWeight(350.0),
            ThemeMode::Dark => gpui::FontWeight(300.0),
        };
        let subtitle_color = match self.mode {
            ThemeMode::Light => gpui::rgba(0xb0b1b2ff),
            ThemeMode::Dark => gpui::rgba(0x5a5a5aff),
        };
        let icon_paths = match self.mode {
            ThemeMode::Light => [
                "icons/settings-plugin-gmail-light.svg",
                "icons/settings-plugin-github-light.svg",
                "icons/settings-plugin-figma-light.svg",
                "icons/settings-plugin-zotero-light.svg",
                "icons/settings-plugin-templates-light.svg",
                "icons/settings-plugin-management-light.svg",
                "icons/settings-plugin-documents-light.svg",
                "icons/settings-plugin-pdf-light.svg",
                "icons/settings-plugin-spreadsheets-light.svg",
                "icons/settings-plugin-presentations-light.svg",
            ],
            ThemeMode::Dark => [
                "icons/settings-plugin-gmail-dark.svg",
                "icons/settings-plugin-github-dark.svg",
                "icons/settings-plugin-figma-dark.svg",
                "icons/settings-plugin-zotero-dark.svg",
                "icons/settings-plugin-templates-dark.svg",
                "icons/settings-plugin-management-dark.svg",
                "icons/settings-plugin-documents-dark.svg",
                "icons/settings-plugin-pdf-dark.svg",
                "icons/settings-plugin-spreadsheets-dark.svg",
                "icons/settings-plugin-presentations-dark.svg",
            ],
        };
        let mut list = div().mt(px(36.0)).flex().flex_col();
        for (index, row) in rows.iter().enumerate() {
            list = list.child(
                div()
                    .h(px(68.0625))
                    .pl(px(9.0))
                    .pr(px(8.0))
                    .rounded(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .size(px(40.0))
                            .rounded(px(10.0))
                            .overflow_hidden()
                            .flex()
                            .flex_col()
                            .items_start()
                            .when(index >= 6 && index < 10, |icon| {
                                icon.relative().top(px(1.0))
                            })
                            .when(index < 10, |icon| {
                                icon.child(gpui::img(icon_paths[index]).size(px(40.0)))
                            })
                            .when(index >= 10, |icon| {
                                icon.border_1()
                                    .border_color(theme.border)
                                    .child(
                                        gpui::img("icons/settings-plugin-broken.svg")
                                            .size(px(16.0))
                                            .flex_none(),
                                    )
                                    .child(
                                        div()
                                            .h(px(20.0))
                                            .flex_none()
                                            .text_size(px(14.0))
                                            .line_height(px(20.0))
                                            .font_weight(gpui::FontWeight::NORMAL)
                                            .child(row.title),
                                    )
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .relative()
                            .top(px(1.0))
                            .flex()
                            .flex_col()
                            .font_family(".SystemUIFont")
                            .gap(px(3.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .line_height(px(21.0))
                                    .font_weight(gpui::FontWeight(500.0))
                                    .child(row.title),
                            )
                            .child(
                                div()
                                    .relative()
                                    .top(px(-1.0))
                                    .text_size(px(13.0))
                                    .line_height(px(21.125))
                                    .font_weight(subtitle_weight)
                                    .text_color(subtitle_color)
                                    .child(row.subtitle),
                            ),
                    )
                    .child(self.switch_control(true, (page.slug, 1, index), theme, cx)),
            );
        }
        div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .pt(px(64.0))
            .pb(px(80.0))
            .child(
                div()
                    .relative()
                    .top(px(2.0))
                    .flex()
                    .items_start()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(5.0))
                            .child(
                                div()
                                    .relative()
                                    .top(px(-1.0))
                                    .text_size(px(24.0))
                                    .line_height(px(31.0))
                                    .font_weight(gpui::FontWeight(300.0))
                                    .child(page.label),
                            )
                            .child(
                                div()
                                    .relative()
                                    .text_size(px(14.0))
                                    .line_height(px(21.0))
                                    .text_color(theme.text_tertiary)
                                    .child(page.sections[0].title),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .w(px(74.0))
                                    .h(px(28.0))
                                    .rounded(px(12.5))
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(theme.settings_button)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(14.0))
                                    .line_height(px(18.0))
                                    .child("浏览目录"),
                            )
                            .child(
                                div()
                                    .w(px(66.0))
                                    .h(px(28.0))
                                    .px(px(8.0))
                                    .rounded(px(12.5))
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(theme.text)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .gap(px(4.0))
                                    .text_size(px(14.0))
                                    .line_height(px(18.0))
                                    .text_color(theme.surface)
                                    .child("添加")
                                    .child(svg().path("icons/chevron-down.svg").size(px(12.0))),
                            ),
                    ),
            )
            .child(
                div()
                    .relative()
                    .mt(px(33.0))
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.0))
                            .text_size(px(14.0))
                            .line_height(px(18.0))
                            .text_color(theme.text_tertiary)
                            .child(
                                div()
                                    .h(px(28.0))
                                    .w(px(67.203125))
                                    .pl(px(9.0))
                                    .pr(px(7.0))
                                    .rounded(px(12.5))
                                    .bg(plugin_hover)
                                    .flex()
                                    .items_center()
                                    .text_color(theme.text)
                                    .child("插件 14"),
                            )
                            .child(
                                div()
                                    .h(px(28.0))
                                    .w(px(60.796875))
                                    .pl(px(9.0))
                                    .pr(px(7.0))
                                    .flex()
                                    .items_center()
                                    .child("应用 8"),
                            )
                            .child(
                                div()
                                    .h(px(28.0))
                                    .w(px(63.5625))
                                    .pl(px(9.0))
                                    .pr(px(7.0))
                                    .flex()
                                    .items_center()
                                    .child("MCP 4"),
                            )
                            .child(
                                div()
                                    .h(px(28.0))
                                    .w(px(60.3125))
                                    .pl(px(9.0))
                                    .pr(px(7.0))
                                    .flex()
                                    .items_center()
                                    .child("技能 2"),
                            ),
                    )
                    .child(
                        div()
                            .w(px(224.0))
                            .h(px(32.0))
                            .px(px(10.0))
                            .rounded_full()
                            .border_1()
                            .border_color(search_border)
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(14.0))
                            .line_height(px(18.0))
                            .text_color(theme.text_tertiary)
                            .child(svg().path("icons/search.svg").size(px(15.0)))
                            .child("搜索插件"),
                    ),
            )
            .child(list)
            .into_any_element()
    }

    fn coding_label(
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

    fn coding_field(
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

    fn coding_button(
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

    fn coding_icon_button(
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

    fn coding_setting_row(
        &self,
        title: &'static str,
        subtitle: &'static str,
        height: f32,
        content_phase: f32,
        last: bool,
        right: gpui::AnyElement,
        theme: Theme,
    ) -> gpui::AnyElement {
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

    fn coding_textarea(
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

    fn git_content(
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
                rows[0].title,
                rows[0].subtitle,
                60.5625,
                0.0,
                false,
                self.coding_field("codex/", 224.0, 36.0, false, theme),
                theme,
            ))
            .child(self.coding_setting_row(
                rows[1].title,
                rows[1].subtitle,
                60.5625,
                1.0,
                false,
                merge,
                theme,
            ))
            .child(
                self.coding_setting_row(
                    rows[2].title,
                    rows[2].subtitle,
                    60.5625,
                    1.0,
                    false,
                    self.switch_control(false, (page.slug, 0, 2), theme, cx)
                        .into_any_element(),
                    theme,
                ),
            )
            .child(
                self.coding_setting_row(
                    rows[3].title,
                    rows[3].subtitle,
                    60.5625,
                    2.0,
                    false,
                    self.switch_control(true, (page.slug, 0, 3), theme, cx)
                        .into_any_element(),
                    theme,
                ),
            )
            .child(self.coding_setting_row(
                rows[4].title,
                rows[4].subtitle,
                60.5625,
                2.0,
                true,
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
                            monitor.rows[0].title,
                            "继续监控，直到 Pull Request 合并",
                            60.5625,
                            0.0,
                            true,
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

    fn local_environments_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
    ) -> gpui::AnyElement {
        let rows = page.sections[0].rows;
        let mut cards = div().mt(px(12.0)).flex().flex_col().gap(px(12.0));
        for (index, row) in rows.iter().enumerate() {
            let subtitle = match index {
                2 => "",
                6 => "openai",
                _ => row.subtitle,
            };
            let expanded = index == 6;
            let row_height = if subtitle.is_empty() {
                52.0
            } else if matches!(index, 3 | 5 | 7) {
                61.5
            } else {
                60.5
            };
            let primary = div()
                .h(px(row_height))
                .flex_none()
                .px(px(16.0))
                .flex()
                .items_center()
                .gap(px(14.0))
                .child(
                    svg()
                        .path("icons/settings-local-project.svg")
                        .size(px(16.0))
                        .text_color(theme.text_tertiary),
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
                                .line_height(px(18.5))
                                .font_weight(gpui::FontWeight(500.0))
                                .child(row.title),
                        )
                        .when(!subtitle.is_empty(), |column| {
                            column.child(
                                div()
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .text_color(theme.text_tertiary)
                                    .child(subtitle),
                            )
                        }),
                )
                .child(self.coding_icon_button("icons/add.svg", true, theme));
            let card = div()
                .w_full()
                .rounded(px(20.0))
                .overflow_hidden()
                .border_1()
                .border_color(theme.border)
                .bg(theme.settings_panel)
                .child(primary)
                .when(expanded, |card| {
                    card.child(
                        div()
                            .h(px(61.5))
                            .flex_none()
                            .px(px(16.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .line_height(px(18.5))
                                            .font_weight(gpui::FontWeight(500.0))
                                            .child("codex"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .line_height(px(16.0))
                                            .text_color(theme.text_tertiary)
                                            .child("environment.toml"),
                                    ),
                            )
                            .child(
                                svg()
                                    .path("icons/settings-chevron-right.svg")
                                    .size(px(14.0))
                                    .text_color(theme.text_tertiary),
                            ),
                    )
                });
            cards = cards.child(card);
        }

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
                    .mt(px(6.0))
                    .h(px(21.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .text_color(theme.text_tertiary)
                    .child("本地环境会告诉 ChatGPT 如何为项目设置工作树。")
                    .child(div().text_color(theme.settings_accent).child("了解更多。")),
            )
            .child(
                div()
                    .mt(px(38.0))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(21.0))
                            .font_weight(gpui::FontWeight(500.0))
                            .child(page.sections[0].title),
                    )
                    .child(self.coding_button("添加项目", 74.0, None, false, theme)),
            )
            .child(cards)
            .into_any_element()
    }

    fn worktrees_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let input_border = match self.mode {
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
        let primary_text = match self.mode {
            ThemeMode::Light => gpui::rgba(0x646464ff),
            ThemeMode::Dark => gpui::rgba(0xb4b4b4ff),
        };
        let secondary_text = match self.mode {
            ThemeMode::Light => gpui::rgba(0xaaaaaaff),
            ThemeMode::Dark => gpui::rgba(0x707070ff),
        };
        let config_text_overlay = match self.mode {
            ThemeMode::Light => "icons/settings-worktrees-config-text-light.svg",
            ThemeMode::Dark => "icons/settings-worktrees-config-text-dark.svg",
        };
        let field = |value: &'static str, width: f32, height: f32, muted: bool| {
            div()
                .w(px(width))
                .h(px(height))
                .flex_none()
                .px(px(if height <= 28.0 { 8.0 } else { 10.0 }))
                .rounded(px(10.0))
                .border_1()
                .border_color(input_border)
                .flex()
                .items_center()
                .text_size(px(13.0))
                .line_height(px(18.5714))
                .text_color(if muted {
                    theme.text_tertiary
                } else {
                    primary_text
                })
                .child(value)
                .into_any_element()
        };
        let setting_row = |top: f32,
                           title: &'static str,
                           subtitle: &'static str,
                           height: f32,
                           last: bool,
                           right: gpui::AnyElement| {
            let title_nudge = if (50.0..100.0).contains(&top) || top > 180.0 {
                -1.0
            } else {
                0.0
            };
            let subtitle_nudge = if top < 1.0 { -3.0 } else { -2.0 };
            let mut label = div()
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
                        .text_color(gpui::rgba(0x00000000))
                        .child(title),
                );
            label =
                if height > 70.0 {
                    label.child(
                        div()
                            .relative()
                            .top(px(subtitle_nudge))
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(gpui::rgba(0x00000000))
                            .flex()
                            .flex_col()
                            .child(div().h(px(16.0)).child(
                                "要保留的托管工作树数量；超过后，较旧的工作树会自动被清理。ChatGPT",
                            ))
                            .child(div().h(px(16.0)).child(
                                "会在删除工作树前创建快照，因此被清理的工作树应始终可以恢复。",
                            )),
                    )
                } else {
                    label.child(
                        div()
                            .relative()
                            .top(px(subtitle_nudge))
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(gpui::rgba(0x00000000))
                            .child(subtitle),
                    )
                };
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(top))
                .h(px(height))
                .px(px(16.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(if height > 70.0 { 48.0 } else { 24.0 }))
                .when(!last, |row| {
                    row.child(
                        div()
                            .absolute()
                            .bottom(px(if (50.0..100.0).contains(&top) {
                                1.0
                            } else {
                                0.0
                            }))
                            .left(px(16.0))
                            .right(px(16.0))
                            .h(px(1.0))
                            .bg(card_border),
                    )
                })
                .child(label)
                .child(
                    div()
                        .flex_none()
                        .relative()
                        .left(px(if top > 180.0 { 1.0 } else { -1.0 }))
                        .child(right),
                )
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
                    .font_family("PingFang SC")
                    .text_color(if danger { danger_text } else { primary_text })
                    .whitespace_nowrap();
                if let Some((path, size)) = icon {
                    node = node.child(svg().path(path).size(px(size)).text_color(if danger {
                        danger_text
                    } else {
                        primary_text
                    }));
                }
                node.child(label).into_any_element()
            };

        let rows = page.sections[0].rows;
        let config = div()
            .w_full()
            .h(px(260.25))
            .flex_none()
            .relative()
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(card_border)
            .bg(theme.settings_panel)
            .child(setting_row(
                0.0,
                rows[0].title,
                rows[0].subtitle,
                60.5625,
                false,
                field("/Users/zp/.codex/worktrees", 288.0, 36.0, true),
            ))
            .child(setting_row(
                60.5625,
                rows[1].title,
                rows[1].subtitle,
                60.5625,
                false,
                self.reference_switch_control(false, (page.slug, 0, 1), theme, cx),
            ))
            .child(setting_row(
                121.125,
                rows[2].title,
                rows[2].subtitle,
                60.5625,
                false,
                self.reference_switch_control(true, (page.slug, 0, 2), theme, cx),
            ))
            .child(setting_row(
                181.6875,
                rows[3].title,
                rows[3].subtitle,
                76.5625,
                true,
                field("15", 96.0, 28.0, false),
            ))
            .child(
                gpui::img(config_text_overlay)
                    .absolute()
                    .left(px(14.0))
                    .top(px(-1.0))
                    .w(px(441.0))
                    .h(px(260.0)),
            );

        let mut content = div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .relative()
            .left(px(1.0))
            .pt(px(66.0))
            .pb(px(80.0))
            .font_family(".SystemUIFont")
            .text_color(primary_text)
            .child(
                div()
                    .relative()
                    .left(px(-1.0))
                    .top(px(-1.0))
                    .text_size(px(24.0))
                    .line_height(px(28.8))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .text_color(theme.text)
                    .child(page.label),
            )
            .child(div().mt(px(32.0)).child(config));

        for (section_index, section) in page.sections.iter().skip(1).enumerate() {
            let card = div()
                .w_full()
                .h(px(137.125))
                .flex_none()
                .relative()
                .left(px(-1.0))
                .px(px(12.0))
                .py(px(12.0))
                .rounded(px(20.0))
                .border_1()
                .border_color(card_border)
                .bg(theme.settings_panel)
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .items_start()
                        .justify_between()
                        .gap(px(16.0))
                        .child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .relative()
                                .left(px(1.0))
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_size(px(13.0))
                                        .line_height(px(18.5714))
                                        .font_weight(gpui::FontWeight(500.0))
                                        .child(section.subtitle),
                                )
                                .child(
                                    div()
                                        .mt(px(4.0))
                                        .relative()
                                        .top(px(-1.0))
                                        .text_size(px(12.0))
                                        .line_height(px(16.0))
                                        .text_color(secondary_text)
                                        .child(section.title),
                                )
                                .child(
                                    div()
                                        .relative()
                                        .top(px(-1.0))
                                        .text_size(px(12.0))
                                        .line_height(px(16.0))
                                        .text_color(secondary_text)
                                        .child(section.rows[0].title),
                                ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(button(
                                    "在此工作树中新建聊天",
                                    178.0,
                                    Some(("icons/settings-new-chat-reference.svg", 16.0)),
                                    false,
                                ))
                                .child(button("删除", 46.0, None, true)),
                        ),
                )
                .child(
                    div()
                        .mt(px(8.0))
                        .relative()
                        .top(px(-2.0))
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(secondary_text)
                        .child(section.rows[2].title),
                )
                .child(
                    div()
                        .mt(px(9.0))
                        .ml(px(8.0))
                        .text_size(px(13.0))
                        .line_height(px(18.5714))
                        .child(section.rows[2].subtitle),
                );
            content = content.child(
                div()
                    .mt(px(46.0))
                    .flex_none()
                    .child(
                        div()
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .relative()
                                    .left(px(1.0))
                                    .top(px(1.0))
                                    .text_size(px(13.0))
                                    .line_height(px(18.5714))
                                    .font_weight(gpui::FontWeight(500.0))
                                    .child(section.title),
                            )
                            .child(
                                div()
                                    .size(px(28.0))
                                    .flex_none()
                                    .relative()
                                    .left(px(if section_index == 0 { -1.0 } else { 0.0 }))
                                    .top(px(if section_index == 0 { 1.0 } else { 0.0 }))
                                    .rounded(px(12.5))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        svg()
                                            .path("icons/settings-refresh.svg")
                                            .size(px(16.0))
                                            .text_color(theme.text_tertiary),
                                    ),
                            ),
                    )
                    .child(div().mt(px(12.0)).child(card)),
            );
        }
        content.into_any_element()
    }

    fn data_controls_content(&self, page: &'static PageSpec, theme: Theme) -> gpui::AnyElement {
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
                    .font_family("PingFang SC")
                    .font_weight(gpui::FontWeight(300.0))
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
            .font_family("PingFang SC")
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
            .font_family("PingFang SC")
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
            .font_family("PingFang SC")
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
                            .font_family("PingFang SC")
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

    fn reference_switch_control(
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

    fn reference_label(
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

    fn reference_button(
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

    fn import_source_row(
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

    fn import_content(
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

    fn agent_select(&self, label: &'static str, width: f32, theme: Theme) -> gpui::AnyElement {
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

    fn agent_row(
        &self,
        title: &'static str,
        subtitle: &'static str,
        right: gpui::AnyElement,
        last: bool,
        theme: Theme,
    ) -> gpui::AnyElement {
        let (title_x, title_y, subtitle_x, subtitle_y) = match title {
            "批准策略" => (0.0, 1.0, 0.0, 0.0),
            "沙盒设置" => (0.0, 0.0, -1.0, 0.0),
            "网页搜索" => (0.0, 0.0, 0.0, -1.0),
            "输出详细程度" | "推理摘要" => (0.0, -1.0, -1.0, -1.0),
            "可用推理强度" => (0.0, 0.0, 0.0, -1.0),
            "模型选择器滑块中的 Ultra" => (0.0, -1.0, 0.0, -1.0),
            "Codex 依赖项" => (0.0, 0.0, -1.0, -1.0),
            _ => (0.0, 0.0, 0.0, 0.0),
        };
        let label = div()
            .min_w(px(0.0))
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .relative()
                    .left(px(title_x))
                    .top(px(title_y))
                    .text_size(px(13.0))
                    .line_height(px(18.5625))
                    .font_weight(gpui::FontWeight(500.0))
                    .text_color(theme.text)
                    .child(title),
            )
            .when(!subtitle.is_empty(), |column| {
                column.child(
                    div()
                        .relative()
                        .left(px(subtitle_x))
                        .top(px(subtitle_y))
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(theme.settings_description)
                        .child(subtitle),
                )
            });
        div()
            .h(px(60.5625))
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
                        .bg(if theme.surface == gpui::rgba(0x181818ff) {
                            gpui::rgba(0x313131ff)
                        } else {
                            gpui::rgba(0xe9e9e9ff)
                        }),
                )
            })
            .child(label)
            .child(right)
            .into_any_element()
    }

    fn agent_card(&self, rows: Vec<gpui::AnyElement>, theme: Theme) -> gpui::AnyElement {
        let mut card = div()
            .w_full()
            .rounded(px(20.0))
            .overflow_hidden()
            .border_1()
            .border_color(if theme.surface == gpui::rgba(0x181818ff) {
                gpui::rgba(0x313131ff)
            } else {
                gpui::rgba(0xe9e9e9ff)
            })
            .bg(theme.settings_panel);
        for row in rows {
            card = card.child(row);
        }
        card.into_any_element()
    }

    fn agent_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let defaults = self.agent_card(
            vec![
                self.agent_row(
                    "批准策略",
                    "选择 ChatGPT 何时请求批准",
                    self.agent_select("按请求", 86.0, theme),
                    false,
                    theme,
                ),
                self.agent_row(
                    "沙盒设置",
                    "选择 ChatGPT 运行命令时的权限范围",
                    self.agent_select("完整访问权限", 128.0, theme),
                    false,
                    theme,
                ),
                self.agent_row(
                    "网页搜索",
                    "选择 ChatGPT 访问网络的方式",
                    self.agent_select("实时", 72.0, theme),
                    false,
                    theme,
                ),
                self.agent_row(
                    "输出详细程度",
                    "选择 ChatGPT 回复包含细节的详细程度",
                    self.agent_select("模型默认", 100.0, theme),
                    false,
                    theme,
                ),
                self.agent_row(
                    "推理摘要",
                    "选择 ChatGPT 总结其推理的方式",
                    self.agent_select("自动", 72.0, theme),
                    true,
                    theme,
                ),
            ],
            theme,
        );
        let model = self.agent_card(
            vec![
                self.agent_row(
                    "可用推理强度",
                    "选择在模型控件中显示哪些推理强度级别。可用性因模型而异",
                    self.agent_select("已选择 6 个", 116.34375, theme),
                    false,
                    theme,
                ),
                self.agent_row(
                    "模型选择器滑块中的 Ultra",
                    "将 Ultra 显示为滑块最高档选项",
                    self.reference_switch_control(true, (page.slug, 1, 1), theme, cx),
                    true,
                    theme,
                ),
            ],
            theme,
        );
        let dependencies = self.agent_card(
            vec![
                self.agent_row(
                    "Codex 依赖项",
                    "允许 ChatGPT 安装并提供随附的 Node.js 和 Python 工具",
                    self.reference_switch_control(true, (page.slug, 2, 0), theme, cx),
                    false,
                    theme,
                ),
                self.agent_row(
                    "诊断 Codex 工作空间中的问题",
                    "检查当前捆绑包并记录诊断日志",
                    self.reference_button(
                        "诊断",
                        64.0,
                        Some(("icons/search.svg", 16.0)),
                        false,
                        theme,
                    ),
                    false,
                    theme,
                ),
                self.agent_row(
                    "重置并安装工作空间",
                    "下载新的软件包并安装，然后重新加载工具",
                    self.reference_button(
                        "重新安装",
                        92.0,
                        Some(("icons/settings-import.svg", 20.0)),
                        true,
                        theme,
                    ),
                    true,
                    theme,
                ),
            ],
            theme,
        );
        let link_color = match self.mode {
            ThemeMode::Light => gpui::rgba(0x339cffff),
            ThemeMode::Dark => gpui::rgba(0x99ceffff),
        };

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
                    .mt(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .text_color(theme.settings_description)
                    .child("配置新聊天的权限、网页访问和智能体回复")
                    .child(div().text_color(link_color).child("了解更多")),
            )
            .child(
                div()
                    .mt(px(41.5))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child("智能体默认设置"),
            )
            .child(
                div()
                    .mt(px(21.5))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(self.agent_select("用户配置", 100.0, theme))
                    .child(
                        div()
                            .w(px(140.328125))
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap(px(4.0))
                            .text_size(px(14.0))
                            .line_height(px(18.0))
                            .text_color(theme.text_tertiary)
                            .child("打开 config.toml")
                            .child(
                                svg()
                                    .path("icons/settings-external.svg")
                                    .size(px(16.0))
                                    .text_color(theme.text_tertiary),
                            ),
                    ),
            )
            .child(div().mt(px(12.0)).child(defaults))
            .child(
                div()
                    .mt(px(49.5))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child("模型功能"),
            )
            .child(div().mt(px(15.5)).child(model))
            .child(
                div()
                    .mt(px(49.5))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child("工作空间依赖项"),
            )
            .child(div().mt(px(15.5)).child(dependencies))
            .child(
                div()
                    .mt(px(6.0))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(theme.settings_description)
                    .child("当前版本：")
                    .child("26.819.11345"),
            )
            .into_any_element()
    }

    fn content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        viewport_width: f32,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        match page.kind {
            PageKind::Profile => self.profile_content(theme, viewport_width),
            PageKind::Pets => self.pets_content(page, theme, cx),
            PageKind::KeyboardShortcuts => self.keyboard_content(page, theme, cx),
            _ if page.slug == "appearance" => self.appearance_content(page, theme, cx),
            _ if page.slug == "appshots" => self.appshots_content(page, theme, cx),
            _ if page.slug == "computer-use" => self.computer_use_content(page, theme, cx),
            _ if page.slug == "personalization" => self.personalization_content(page, theme, cx),
            _ if page.slug == "chronicle" => self.chronicle_content(page, theme),
            _ if page.slug == "plugins-settings" => self.plugins_content(page, theme, cx),
            _ if page.slug == "hooks-settings" => self.hooks_content(page, theme),
            _ if page.slug == "connections" => self.connections_content(page, theme, cx),
            _ if page.slug == "browser-use" => self.browser_content(page, theme, cx),
            _ if page.slug == "import" => self.import_content(page, theme, cx),
            _ if page.slug == "agent" => self.agent_content(page, theme, cx),
            _ if page.slug == "git-settings" => self.git_content(page, theme, cx),
            _ if page.slug == "local-environments" => self.local_environments_content(page, theme),
            _ if page.slug == "worktrees" => self.worktrees_content(page, theme, cx),
            _ if page.slug == "data-controls" => self.data_controls_content(page, theme),
            PageKind::Standard | PageKind::Usage => {
                self.standard_content(page, theme, cx).into_any_element()
            }
        }
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_mode(self.mode);
        let viewport_width = f32::from(window.viewport_size().width);
        let selected =
            page(self.selected).unwrap_or_else(|| pages().next().expect("settings pages"));
        let nav_scroll = self.nav_scroll.clone();
        let content_scroll = self.content_scroll.clone();

        div()
            .id("settings-shell")
            .size_full()
            .bg(theme.surface)
            .font_family("PingFang SC")
            .text_color(theme.text)
            .flex()
            .child(
                div()
                    .id("settings-sidebar")
                    .w(px(264.3125))
                    .h_full()
                    .flex_none()
                    .relative()
                    .bg(theme.settings_sidebar)
                    .border_r_1()
                    .border_color(theme.border)
                    .flex()
                    .flex_col()
                    .child(div().h(px(46.0)).flex_none())
                    .child(
                        div()
                            .id("settings-back")
                            .mx(px(8.0))
                            .mb(px(8.0))
                            .h(px(31.0))
                            .px(px(8.0))
                            .rounded(px(12.5))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(14.0))
                            .line_height(px(21.0))
                            .text_color(theme.settings_description)
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.sidebar_hover))
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(CloseSettings)))
                            .child(
                                svg()
                                    .path("icons/back.svg")
                                    .size(px(16.0))
                                    .text_color(theme.settings_description),
                            )
                            .child("返回应用"),
                    )
                    .child(
                        div()
                            .mx(px(8.0))
                            .mb(px(10.0))
                            .h(px(29.0))
                            .px(px(8.0))
                            .rounded(px(12.5))
                            .bg(theme.settings_search)
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(14.0))
                            .line_height(px(18.0))
                            .text_color(theme.text_tertiary)
                            .child(
                                svg()
                                    .path("icons/search.svg")
                                    .size(px(18.0))
                                    .text_color(theme.text_tertiary),
                            )
                            .child("搜索设置…"),
                    )
                    .child(
                        div()
                            .id("settings-nav-scroll")
                            .min_h(px(0.0))
                            .flex_1()
                            .overflow_y_scroll()
                            .scrollbar_width(px(0.0))
                            .track_scroll(&nav_scroll)
                            .pl(px(8.0))
                            .pr(px(8.0))
                            .pt(px(1.0))
                            .pb(px(8.0))
                            .flex()
                            .flex_col()
                            .gap(px(11.0))
                            .child(self.nav_group(
                                "个人",
                                &[
                                    "general-settings",
                                    "import",
                                    "profile",
                                    "appearance",
                                    "voice",
                                    "agent",
                                    "personalization",
                                    "pets",
                                    "keyboard-shortcuts",
                                    "usage",
                                    "account",
                                ],
                                theme,
                                cx,
                            ))
                            .child(self.nav_group(
                                "集成",
                                &[
                                    "computer-use",
                                    "chronicle",
                                    "appshots",
                                    "plugins-settings",
                                    "browser-use",
                                ],
                                theme,
                                cx,
                            ))
                            .child(self.nav_group(
                                "编码",
                                &[
                                    "hooks-settings",
                                    "connections",
                                    "git-settings",
                                    "local-environments",
                                    "worktrees",
                                ],
                                theme,
                                cx,
                            ))
                            .child(self.nav_group("已归档", &["data-controls"], theme, cx)),
                    )
                    .child(Self::sidebar_edge_shade(theme)),
            )
            .child(
                div()
                    .id("settings-content-scroll")
                    .min_w(px(0.0))
                    .h_full()
                    .flex_1()
                    .overflow_y_scroll()
                    .track_scroll(&content_scroll)
                    .pl(px(41.0))
                    .pr(px(40.0))
                    .child(self.content(selected, theme, viewport_width, cx)),
            )
    }
}
