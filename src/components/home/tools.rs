//! Tools presentation and interaction for the conversation view.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    time::Instant,
};

use gpui::{
    App, BoxShadow, Div, Entity, FontWeight, IntoElement, Role, ScrollHandle, SharedString,
    Transformation, div, linear_color_stop, linear_gradient, prelude::*, px, radians, rgba,
};

use super::{
    COMMAND_ACTIVITY_CHEVRON_SIZE, COMMAND_ACTIVITY_CONTENT_GAP, COMMAND_ACTIVITY_ICON_SIZE,
    COMMAND_CARD_COMMAND_MAX_HEIGHT, COMMAND_CARD_HEADER_LINE_HEIGHT, COMMAND_CARD_HEADER_SIZE,
    COMMAND_CARD_LINE_HEIGHT, COMMAND_CARD_OUTPUT_MAX_HEIGHT, COMMAND_CARD_RADIUS,
    COMMAND_CARD_STATUS_HEIGHT, COMMAND_CARD_TEXT_SIZE, DISCLOSURE_FOCUS_PADDING, HomeView,
    TOOL_GROUP_BODY_MAX_HEIGHT, TOOL_GROUP_CHEVRON_SIZE, TOOL_GROUP_EDGE_FADE_DISTANCE,
    TOOL_GROUP_HEADER_CHEVRON_GAP, TOOL_GROUP_HEADER_HEIGHT, TOOL_GROUP_ICON_TEXT_GAP,
    TOOL_GROUP_ITEM_GAP, TOOL_GROUP_LINE_HEIGHT, TOOL_GROUP_TEXT_SIZE,
    context::ToolGroupDisclosure,
    conversation::nested_scroll_consumed,
    mcp::computer_use_activity,
    notices::{activity_group_icon, web_search_activity},
    timeline::{
        CommandActivitySummary, ToolActivityGroupPresentation, command_action_summary,
        command_activity_row_count, command_activity_summaries, command_activity_summary,
        completed_tool_group_summary, strip_terminal_line_ending, tool_group_reasoning_title,
        tool_group_row_count,
    },
};
use crate::{
    agent::{
        AgentFileChangeStatus, CommandExecution, CommandExecutionAction, CommandExecutionStatus,
    },
    components::{file_change::FileChangeActivityCallback, icons::icon},
    conversation::ConversationActivity,
    theme::{Theme, UI_MONOSPACE_FONT_FAMILY},
};

#[derive(Clone, Copy, Debug)]
pub(super) struct ToolGroupDisclosureTransition {
    pub(super) progress: f32,
    pub(super) chevron_progress: f32,
    pub(super) from: f32,
    pub(super) chevron_from: f32,
    pub(super) target: f32,
    pub(super) started_at: Option<Instant>,
}

impl ToolGroupDisclosureTransition {
    pub(super) fn settled(expanded: bool) -> Self {
        let progress = if expanded { 1.0 } else { 0.0 };
        Self {
            progress,
            chevron_progress: progress,
            from: progress,
            chevron_from: progress,
            target: progress,
            started_at: None,
        }
    }
}

pub(super) fn toggle_tool_activity_group(
    home_entity: &Entity<HomeView>,
    group_id: &str,
    active: bool,
    scroll_handle: &ScrollHandle,
    cx: &mut App,
) {
    let group_id = group_id.to_owned();
    home_entity.update(cx, |home, cx| {
        let expanded = if active {
            if home.collapsed_active_tool_groups.remove(&group_id) {
                true
            } else {
                home.collapsed_active_tool_groups.insert(group_id.clone());
                false
            }
        } else if home.expanded_tool_groups.remove(&group_id) {
            false
        } else {
            home.expanded_tool_groups.insert(group_id);
            true
        };
        if expanded {
            scroll_handle.scroll_to_bottom();
        }
        home.conversation_cache_dirty = true;
        cx.notify();
    });
}

