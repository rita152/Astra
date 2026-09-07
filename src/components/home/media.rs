//! Media presentation and interaction for the conversation view.

use gpui::{
    BoxShadow, Entity, FontWeight, IntoElement, ObjectFit, Role, ScrollHandle, SharedString,
    Transformation, div, prelude::*, px, radians, rgba,
};

use super::{HomeView, OpenImagePreview, RetryImageGeneration, tools::toggle_tool_activity_group};
use crate::{
    agent::{
        AgentImageGeneration, AgentImageGenerationFailure, AgentImageGenerationStatus,
        AgentImageView,
    },
    components::icons::icon,
    theme::Theme,
};

pub(super) fn image_generation_preview_size(dimensions: Option<(u32, u32)>) -> (f32, f32) {
    let Some((width, height)) = dimensions.filter(|(width, height)| *width > 0 && *height > 0)
    else {
        return (480.0, 480.0);
    };
    let aspect = width as f32 / height as f32;
    if aspect >= 1.0 {
        (480.0, 480.0 / aspect)
    } else {
        (480.0 * aspect, 480.0)
    }
}

pub(super) fn image_generation_failure_copy(image: &AgentImageGeneration) -> (String, String) {
    if let Some(AgentImageGenerationFailure::UsageLimitExceeded {
        limit_id,
        resets_at,
    }) = &image.failure
    {
        let reset = resets_at
            .and_then(|timestamp| chrono::DateTime::from_timestamp(timestamp, 0))
            .map(|timestamp| {
                timestamp
                    .with_timezone(&chrono::Local)
                    .format("%m月%d日 %H:%M")
                    .to_string()
            });
        let detail = match reset {
            Some(reset) => format!("额度 {limit_id} 将于 {reset} 重置。"),
            None => format!("额度 {limit_id} 暂时不可用。"),
        };
        return ("图像生成额度已用完".to_owned(), detail);
    }
    (
        "无法显示生成的图像".to_owned(),
        image
            .load_error
            .clone()
            .unwrap_or_else(|| "图像生成失败，请重试。".to_owned()),
    )
}

pub(super) fn image_generation_error_activity(
    home_entity: Entity<HomeView>,
    image: AgentImageGeneration,
    theme: Theme,
) -> gpui::AnyElement {
    let item_id = image.id.clone();
    let retry_home = home_entity.clone();
    let key_home = home_entity;
    let (title, detail) = image_generation_failure_copy(&image);
    div()
        .id(SharedString::from(format!(
            "image-generation-error-{item_id}"
        )))
        .w(px(360.0))
        .max_w_full()
        .p(px(16.0))
        .rounded(px(16.0))
        .border(px(1.0))
        .border_color(theme.warning.alpha(0.24))
        .bg(theme.command_surface)
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(
            div()
                .flex()
                .items_start()
                .gap(px(10.0))
                .child(
                    icon("permission-warning", theme.warning.into())
                        .size(px(20.0))
                        .flex_none(),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .child(
                            div()
                                .font_family(".SystemUIFont")
                                .text_size(px(14.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text)
                                .child(title),
                        )
                        .child(
                            div()
                                .font_family(".SystemUIFont")
                                .text_size(px(13.0))
                                .line_height(px(18.0))
                                .text_color(theme.text_secondary)
                                .child(detail),
                        ),
                ),
        )
        .child(
            div()
                .id(SharedString::from(format!(
                    "image-generation-retry-{item_id}"
                )))
                .debug_selector(|| "image-generation-retry".to_owned())
                .h(px(36.0))
                .px(px(14.0))
                .self_start()
                .rounded(px(10.0))
                .border(px(1.0))
                .border_color(theme.border)
                .bg(theme.control)
                .role(Role::Button)
                .aria_label("重试图像生成")
                .focusable()
                .tab_stop(true)
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .font_family(".SystemUIFont")
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text)
                .hover(move |button| button.bg(theme.elevated))
                .on_click(move |_, _, cx| {
                    retry_home.update(cx, |_, cx| cx.emit(RetryImageGeneration));
                    cx.stop_propagation();
                })
                .on_key_down(move |event, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        key_home.update(cx, |_, cx| cx.emit(RetryImageGeneration));
                        cx.stop_propagation();
                    }
                })
                .child("重试"),
        )
        .into_any_element()
}

