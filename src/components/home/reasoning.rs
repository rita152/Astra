//! Reasoning presentation and interaction for the conversation view.

use std::time::Instant;

use gpui::{
    App, BoxShadow, Div, Entity, FontWeight, IntoElement, Role, ScrollHandle, SharedString,
    Transformation, div, prelude::*, px, radians, rgba,
};

use super::{
    DISCLOSURE_FOCUS_PADDING, HomeView, REASONING_BODY_MAX_HEIGHT, REASONING_BODY_TOP_GAP,
    REASONING_CHEVRON_SIZE, REASONING_HEADER_HEIGHT, REASONING_LINE_HEIGHT, REASONING_TEXT_SIZE,
    animation::thinking_shimmer, conversation::nested_scroll_consumed,
};
use crate::{components::icons::icon, conversation::ReasoningActivityPresentation, theme::Theme};

#[derive(Clone, Copy, Debug)]
pub(super) struct ReasoningDisclosureTransition {
    pub(super) progress: f32,
    pub(super) from: f32,
    pub(super) target: f32,
    pub(super) started_at: Option<Instant>,
}

impl ReasoningDisclosureTransition {
    pub(super) fn settled(expanded: bool) -> Self {
        let progress = if expanded { 1.0 } else { 0.0 };
        Self {
            progress,
            from: progress,
            target: progress,
            started_at: None,
        }
    }
}

pub(super) fn format_reasoning_elapsed(elapsed_ms: u64) -> String {
    let total_seconds = elapsed_ms.div_ceil(1_000).max(1);
    let hours = total_seconds / 3_600;
    let minutes = total_seconds % 3_600 / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        if minutes > 0 {
            format!("{hours}h {minutes}m")
        } else {
            format!("{hours}h")
        }
    } else if minutes > 0 {
        if seconds > 0 {
            format!("{minutes}m {seconds}s")
        } else {
            format!("{minutes}m")
        }
    } else {
        format!("{seconds}s")
    }
}

pub(super) fn reasoning_header_label(reasoning: &ReasoningActivityPresentation) -> String {
    if reasoning.is_active() {
        "正在思考".to_owned()
    } else if let Some(elapsed_ms) = reasoning.elapsed_ms() {
        format!("思考了 {}", format_reasoning_elapsed(elapsed_ms))
    } else {
        "完成思考".to_owned()
    }
}

pub(super) fn active_reasoning_body(text: &str) -> String {
    let trimmed = text.trim_start();
    let Some(after_opening) = trimmed.strip_prefix("**") else {
        return trimmed.to_owned();
    };
    let first_line = after_opening
        .split_once('\n')
        .map_or(after_opening, |(line, _)| line);
    let Some(closing) = first_line.find("**") else {
        return String::new();
    };
    after_opening[closing + 2..].trim_start().to_owned()
}

pub(super) fn reasoning_body_text(reasoning: &ReasoningActivityPresentation) -> String {
    let display_text = reasoning.display_text();
    if reasoning.is_active() {
        active_reasoning_body(&display_text)
    } else {
        display_text
    }
}

pub(super) fn completed_reasoning_body(text: &str) -> (Option<String>, String) {
    let trimmed = text.trim_start();
    let Some(after_opening) = trimmed.strip_prefix("**") else {
        return (None, trimmed.to_owned());
    };
    let first_line = after_opening
        .split_once('\n')
        .map_or(after_opening, |(line, _)| line);
    let Some(closing) = first_line.find("**") else {
        return (None, trimmed.to_owned());
    };
    let title = after_opening[..closing].trim().to_owned();
    let body = after_opening[closing + 2..].trim_start().to_owned();
    ((!title.is_empty()).then_some(title), body)
}

pub(super) fn completed_reasoning_body_element(text: String) -> Div {
    let (title, body) = completed_reasoning_body(&text);
    let has_title = title.is_some();
    div()
        .w_full()
        .flex()
        .flex_col()
        .when_some(title, |content, title| {
            content.child(div().font_weight(FontWeight::SEMIBOLD).child(title))
        })
        .when(!body.is_empty(), |content| {
            content.child(
                div()
                    .when(has_title, |body| body.mt(px(REASONING_BODY_TOP_GAP)))
                    .font_weight(FontWeight::NORMAL)
                    .child(body),
            )
        })
}

pub(super) fn toggle_reasoning_item(
    home_entity: &Entity<HomeView>,
    item_id: &str,
    scroll_handle: &ScrollHandle,
    cx: &mut App,
) {
    let item_id = item_id.to_owned();
    home_entity.update(cx, |home, cx| {
        if !home.expanded_reasoning.remove(&item_id) {
            home.expanded_reasoning.insert(item_id);
            scroll_handle.scroll_to_bottom();
        }
        home.conversation_cache_dirty = true;
        cx.notify();
    });
}

