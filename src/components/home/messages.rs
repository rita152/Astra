//! Messages presentation and interaction for the conversation view.

use gpui::{
    App, Bounds, Div, Entity, FontWeight, IntoElement, ObjectFit, PathBuilder, Pixels, Role,
    SharedString, TextRun, Window, canvas, div, point, prelude::*, px,
};

use super::{
    CONVERSATION_CONTENT_MAX_WIDTH, HomeView, OpenImagePreview,
    RESPONSE_ACTION_FOOTER_ELECTRON_SHIFT, RESPONSE_ACTION_FOOTER_HEIGHT,
    RESPONSE_ACTION_FOOTER_OFFSET, RESPONSE_ACTION_GAP, RESPONSE_ACTION_ICON_SIZE,
    RESPONSE_TIME_LINE_HEIGHT, RESPONSE_TIME_MARGIN, RESPONSE_TIME_SIZE,
    USER_MESSAGE_BUBBLE_RADIUS, USER_MESSAGE_BUBBLE_SUPERELLIPSE, USER_MESSAGE_FOOTER_GAP,
    USER_MESSAGE_FOOTER_HEIGHT, USER_MESSAGE_FOOTER_OFFSET, USER_MESSAGE_FOOTER_SIDE_MARGIN,
    USER_MESSAGE_HORIZONTAL_PADDING, USER_MESSAGE_LINE_HEIGHT, USER_MESSAGE_MAX_WIDTH_RATIO,
    USER_MESSAGE_PARAGRAPH_GAP, USER_MESSAGE_TEXT_LAYOUT_EPSILON, USER_MESSAGE_TEXT_SIZE,
    USER_MESSAGE_TIME_LINE_HEIGHT, USER_MESSAGE_TIME_SIZE, USER_MESSAGE_VERTICAL_PADDING,
};
use crate::{components::icons::icon, theme::Theme};

pub(super) fn superellipse_corner_points(
    center_x: f32,
    center_y: f32,
    radius: f32,
    start_angle: f32,
) -> impl Iterator<Item = gpui::Point<Pixels>> {
    const SEGMENTS: usize = 12;
    let exponent = 2.0_f32.powf(USER_MESSAGE_BUBBLE_SUPERELLIPSE);
    (1..=SEGMENTS).map(move |step| {
        let angle = start_angle + std::f32::consts::FRAC_PI_2 * step as f32 / SEGMENTS as f32;
        let cosine = angle.cos();
        let sine = angle.sin();
        point(
            px(center_x + cosine.signum() * cosine.abs().powf(2.0 / exponent) * radius),
            px(center_y + sine.signum() * sine.abs().powf(2.0 / exponent) * radius),
        )
    })
}

pub(super) fn user_message_bubble_path(bounds: Bounds<Pixels>) -> gpui::Path<Pixels> {
    let left = f32::from(bounds.left());
    let top = f32::from(bounds.top());
    let right = f32::from(bounds.right());
    let bottom = f32::from(bounds.bottom());
    let radius = USER_MESSAGE_BUBBLE_RADIUS
        .min((right - left) * 0.5)
        .min((bottom - top) * 0.5);
    let mut builder = PathBuilder::fill();
    builder.move_to(point(px(left + radius), px(top)));
    builder.line_to(point(px(right - radius), px(top)));
    for point in superellipse_corner_points(
        right - radius,
        top + radius,
        radius,
        -std::f32::consts::FRAC_PI_2,
    ) {
        builder.line_to(point);
    }
    builder.line_to(point(px(right), px(bottom - radius)));
    for point in superellipse_corner_points(right - radius, bottom - radius, radius, 0.0) {
        builder.line_to(point);
    }
    builder.line_to(point(px(left + radius), px(bottom)));
    for point in superellipse_corner_points(
        left + radius,
        bottom - radius,
        radius,
        std::f32::consts::FRAC_PI_2,
    ) {
        builder.line_to(point);
    }
    builder.line_to(point(px(left), px(top + radius)));
    for point in
        superellipse_corner_points(left + radius, top + radius, radius, std::f32::consts::PI)
    {
        builder.line_to(point);
    }
    builder.close();
    builder
        .build()
        .expect("user message superellipse should tessellate")
}