pub(super) fn image_generation_activity(
    home_entity: Entity<HomeView>,
    image: AgentImageGeneration,
    shimmer_progress: f32,
    theme: Theme,
) -> gpui::AnyElement {
    let item_id = image.id.clone();
    if image.status == AgentImageGenerationStatus::InProgress {
        let dots = (0..64).map(|index| {
            let row = index / 8;
            let column = index % 8;
            let phase = (shimmer_progress + (row + column) as f32 / 14.0) % 1.0;
            let distance = ((row as f32 - 5.0).powi(2) + (column as f32 - 5.0).powi(2)).sqrt();
            let alpha = ((1.0 - distance / 8.0) * (0.10 + phase * 0.18)).clamp(0.03, 0.28);
            div()
                .size(px(2.0))
                .rounded_full()
                .bg(theme.text.alpha(alpha))
        });
        return div()
            .id(SharedString::from(format!(
                "image-generation-loading-{item_id}"
            )))
            .size(px(178.0))
            .flex_none()
            .relative()
            .overflow_hidden()
            .rounded(px(16.0))
            .bg(theme.text.alpha(0.055))
            .role(Role::Status)
            .aria_label("正在生成图像...")
            .child(
                div()
                    .absolute()
                    .right(px(18.0))
                    .bottom(px(18.0))
                    .w(px(58.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(5.0))
                    .children(dots),
            )
            .into_any_element();
    }

    let path_missing = image.path.as_ref().is_some_and(|path| !path.is_file());
    if image.status == AgentImageGenerationStatus::Failed
        || image.load_error.is_some()
        || image.path.is_none()
        || path_missing
    {
        let mut failed = image;
        if path_missing && failed.load_error.is_none() {
            failed.load_error = Some("生成的图像文件已移动或删除。".to_owned());
        }
        return image_generation_error_activity(home_entity, failed, theme);
    }

    let path = image
        .path
        .clone()
        .expect("completed image path checked above");
    let preview_path = path.clone();
    let keyboard_path = path.clone();
    let click_home = home_entity.clone();
    let keyboard_home = home_entity;
    let (width, height) = image_generation_preview_size(image.dimensions);
    let dimensions = image
        .dimensions
        .map(|(width, height)| format!("，{width}×{height}"))
        .unwrap_or_default();
    div()
        .id(SharedString::from(format!("image-generation-{item_id}")))
        .debug_selector(|| "image-generation-preview".to_owned())
        .w(px(width))
        .h(px(height))
        .max_w_full()
        .flex_none()
        .overflow_hidden()
        .rounded(px(16.0))
        .bg(theme.surface)
        .role(Role::Button)
        .aria_label(format!("已生成图像 1{dimensions}"))
        .focusable()
        .tab_stop(true)
        .cursor_pointer()
        .focus_visible(|style| {
            style.shadow(vec![
                BoxShadow::new(px(0.0), px(0.0), rgba(0x3a83f7ff).into()).spread_radius(px(2.0)),
            ])
        })
        .on_click(move |_, _, cx| {
            click_home.update(cx, |_, cx| cx.emit(OpenImagePreview(preview_path.clone())));
            cx.stop_propagation();
        })
        .on_key_down(move |event, _, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                keyboard_home.update(cx, |_, cx| cx.emit(OpenImagePreview(keyboard_path.clone())));
                cx.stop_propagation();
            }
        })
        .child(
            gpui::img(path)
                .w(px(width))
                .h(px(height))
                .rounded(px(16.0))
                .object_fit(ObjectFit::Contain),
        )
        .into_any_element()
}