pub(super) fn tool_activity_group(
    home_entity: Entity<HomeView>,
    group: ToolActivityGroupPresentation,
    disclosure: ToolGroupDisclosure,
    expanded_commands: &HashSet<String>,
    command_scroll_handles: &HashMap<String, ScrollHandle>,
    theme: Theme,
) -> Div {
    let ToolGroupDisclosure {
        review_views,
        expanded,
        disclosure_progress,
        chevron_progress,
        scroll_handle,
    } = disclosure;
    if let [ConversationActivity::Command(command)] = group.activities.as_slice()
        && command_activity_row_count(command) == 1
    {
        return command_execution_activity(
            home_entity,
            command.clone(),
            expanded_commands,
            command_scroll_handles,
            theme,
        );
    }
    let group_id = group.id.clone();
    let active = group.is_active();
    let reasoning_title = active.then(|| tool_group_reasoning_title(&group)).flatten();
    let summary = if let Some(title) = reasoning_title {
        CommandActivitySummary {
            icon: "",
            text: title,
            reads_files: false,
            runs_command: false,
        }
    } else if active {
        if group
            .file_changes
            .iter()
            .any(|change| change.status == AgentFileChangeStatus::InProgress)
        {
            CommandActivitySummary {
                icon: "message-edit",
                text: "正在编辑文件".to_owned(),
                reads_files: false,
                runs_command: false,
            }
        } else {
            group
                .commands
                .iter()
                .rev()
                .find(|command| command.status == CommandExecutionStatus::InProgress)
                .or_else(|| group.commands.last())
                .and_then(|command| command_activity_summaries(command).into_iter().last())
                .unwrap_or_else(|| CommandActivitySummary {
                    icon: "panel-terminal",
                    text: "正在工作".to_owned(),
                    reads_files: false,
                    runs_command: true,
                })
        }
    } else {
        completed_tool_group_summary(&group)
    };
    let has_header_icon = !summary.icon.is_empty();
    let accessible_label = if expanded {
        format!("{}，折叠工具调用", summary.text)
    } else {
        format!("{}，展开工具调用", summary.text)
    };
    let hover_group: SharedString = format!("tool-activity-group-{group_id}").into();
    let click_home = home_entity.clone();
    let click_group_id = group_id.clone();
    let click_scroll = scroll_handle.clone();
    let key_group_id = group_id.clone();
    let key_scroll = scroll_handle.clone();
    let scroll_home = home_entity.clone();
    let nested_scroll_handle = scroll_handle.clone();
    let visibility = disclosure_progress.clamp(0.0, 1.0);
    let chevron_visibility = chevron_progress.clamp(0.0, 1.0);
    let scroll_top = -f32::from(scroll_handle.offset().y);
    let max_scroll = f32::from(scroll_handle.max_offset().y);
    let row_count = tool_group_row_count(&group);
    let estimated_rows_height = TOOL_GROUP_ITEM_GAP
        + row_count as f32 * TOOL_GROUP_HEADER_HEIGHT
        + row_count.saturating_sub(1) as f32 * TOOL_GROUP_ITEM_GAP;
    let has_overflow = max_scroll > 0.5 || estimated_rows_height > TOOL_GROUP_BODY_MAX_HEIGHT;
    let show_top_fade = scroll_top > 0.5;
    let show_bottom_fade = has_overflow && (max_scroll <= 0.5 || scroll_top + 0.5 < max_scroll);

    // Render the transport order, including interleaved edits and computer use.
    let activity_rows = group.activities.into_iter().fold(
        div()
            .w_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(TOOL_GROUP_ITEM_GAP)),
        |rows, activity| match activity {
            ConversationActivity::AutoApprovalReview(review) => {
                rows.children(review_views.get(&review.review.key).cloned())
            }
            ConversationActivity::Command(command) => rows.child(command_execution_activity(
                home_entity.clone(),
                command,
                expanded_commands,
                command_scroll_handles,
                theme,
            )),
            ConversationActivity::FileChange(file_change) => {
                let target = home_entity.clone();
                let callback = FileChangeActivityCallback::new(move |event, _, cx| {
                    target.update(cx, move |home, cx| {
                        home.handle_file_change_activity_event(event, cx)
                    });
                });
                rows.child(div().w_full().flex_none().child(
                    crate::components::file_change::render_grouped_file_change(
                        &file_change,
                        expanded_commands,
                        theme,
                        callback,
                    ),
                ))
            }
            ConversationActivity::WebSearch {
                item_id,
                query,
                results,
            } => rows.child(web_search_activity(item_id, query, results, theme)),
            ConversationActivity::McpToolCall(call) => {
                let expanded = expanded_commands.contains(&call.id);
                rows.child(computer_use_activity(
                    home_entity.clone(),
                    *call,
                    expanded,
                    theme,
                ))
            }
            _ => rows,
        },
    );

    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .items_start()
        .child(
            div()
                .id(SharedString::from(format!(
                    "tool-activity-group-{group_id}"
                )))
                .group(hover_group.clone())
                .h(px(TOOL_GROUP_HEADER_HEIGHT))
                .max_w_full()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(TOOL_GROUP_HEADER_CHEVRON_GAP))
                .rounded(px(6.0))
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
                    toggle_tool_activity_group(
                        &click_home,
                        &click_group_id,
                        active,
                        &click_scroll,
                        cx,
                    );
                })
                .on_key_down(move |event, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        toggle_tool_activity_group(
                            &home_entity,
                            &key_group_id,
                            active,
                            &key_scroll,
                            cx,
                        );
                        cx.stop_propagation();
                    }
                })
                .child(
                    div()
                        .min_w(px(0.0))
                        .max_w(px(718.0))
                        .flex()
                        .items_center()
                        .gap(px(TOOL_GROUP_ICON_TEXT_GAP))
                        .text_color(theme.text.alpha(0.60))
                        .when(has_header_icon, |content| {
                            content.child(activity_group_icon(summary.icon, theme))
                        })
                        .child(
                            div()
                                .min_w(px(0.0))
                                .max_w(px(if has_header_icon { 696.0 } else { 718.0 }))
                                .truncate()
                                .text_size(px(TOOL_GROUP_TEXT_SIZE))
                                .line_height(px(TOOL_GROUP_LINE_HEIGHT))
                                .font_family(".SystemUIFont")
                                .font_weight(FontWeight::NORMAL)
                                .child(summary.text),
                        ),
                )
                .child(
                    icon("settings-chevron-right", theme.text.alpha(0.60).into())
                        .size(px(TOOL_GROUP_CHEVRON_SIZE))
                        .flex_none()
                        .opacity(if expanded || visibility > f32::EPSILON {
                            1.0
                        } else {
                            0.0
                        })
                        .group_hover(hover_group, |chevron| chevron.opacity(1.0))
                        .with_transformation(Transformation::rotate(radians(
                            std::f32::consts::FRAC_PI_2 * chevron_visibility,
                        ))),
                ),
        )
        .when(visibility > f32::EPSILON, |group| {
            group.child(
                div()
                    .w_full()
                    .max_h(px(TOOL_GROUP_BODY_MAX_HEIGHT * visibility))
                    .overflow_hidden()
                    .opacity(visibility)
                    .when(visibility <= f32::EPSILON, |body| body.invisible())
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .max_h(px(TOOL_GROUP_BODY_MAX_HEIGHT))
                            .child(
                                div()
                                    .id(SharedString::from(format!(
                                        "tool-activity-body-{group_id}"
                                    )))
                                    .ml(px(-8.0))
                                    .pl(px(8.0))
                                    .w_full()
                                    .max_h(px(TOOL_GROUP_BODY_MAX_HEIGHT))
                                    .overflow_scroll()
                                    .restrict_scroll_to_axis()
                                    .scrollbar_width(px(0.0))
                                    .track_scroll(&scroll_handle)
                                    .pt(px(TOOL_GROUP_ITEM_GAP))
                                    .on_scroll_wheel(move |event, window, cx| {
                                        if nested_scroll_consumed(
                                            &nested_scroll_handle,
                                            event,
                                            window,
                                        ) {
                                            cx.stop_propagation();
                                        }
                                        let home = scroll_home.clone();
                                        window.on_next_frame(move |_, cx| {
                                            home.update(cx, |_, cx| cx.notify());
                                        });
                                    })
                                    .child(activity_rows),
                            )
                            .when(show_top_fade, |body| {
                                body.child(
                                    div()
                                        .absolute()
                                        .top_0()
                                        .left_0()
                                        .w_full()
                                        .h(px(TOOL_GROUP_EDGE_FADE_DISTANCE))
                                        .bg(linear_gradient(
                                            0.0,
                                            linear_color_stop(theme.surface.alpha(0.0), 0.0),
                                            linear_color_stop(theme.surface, 1.0),
                                        )),
                                )
                            })
                            .when(show_bottom_fade, |body| {
                                body.child(
                                    div()
                                        .absolute()
                                        .bottom_0()
                                        .left_0()
                                        .w_full()
                                        .h(px(TOOL_GROUP_EDGE_FADE_DISTANCE))
                                        .bg(linear_gradient(
                                            180.0,
                                            linear_color_stop(theme.surface.alpha(0.0), 0.0),
                                            linear_color_stop(theme.surface, 1.0),
                                        )),
                                )
                            }),
                    ),
            )
        })
}

