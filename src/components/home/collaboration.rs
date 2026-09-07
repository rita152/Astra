//! Collaboration presentation and interaction for the conversation view.

use gpui::{
    App, BoxShadow, Entity, FontWeight, Role, SharedString, div, prelude::*, px, rgba, svg,
};

use super::{
    COLLABORATION_DETAIL_MAX_WIDTH, COLLABORATION_ICON_SIZE, COLLABORATION_ROW_HEIGHT,
    COLLABORATION_TEXT_SIZE, HomeView, OpenSubAgentPanel,
};
use crate::{
    agent::{
        AgentCollaboration, AgentCollaborationStatus, AgentCollaboratorStatus,
        LegacySubAgentActivityKind,
    },
    theme::Theme,
};

pub(super) fn collaboration_display_name(
    collaboration: &AgentCollaboration,
    thread_id: &str,
) -> String {
    if let Some(name) = collaboration
        .agents_states
        .get(thread_id)
        .and_then(|state| state.name.as_deref())
        .filter(|name| !name.trim().is_empty())
    {
        return name.to_owned();
    }
    if let Some(path) = collaboration
        .legacy_agent_path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
    {
        let leaf = path
            .rsplit('/')
            .find(|part| !part.is_empty())
            .unwrap_or(path);
        let mut label = leaf.replace(['_', '-'], " ");
        if let Some(first) = label.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        return label;
    }
    if let Some(prompt) = collaboration
        .prompt
        .as_deref()
        .and_then(|prompt| prompt.lines().find(|line| !line.trim().is_empty()))
    {
        let prompt = prompt.trim();
        if prompt.chars().count() <= 48 {
            return prompt.to_owned();
        }
    }
    let short_id = thread_id.rsplit('-').next().unwrap_or(thread_id);
    if short_id.is_empty() {
        "子智能体".to_owned()
    } else {
        format!("子智能体 {short_id}")
    }
}

pub(super) fn collaboration_thread_ids(collaboration: &AgentCollaboration) -> Vec<String> {
    let mut thread_ids = collaboration.receiver_thread_ids.clone();
    for thread_id in collaboration.agents_states.keys() {
        if !thread_ids.contains(thread_id) {
            thread_ids.push(thread_id.clone());
        }
    }
    if thread_ids.is_empty() {
        thread_ids.push(String::new());
    }
    thread_ids
}

pub(super) fn collaboration_ui_identity(collaboration: &AgentCollaboration) -> String {
    if collaboration.legacy_kind.is_some()
        && let Some(thread_id) = collaboration.receiver_thread_ids.first()
    {
        return format!("legacy-{thread_id}");
    }
    collaboration.id.clone()
}

pub(super) fn collaboration_status_label(
    collaboration: &AgentCollaboration,
    thread_id: &str,
) -> &'static str {
    if let Some(kind) = collaboration.legacy_kind {
        return match kind {
            LegacySubAgentActivityKind::Started => "开始工作",
            LegacySubAgentActivityKind::Interacted => "已更新",
            LegacySubAgentActivityKind::Interrupted => "已中断",
            LegacySubAgentActivityKind::Completed => "已完成",
        };
    }
    if collaboration.status == AgentCollaborationStatus::Failed {
        return "失败";
    }
    if collaboration.status == AgentCollaborationStatus::Interrupted {
        return "已中断";
    }
    match collaboration
        .agents_states
        .get(thread_id)
        .map(|state| state.status)
    {
        Some(AgentCollaboratorStatus::PendingInit) => "正在启动",
        Some(AgentCollaboratorStatus::Running) => "开始工作",
        Some(AgentCollaboratorStatus::Interrupted) => "已中断",
        Some(AgentCollaboratorStatus::Completed) => "已完成",
        Some(AgentCollaboratorStatus::Errored) => "失败",
        Some(AgentCollaboratorStatus::Shutdown) => "已关闭",
        Some(AgentCollaboratorStatus::NotFound) => "未找到",
        None if collaboration.status == AgentCollaborationStatus::Completed => "已完成",
        None => "正在工作",
    }
}

#[cfg(test)]
pub(super) fn toggle_collaboration_item(
    home_entity: &Entity<HomeView>,
    item_id: &str,
    cx: &mut App,
) {
    let item_id = item_id.to_owned();
    home_entity.update(cx, |home, cx| {
        if !home.expanded_collaborations.remove(&item_id) {
            home.expanded_collaborations.insert(item_id);
        }
        cx.notify();
    });
}

pub(super) fn open_sub_agent_panel(
    home_entity: &Entity<HomeView>,
    thread_id: &str,
    name: &str,
    cx: &mut App,
) {
    let event = OpenSubAgentPanel {
        thread_id: thread_id.to_owned(),
        name: name.to_owned(),
    };
    home_entity.update(cx, |_, cx| cx.emit(event));
}

