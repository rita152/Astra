//! Mcp presentation and interaction for the conversation view.

use std::sync::Arc;

use gpui::{
    Div, Entity, FontWeight, ObjectFit, Role, ScrollHandle, SharedString, Transformation, div,
    prelude::*, px, radians, rgba,
};

use super::{
    HomeView, MCP_TOOL_CALL_ICON_SIZE, MCP_TOOL_CALL_ICON_TEXT_GAP, MCP_TOOL_CALL_ROW_HEIGHT,
    MCP_TOOL_CALL_TEXT_SIZE, media::tool_image_format, notices::activity_group_icon,
    tools::toggle_command_activity,
};
use crate::{
    agent::{AgentMcpToolCall, AgentMcpToolCallStatus},
    components::icons::icon,
    theme::Theme,
};

pub(super) fn is_computer_use_call(call: &AgentMcpToolCall) -> bool {
    call.server == "cua_repl" && matches!(call.tool.as_str(), "js" | "js_reset")
}

pub(super) fn computer_use_surface_label(call: &AgentMcpToolCall) -> Option<String> {
    let metadata = call.result.as_ref()?.get("_meta")?;
    let surface = metadata.get("codex/toolSurface");
    if surface
        .and_then(|s| s.get("kind"))
        .and_then(serde_json::Value::as_str)
        == Some("browserUse")
        || metadata
            .get("codex/browserUse")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
    {
        return Some("浏览器".into());
    }
    let surface = surface?;
    if surface.get("kind").and_then(serde_json::Value::as_str) != Some("computerUse") {
        return None;
    }
    Some(
        surface
            .get("app")
            .and_then(|a| a.get("appId").or_else(|| a.get("name")))
            .and_then(serde_json::Value::as_str)
            .map(|name| {
                name.split('-')
                    .map(|part| {
                        let mut chars = part.chars();
                        chars
                            .next()
                            .map(|c| c.to_uppercase().chain(chars).collect::<String>())
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_else(|| "Computer Use".into()),
    )
}

pub(super) fn computer_use_activity(
    home: Entity<HomeView>,
    call: AgentMcpToolCall,
    expanded: bool,
    theme: Theme,
) -> Div {
    let label = mcp_tool_call_label(&call);
    let id = call.id.clone();
    let click_id = id.clone();
    let click_home = home.clone();
    let hover: SharedString = format!("computer-use-{id}").into();
    let color = theme.text.alpha(0.60);
    let icon_name = if computer_use_surface_label(&call)
        .is_some_and(|s| s != "浏览器" && s != "Computer Use")
    {
        "activity-native-app"
    } else {
        "activity-computer-use"
    };
    let mut body = div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .pt(px(4.0));
    if expanded {
        if let Some(parts) = call
            .result
            .as_ref()
            .and_then(|r| r.get("content"))
            .and_then(serde_json::Value::as_array)
        {
            for (index, part) in parts.iter().enumerate() {
                if let Some(text) = part.get("text").and_then(serde_json::Value::as_str) {
                    body = body.child(crate::components::markdown::render_tool_text(
                        text,
                        theme,
                        &format!("{id}-{index}"),
                    ));
                } else if part.get("type").and_then(serde_json::Value::as_str) == Some("image") {
                    use base64::Engine as _;
                    if let Some(bytes) = part
                        .get("data")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|s| base64::engine::general_purpose::STANDARD.decode(s).ok())
                    {
                        let format = tool_image_format(
                            &bytes,
                            part.get("mimeType").and_then(serde_json::Value::as_str),
                        );
                        let (width, height) =
                            crate::media::encoded_image_dimensions(&bytes).unwrap_or((160, 160));
                        let height_scale = (160.0 / height.max(1) as f32).min(1.0);
                        body = body.child(
                            gpui::img(Arc::new(gpui::Image::from_bytes(format, bytes)))
                                .w(px(width as f32 * height_scale))
                                .h(px(height as f32 * height_scale))
                                .max_w_full()
                                .object_fit(ObjectFit::Contain)
                                .rounded(px(10.0)),
                        );
                    }
                }
            }
        }
        if let Some(error) = call.error {
            body = body.child(div().text_color(theme.warning).child(error));
        }
    }
    div()
        .w_full()
        .min_w(px(0.0))
        .flex_none()
        .flex()
        .flex_col()
        .items_start()
        .child(
            div()
                .id(SharedString::from(format!("computer-use-{id}")))
                .group(hover.clone())
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
                .aria_label(format!(
                    "{label}，{}详情",
                    if expanded { "折叠" } else { "展开" }
                ))
                .focus_visible(|s| s.border_1().border_color(rgba(0x3a83f7ff)))
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    toggle_command_activity(&click_home, &click_id, &ScrollHandle::new(), cx)
                })
                .on_key_down(move |e, _, cx| {
                    if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                        toggle_command_activity(&home, &id, &ScrollHandle::new(), cx);
                        cx.stop_propagation();
                    }
                })
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(activity_group_icon(icon_name, theme))
                        .child(
                            div()
                                .min_w(px(0.0))
                                .truncate()
                                .text_size(px(14.0))
                                .line_height(px(21.0))
                                .font_family(".SystemUIFont")
                                .font_weight(FontWeight::NORMAL)
                                .text_color(color)
                                .child(label),
                        ),
                )
                .child(
                    icon("settings-chevron-right", color.into())
                        .size(px(12.0))
                        .flex_none()
                        .opacity(if expanded { 1.0 } else { 0.0 })
                        .group_hover(hover, |s| s.opacity(1.0))
                        .with_transformation(Transformation::rotate(radians(if expanded {
                            std::f32::consts::FRAC_PI_2
                        } else {
                            0.0
                        }))),
                ),
        )
        .when(expanded, |row| row.child(body))
}

