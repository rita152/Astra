//! Activity presentation and interaction for the conversation view.

use gpui::{Div, IntoElement, ScrollHandle, SharedString, div, prelude::*, px};

use super::{
    NOTICE_ERROR_CONTENT_GAP, NOTICE_ERROR_GAP, NOTICE_WARNING_CONTENT_GAP, NOTICE_WARNING_GAP,
    animation::thinking_shimmer,
    collaboration::{collaboration_activity, collaboration_ui_identity},
    context::{ConversationRenderContext, NoticePresentation, ToolGroupDisclosure},
    mcp::mcp_tool_call_activity,
    media::{image_generation_activity, image_view_activity},
    notices::{
        ConfigWarningFile, context_compaction_activity, notice_activity, retrying_error_activity,
        web_search_activity,
    },
    reasoning::reasoning_activity,
    timeline::{ActivityStreamUnit, activity_stream_units},
    tools::{command_execution_activity, tool_activity_group},
};
use crate::{
    components::{
        file_change::{FileChangeActivityCallback, render_file_change_activity},
        markdown::render_assistant_markdown,
    },
    conversation::ConversationActivity,
};

pub(super) fn render_activity_stream_unit(
    render: &ConversationRenderContext,
    index: usize,
    unit: ActivityStreamUnit,
    show_thinking_tail: bool,
) -> gpui::AnyElement {
    let home_entity = render.home_entity.clone();
    let theme = render.theme;
    let thinking_shimmer_progress = render.thinking_shimmer_progress;
    let expanded_reasoning = &render.disclosures.expanded_reasoning;
    let reasoning_disclosure_progress = &render.disclosures.reasoning_disclosure_progress;
    let reasoning_scroll_handles = &render.disclosures.reasoning_scroll_handles;
    let expanded_tool_groups = &render.disclosures.expanded_tool_groups;
    let collapsed_active_tool_groups = &render.disclosures.collapsed_active_tool_groups;
    let tool_group_disclosure_progress = &render.disclosures.tool_group_disclosure_progress;
    let tool_group_scroll_handles = &render.disclosures.tool_group_scroll_handles;
    let expanded_commands = &render.disclosures.expanded_commands;
    let command_scroll_handles = &render.disclosures.command_scroll_handles;
    let expanded_collaborations = &render.disclosures.expanded_collaborations;
    match unit {
        ActivityStreamUnit::ToolGroup(group) => {
            let active = group.is_active();
            let expanded = if active {
                !collapsed_active_tool_groups.contains(&group.id)
            } else {
                expanded_tool_groups.contains(&group.id)
            };
            let settled_progress = if expanded { 1.0 } else { 0.0 };
            let (disclosure_progress, chevron_progress) = tool_group_disclosure_progress
                .get(&group.id)
                .copied()
                .unwrap_or((settled_progress, settled_progress));
            let scroll_handle = tool_group_scroll_handles
                .get(&group.id)
                .cloned()
                .unwrap_or_else(ScrollHandle::new);
            tool_activity_group(
                home_entity,
                group,
                ToolGroupDisclosure {
                    review_views: render.disclosures.auto_review_views.clone(),
                    expanded,
                    disclosure_progress,
                    chevron_progress,
                    scroll_handle,
                },
                expanded_commands,
                command_scroll_handles,
                theme,
            )
            .into_any_element()
        }
        ActivityStreamUnit::Standalone(activity) => match activity {
            ConversationActivity::AutoApprovalReview(review) => div()
                .w_full()
                .children(
                    render
                        .disclosures
                        .auto_review_views
                        .get(&review.review.key)
                        .cloned(),
                )
                .into_any_element(),
            ConversationActivity::StrictReview(requirement) => {
                crate::components::auto_approval::strict_review(&requirement, theme)
                    .into_any_element()
            }
            ConversationActivity::GuardianWarning(warning) => {
                crate::components::auto_approval::guardian_warning(&warning, theme)
                    .into_any_element()
            }
            ConversationActivity::AssistantMessage { item_id, text } if !text.is_empty() => {
                render_assistant_markdown(&text, theme, &item_id).into_any_element()
            }
            ConversationActivity::Reasoning(reasoning) => {
                let expanded =
                    reasoning.is_active() || expanded_reasoning.contains(&reasoning.item_id);
                let disclosure_progress = reasoning_disclosure_progress
                    .get(&reasoning.item_id)
                    .copied()
                    .unwrap_or(if expanded { 1.0 } else { 0.0 });
                let scroll_handle = reasoning_scroll_handles
                    .get(&reasoning.item_id)
                    .cloned()
                    .unwrap_or_else(ScrollHandle::new);
                reasoning_activity(
                    home_entity,
                    reasoning,
                    expanded,
                    disclosure_progress,
                    scroll_handle,
                    thinking_shimmer_progress,
                    theme,
                )
                .into_any_element()
            }
            ConversationActivity::ImageView(image) => {
                let expanded = if show_thinking_tail {
                    !collapsed_active_tool_groups.contains(&image.id)
                } else {
                    expanded_tool_groups.contains(&image.id)
                };
                image_view_activity(
                    home_entity,
                    vec![image],
                    expanded,
                    show_thinking_tail,
                    theme,
                )
                .into_any_element()
            }
            ConversationActivity::ImageViews(images) => {
                let id = &images[0].id;
                let expanded = if show_thinking_tail {
                    !collapsed_active_tool_groups.contains(id)
                } else {
                    expanded_tool_groups.contains(id)
                };
                image_view_activity(home_entity, images, expanded, show_thinking_tail, theme)
                    .into_any_element()
            }
            ConversationActivity::ImageGeneration(image) => {
                image_generation_activity(home_entity, image, thinking_shimmer_progress, theme)
                    .into_any_element()
            }
            ConversationActivity::ContextCompaction(compaction) => {
                context_compaction_activity(compaction, thinking_shimmer_progress, theme)
                    .into_any_element()
            }
            ConversationActivity::Collaboration(collaboration) => {
                let expanded =
                    expanded_collaborations.contains(&collaboration_ui_identity(&collaboration));
                collaboration_activity(home_entity, collaboration, expanded, theme)
                    .into_any_element()
            }
            ConversationActivity::Plan(plan) => {
                let feedback_open =
                    expanded_commands.contains(&format!("plan-feedback-{}", plan.id));
                let in_panel = expanded_commands.contains(&format!("plan-in-panel:{}", plan.id));
                super::progress::plan_activity(
                    home_entity,
                    plan,
                    feedback_open,
                    in_panel,
                    thinking_shimmer_progress,
                    theme,
                )
                .into_any_element()
            }
            ConversationActivity::Sleep(sleep) => {
                super::progress::sleep_activity(sleep, theme).into_any_element()
            }
            ConversationActivity::TurnPlan(_) => div().into_any_element(),
            ConversationActivity::WebSearch(search) => {
                web_search_activity(search, theme).into_any_element()
            }
            ConversationActivity::QuestionReply {
                item_id,
                question,
                answer,
            } => div()
                .w_full()
                .flex()
                .justify_end()
                .child(
                    div()
                        .id(SharedString::from(format!("question-reply-{item_id}")))
                        .w(px(515.2))
                        .max_w_full()
                        .min_w(px(0.0))
                        .px(px(16.0))
                        .py(px(10.0))
                        .rounded(px(22.0))
                        .bg(theme.user_message_surface)
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .text_size(px(14.0))
                        .line_height(px(22.75))
                        .text_color(theme.user_message_text)
                        .child(
                            div()
                                .text_color(theme.user_message_text.alpha(0.65))
                                .truncate()
                                .child(question),
                        )
                        .child(answer),
                )
                .into_any_element(),
            ConversationActivity::McpToolCall(tool_call) => {
                mcp_tool_call_activity(*tool_call, theme).into_any_element()
            }
            ConversationActivity::Command(command) => command_execution_activity(
                home_entity,
                command,
                expanded_commands,
                command_scroll_handles,
                theme,
            )
            .into_any_element(),
            ConversationActivity::FileChange(model) => {
                let target = home_entity;
                let callback = FileChangeActivityCallback::new(move |event, _, cx| {
                    target.update(cx, move |home, cx| {
                        home.handle_file_change_activity_event(event, cx)
                    });
                });
                let expanded = expanded_commands.contains(&model.item_id);
                render_file_change_activity(&model, expanded, theme, callback).into_any_element()
            }
            ConversationActivity::ProtocolError {
                message,
                details,
                will_retry: true,
            } => retrying_error_activity(index, message, details, theme).into_any_element(),
            ConversationActivity::ProtocolError {
                message,
                details,
                will_retry: false,
            } => notice_activity(
                NoticePresentation {
                    summary: message,
                    details,
                    file: None,
                    accessible_kind: "Codex 错误",
                    outer_gap: NOTICE_ERROR_GAP,
                    content_gap: NOTICE_ERROR_CONTENT_GAP,
                },
                index,
                theme,
            )
            .into_any_element(),
            ConversationActivity::Warning { message } => notice_activity(
                NoticePresentation {
                    summary: message,
                    details: None,
                    file: None,
                    accessible_kind: "Codex 警告",
                    outer_gap: NOTICE_WARNING_GAP,
                    content_gap: NOTICE_WARNING_CONTENT_GAP,
                },
                index,
                theme,
            )
            .into_any_element(),
            ConversationActivity::ConfigWarning(warning) => {
                let file = warning.path.map(|path| ConfigWarningFile {
                    path,
                    line: warning.line,
                    column: warning.column,
                });
                notice_activity(
                    NoticePresentation {
                        summary: warning.summary,
                        details: warning.details,
                        file,
                        accessible_kind: "Codex 配置警告",
                        outer_gap: NOTICE_WARNING_GAP,
                        content_gap: NOTICE_WARNING_CONTENT_GAP,
                    },
                    index,
                    theme,
                )
                .into_any_element()
            }
            ConversationActivity::Error { message } => notice_activity(
                NoticePresentation {
                    summary: message,
                    details: None,
                    file: None,
                    accessible_kind: "Codex turn 失败",
                    outer_gap: NOTICE_ERROR_GAP,
                    content_gap: NOTICE_ERROR_CONTENT_GAP,
                },
                index,
                theme,
            )
            .into_any_element(),
            ConversationActivity::Approval(_)
            | ConversationActivity::FileApproval(_)
            | ConversationActivity::PermissionsApproval(_)
            | ConversationActivity::UserInput(_)
            | ConversationActivity::AssistantMessage { .. } => div().into_any_element(),
        },
    }
}

pub(super) fn activity_stream(
    render: &ConversationRenderContext,
    activities: Vec<ConversationActivity>,
    show_thinking_tail: bool,
) -> Div {
    let theme = render.theme;
    let thinking_shimmer_progress = render.thinking_shimmer_progress;
    let stream = activity_stream_units(&activities)
        .into_iter()
        .enumerate()
        .fold(
            div().w_full().flex().flex_col().gap(px(16.0)),
            |stream, (index, unit)| {
                stream.child(render_activity_stream_unit(
                    render,
                    index,
                    unit,
                    show_thinking_tail,
                ))
            },
        );
    stream.when(show_thinking_tail, |stream| {
        stream.child(thinking_shimmer(theme, thinking_shimmer_progress))
    })
}
