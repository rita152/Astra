//! Chronicle settings presentation.

use gpui::{IntoElement, div, point, prelude::*, px};

use super::SettingsView;
use crate::{
    settings::PageSpec,
    theme::{Theme, ThemeMode},
};

impl SettingsView {
    pub(super) fn chronicle_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
    ) -> gpui::AnyElement {
        let rows = page.sections[0].rows;
        let link_color = match self.mode {
            ThemeMode::Light => gpui::rgba(0x339cffff),
            ThemeMode::Dark => gpui::rgba(0x99ceffff),
        };
        let inner_fill = match self.mode {
            ThemeMode::Light => gpui::rgba(0xf7f7feff),
            ThemeMode::Dark => gpui::rgba(0x1c1c23ff),
        };
        let inner_tiles = match self.mode {
            ThemeMode::Light => super::artwork::chronicle_inner_tiles(ThemeMode::Light),
            ThemeMode::Dark => super::artwork::chronicle_inner_tiles(ThemeMode::Dark),
        };
        let chronicle_text_weight = crate::theme::UI_BODY_FONT_WEIGHT;
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
                        for &(x, y, width, height, color) in super::artwork::chronicle_art_tiles() {
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
}