pub(super) fn user_message_paragraphs(source: &str) -> Vec<String> {
    let mut paragraphs = Vec::new();
    let mut paragraph = String::new();
    for line in source.split('\n') {
        if line.trim().is_empty() {
            if !paragraph.is_empty() {
                paragraphs.push(std::mem::take(&mut paragraph));
            }
        } else {
            if !paragraph.is_empty() {
                paragraph.push('\n');
            }
            paragraph.push_str(line);
        }
    }
    if !paragraph.is_empty() {
        paragraphs.push(paragraph);
    }
    paragraphs
}

pub(super) fn render_user_message_text(source: String) -> Div {
    user_message_paragraphs(&source)
        .into_iter()
        .enumerate()
        .fold(
            div().relative().w_full().min_w(px(0.0)).flex().flex_col(),
            |content, (index, paragraph)| {
                content.child(
                    div()
                        .when(index > 0, |paragraph| {
                            paragraph.mt(px(USER_MESSAGE_PARAGRAPH_GAP))
                        })
                        .child(paragraph),
                )
            },
        )
}

pub(super) fn user_message_text_width(message: &str, window: &mut Window) -> f32 {
    let mut font = window.text_style().font();
    font.family = ".SystemUIFont".into();
    font.weight = FontWeight(430.0);
    let color = window.text_style().color;
    message
        .split('\n')
        .filter(|line| !line.is_empty())
        .map(|line| {
            let run = TextRun {
                len: line.len(),
                font: font.clone(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            f32::from(
                window
                    .text_system()
                    .shape_line(
                        line.to_owned().into(),
                        px(USER_MESSAGE_TEXT_SIZE),
                        &[run],
                        None,
                    )
                    .width(),
            )
        })
        .fold(0.0_f32, f32::max)
}

pub(super) fn user_message_bubble(
    message: String,
    theme: Theme,
    width: f32,
    window: &mut Window,
) -> Div {
    let text_width = user_message_text_width(&message, window);
    let measured_width =
        px(text_width + USER_MESSAGE_HORIZONTAL_PADDING * 2.0 + USER_MESSAGE_TEXT_LAYOUT_EPSILON);
    div()
        .w(measured_width.min(px(width)))
        .max_w_full()
        .min_w(px(0.0))
        .px(px(USER_MESSAGE_HORIZONTAL_PADDING))
        .py(px(USER_MESSAGE_VERTICAL_PADDING))
        .relative()
        .text_size(px(USER_MESSAGE_TEXT_SIZE))
        .line_height(px(USER_MESSAGE_LINE_HEIGHT))
        .font_family(".SystemUIFont")
        .font_weight(FontWeight(430.0))
        .text_color(theme.user_message_text)
        .child(
            canvas(
                |bounds, _, _| user_message_bubble_path(bounds),
                move |_, path, window, _| {
                    window.paint_path(path, theme.user_message_surface);
                },
            )
            .absolute()
            .inset_0(),
        )
        .child(render_user_message_text(message))
}

pub(super) fn user_message_images(
    images: Vec<crate::agent::UserMessageAttachment>,
    home: Entity<HomeView>,
    theme: Theme,
) -> Div {
    use crate::agent::UserMessageAttachment;
    let width = images.len() as f32 * 88.0 - 8.0;
    div().w(px(width)).max_w_full().mb(px(8.0)).child(
        div()
            .id("user-image-scroll")
            .w_full()
            .min_w(px(0.0))
            .overflow_x_scroll()
            .restrict_scroll_to_axis()
            .scrollbar_width(px(0.0))
            .child(div().w(px(width)).flex().gap(px(8.0)).children(
                images.into_iter().enumerate().map(|(index, image)| {
                    let unavailable = match &image {
                        UserMessageAttachment::Local(path) => !path.is_file(),
                        UserMessageAttachment::Unavailable(_) => true,
                        UserMessageAttachment::Remote(_) | UserMessageAttachment::File(_) => false,
                    };
                    let thumbnail = if let UserMessageAttachment::File(path) = &image {
                        div()
                            .p(px(6.0))
                            .text_size(px(11.0))
                            .text_color(theme.text)
                            .child(
                                path.file_name()
                                    .unwrap_or(path.as_os_str())
                                    .to_string_lossy()
                                    .into_owned(),
                            )
                            .into_any_element()
                    } else if unavailable {
                        div()
                            .p(px(6.0))
                            .text_size(px(11.0))
                            .text_color(theme.text_tertiary)
                            .child("图片不可用")
                            .into_any_element()
                    } else {
                        let source: gpui::ImageSource = match &image {
                            UserMessageAttachment::Local(path) => path.clone().into(),
                            UserMessageAttachment::Remote(url) => {
                                SharedString::from(url.clone()).into()
                            }
                            UserMessageAttachment::Unavailable(_)
                            | UserMessageAttachment::File(_) => unreachable!(),
                        };
                        gpui::img(source)
                            .size_full()
                            .rounded(px(10.0))
                            .object_fit(ObjectFit::Cover)
                            .into_any_element()
                    };
                    let click_image = image.clone();
                    let click_home = home.clone();
                    let key_home = home.clone();
                    div()
                        .id(("user-image", index))
                        .debug_selector(move || format!("user-image-{index}"))
                        .size(px(80.0))
                        .flex_none()
                        .rounded(px(12.5))
                        .border_1()
                        .border_color(theme.text.alpha(0.157))
                        .overflow_hidden()
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(!unavailable, |element| {
                            element
                                .role(Role::Button)
                                .aria_label(if let UserMessageAttachment::File(path) = &image {
                                    format!("打开文件 {}", path.display())
                                } else {
                                    format!("打开图片 {}", index + 1)
                                })
                                .focusable()
                                .tab_stop(true)
                                .cursor_pointer()
                                .hover(|style| style.border_color(theme.text.alpha(0.4)))
                                .focus_visible(|style| style.border_color(theme.text))
                                .on_click(move |_, _, cx| {
                                    open_user_image(&click_image, &click_home, cx);
                                })
                                .on_key_down(move |event, window, cx| {
                                    if event.keystroke.key == "tab" {
                                        if event.keystroke.modifiers.shift {
                                            window.focus_prev(cx);
                                        } else {
                                            window.focus_next(cx);
                                        }
                                        cx.stop_propagation();
                                    } else if matches!(
                                        event.keystroke.key.as_str(),
                                        "enter" | "space"
                                    ) {
                                        open_user_image(&image, &key_home, cx);
                                    }
                                })
                        })
                        .child(thumbnail)
                }),
            )),
    )
}

pub(super) fn open_user_image(
    image: &crate::agent::UserMessageAttachment,
    home: &Entity<HomeView>,
    cx: &mut gpui::App,
) {
    match image {
        crate::agent::UserMessageAttachment::Local(path) => {
            home.update(cx, |_, cx| cx.emit(OpenImagePreview(path.clone())))
        }
        crate::agent::UserMessageAttachment::Remote(url) => cx.open_url(url),
        crate::agent::UserMessageAttachment::File(path) => cx.open_with_system(path),
        crate::agent::UserMessageAttachment::Unavailable(_) => {}
    }
    cx.stop_propagation();
}

pub(super) struct UserMessageContent {
    pub(super) continuation: bool,
    pub(super) text: String,
    pub(super) images: Vec<crate::agent::UserMessageAttachment>,
    pub(super) time: String,
}

pub(super) fn current_user_message(
    message: UserMessageContent,
    actions_visible_for_capture: bool,
    theme: Theme,
    window: &mut Window,
    home: Entity<HomeView>,
    content_width: f32,
) -> Div {
    let UserMessageContent {
        continuation,
        text: user_message,
        images: user_images,
        time: user_message_time,
    } = message;
    let reserve_footer = !continuation || actions_visible_for_capture;
    let hover_group: SharedString = "user-message-hover".into();
    let copied_user_message = user_message.clone();
    let keyboard_user_message = user_message.clone();
    div()
        .min_h(px(USER_MESSAGE_VERTICAL_PADDING * 2.0
            + USER_MESSAGE_LINE_HEIGHT
            + if reserve_footer {
                USER_MESSAGE_FOOTER_OFFSET + USER_MESSAGE_FOOTER_HEIGHT
            } else {
                0.0
            }))
        .w_full()
        .flex()
        .flex_col()
        .items_end()
        .child(
            div()
                .relative()
                .group(hover_group.clone())
                .w(px(content_width.min(CONVERSATION_CONTENT_MAX_WIDTH)
                    * USER_MESSAGE_MAX_WIDTH_RATIO))
                .max_w_full()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .items_end()
                .when(!user_images.is_empty(), |container| {
                    container.child(user_message_images(user_images, home, theme))
                })
                .when(!user_message.is_empty(), |container| {
                    container.child(user_message_bubble(
                        user_message,
                        theme,
                        content_width.min(CONVERSATION_CONTENT_MAX_WIDTH)
                            * USER_MESSAGE_MAX_WIDTH_RATIO,
                        window,
                    ))
                })
                .child(
                    div()
                        .when(!reserve_footer, |footer| {
                            footer.absolute().right_0().bottom(px(
                                -(USER_MESSAGE_FOOTER_OFFSET + USER_MESSAGE_FOOTER_HEIGHT)
                            ))
                        })
                        .mt(px(USER_MESSAGE_FOOTER_OFFSET))
                        .mx(px(USER_MESSAGE_FOOTER_SIDE_MARGIN))
                        .h(px(USER_MESSAGE_FOOTER_HEIGHT))
                        .flex()
                        .items_center()
                        .gap(px(USER_MESSAGE_FOOTER_GAP))
                        .child(
                            div()
                                .text_size(px(USER_MESSAGE_TIME_SIZE))
                                .line_height(px(USER_MESSAGE_TIME_LINE_HEIGHT))
                                .text_color(theme.text_tertiary)
                                .opacity(if actions_visible_for_capture {
                                    1.0
                                } else {
                                    0.0
                                })
                                .group_hover(hover_group.clone(), |time| time.opacity(1.0))
                                .child(user_message_time),
                        )
                        .child(
                            div()
                                .id("user-message-copy")
                                .role(Role::Button)
                                .aria_label("复制消息")
                                .focusable()
                                .tab_stop(true)
                                .focus_visible(|s| s.opacity(1.0))
                                .on_key_down(move |e: &gpui::KeyDownEvent, _, cx| {
                                    if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                            keyboard_user_message.clone(),
                                        ));
                                        cx.stop_propagation();
                                    }
                                })
                                .debug_selector(|| "USER_MESSAGE_COPY".to_owned())
                                .size(px(26.0))
                                .rounded(px(10.0))
                                .opacity(if actions_visible_for_capture {
                                    1.0
                                } else {
                                    0.0
                                })
                                .group_hover(hover_group, |button| button.opacity(1.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .hover(move |button| button.bg(theme.sidebar_hover))
                                .active(move |button| button.bg(theme.text.alpha(0.12)))
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                        copied_user_message.clone(),
                                    ));
                                })
                                .child(
                                    icon("message-copy", theme.text_tertiary.into())
                                        .size(px(RESPONSE_ACTION_ICON_SIZE)),
                                ),
                        ),
                ),
        )
}

