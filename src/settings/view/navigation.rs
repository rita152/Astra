//! Navigation settings presentation.

use gpui::{Context, IntoElement, div, prelude::*, px, svg};

use super::SettingsView;
use crate::{
    settings::{PageSpec, page},
    theme::Theme,
};

impl SettingsView {
    pub(super) fn nav_icon(slug: &'static str, theme: Theme) -> impl IntoElement {
        svg()
            .path(format!("icons/settings-{slug}.svg"))
            .size(px(16.0))
            .relative()
            .top(px(1.0))
            .text_color(theme.text)
    }
    pub(super) fn sidebar_edge_shade(theme: Theme) -> gpui::AnyElement {
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
    pub(super) fn nav_row(
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
    pub(super) fn nav_group(
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
}