pub(super) fn humanize_mcp_tool_name(tool: &str) -> String {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut previous_was_lowercase = false;
    for character in tool.chars() {
        if !character.is_alphanumeric() {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
            previous_was_lowercase = false;
            continue;
        }
        if character.is_uppercase() && previous_was_lowercase && !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
        previous_was_lowercase = character.is_lowercase();
        word.extend(character.to_lowercase());
    }
    if !word.is_empty() {
        words.push(word);
    }
    let mut label = words.join(" ");
    if label.is_empty() {
        return "Tool call".to_owned();
    }
    if let Some(first) = label.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    label
}

pub(super) fn mcp_tool_call_label(tool_call: &AgentMcpToolCall) -> String {
    if is_computer_use_call(tool_call)
        && let Some(title) = tool_call
            .arguments
            .get("title")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.trim().is_empty())
    {
        return title.to_owned();
    }
    tool_call
        .app_context
        .as_ref()
        .and_then(|context| context.get("actionName"))
        .and_then(serde_json::Value::as_str)
        .filter(|label| !label.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| humanize_mcp_tool_name(&tool_call.tool))
}

pub(super) fn mcp_tool_call_activity(
    tool_call: AgentMcpToolCall,
    theme: Theme,
) -> impl IntoElement {
    let label = mcp_tool_call_label(&tool_call);
    let failed = tool_call.status == AgentMcpToolCallStatus::Failed;
    let display_label = if failed {
        format!("{label} 失败")
    } else {
        label
    };
    let status = match tool_call.status {
        AgentMcpToolCallStatus::InProgress => "进行中",
        AgentMcpToolCallStatus::Completed => "已完成",
        AgentMcpToolCallStatus::Failed => "失败",
    };
    let accessible_label = if let Some(error) = tool_call.error.as_deref() {
        format!(
            "MCP 工具 {} 的 {}，{status}：{error}",
            tool_call.server, tool_call.tool
        )
    } else {
        format!(
            "MCP 工具 {} 的 {}，{status}",
            tool_call.server, tool_call.tool
        )
    };
    let foreground = if failed {
        theme.warning
    } else {
        theme.text.alpha(0.60)
    };
    let label_color = if failed {
        theme.warning
    } else {
        // ChatGPT's inner label declares text/40, but the enclosing
        // `[&_*:not(button)]:!text-text/60` rule wins in the live DOM.
        theme.text.alpha(0.60)
    };

    div()
        .id(SharedString::from(format!(
            "mcp-tool-call-{}",
            tool_call.id
        )))
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .aria_label(accessible_label)
        .child(
            div()
                .h(px(MCP_TOOL_CALL_ROW_HEIGHT))
                .max_w_full()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(MCP_TOOL_CALL_ICON_TEXT_GAP))
                .child(
                    icon("mcp-tool-call", foreground.into())
                        .size(px(MCP_TOOL_CALL_ICON_SIZE))
                        // CoreGraphics places the same 16 px SVG silhouette one
                        // Retina sample right/up of Chromium's live MCP row.
                        // A half-point optical offset aligns the native raster
                        // without changing the measured flex advance.
                        .relative()
                        .left(px(-0.5))
                        .top(px(0.5))
                        .flex_none(),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(MCP_TOOL_CALL_TEXT_SIZE))
                        .line_height(px(MCP_TOOL_CALL_ROW_HEIGHT))
                        .font_family(".SystemUIFont")
                        // CoreText exposes the system UI face as a discrete
                        // regular weight; that is the closest native match for
                        // Chromium's variable CSS weight 430.
                        .font_weight(FontWeight::NORMAL)
                        .text_color(label_color)
                        .child(display_label),
                ),
        )
        .when_some(tool_call.error, |activity, error| {
            activity.child(
                div()
                    .ml(px(22.0))
                    .text_size(px(13.0))
                    .line_height(px(20.0))
                    .font_family(".SystemUIFont")
                    .text_color(theme.warning)
                    .child(error),
            )
        })
}