pub(super) struct ResponseFooterMetadata {
    pub completed_at: Option<String>,
    pub hooks: Vec<crate::agent::AgentHookRun>,
}

pub(super) fn current_response_footer(
    turn_scope: &str,
    assistant_message: String,
    metadata: ResponseFooterMetadata,
    response_feedback: i8,
    home_entity: Entity<HomeView>,
    theme: Theme,
    cx: &mut App,
) -> Div {
    let ResponseFooterMetadata {
        completed_at,
        hooks,
    } = metadata;
    use std::hash::{Hash, Hasher};
    let mut hash = std::hash::DefaultHasher::new();
    turn_scope.hash(&mut hash);
    assistant_message.hash(&mut hash);
    let feedback_id = hash.finish();
    let feedback_open = home_entity.read(cx).response_feedback_menu == Some(feedback_id);
    let feedback_home = home_entity.clone();
    let hook_home = home_entity.clone();
    div()
        .group("response-footer")
        .relative()
        .left(px(RESPONSE_ACTION_FOOTER_ELECTRON_SHIFT))
        .mt(px(RESPONSE_ACTION_FOOTER_OFFSET))
        .w_full()
        .h(px(RESPONSE_ACTION_FOOTER_HEIGHT))
        .flex()
        .items_center()
        .gap(px(RESPONSE_ACTION_GAP))
        .child(
            div()
                .h_full()
                .flex()
                .items_center()
                .gap(px(RESPONSE_ACTION_GAP))
                .child(message_action(
                    "message-copy",
                    "response-copy",
                    0,
                    false,
                    assistant_message.clone(),
                    home_entity.clone(),
                    theme,
                ))
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "response-feedback-{feedback_id}"
                        )))
                        .size(px(26.0))
                        .rounded(px(10.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .role(Role::Button)
                        .aria_label("评价回复")
                        .cursor_pointer()
                        .hover(move |button| button.bg(theme.sidebar_hover))
                        .on_click(move |_, _, cx| {
                            feedback_home.update(cx, |home, cx| {
                                home.response_feedback_menu = if feedback_open {
                                    None
                                } else {
                                    Some(feedback_id)
                                };
                                cx.notify();
                            })
                        })
                        .child(icon("message-feedback", theme.text_tertiary.into()).size(px(16.0))),
                )
                .when(feedback_open, |actions| {
                    actions
                        .child(message_action(
                            "message-thumb-up",
                            "response-thumb-up",
                            1,
                            response_feedback == 1,
                            assistant_message.clone(),
                            home_entity.clone(),
                            theme,
                        ))
                        .child(message_action(
                            "message-thumb-down",
                            "response-thumb-down",
                            2,
                            response_feedback == -1,
                            assistant_message.clone(),
                            home_entity.clone(),
                            theme,
                        ))
                })
                .when(
                    home_entity.read(cx).presentation != super::HomePresentation::SideChat,
                    |actions| {
                        actions.child(message_action(
                            "message-branch",
                            "response-branch",
                            3,
                            false,
                            assistant_message,
                            home_entity,
                            theme,
                        ))
                    },
                ),
        )
        .when(!hooks.is_empty(), |footer| {
            footer.child(super::runtime::hook_button(
                turn_scope, hooks, theme, hook_home, cx,
            ))
        })
        .when_some(completed_at, |footer, completed_at| {
            footer.child(
                div()
                    .ml(px(RESPONSE_TIME_MARGIN))
                    .opacity(0.0)
                    .group_hover("response-footer", |time| time.opacity(1.0))
                    .h_full()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(RESPONSE_TIME_SIZE))
                            .line_height(px(RESPONSE_TIME_LINE_HEIGHT))
                            .font_weight(gpui::FontWeight::NORMAL)
                            .text_color(theme.text_tertiary)
                            .child(completed_at),
                    ),
            )
        })
}

