use super::{
    mcp::{computer_use_surface_label, mcp_tool_call_label},
    timeline::{ActivityStreamUnit, activity_stream_units, completed_tool_group_summary},
};
use crate::{
    agent::{
        AgentImageView, AgentMcpToolCall, AgentMcpToolCallStatus, CommandExecution,
        CommandExecutionStatus,
    },
    conversation::ConversationActivity,
};

fn computer_call(id: &str, title: &str, surface: serde_json::Value) -> AgentMcpToolCall {
    AgentMcpToolCall {
        id: id.into(),
        server: "cua_repl".into(),
        tool: "js".into(),
        status: AgentMcpToolCallStatus::Completed,
        arguments: serde_json::json!({"title": title, "code": "await cua.getState()"}),
        app_context: None,
        plugin_id: Some("unified-computer-use@openai-bundled".into()),
        result: Some(serde_json::json!({"content": [], "_meta": {"codex/toolSurface": surface}})),
        error: None,
        legacy_resource_uri: None,
        read_only_hint: Some(true),
        duration_ms: Some(300),
        progress: vec![],
    }
}

#[test]
fn computer_use_keeps_its_title_and_chronological_place_inside_command_groups() {
    let call = computer_call(
        "computer",
        "枚举应用以准备独立 GPUI 界面验收",
        serde_json::json!({"kind":"browserUse"}),
    );
    assert_eq!(
        mcp_tool_call_label(&call),
        "枚举应用以准备独立 GPUI 界面验收"
    );
    let command = |id: &str| {
        ConversationActivity::Command(CommandExecution {
            id: id.into(),
            command: "pwd".into(),
            actions: vec![],
            cwd: "/tmp".into(),
            output: String::new(),
            terminal_process_id: None,
            status: CommandExecutionStatus::Completed,
            exit_code: Some(0),
        })
    };
    let activities = vec![
        command("before"),
        ConversationActivity::McpToolCall(Box::from(call)),
        command("after"),
    ];
    let units = activity_stream_units(&activities);
    let [ActivityStreamUnit::ToolGroup(group)] = units.as_slice() else {
        panic!("computer use must not split the group")
    };
    assert_eq!(group.activities, activities);
    assert_eq!(
        completed_tool_group_summary(group).text,
        "已使用 浏览器运行了命令"
    );
}

#[test]
fn surface_metadata_and_missing_metadata_do_not_erase_computer_calls() {
    let app = computer_call(
        "app",
        "连接独立 GPUI Capture",
        serde_json::json!({"kind":"computerUse","app":{"kind":"appId","appId":"com.openai.gpui-chat-clone.capture"}}),
    );
    assert_eq!(
        computer_use_surface_label(&app).as_deref(),
        Some("Com.openai.gpui Chat Clone.capture")
    );
    let mut reset = computer_call("reset", "", serde_json::Value::Null);
    reset.tool = "js_reset".into();
    reset.arguments = serde_json::json!({});
    assert_eq!(mcp_tool_call_label(&reset), "Js reset");
    let units = activity_stream_units(&[
        ConversationActivity::McpToolCall(Box::from(app)),
        ConversationActivity::McpToolCall(Box::from(reset)),
    ]);
    let [ActivityStreamUnit::ToolGroup(group)] = units.as_slice() else {
        panic!("reset belongs to the surrounding group")
    };
    assert_eq!(group.activities.len(), 2);
    assert_eq!(
        completed_tool_group_summary(group).text,
        "已使用 Com.openai.gpui Chat Clone.capture 集成"
    );
}

#[test]
fn adjacent_image_inspections_share_a_disclosure_without_losing_paths() {
    let images = vec![
        AgentImageView {
            id: "a".into(),
            path: "/tmp/a.png".into(),
        },
        AgentImageView {
            id: "b".into(),
            path: "/tmp/b.png".into(),
        },
    ];
    let units = activity_stream_units(
        &images
            .iter()
            .cloned()
            .map(ConversationActivity::ImageView)
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        units,
        vec![ActivityStreamUnit::Standalone(
            ConversationActivity::ImageViews(images)
        )]
    );
}
