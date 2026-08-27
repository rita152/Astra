use gpui::{Hsla, Svg, prelude::*, px, svg};

pub fn icon(name: &'static str, color: Hsla) -> Svg {
    svg()
        .path(format!("icons/{name}.svg"))
        .size(px(16.0))
        .text_color(color)
}

pub fn suggestion_icon(color: Hsla) -> Svg {
    svg()
        .path("icons/suggestion.svg")
        .w(px(14.0))
        .h(px(12.0))
        .text_color(color)
}

pub fn chevron(color: Hsla) -> Svg {
    svg()
        .path("icons/chevron-down.svg")
        .size(px(12.0))
        .text_color(color)
}