pub(super) fn message_action(
    glyph: &'static str,
    id: &'static str,
    action: usize,
    active: bool,
    assistant_message: String,
    home_entity: Entity<HomeView>,
    theme: Theme,
) -> impl IntoElement {
    let keyboard_message = assistant_message.clone();
    let keyboard_home = home_entity.clone();
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(match action {
            0 => "复制",
            1 => "赞",
            2 => "踩",
            _ => "从此处分叉",
        })
        .focusable()
        .tab_stop(true)
        .focus_visible(move |s| s.bg(theme.sidebar_hover))
        .on_key_down(move |e: &gpui::KeyDownEvent, _, cx| {
            if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                match action {
                    0 => cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                        keyboard_message.clone(),
                    )),
                    1 | 2 => keyboard_home.update(cx, |home, cx| {
                        let value = if action == 1 { 1 } else { -1 };
                        home.response_feedback = if home.response_feedback == value {
                            0
                        } else {
                            value
                        };
                        cx.notify();
                    }),
                    _ => {}
                }
                cx.stop_propagation();
            }
        })
        .size(px(26.0))
        .rounded(px(10.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .when(active, |button| {
            button.bg(theme.sidebar_hover).text_color(theme.text)
        })
        .hover(move |style| style.bg(theme.sidebar_hover))
        .active(move |style| style.bg(theme.text.alpha(0.12)))
        .on_click(move |_, _, cx| match action {
            0 => cx.write_to_clipboard(gpui::ClipboardItem::new_string(assistant_message.clone())),
            1 | 2 => {
                let value = if action == 1 { 1 } else { -1 };
                home_entity.update(cx, |home, cx| {
                    home.response_feedback = if home.response_feedback == value {
                        0
                    } else {
                        value
                    };
                    cx.notify();
                });
            }
            _ => {}
        })
        .child(icon(glyph, theme.text_tertiary.into()).size(px(RESPONSE_ACTION_ICON_SIZE)))
}