pub(super) fn toggle_command_activity(
    home_entity: &Entity<HomeView>,
    item_id: &str,
    scroll_handle: &ScrollHandle,
    cx: &mut App,
) {
    let item_id = item_id.to_owned();
    home_entity.update(cx, |home, cx| {
        if !home.expanded_commands.remove(&item_id) {
            home.expanded_commands.insert(item_id);
            scroll_handle.scroll_to_bottom();
        }
        cx.notify();
    });
}

pub(super) fn static_command_action_activity(
    row_id: String,
    summary: CommandActivitySummary,
    action: CommandExecutionAction,
    cwd: String,
    theme: Theme,
) -> impl IntoElement {
    let hover_group: SharedString = format!("command-action-{row_id}").into();
    let read_link = match action {
        CommandExecutionAction::Read { name, path, .. } => {
            let label = if name.trim().is_empty() {
                path.clone()
            } else {
                name
            };
            let path = PathBuf::from(path);
            let path = if path.is_absolute() {
                path
            } else {
                PathBuf::from(cwd).join(path)
            };
            Some((label, path))
        }
        _ => None,
    };
    let summary_text = summary.text.clone();
    div()
        .id(SharedString::from(format!("command-action-{row_id}")))
        .group(hover_group.clone())
        .h(px(TOOL_GROUP_HEADER_HEIGHT))
        .flex_none()
        .overflow_hidden()
        .max_w_full()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .gap(px(COMMAND_ACTIVITY_CONTENT_GAP))
        .text_color(theme.text.alpha(0.60))
        .child(
            icon(summary.icon, theme.text.alpha(0.60).into())
                .size(px(COMMAND_ACTIVITY_ICON_SIZE))
                .flex_none(),
        )
        .child(
            div()
                .min_w(px(0.0))
                .max_w(px(696.0))
                .truncate()
                .text_size(px(TOOL_GROUP_TEXT_SIZE))
                .line_height(px(TOOL_GROUP_LINE_HEIGHT))
                .font_family(".SystemUIFont")
                .text_color(theme.text.alpha(0.60))
                .group_hover(hover_group, move |label| label.text_color(theme.text))
                .child(if let Some((label, path)) = read_link {
                    let prefix = summary_text
                        .strip_suffix(&label)
                        .unwrap_or(&summary_text)
                        .to_owned();
                    let click_path = path.clone();
                    let key_path = path.clone();
                    div()
                        .min_w(px(0.0))
                        .flex()
                        .items_center()
                        .child(prefix)
                        .child(
                            div()
                                .id(SharedString::from(format!("command-read-link-{row_id}")))
                                .role(Role::Link)
                                .aria_label(format!("打开 {}", path.display()))
                                .focusable()
                                .tab_stop(true)
                                .min_w(px(0.0))
                                .max_w_full()
                                .truncate()
                                .rounded(px(4.0))
                                .cursor_pointer()
                                .underline()
                                .focus_visible(|style| {
                                    style.shadow(vec![
                                        BoxShadow::new(px(0.0), px(0.0), rgba(0x3a83f7ff).into())
                                            .spread_radius(px(2.0))
                                            .inset(),
                                    ])
                                })
                                .on_click(move |_, window, cx| {
                                    window.dispatch_action(
                                        Box::new(
                                            crate::components::file_panel::OpenWorkspaceFile {
                                                path: click_path.to_string_lossy().into_owned(),
                                                line: None,
                                            },
                                        ),
                                        cx,
                                    );
                                    cx.stop_propagation();
                                })
                                .on_key_down(move |event, window, cx| {
                                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                        window.dispatch_action(
                                            Box::new(
                                                crate::components::file_panel::OpenWorkspaceFile {
                                                    path: key_path.to_string_lossy().into_owned(),
                                                    line: None,
                                                },
                                            ),
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    }
                                })
                                .child(label),
                        )
                        .into_any_element()
                } else {
                    div().child(summary.text).into_any_element()
                }),
        )
}