pub(super) fn reasoning_activity(
    home_entity: Entity<HomeView>,
    reasoning: ReasoningActivityPresentation,
    expanded: bool,
    disclosure_progress: f32,
    scroll_handle: ScrollHandle,
    thinking_shimmer_progress: f32,
    theme: Theme,
) -> Div {
    let item_id = reasoning.item_id.clone();
    let hover_group: SharedString = format!("reasoning-activity-{item_id}").into();
    let active = reasoning.is_active();
    let body_text = reasoning_body_text(&reasoning);
    let has_content = !body_text.trim().is_empty();
    let can_toggle = !active && has_content;
    let header_label = reasoning_header_label(&reasoning);
    let accessible_label = if expanded {
        format!("{header_label}，折叠推理内容")
    } else {
        format!("{header_label}，展开推理内容")
    };
    let click_home_entity = home_entity.clone();
    let click_item_id = item_id.clone();
    let click_scroll_handle = scroll_handle.clone();
    let key_item_id = item_id.clone();
    let key_scroll_handle = scroll_handle.clone();
    let nested_scroll_handle = scroll_handle.clone();

    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .items_start()
        .child(
            div()
                .id(SharedString::from(format!("reasoning-activity-{item_id}")))
                .group(hover_group.clone())
                .h(px(REASONING_HEADER_HEIGHT))
                .max_w_full()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(4.0))
                .rounded(px(6.0))
                .when(can_toggle, |header| {
                    header
                        .focusable()
                        .tab_stop(true)
                        .role(Role::Button)
                        .aria_expanded(expanded)
                        .aria_label(accessible_label)
                        .focus_visible(|style| {
                            style.px(px(DISCLOSURE_FOCUS_PADDING)).shadow(vec![
                                BoxShadow::new(px(0.0), px(0.0), rgba(0x3a83f7ff).into())
                                    .spread_radius(px(2.0))
                                    .inset(),
                            ])
                        })
                        .cursor_pointer()
                        .on_click(move |_, _, cx| {
                            toggle_reasoning_item(
                                &click_home_entity,
                                &click_item_id,
                                &click_scroll_handle,
                                cx,
                            );
                        })
                        .on_key_down(move |event, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                toggle_reasoning_item(
                                    &home_entity,
                                    &key_item_id,
                                    &key_scroll_handle,
                                    cx,
                                );
                                cx.stop_propagation();
                            }
                        })
                })
                .child(if active {
                    div()
                        .child(thinking_shimmer(theme, thinking_shimmer_progress))
                        .into_any_element()
                } else {
                    div()
                        .min_w(px(0.0))
                        .max_w(px(718.0))
                        .truncate()
                        .text_size(px(REASONING_TEXT_SIZE))
                        .line_height(px(REASONING_LINE_HEIGHT))
                        .font_family(".SystemUIFont")
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.text.alpha(0.30))
                        .group_hover(hover_group.clone(), move |label| {
                            label.text_color(theme.text)
                        })
                        .child(header_label)
                        .into_any_element()
                })
                .when(can_toggle, |header| {
                    header.child(
                        icon("settings-chevron-right", theme.text.alpha(0.60).into())
                            .size(px(REASONING_CHEVRON_SIZE))
                            .flex_none()
                            .opacity(disclosure_progress.clamp(0.0, 1.0))
                            .group_hover(hover_group, |chevron| chevron.opacity(1.0))
                            .with_transformation(Transformation::rotate(radians(
                                std::f32::consts::FRAC_PI_2 * disclosure_progress.clamp(0.0, 1.0),
                            ))),
                    )
                }),
        )
        .when(has_content, |activity| {
            let visibility = disclosure_progress.clamp(0.0, 1.0);
            activity.child(
                div()
                    .w_full()
                    .overflow_hidden()
                    .max_h(px(
                        (REASONING_BODY_MAX_HEIGHT + REASONING_BODY_TOP_GAP) * visibility
                    ))
                    .opacity(visibility)
                    .when(visibility <= f32::EPSILON, |body| body.invisible())
                    .child(
                        div().w_full().pt(px(REASONING_BODY_TOP_GAP)).child(
                            div()
                                .id(SharedString::from(format!("reasoning-body-{item_id}")))
                                .w_full()
                                .max_h(px(REASONING_BODY_MAX_HEIGHT))
                                .overflow_scroll()
                                .restrict_scroll_to_axis()
                                .scrollbar_width(px(0.0))
                                .track_scroll(&scroll_handle)
                                .on_scroll_wheel(move |event, window, cx| {
                                    if nested_scroll_consumed(&nested_scroll_handle, event, window)
                                    {
                                        cx.stop_propagation();
                                    }
                                })
                                .text_size(px(REASONING_TEXT_SIZE))
                                .line_height(px(REASONING_LINE_HEIGHT))
                                .font_family(".SystemUIFont")
                                .font_weight(FontWeight::NORMAL)
                                .text_color(theme.text.alpha(0.50))
                                .child(if active {
                                    div().child(body_text)
                                } else {
                                    completed_reasoning_body_element(body_text)
                                }),
                        ),
                    ),
            )
        })
}
