//! Notices presentation and interaction for the conversation view.

use std::path::PathBuf;

use gpui::{BoxShadow, Div, IntoElement, ObjectFit, Role, SharedString, div, prelude::*, px, rgba};

use super::{
    NOTICE_BUTTON_HEIGHT, NOTICE_ICON_SIZE, NOTICE_LINE_HEIGHT, NOTICE_RADIUS, NOTICE_TEXT_SIZE,
    TOOL_GROUP_ICON_SIZE, animation::shimmer_label, context::NoticePresentation,
};
use crate::{components::icons::icon, theme::Theme};

#[derive(Clone)]
pub(super) struct ConfigWarningFile {
    pub(super) path: String,
    pub(super) line: Option<u64>,
    pub(super) column: Option<u64>,
}

pub(super) fn retrying_error_activity(
    index: usize,
    message: String,
    details: Option<String>,
    theme: Theme,
) -> impl IntoElement {
    let mut accessible_label = format!("Codex 错误，正在重试：{message}");
    if let Some(details) = details
        .as_deref()
        .filter(|details| !details.trim().is_empty())
    {
        accessible_label.push_str(&format!("；{details}"));
    }
    div()
        .id(("conversation-retrying-error", index))
        .role(Role::Alert)
        .aria_label(accessible_label)
        .w_full()
        .flex()
        .items_start()
        .gap(px(6.0))
        .text_size(px(14.0))
        .line_height(px(21.0))
        .text_color(theme.text_tertiary)
        .child(
            icon("settings-hooks-refresh", theme.text_tertiary.into())
                .size(px(16.0))
                .mt(px(2.0))
                .flex_none(),
        )
        .child(div().min_w(px(0.0)).flex_1().child(message).when_some(
            details.filter(|details| !details.trim().is_empty()),
            |text, details| {
                text.child(
                    div()
                        .text_size(px(NOTICE_TEXT_SIZE))
                        .line_height(px(NOTICE_LINE_HEIGHT))
                        .text_color(theme.text_tertiary)
                        .child(details),
                )
            },
        ))
}

pub(super) fn notice_activity(
    notice: NoticePresentation,
    index: usize,
    theme: Theme,
) -> impl IntoElement {
    let NoticePresentation {
        summary,
        details,
        file,
        accessible_kind,
        outer_gap,
        content_gap,
    } = notice;
    let file_label = file.as_ref().map(|file| {
        let mut label = format!("文件：{}", file.path);
        match (file.line, file.column) {
            (Some(line), Some(column)) => {
                label.push_str(&format!("（第 {line} 行，第 {column} 列）"));
            }
            (Some(line), None) => label.push_str(&format!("（第 {line} 行）")),
            _ => {}
        }
        label
    });
    let mut accessible_label = format!("{accessible_kind}：{summary}");
    if let Some(details) = details
        .as_deref()
        .filter(|details| !details.trim().is_empty())
    {
        accessible_label.push_str(&format!("；{details}"));
    }
    if let Some(file_label) = &file_label {
        accessible_label.push_str(&format!("；{file_label}"));
    }
    let content = div()
        .min_w(px(0.0))
        .flex_1()
        .flex()
        .flex_col()
        .gap(px(content_gap))
        .text_size(px(NOTICE_TEXT_SIZE))
        .line_height(px(NOTICE_LINE_HEIGHT))
        .text_color(theme.text)
        .child(summary)
        .when_some(
            details.filter(|details| !details.trim().is_empty()),
            |content, details| content.child(div().text_color(theme.text_secondary).child(details)),
        )
        .when_some(file_label, |content, file_label| {
            content.child(div().text_color(theme.text_secondary).child(file_label))
        });

    div()
        .id(("conversation-notice", index))
        .role(Role::Alert)
        .aria_label(accessible_label)
        .w_full()
        .py(px(8.0))
        .pl(px(12.0))
        .pr(px(8.0))
        .flex()
        .items_center()
        .gap(px(outer_gap))
        .rounded(px(NOTICE_RADIUS))
        .bg(theme.surface)
        .shadow(vec![
            BoxShadow::new(px(0.0), px(0.0), theme.command_border.into()).spread_radius(px(0.5)),
            BoxShadow::new(px(0.0), px(1.0), rgba(0x0000000d).into()).blur_radius(px(2.0)),
        ])
        .child(
            icon("settings-warning", theme.warning.into())
                .size(px(NOTICE_ICON_SIZE))
                .flex_none(),
        )
        .child(content)
        .when_some(file, |notice, file| {
            let path = PathBuf::from(&file.path);
            notice.child(
                div()
                    .id(("config-warning-open", index))
                    .role(Role::Button)
                    .aria_label(format!("打开配置文件 {}", file.path))
                    .h(px(NOTICE_BUTTON_HEIGHT))
                    .px(px(8.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(9999.0))
                    .border(px(1.0))
                    .border_color(theme.border)
                    .bg(theme.command_surface)
                    .text_size(px(NOTICE_TEXT_SIZE))
                    .line_height(px(18.0))
                    .text_color(theme.text)
                    .cursor_pointer()
                    .hover(|button| button.bg(theme.sidebar_hover))
                    .on_click(move |_, _, cx| cx.open_with_system(&path))
                    .child("打开文件"),
            )
        })
}

pub(super) fn web_search_activity(
    item_id: String,
    query: String,
    _results: serde_json::Value,
    theme: Theme,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("web-search-{item_id}")))
        .h(px(21.0))
        .min_w(px(0.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_color(theme.text.alpha(0.60))
        .child(
            icon("search", theme.text.alpha(0.60).into())
                .size(px(16.0))
                .flex_none(),
        )
        .child(
            div()
                .min_w(px(0.0))
                .truncate()
                .text_size(px(14.0))
                .line_height(px(21.0))
                .child(format!("已搜索网页 ：{query}")),
        )
}

pub(super) fn activity_group_icon(name: &'static str, theme: Theme) -> gpui::AnyElement {
    if name == "activity-native-app" {
        gpui::img(gpui::ImageSource::Resource(gpui::Resource::Embedded(
            "icons/activity-app-placeholder.png".into(),
        )))
        .size(px(TOOL_GROUP_ICON_SIZE))
        .flex_none()
        .object_fit(ObjectFit::Contain)
        .into_any_element()
    } else {
        icon(name, theme.text.alpha(0.60).into())
            .size(px(TOOL_GROUP_ICON_SIZE))
            .flex_none()
            .into_any_element()
    }
}

pub(super) fn context_compaction_activity(
    compaction: crate::agent::AgentContextCompaction,
    shimmer_progress: f32,
    theme: Theme,
) -> Div {
    let color = theme.text.alpha(0.60);
    div()
        .h(px(21.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(px(14.0))
        .line_height(px(21.0))
        .font_family(".SystemUIFont")
        .text_color(color)
        .child(icon("context-compaction", color.into()).size(px(20.0)))
        .when(compaction.completed, |row| row.child("上下文已自动压缩"))
        .when(!compaction.completed, |row| {
            row.child(shimmer_label(
                "正在压缩上下文",
                98.0,
                theme,
                shimmer_progress,
            ))
        })
}