pub(super) fn collaboration_activity(
    home_entity: Entity<HomeView>,
    collaboration: AgentCollaboration,
    expanded: bool,
    theme: Theme,
) -> impl IntoElement {
    let item_id = collaboration_ui_identity(&collaboration);
    let receiver_thread_ids = collaboration_thread_ids(&collaboration);
    let mut content = div()
        .id(SharedString::from(format!(
            "collaboration-activity-{item_id}"
        )))
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .items_start()
        .gap(px(4.0));

    for (index, thread_id) in receiver_thread_ids.iter().enumerate() {
        let name = collaboration_display_name(&collaboration, thread_id);
        let status = collaboration_status_label(&collaboration, thread_id);
        let accessible_label = format!("在右侧打开子智能体 {name}，状态{status}");
        let click_home = home_entity.clone();
        let key_home = home_entity.clone();
        let click_thread_id = thread_id.clone();
        let key_thread_id = thread_id.clone();
        let click_name = name.clone();
        let key_name = name.clone();
        let can_open = !thread_id.is_empty();
        let failure = matches!(status, "失败" | "未找到");
        let text_color = if failure {
            theme.warning
        } else {
            theme.text.alpha(0.65)
        };
        content = content.child(
            div()
                .h(px(COLLABORATION_ROW_HEIGHT))
                .w_full()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .font_family(".SystemUIFont")
                .font_weight(FontWeight(430.0))
                .text_size(px(COLLABORATION_TEXT_SIZE))
                .line_height(px(COLLABORATION_ROW_HEIGHT))
                .text_color(text_color)
                .child(
                    svg()
                        .path("icons/subagent-activity.svg")
                        .size(px(COLLABORATION_ICON_SIZE))
                        .text_color(rgba(0xb9afd3ff))
                        .flex_none(),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .id(SharedString::from(format!(
                                    "collaboration-agent-{item_id}-{index}"
                                )))
                                .debug_selector(|| "collaboration-agent".to_owned())
                                .rounded(px(6.0))
                                .when(can_open, |label| {
                                    label
                                        .focusable()
                                        .tab_stop(true)
                                        .role(Role::Button)
                                        .aria_label(accessible_label)
                                        .cursor_pointer()
                                        .hover(move |label| label.text_color(theme.text))
                                        .focus_visible(|style| {
                                            style.shadow(vec![
                                                BoxShadow::new(
                                                    px(0.0),
                                                    px(0.0),
                                                    rgba(0x3a83f7ff).into(),
                                                )
                                                .spread_radius(px(2.0))
                                                .inset(),
                                            ])
                                        })
                                })
                                .on_click(move |_, _, cx| {
                                    if !click_thread_id.is_empty() {
                                        open_sub_agent_panel(
                                            &click_home,
                                            &click_thread_id,
                                            &click_name,
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    }
                                })
                                .on_key_down(move |event, _, cx| {
                                    if !key_thread_id.is_empty()
                                        && matches!(event.keystroke.key.as_str(), "enter" | "space")
                                    {
                                        open_sub_agent_panel(
                                            &key_home,
                                            &key_thread_id,
                                            &key_name,
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    }
                                })
                                .child(name),
                        )
                        .child(status),
                ),
        );
    }

    content.when(expanded, |content| {
        let mut detail = div()
            .ml(px(22.0))
            .mt(px(4.0))
            .w_full()
            .max_w(px(COLLABORATION_DETAIL_MAX_WIDTH))
            .p(px(10.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .rounded(px(12.0))
            .bg(theme.command_surface)
            .shadow(vec![
                BoxShadow::new(px(0.0), px(0.0), theme.command_border.into())
                    .spread_radius(px(0.5)),
            ])
            .text_size(px(13.0))
            .line_height(px(18.0))
            .text_color(theme.text_secondary);
        if let Some(prompt) = collaboration
            .prompt
            .as_deref()
            .filter(|prompt| !prompt.trim().is_empty())
        {
            detail = detail.child(div().text_color(theme.text).child(prompt.to_owned()));
        }
        if let Some(metadata) = match (
            collaboration.model.as_deref(),
            collaboration.reasoning_effort.as_deref(),
        ) {
            (Some(model), Some(effort)) => Some(format!("{model} · {effort}")),
            (Some(model), None) => Some(model.to_owned()),
            (None, Some(effort)) => Some(effort.to_owned()),
            (None, None) => None,
        } {
            detail = detail.child(metadata);
        }
        for thread_id in collaboration_thread_ids(&collaboration) {
            let name = collaboration_display_name(&collaboration, &thread_id);
            let status = collaboration_status_label(&collaboration, &thread_id);
            let click_home = home_entity.clone();
            let key_home = home_entity.clone();
            let click_thread_id = thread_id.clone();
            let key_thread_id = thread_id.clone();
            let click_name = name.clone();
            let key_name = name.clone();
            let can_open = !thread_id.is_empty();
            let receiver_label = format!("在右侧打开子智能体 {name}，状态{status}");
            let message = collaboration
                .agents_states
                .get(&thread_id)
                .and_then(|state| state.message.clone());
            detail = detail.child(
                div()
                    .id(SharedString::from(format!(
                        "collaboration-open-{thread_id}"
                    )))
                    .debug_selector(|| "collaboration-receiver".to_owned())
                    .min_h(px(28.0))
                    .px(px(8.0))
                    .flex()
                    .flex_col()
                    .justify_center()
                    .rounded(px(8.0))
                    .when(can_open, |row| {
                        row.focusable()
                            .tab_stop(true)
                            .role(Role::Button)
                            .aria_label(receiver_label)
                            .cursor_pointer()
                            .hover(move |row| row.bg(theme.sidebar_hover))
                    })
                    .on_click(move |_, _, cx| {
                        if !click_thread_id.is_empty() {
                            open_sub_agent_panel(&click_home, &click_thread_id, &click_name, cx)
                        }
                    })
                    .on_key_down(move |event, _, cx| {
                        if !key_thread_id.is_empty()
                            && matches!(event.keystroke.key.as_str(), "enter" | "space")
                        {
                            open_sub_agent_panel(&key_home, &key_thread_id, &key_name, cx);
                            cx.stop_propagation();
                        }
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .text_color(theme.text)
                            .child(name)
                            .child(status),
                    )
                    .when_some(message, |row, message| {
                        row.child(div().truncate().child(message))
                    }),
            );
        }
        content.child(detail)
    })
}