pub(super) fn command_execution_activity(
    home_entity: Entity<HomeView>,
    command: CommandExecution,
    expanded_commands: &HashSet<String>,
    command_scroll_handles: &HashMap<String, ScrollHandle>,
    theme: Theme,
) -> Div {
    let execution_id = command.id.clone();
    let actions = if command.actions.is_empty() {
        vec![CommandExecutionAction::Unknown {
            command: command.command.clone(),
        }]
    } else {
        command.actions.clone()
    };
    let action_count = actions.len();
    let scroll_handle = command_scroll_handles
        .get(&execution_id)
        .cloned()
        .unwrap_or_else(ScrollHandle::new);

    actions.into_iter().enumerate().fold(
        div()
            .w_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(TOOL_GROUP_ITEM_GAP)),
        |rows, (index, action)| {
            let row_id = if action_count == 1 {
                execution_id.clone()
            } else {
                format!("{execution_id}-action-{index}")
            };
            let summary = command_action_summary(&command, &action);
            match action {
                CommandExecutionAction::Unknown {
                    command: action_command,
                } => {
                    let mut row_command = command.clone();
                    row_command.id = row_id.clone();
                    row_command.command = action_command.clone();
                    row_command.actions = vec![CommandExecutionAction::Unknown {
                        command: action_command,
                    }];
                    rows.child(command_activity(
                        home_entity.clone(),
                        row_command,
                        expanded_commands.contains(&row_id),
                        scroll_handle.clone(),
                        theme,
                    ))
                }
                action => rows.child(static_command_action_activity(
                    row_id,
                    summary,
                    action,
                    command.cwd.clone(),
                    theme,
                )),
            }
        },
    )
}