pub(super) fn image_view_activity(
    home_entity: Entity<HomeView>,
    images: Vec<AgentImageView>,
    expanded: bool,
    active_turn: bool,
    theme: Theme,
) -> impl IntoElement {
    let image = &images[0];
    let item_id = image.id.clone();
    let count = images.len();
    let click_home = home_entity.clone();
    let key_home = home_entity.clone();
    let click_item_id = item_id.clone();
    let key_item_id = item_id.clone();
    let hover_group: SharedString = format!("image-view-header-{item_id}").into();
    let label = if expanded {
        format!("已查看 {count} 张图像，折叠图像")
    } else {
        format!("已查看 {count} 张图像，展开图像")
    };

    div()
        .id(SharedString::from(format!("image-view-{item_id}")))
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .items_start()
        .child(
            div()
                .id(SharedString::from(format!("image-view-header-{item_id}")))
                .group(hover_group.clone())
                .h(px(21.0))
                .max_w_full()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(4.0))
                .rounded(px(6.0))
                .focusable()
                .tab_stop(true)
                .role(Role::Button)
                .aria_expanded(expanded)
                .aria_label(label)
                .cursor_pointer()
                .focus_visible(|style| {
                    style.px(px(2.0)).shadow(vec![
                        BoxShadow::new(px(0.0), px(0.0), rgba(0x3a83f7ff).into())
                            .spread_radius(px(2.0))
                            .inset(),
                    ])
                })
                .on_click(move |_, _, cx| {
                    toggle_tool_activity_group(
                        &click_home,
                        &click_item_id,
                        active_turn,
                        &ScrollHandle::new(),
                        cx,
                    );
                })
                .on_key_down(move |event, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        toggle_tool_activity_group(
                            &key_home,
                            &key_item_id,
                            active_turn,
                            &ScrollHandle::new(),
                            cx,
                        );
                        cx.stop_propagation();
                    }
                })
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_color(theme.text.alpha(0.60))
                        .child(
                            icon("activity-image", theme.text.alpha(0.60).into())
                                .size(px(21.0))
                                .flex_none(),
                        )
                        .child(
                            div()
                                .min_w(px(0.0))
                                .truncate()
                                .text_size(px(14.0))
                                .line_height(px(21.0))
                                .font_family(".SystemUIFont")
                                .font_weight(FontWeight::NORMAL)
                                .text_color(theme.text.alpha(0.40))
                                .child(format!("已查看 {count} 张图像")),
                        ),
                )
                .child(
                    icon("settings-chevron-right", theme.text.alpha(0.60).into())
                        .size(px(20.0))
                        .flex_none()
                        .opacity(if expanded { 1.0 } else { 0.0 })
                        .group_hover(hover_group, |chevron| chevron.opacity(1.0))
                        .with_transformation(Transformation::rotate(radians(if expanded {
                            std::f32::consts::FRAC_PI_2
                        } else {
                            0.0
                        }))),
                ),
        )
        .when(expanded, |activity| {
            activity.child(div().pt(px(8.0)).pb(px(4.0)).flex().gap(px(8.0)).children(
                images.into_iter().map(|image| {
                    let item_id = image.id;
                    let path = image.path;
                    let thumbnail_path = path.clone();
                    let thumbnail_key_path = path.clone();
                    let preview_home = home_entity.clone();
                    let preview_key_home = home_entity.clone();
                    div()
                        .id(SharedString::from(format!(
                            "image-view-thumbnail-{item_id}"
                        )))
                        .size(px(80.0))
                        .flex_none()
                        .rounded(px(8.0))
                        .border(px(1.0))
                        .border_color(theme.text.alpha(0.20))
                        .overflow_hidden()
                        .role(Role::Button)
                        .aria_label("已检查的图像")
                        .focusable()
                        .tab_stop(true)
                        .cursor_pointer()
                        .focus_visible(|style| {
                            style.shadow(vec![
                                BoxShadow::new(px(0.0), px(0.0), rgba(0x3a83f7ff).into())
                                    .spread_radius(px(2.0)),
                            ])
                        })
                        .on_click(move |_, _, cx| {
                            preview_home.update(cx, |_, cx| {
                                cx.emit(OpenImagePreview(thumbnail_path.clone()));
                            });
                            cx.stop_propagation();
                        })
                        .on_key_down(move |event, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                preview_key_home.update(cx, |_, cx| {
                                    cx.emit(OpenImagePreview(thumbnail_key_path.clone()));
                                });
                                cx.stop_propagation();
                            }
                        })
                        .child(
                            gpui::img(path)
                                .size_full()
                                .rounded(px(6.0))
                                .object_fit(ObjectFit::Cover),
                        )
                }),
            ))
        })
}

pub(super) fn tool_image_format(bytes: &[u8], mime: Option<&str>) -> gpui::ImageFormat {
    // Older Computer Use results sometimes label JPEG screenshots as image/png.
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        gpui::ImageFormat::Png
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        gpui::ImageFormat::Jpeg
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        gpui::ImageFormat::Gif
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        gpui::ImageFormat::Webp
    } else {
        mime.and_then(gpui::ImageFormat::from_mime_type)
            .unwrap_or(gpui::ImageFormat::Png)
    }
}