pub(super) fn command_activity(
    home_entity: Entity<HomeView>,
    command: CommandExecution,
    expanded: bool,
    scroll_handle: ScrollHandle,
    theme: Theme,
) -> Div {
    let item_id = command.id.clone();
    let output_scroll_id: SharedString = format!("command-output-{item_id}").into();
    let hover_group: SharedString = format!("command-activity-{item_id}").into();
    let summary = command_activity_summary(&command);
    let status_label = match command.status {
        CommandExecutionStatus::InProgress => "运行中",
        CommandExecutionStatus::Completed => "成功",
        CommandExecutionStatus::Failed => "失败",
    };
    let status_icon = match command.status {
        CommandExecutionStatus::Failed => "settings-warning",
        _ => "check",
    };
    let status_color = if command.status == CommandExecutionStatus::Failed {
        theme.warning
    } else {
        theme.command_muted
    };
    let display_command = if command.command.is_empty() {
        "命令".to_owned()
    } else {
        command.command.clone()
    };
    let output = if command.output.is_empty() {
        if command.status == CommandExecutionStatus::InProgress {
            "等待输出…".to_owned()
        } else {
            "（无输出）".to_owned()
        }
    } else {
        // A terminal normally returns one final line ending. Browsers do not
        // allocate another visible line for it in ChatGPT's shell card, while
        // GPUI's text layout does, so omit exactly that transport delimiter.
        strip_terminal_line_ending(&command.output).to_owned()
    };
    let command_for_body = display_command.clone();
    let accessible_label = if expanded {
        format!("{}，折叠详情", summary.text)
    } else {
        format!("{}，展开详情", summary.text)
    };
    let accessible_label = if command.status == CommandExecutionStatus::Failed {
        format!("{accessible_label}，命令失败")
    } else {
        accessible_label
    };
    let click_home = home_entity.clone();
    let click_item_id = item_id.clone();
    let click_scroll = scroll_handle.clone();
    let key_item_id = item_id.clone();
    let key_scroll = scroll_handle.clone();
    let nested_scroll_handle = scroll_handle.clone();

    div()
        .w_full()
        .min_w(px(0.0))
        .flex_none()
        .flex()
        .flex_col()
        .items_start()
        .child(
            div()
                .id(SharedString::from(format!("command-activity-{item_id}")))
                .group(hover_group.clone())
                .h(px(21.0))
                .flex_none()
                .overflow_hidden()
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
                    toggle_command_activity(&click_home, &click_item_id, &click_scroll, cx);
                })
                .on_key_down(move |event, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        toggle_command_activity(&home_entity, &key_item_id, &key_scroll, cx);
                        cx.stop_propagation();
                    }
                })
                .child(
                    div()
                        .min_w(px(0.0))
                        .max_w(px(718.0))
                        .flex()
                        .items_center()
                        .gap(px(COMMAND_ACTIVITY_CONTENT_GAP))
                        .text_color(theme.text.alpha(0.60))
                        .child(
                            icon(summary.icon, theme.text.alpha(0.60).into())
                                .size(px(COMMAND_ACTIVITY_ICON_SIZE))
                                .flex_none(),
                        )
                        .child(
                            div()
                                .min_w(px(0.0))
                                .max_w(px(696.0))
                                .truncate()
                                .text_size(px(14.0))
                                .line_height(px(21.0))
                                .font_family(".SystemUIFont")
                                .child(summary.text),
                        ),
                )
                .child(
                    icon("settings-chevron-right", theme.text.alpha(0.60).into())
                        .size(px(COMMAND_ACTIVITY_CHEVRON_SIZE))
                        .flex_none()
                        .opacity(if expanded { 1.0 } else { 0.0 })
                        .group_hover(hover_group, |chevron| chevron.opacity(1.0))
                        .when(expanded, |chevron| {
                            chevron.with_transformation(Transformation::rotate(radians(
                                std::f32::consts::FRAC_PI_2,
                            )))
                        }),
                ),
        )
        .when(expanded, |activity| {
            activity.child(
                div().w_full().pt(px(8.0)).pb(px(4.0)).child(
                    div()
                        .w_full()
                        .overflow_hidden()
                        .rounded(px(COMMAND_CARD_RADIUS))
                        .border(px(1.0))
                        .border_color(theme.command_border)
                        .bg(theme.command_surface)
                        .child(
                            div()
                                .px(px(8.0))
                                .py(px(4.0))
                                .flex()
                                .items_center()
                                .text_size(px(COMMAND_CARD_HEADER_SIZE))
                                .line_height(px(COMMAND_CARD_HEADER_LINE_HEIGHT))
                                .font_weight(FontWeight::NORMAL)
                                .font_family(".SystemUIFont")
                                .text_color(theme.command_text)
                                .child("Shell"),
                        )
                        .child(
                            div()
                                .px(px(8.0))
                                .pt(px(8.0))
                                .text_size(px(COMMAND_CARD_TEXT_SIZE))
                                .line_height(px(COMMAND_CARD_LINE_HEIGHT))
                                .font_weight(FontWeight::NORMAL)
                                .font_family(UI_MONOSPACE_FONT_FAMILY)
                                .text_color(theme.command_text)
                                .child(
                                    div()
                                        .flex()
                                        .items_start()
                                        .pr(px(24.0))
                                        .child(
                                            div()
                                                .mr(px(8.0))
                                                .text_color(theme.command_muted)
                                                .child("$"),
                                        )
                                        .child(
                                            div()
                                                .min_w(px(0.0))
                                                .flex_1()
                                                .max_h(px(COMMAND_CARD_COMMAND_MAX_HEIGHT))
                                                .overflow_hidden()
                                                .child(command_for_body),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .id(output_scroll_id)
                                .max_h(px(COMMAND_CARD_OUTPUT_MAX_HEIGHT))
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
                                .p(px(8.0))
                                .text_size(px(COMMAND_CARD_TEXT_SIZE))
                                .line_height(px(COMMAND_CARD_LINE_HEIGHT))
                                .font_weight(FontWeight::MEDIUM)
                                .font_family(UI_MONOSPACE_FONT_FAMILY)
                                .text_color(theme.command_text)
                                .child(output),
                        )
                        .child(
                            div()
                                .h(px(COMMAND_CARD_STATUS_HEIGHT))
                                .px(px(10.0))
                                .pt(px(2.0))
                                .pb(px(4.0))
                                .flex()
                                .items_center()
                                .justify_end()
                                .gap(px(4.0))
                                .text_size(px(14.0))
                                .line_height(px(21.0))
                                .font_weight(FontWeight::NORMAL)
                                .text_color(status_color)
                                .child(icon(status_icon, status_color.into()).size(px(12.0)))
                                .child(status_label),
                        ),
                ),
            )
        })
}
