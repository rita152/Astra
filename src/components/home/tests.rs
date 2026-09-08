use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use base64::Engine as _;
use gpui::{
    AppContext, Bounds, KeyBinding, MouseButton, TestApp, TestAppWindow, WindowBounds,
    WindowOptions, point, px, size,
};

use super::{
    COMMAND_ACTIVITY_CHEVRON_SIZE, COMMAND_ACTIVITY_CONTENT_GAP, COMMAND_ACTIVITY_ICON_SIZE,
    COMMAND_CARD_COMMAND_MAX_HEIGHT, COMMAND_CARD_HEADER_LINE_HEIGHT, COMMAND_CARD_HEADER_SIZE,
    COMMAND_CARD_LINE_HEIGHT, COMMAND_CARD_OUTPUT_MAX_HEIGHT, COMMAND_CARD_RADIUS,
    COMMAND_CARD_STATUS_HEIGHT, COMMAND_CARD_TEXT_SIZE, COMPOSER_BOTTOM_INSET,
    CONVERSATION_BOTTOM_INSET, CONVERSATION_TOP_INSET, DISCLOSURE_FOCUS_PADDING, HomeView,
    MCP_TOOL_CALL_ICON_SIZE, MCP_TOOL_CALL_ICON_TEXT_GAP, MCP_TOOL_CALL_ROW_HEIGHT,
    MCP_TOOL_CALL_TEXT_SIZE, NOTICE_BUTTON_HEIGHT, NOTICE_ERROR_CONTENT_GAP, NOTICE_ERROR_GAP,
    NOTICE_ICON_SIZE, NOTICE_LINE_HEIGHT, NOTICE_RADIUS, NOTICE_TEXT_SIZE,
    NOTICE_WARNING_CONTENT_GAP, NOTICE_WARNING_GAP, OpenImagePreview, OpenSubAgentPanel,
    REASONING_BODY_MAX_HEIGHT, REASONING_CHEVRON_SIZE, REASONING_HEADER_HEIGHT,
    REASONING_LINE_HEIGHT, REASONING_TEXT_SIZE, REASONING_TRANSITION_DURATION,
    RESPONSE_ACTION_FOOTER_ELECTRON_SHIFT, RESPONSE_ACTION_FOOTER_HEIGHT,
    RESPONSE_ACTION_FOOTER_OFFSET, RESPONSE_ACTION_GAP, RESPONSE_ACTION_ICON_SIZE,
    RESPONSE_TIME_LINE_HEIGHT, RESPONSE_TIME_MARGIN, RESPONSE_TIME_SIZE, RetryImageGeneration,
    SUGGESTION_PRESSED_SCALE, THINKING_SHIMMER_DURATION, THINKING_SHIMMER_FRAME_INTERVAL,
    THINKING_SHIMMER_STEPS, THINKING_SHIMMER_WIDTH, TOOL_GROUP_BODY_MAX_HEIGHT,
    TOOL_GROUP_CHEVRON_SIZE, TOOL_GROUP_EDGE_FADE_DISTANCE, TOOL_GROUP_HEADER_CHEVRON_GAP,
    TOOL_GROUP_HEADER_HEIGHT, TOOL_GROUP_ICON_SIZE, TOOL_GROUP_ICON_TEXT_GAP, TOOL_GROUP_ITEM_GAP,
    TOOL_GROUP_LINE_HEIGHT, TOOL_GROUP_TEXT_SIZE, TOOL_GROUP_TRANSITION_DURATION,
    USER_MESSAGE_BUBBLE_RADIUS, USER_MESSAGE_BUBBLE_SUPERELLIPSE, USER_MESSAGE_FOOTER_GAP,
    USER_MESSAGE_FOOTER_HEIGHT, USER_MESSAGE_FOOTER_OFFSET, USER_MESSAGE_FOOTER_SIDE_MARGIN,
    USER_MESSAGE_HORIZONTAL_PADDING, USER_MESSAGE_LINE_HEIGHT, USER_MESSAGE_MAX_WIDTH_RATIO,
    USER_MESSAGE_PARAGRAPH_GAP, USER_MESSAGE_TEXT_SIZE, USER_MESSAGE_TIME_LINE_HEIGHT,
    USER_MESSAGE_TIME_SIZE, USER_MESSAGE_VERTICAL_PADDING,
    animation::{
        reasoning_transition_ease, thinking_shimmer_alpha, thinking_shimmer_band_left,
        thinking_shimmer_progress, thinking_shimmer_step, tool_group_chevron_transition_ease,
    },
    collaboration::{
        collaboration_display_name, collaboration_status_label, collaboration_ui_identity,
        toggle_collaboration_item,
    },
    mcp::{humanize_mcp_tool_name, mcp_tool_call_label},
    media::image_generation_preview_size,
    messages::user_message_paragraphs,
    reasoning::{
        active_reasoning_body, completed_reasoning_body, format_reasoning_elapsed,
        reasoning_header_label, toggle_reasoning_item,
    },
    timeline::{
        ActivityStreamUnit, activity_stream_units, command_activity_row_count,
        command_activity_summaries, command_activity_summary, completed_tool_group_summary,
        conversation_status, generic_command_activity_summary, reasoning_activity_title,
        strip_terminal_line_ending, tool_group_reasoning_title,
    },
    tools::toggle_tool_activity_group,
};
use crate::{
    agent::{
        AgentCollaboration, AgentCollaborationStatus, AgentCollaborationTool,
        AgentCollaboratorState, AgentCollaboratorStatus, AgentContextCompaction,
        AgentImageGeneration, AgentImageGenerationFailure, AgentImageGenerationStatus,
        AgentImageView, AgentMcpToolCall, AgentMcpToolCallStatus, CommandExecution,
        CommandExecutionAction, CommandExecutionStatus, HistoryItemDetail, HistoryTurnStatus,
        LegacySubAgentActivityKind, ThreadActivity, ThreadHistory, ThreadHistoryItem,
        ThreadSummary, ThreadTurn,
    },
    components::{
        composer::COMPOSER_CORNER_RADIUS,
        file_change::captured_file_change_activity_fixture,
        prompt_input::Submit,
        user_input_request::{UserInputKeyboardFocus, UserInputRequestStatus},
    },
    conversation::{ConversationActivity, ConversationPhase, ReasoningActivityPresentation},
    theme::ThemeMode,
};

fn simulate_next_frame(app: &mut TestApp, window: &TestAppWindow<HomeView>, elapsed_ms: u64) {
    app.advance_clock(Duration::from_millis(elapsed_ms));
    let handle = window.handle();
    app.update(|cx| {
        cx.update_window(handle.into(), |_, window, cx| {
            window.simulate_next_frame(cx)
        })
        .unwrap()
    });
}

fn reasoning(id: &str, title: &str, active: bool) -> ReasoningActivityPresentation {
    ReasoningActivityPresentation {
        item_id: id.to_owned(),
        summary: vec![title.to_owned()],
        content: Vec::new(),
        started_at_ms: 1_000,
        completed_at_ms: (!active).then_some(2_000),
    }
}

fn command(
    id: &str,
    action: CommandExecutionAction,
    status: CommandExecutionStatus,
) -> CommandExecution {
    let command = match &action {
        CommandExecutionAction::Read { command, .. }
        | CommandExecutionAction::ListFiles { command, .. }
        | CommandExecutionAction::Search { command, .. }
        | CommandExecutionAction::Unknown { command } => command.clone(),
    };
    CommandExecution {
        id: id.to_owned(),
        command,
        actions: vec![action],
        cwd: "/tmp/project".to_owned(),
        output: String::new(),
        terminal_process_id: None,
        status,
        exit_code: (status == CommandExecutionStatus::Completed).then_some(0),
    }
}

fn collaboration(kind: LegacySubAgentActivityKind) -> AgentCollaboration {
    let (status, agent_status) = match kind {
        LegacySubAgentActivityKind::Started | LegacySubAgentActivityKind::Interacted => (
            AgentCollaborationStatus::InProgress,
            AgentCollaboratorStatus::Running,
        ),
        LegacySubAgentActivityKind::Interrupted => (
            AgentCollaborationStatus::Interrupted,
            AgentCollaboratorStatus::Interrupted,
        ),
        LegacySubAgentActivityKind::Completed => (
            AgentCollaborationStatus::Completed,
            AgentCollaboratorStatus::Completed,
        ),
    };
    AgentCollaboration {
        id: "collaboration_1".into(),
        tool: AgentCollaborationTool::LegacyActivity,
        status,
        sender_thread_id: String::new(),
        receiver_thread_ids: vec!["agent_1".into()],
        agents_states: BTreeMap::from([(
            "agent_1".into(),
            AgentCollaboratorState {
                status: agent_status,
                message: None,
                name: None,
            },
        )]),
        prompt: None,
        model: None,
        reasoning_effort: None,
        legacy_agent_path: Some("/root/collab_evidence_probe".into()),
        legacy_kind: Some(kind),
    }
}

#[test]
fn consecutive_reasoning_and_commands_form_one_tool_activity_group() {
    let activities = vec![
        ConversationActivity::Reasoning(reasoning("reasoning_1", "Inspecting runtime", false)),
        ConversationActivity::Command(command(
            "read_1",
            CommandExecutionAction::Read {
                command: "sed -n '1,20p' src/main.rs".into(),
                name: "main.rs".into(),
                path: "src/main.rs".into(),
            },
            CommandExecutionStatus::Completed,
        )),
        ConversationActivity::Reasoning(reasoning("reasoning_2", "Checking tests", false)),
        ConversationActivity::Command(command(
            "run_1",
            CommandExecutionAction::Unknown {
                command: "cargo test".into(),
            },
            CommandExecutionStatus::Completed,
        )),
        ConversationActivity::AssistantMessage {
            item_id: "message_1".into(),
            text: "完成。".into(),
        },
    ];

    let units = activity_stream_units(&activities);
    assert_eq!(units.len(), 2);
    let ActivityStreamUnit::ToolGroup(group) = &units[0] else {
        panic!("expected grouped tool activity");
    };
    assert_eq!(group.id, "reasoning_1");
    assert_eq!(group.reasoning.len(), 2);
    assert_eq!(group.commands.len(), 2);
    assert_eq!(
        tool_group_reasoning_title(group).as_deref(),
        Some("Checking tests")
    );
    assert_eq!(
        completed_tool_group_summary(group).text,
        "已读取文件运行了命令"
    );
    assert!(matches!(units[1], ActivityStreamUnit::Standalone(_)));

    let separated = vec![
        activities[1].clone(),
        activities[4].clone(),
        activities[3].clone(),
    ];
    let separated_units = activity_stream_units(&separated);
    assert_eq!(separated_units.len(), 3);
    assert!(matches!(
        separated_units[0],
        ActivityStreamUnit::ToolGroup(_)
    ));
    assert!(matches!(
        separated_units[2],
        ActivityStreamUnit::ToolGroup(_)
    ));
}

#[test]
fn image_generation_is_standalone_and_preserves_reference_geometry() {
    let image = AgentImageGeneration {
        id: "generated_1".into(),
        status: AgentImageGenerationStatus::Completed,
        revised_prompt: None,
        path: Some(PathBuf::from("/tmp/generated.png")),
        dimensions: Some((1024, 512)),
        transparent_background: Some(false),
        failure: None,
        load_error: None,
    };
    let units = activity_stream_units(&[
        ConversationActivity::Command(command(
            "read_1",
            CommandExecutionAction::Read {
                command: "cat prompt.txt".into(),
                name: "prompt.txt".into(),
                path: "prompt.txt".into(),
            },
            CommandExecutionStatus::Completed,
        )),
        ConversationActivity::ImageGeneration(image.clone()),
    ]);
    assert_eq!(units.len(), 2);
    assert!(matches!(units[0], ActivityStreamUnit::ToolGroup(_)));
    assert!(matches!(
        &units[1],
        ActivityStreamUnit::Standalone(ConversationActivity::ImageGeneration(actual))
            if actual == &image
    ));
    assert_eq!(
        image_generation_preview_size(Some((1024, 1024))),
        (480.0, 480.0)
    );
    assert_eq!(
        image_generation_preview_size(Some((1024, 512))),
        (480.0, 240.0)
    );
    assert_eq!(
        image_generation_preview_size(Some((512, 1024))),
        (240.0, 480.0)
    );
}

#[test]
fn image_generation_failure_copy_keeps_typed_quota_metadata() {
    let image = AgentImageGeneration {
        id: "generated_failed".into(),
        status: AgentImageGenerationStatus::Failed,
        revised_prompt: None,
        path: None,
        dimensions: None,
        transparent_background: None,
        failure: Some(AgentImageGenerationFailure::UsageLimitExceeded {
            limit_id: "image_generation".into(),
            resets_at: Some(1_788_566_400),
        }),
        load_error: None,
    };
    let (title, detail) = super::media::image_generation_failure_copy(&image);
    assert_eq!(title, "图像生成额度已用完");
    assert!(detail.contains("image_generation"));
    assert!(detail.contains("重置"));

    fn assert_retry_event<T: gpui::EventEmitter<RetryImageGeneration>>() {}
    assert_retry_event::<HomeView>();
}

#[test]
fn image_view_stays_a_standalone_disclosure_between_tool_groups() {
    let activities = vec![
        ConversationActivity::Command(command(
            "read_1",
            CommandExecutionAction::Read {
                command: "sed -n '1,20p' screenshot.png".into(),
                name: "screenshot.png".into(),
                path: "screenshot.png".into(),
            },
            CommandExecutionStatus::Completed,
        )),
        ConversationActivity::ImageView(AgentImageView {
            id: "image_1".into(),
            path: PathBuf::from("/tmp/screenshot.png"),
        }),
        ConversationActivity::Command(command(
            "test_1",
            CommandExecutionAction::Unknown {
                command: "cargo test".into(),
            },
            CommandExecutionStatus::Completed,
        )),
    ];

    let units = activity_stream_units(&activities);
    assert_eq!(units.len(), 3);
    assert!(matches!(units[0], ActivityStreamUnit::ToolGroup(_)));
    assert!(matches!(
        &units[1],
        ActivityStreamUnit::Standalone(ConversationActivity::ImageView(image))
            if image.id == "image_1" && image.path.as_path() == std::path::Path::new("/tmp/screenshot.png")
    ));
    assert!(matches!(units[2], ActivityStreamUnit::ToolGroup(_)));
}

#[test]
fn context_compaction_stays_standalone_between_tool_groups() {
    let command = |id: &str| {
        ConversationActivity::Command(command(
            id,
            CommandExecutionAction::Unknown {
                command: "cargo test".into(),
            },
            CommandExecutionStatus::Completed,
        ))
    };
    let activities = vec![
        command("before"),
        ConversationActivity::ContextCompaction(AgentContextCompaction {
            id: "compact_1".into(),
            completed: true,
        }),
        command("after"),
    ];

    let units = activity_stream_units(&activities);
    assert_eq!(units.len(), 3);
    assert!(matches!(units[0], ActivityStreamUnit::ToolGroup(_)));
    assert!(matches!(
        &units[1],
        ActivityStreamUnit::Standalone(ConversationActivity::ContextCompaction(compaction))
            if compaction.id == "compact_1" && compaction.completed
    ));
    assert!(matches!(units[2], ActivityStreamUnit::ToolGroup(_)));
}

#[test]
fn collaboration_stays_standalone_and_maps_reference_labels() {
    let command = |id: &str| {
        ConversationActivity::Command(command(
            id,
            CommandExecutionAction::Unknown {
                command: "cargo test".into(),
            },
            CommandExecutionStatus::Completed,
        ))
    };
    let running = collaboration(LegacySubAgentActivityKind::Started);
    let activities = vec![
        command("before"),
        ConversationActivity::Collaboration(running.clone()),
        command("after"),
    ];
    let units = activity_stream_units(&activities);
    assert_eq!(units.len(), 3);
    assert!(matches!(units[0], ActivityStreamUnit::ToolGroup(_)));
    assert!(matches!(
        &units[1],
        ActivityStreamUnit::Standalone(ConversationActivity::Collaboration(item))
            if item.id == "collaboration_1"
    ));
    assert!(matches!(units[2], ActivityStreamUnit::ToolGroup(_)));

    assert_eq!(
        collaboration_display_name(&running, "agent_1"),
        "Collab evidence probe"
    );
    assert_eq!(collaboration_status_label(&running, "agent_1"), "开始工作");
    assert_eq!(
        collaboration_status_label(
            &collaboration(LegacySubAgentActivityKind::Interacted),
            "agent_1"
        ),
        "已更新"
    );
    assert_eq!(
        collaboration_status_label(
            &collaboration(LegacySubAgentActivityKind::Interrupted),
            "agent_1"
        ),
        "已中断"
    );
    assert_eq!(
        collaboration_status_label(
            &collaboration(LegacySubAgentActivityKind::Completed),
            "agent_1"
        ),
        "已完成"
    );

    let mut failed = running;
    failed.tool = AgentCollaborationTool::SpawnAgent;
    failed.status = AgentCollaborationStatus::Failed;
    failed.legacy_kind = None;
    failed.legacy_agent_path = None;
    failed.agents_states.get_mut("agent_1").unwrap().status = AgentCollaboratorStatus::Errored;
    assert_eq!(collaboration_status_label(&failed, "agent_1"), "失败");
    assert_eq!(
        collaboration_ui_identity(&collaboration(LegacySubAgentActivityKind::Started)),
        collaboration_ui_identity(&collaboration(LegacySubAgentActivityKind::Completed)),
        "legacy lifecycle item ids differ, so disclosure identity must follow the agent thread"
    );
    assert_eq!(collaboration_ui_identity(&failed), "collaboration_1");
}

#[test]
fn collaboration_disclosure_fixture_and_panel_event_keep_stable_identity() {
    let mut app = TestApp::new();
    let home = app.new_entity(|cx| HomeView::new(ThemeMode::Dark, cx));
    app.update(|cx| toggle_collaboration_item(&home, "collaboration_1", cx));
    assert!(app.read_entity(&home, |home, _| {
        home.expanded_collaborations.contains("collaboration_1")
    }));
    app.update(|cx| toggle_collaboration_item(&home, "collaboration_1", cx));
    assert!(!app.read_entity(&home, |home, _| {
        home.expanded_collaborations.contains("collaboration_1")
    }));

    let opened = Arc::new(Mutex::new(None));
    let observed = opened.clone();
    let _observer = app.new_entity(|cx| {
        cx.subscribe(&home, move |_: &mut (), _, event: &OpenSubAgentPanel, _| {
            *observed.lock().unwrap() = Some(event.clone());
        })
        .detach();
    });
    app.update(|cx| {
        super::collaboration::open_sub_agent_panel(&home, "agent_1", "Evidence agent", cx)
    });
    assert_eq!(
        *opened.lock().unwrap(),
        Some(OpenSubAgentPanel {
            thread_id: "agent_1".to_owned(),
            name: "Evidence agent".to_owned(),
        })
    );
}

#[test]
fn collaboration_row_opens_the_subagent_panel_with_pointer_and_keyboard() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| HomeView::new(ThemeMode::Light, cx),
    );
    window.update(|home, _, cx| home.set_collaboration_for_capture("running", cx));

    let opened = Arc::new(Mutex::new(None));
    let observed = opened.clone();
    let home = window.root();
    let _observer = app.new_entity(|cx| {
        cx.subscribe(&home, move |_: &mut (), _, event: &OpenSubAgentPanel, _| {
            *observed.lock().unwrap() = Some(event.clone());
        })
        .detach();
    });

    window.draw();
    // TestApp's titlebar inset places the first 21 px activity header at
    // y=166..187. This point is inside the focusable agent label.
    window.simulate_click(point(px(180.0), px(180.0)), MouseButton::Left);
    assert_eq!(
        *opened.lock().unwrap(),
        Some(OpenSubAgentPanel {
            thread_id: "01a06b7a-14c2-73b3-9c62-b29e27bd8689".to_owned(),
            name: "Collab evidence probe".to_owned(),
        })
    );
    *opened.lock().unwrap() = None;

    window.simulate_keystrokes("space");
    assert_eq!(
        *opened.lock().unwrap(),
        Some(OpenSubAgentPanel {
            thread_id: "01a06b7a-14c2-73b3-9c62-b29e27bd8689".to_owned(),
            name: "Collab evidence probe".to_owned(),
        })
    );
}

#[test]
fn file_change_joins_the_tool_group_and_leads_its_completed_summary() {
    let activities = vec![
        ConversationActivity::Reasoning(reasoning("reasoning_1", "Applying changes", false)),
        ConversationActivity::Command(command(
            "read_1",
            CommandExecutionAction::Read {
                command: "sed -n '1,20p' src/main.rs".into(),
                name: "main.rs".into(),
                path: "src/main.rs".into(),
            },
            CommandExecutionStatus::Completed,
        )),
        ConversationActivity::FileChange(captured_file_change_activity_fixture("completed")),
        ConversationActivity::Command(command(
            "run_1",
            CommandExecutionAction::Unknown {
                command: "cargo test".into(),
            },
            CommandExecutionStatus::Completed,
        )),
    ];

    let units = activity_stream_units(&activities);
    assert_eq!(units.len(), 1);
    let ActivityStreamUnit::ToolGroup(group) = &units[0] else {
        panic!("expected fileChange inside the tool activity group");
    };
    assert_eq!(group.file_changes.len(), 1);
    assert_eq!(
        completed_tool_group_summary(group).text,
        "编辑了文件读取文件运行了命令"
    );
    assert_eq!(completed_tool_group_summary(group).icon, "message-edit");
}

#[test]
fn active_reasoning_follows_the_latest_json_rpc_item_until_it_completes() {
    let later_items = || {
        vec![
            ConversationActivity::AssistantMessage {
                item_id: "message_1".into(),
                text: "先说明当前进度。".into(),
            },
            ConversationActivity::Command(command(
                "read_1",
                CommandExecutionAction::Read {
                    command: "sed -n '1,20p' src/main.rs".into(),
                    name: "main.rs".into(),
                    path: "src/main.rs".into(),
                },
                CommandExecutionStatus::Completed,
            )),
            ConversationActivity::AssistantMessage {
                item_id: "message_2".into(),
                text: "继续分析读取结果。".into(),
            },
        ]
    };

    let mut active_activities = vec![ConversationActivity::Reasoning(reasoning(
        "reasoning_1",
        "Inspecting the implementation",
        true,
    ))];
    active_activities.extend(later_items());
    let active_units = activity_stream_units(&active_activities);

    assert_eq!(active_units.len(), 4);
    assert!(matches!(
        &active_units[0],
        ActivityStreamUnit::Standalone(ConversationActivity::AssistantMessage {
            item_id,
            ..
        }) if item_id == "message_1"
    ));
    assert!(matches!(active_units[1], ActivityStreamUnit::ToolGroup(_)));
    assert!(matches!(
        &active_units[2],
        ActivityStreamUnit::Standalone(ConversationActivity::AssistantMessage {
            item_id,
            ..
        }) if item_id == "message_2"
    ));
    assert!(matches!(
        &active_units[3],
        ActivityStreamUnit::Standalone(ConversationActivity::Reasoning(reasoning))
            if reasoning.item_id == "reasoning_1" && reasoning.is_active()
    ));

    let mut completed_activities = vec![ConversationActivity::Reasoning(reasoning(
        "reasoning_1",
        "Inspecting the implementation",
        false,
    ))];
    completed_activities.extend(later_items());
    let completed_units = activity_stream_units(&completed_activities);
    assert_eq!(completed_units.len(), 3);
    assert!(matches!(
        &completed_units[0],
        ActivityStreamUnit::Standalone(ConversationActivity::AssistantMessage {
            item_id,
            ..
        }) if item_id == "message_1"
    ));
    assert!(!completed_units.iter().any(|unit| matches!(
        unit,
        ActivityStreamUnit::Standalone(ConversationActivity::Reasoning(_))
    )));
}

#[test]
fn completed_reasoning_without_a_following_command_is_not_rendered() {
    let activities = vec![
        ConversationActivity::Reasoning(reasoning("reasoning_1", "Only reasoning", false)),
        ConversationActivity::AssistantMessage {
            item_id: "message_1".into(),
            text: "结论。".into(),
        },
    ];
    let units = activity_stream_units(&activities);
    assert_eq!(units.len(), 1);
    assert!(matches!(
        &units[0],
        ActivityStreamUnit::Standalone(ConversationActivity::AssistantMessage {
            item_id,
            ..
        }) if item_id == "message_1"
    ));

    let titled = ReasoningActivityPresentation {
        summary: vec!["**Header title**".into(), "Longer reasoning body".into()],
        ..reasoning("reasoning_title", "unused", false)
    };
    assert_eq!(
        reasoning_activity_title(&titled).as_deref(),
        Some("Header title")
    );
}

#[test]
fn mcp_tool_call_label_matches_the_captured_chatgpt_row() {
    assert_eq!(MCP_TOOL_CALL_ROW_HEIGHT, 21.0);
    assert_eq!(MCP_TOOL_CALL_ICON_SIZE, 16.0);
    assert_eq!(MCP_TOOL_CALL_ICON_TEXT_GAP, 6.0);
    assert_eq!(MCP_TOOL_CALL_TEXT_SIZE, 14.0);
    assert_eq!(
        humanize_mcp_tool_name("get_usage_limits"),
        "Get usage limits"
    );
    assert_eq!(humanize_mcp_tool_name("createThread"), "Create thread");
    let mut tool_call = AgentMcpToolCall {
        id: "mcp_1".into(),
        server: "codex_app".into(),
        tool: "get_usage_limits".into(),
        status: AgentMcpToolCallStatus::Completed,
        arguments: serde_json::json!({}),
        app_context: None,
        plugin_id: None,
        result: Some(serde_json::json!({"content": []})),
        error: None,
        legacy_resource_uri: None,
        read_only_hint: Some(true),
        duration_ms: Some(1535),
        progress: Vec::new(),
    };
    assert_eq!(mcp_tool_call_label(&tool_call), "Get usage limits");
    tool_call.app_context = Some(serde_json::json!({
        "connectorId": "connector_1",
        "actionName": "Account limits"
    }));
    assert_eq!(mcp_tool_call_label(&tool_call), "Account limits");

    let units = activity_stream_units(&[ConversationActivity::McpToolCall(Box::from(tool_call))]);
    assert!(matches!(
        &units[0],
        ActivityStreamUnit::Standalone(ConversationActivity::McpToolCall(call))
            if call.id == "mcp_1"
    ));
}

#[test]
fn command_rows_use_the_app_server_action_semantics() {
    let read = command(
        "read",
        CommandExecutionAction::Read {
            command: "sed -n '1,20p' src/main.rs".into(),
            name: "main.rs".into(),
            path: "src/main.rs".into(),
        },
        CommandExecutionStatus::Completed,
    );
    assert_eq!(command_activity_summary(&read).text, "已读取 main.rs");

    let search = command(
        "search",
        CommandExecutionAction::Search {
            command: "rg needle src".into(),
            path: Some("src".into()),
            query: Some("needle".into()),
        },
        CommandExecutionStatus::InProgress,
    );
    assert_eq!(
        command_activity_summary(&search).text,
        "正在 src 中搜索“needle”"
    );

    let shell = command(
        "shell",
        CommandExecutionAction::Unknown {
            command: "cargo check".into(),
        },
        CommandExecutionStatus::Completed,
    );
    assert_eq!(command_activity_summary(&shell).text, "已运行 cargo check");
}

#[test]
fn multiline_history_command_summary_collapses_to_one_activity_row() {
    let command = command(
        "multiline-command",
        CommandExecutionAction::Unknown {
            command: "python3 - <<'PY'\nfrom PIL import Image\nprint('done')\nPY".to_owned(),
        },
        CommandExecutionStatus::Completed,
    );

    let summary = generic_command_activity_summary(&command, &command.command);
    assert_eq!(
        summary.text,
        "已运行 python3 - <<'PY' from PIL import Image print('done') PY"
    );
    assert_eq!(summary.text.lines().count(), 1);
}

#[test]
fn one_command_execution_renders_every_structured_action_as_its_own_row() {
    let command = CommandExecution {
        id: "exec_many".into(),
        command: "compound command".into(),
        actions: vec![
            CommandExecutionAction::Read {
                command: "sed main.rs".into(),
                name: "main.rs".into(),
                path: "src/main.rs".into(),
            },
            CommandExecutionAction::Search {
                command: "rg needle src".into(),
                path: Some("src".into()),
                query: Some("needle".into()),
            },
            CommandExecutionAction::Unknown {
                command: "cargo check".into(),
            },
        ],
        cwd: "/tmp/project".into(),
        output: String::new(),
        terminal_process_id: None,
        status: CommandExecutionStatus::Completed,
        exit_code: Some(0),
    };

    let summaries = command_activity_summaries(&command);
    assert_eq!(command_activity_row_count(&command), 3);
    assert_eq!(
        summaries
            .into_iter()
            .map(|summary| summary.text)
            .collect::<Vec<_>>(),
        vec![
            "已读取 main.rs",
            "已在 src 中搜索“needle”",
            "已运行 cargo check",
        ]
    );
}

#[test]
fn tool_group_matches_the_live_cdp_geometry() {
    assert_eq!(TOOL_GROUP_HEADER_HEIGHT, 21.0);
    assert_eq!(TOOL_GROUP_TEXT_SIZE, 14.0);
    assert_eq!(TOOL_GROUP_LINE_HEIGHT, 21.0);
    assert_eq!(TOOL_GROUP_ICON_SIZE, 16.0);
    assert_eq!(TOOL_GROUP_ICON_TEXT_GAP, 6.0);
    assert_eq!(TOOL_GROUP_HEADER_CHEVRON_GAP, 4.0);
    assert_eq!(TOOL_GROUP_CHEVRON_SIZE, 14.0);
    assert_eq!(TOOL_GROUP_ITEM_GAP, 4.0);
    assert_eq!(TOOL_GROUP_BODY_MAX_HEIGHT, 224.0);
    assert_eq!(TOOL_GROUP_EDGE_FADE_DISTANCE, 24.0);
    assert_eq!(DISCLOSURE_FOCUS_PADDING, 2.0);
    assert_eq!(TOOL_GROUP_TRANSITION_DURATION, Duration::from_millis(300));
    assert!(tool_group_chevron_transition_ease(0.5) > 0.7);
    assert!(tool_group_chevron_transition_ease(0.5) < 0.9);
}

#[test]
fn conversation_viewport_scrolls_and_does_not_snap_back_after_user_scrolls_up() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(420.0)),
            })),
            ..Default::default()
        },
        |_, cx| HomeView::new(ThemeMode::Dark, cx),
    );

    window.update(|home, _, cx| home.set_tool_group_for_capture(true, false, cx));
    window.draw();

    assert!(window.read(|home, _| home.conversation_list.is_following_tail()));

    // Use the empty gutter beside the centered 736px message column so
    // only the main conversation viewport receives this gesture.
    window.simulate_scroll(point(px(20.0), px(200.0)), point(px(0.0), px(96.0)));
    let user_offset = window.read(|home, _| home.conversation_list.logical_scroll_top());
    assert!(!window.read(|home, _| home.conversation_list.is_following_tail()));

    // A streaming repaint must preserve the user's reading position.
    window.update(|_, _, cx| cx.notify());
    window.draw();
    let repainted_offset = window.read(|home, _| home.conversation_list.logical_scroll_top());
    assert_eq!(repainted_offset.item_ix, user_offset.item_ix);
    assert_eq!(repainted_offset.offset_in_item, user_offset.offset_in_item);

    window.simulate_scroll(point(px(20.0), px(200.0)), point(px(0.0), px(-10_000.0)));
    window.draw();
    window.read(|home, _| {
        assert!(home.conversation_list.is_following_tail());
    });
}

#[test]
fn conversation_insets_preserve_the_full_viewport_and_composer_clearance() {
    assert_eq!(CONVERSATION_TOP_INSET, 78.0);
    assert_eq!(CONVERSATION_BOTTOM_INSET, 153.0);
    assert_eq!(COMPOSER_BOTTOM_INSET, 15.0);
    assert_eq!(COMPOSER_CORNER_RADIUS, 24.0);
    const {
        assert!(CONVERSATION_BOTTOM_INSET > COMPOSER_BOTTOM_INSET + 98.0);
    }
}

#[test]
fn nested_tool_scroll_does_not_move_the_conversation_until_it_reaches_an_edge() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(420.0)),
            })),
            ..Default::default()
        },
        |_, cx| HomeView::new(ThemeMode::Dark, cx),
    );

    window.update(|home, _, cx| home.set_tool_group_for_capture(true, false, cx));
    window.draw();
    let (outer_before, inner_before, inner_max, inner_position) = window.read(|home, _| {
        let inner = home
            .tool_group_scroll_handles
            .get("tool-group-ui-capture")
            .expect("tool group scroll handle");
        (
            home.conversation_list.is_following_tail(),
            f32::from(inner.offset().y),
            f32::from(inner.max_offset().y),
            inner.bounds().center(),
        )
    });
    assert!(inner_max > 0.0);
    assert!((inner_before + inner_max).abs() < 0.01);

    window.simulate_scroll(inner_position, point(px(0.0), px(48.0)));
    let (outer_after, inner_after) = window.read(|home, _| {
        let inner = home
            .tool_group_scroll_handles
            .get("tool-group-ui-capture")
            .expect("tool group scroll handle");
        (
            home.conversation_list.is_following_tail(),
            f32::from(inner.offset().y),
        )
    });
    assert!(
        inner_after > inner_before,
        "inner={inner_before}->{inner_after}, outer={outer_before}->{outer_after}, position={inner_position:?}"
    );
    assert!(
        outer_after == outer_before,
        "inner={inner_before}->{inner_after}, outer={outer_before}->{outer_after}, position={inner_position:?}"
    );

    window.simulate_scroll(inner_position, point(px(0.0), px(48.0)));
    let (outer_at_edge, inner_at_edge) = window.read(|home, _| {
        let inner = home
            .tool_group_scroll_handles
            .get("tool-group-ui-capture")
            .expect("tool group scroll handle");
        (
            home.conversation_list.is_following_tail(),
            f32::from(inner.offset().y),
        )
    });
    assert!(!outer_at_edge && outer_after);
    assert!(inner_at_edge.abs() < 0.01);
}

#[test]
fn tool_group_disclosure_uses_one_toggle_path_for_pointer_and_keyboard_activation() {
    let mut app = TestApp::new();
    let home = app.new_entity(|cx| HomeView::new(ThemeMode::Dark, cx));
    let scroll_handle = gpui::ScrollHandle::new();

    app.update(|cx| {
        toggle_tool_activity_group(&home, "group_1", false, &scroll_handle, cx);
    });
    assert!(app.read_entity(&home, |home, _| {
        home.expanded_tool_groups.contains("group_1")
    }));

    app.update(|cx| {
        toggle_tool_activity_group(&home, "group_1", false, &scroll_handle, cx);
    });
    assert!(!app.read_entity(&home, |home, _| {
        home.expanded_tool_groups.contains("group_1")
    }));

    app.update(|cx| {
        toggle_tool_activity_group(&home, "group_1", true, &scroll_handle, cx);
    });
    assert!(app.read_entity(&home, |home, _| {
        home.collapsed_active_tool_groups.contains("group_1")
    }));
}

#[test]
fn resumed_historical_tool_group_keeps_its_disclosure_state() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| HomeView::new(ThemeMode::Dark, cx),
    );
    let command_turn = |turn_id: &str, item_id: &str, prompt: &str| ThreadTurn {
        turn_id: turn_id.to_owned(),
        status: HistoryTurnStatus::Completed,
        items_view: HistoryItemDetail::Full,
        items: vec![
            ThreadHistoryItem::UserMessage {
                images: Vec::new(),
                item_id: format!("{turn_id}-user"),
                text: prompt.to_owned(),
            },
            ThreadHistoryItem::Command {
                item_id: item_id.to_owned(),
                command: "cargo check".to_owned(),
                output: "Finished dev profile".to_owned(),
                status: CommandExecutionStatus::Completed,
                actions: Vec::new(),
                cwd: None,
                exit_code: None,
            },
        ],
        started_at: Some(1_000),
        completed_at: Some(2_000),
        duration_ms: Some(1_000),
        error: None,
    };
    let history = ThreadHistory {
        thread: ThreadSummary {
            thread_id: "resume-thread".to_owned(),
            title: "Resume disclosure".to_owned(),
            preview: String::new(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            section: None,
            created_at: 1,
            updated_at: 2,
            recency_at: Some(2),
            activity: ThreadActivity::Idle,
        },
        turns: vec![
            command_turn("historical-turn", "historical-command", "first"),
            command_turn("current-turn", "current-command", "second"),
        ],
        next_turn_cursor: None,
        backwards_turn_cursor: None,
    };

    window.update(|home, _, cx| {
        home.composer_entity()
            .update(cx, |composer, cx| composer.hydrate_history(history, cx));
    });
    window.draw();
    window.read(|home, _| {
        assert!(
            home.tool_group_disclosure_transitions
                .contains_key("historical-command")
        );
        assert!(
            home.command_scroll_handles
                .contains_key("historical-command")
        );
    });

    window.update(|home, _, cx| {
        home.expanded_tool_groups
            .insert("historical-command".to_owned());
        cx.notify();
    });
    window.draw();
    window.read(|home, _| {
        assert!(home.expanded_tool_groups.contains("historical-command"));
        assert_eq!(
            home.tool_group_disclosure_transitions
                .get("historical-command")
                .expect("historical transition")
                .target,
            1.0
        );
    });
}

#[test]
fn dense_resumed_tool_group_preserves_row_height_and_scrolls_instead_of_overlapping() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| HomeView::new(ThemeMode::Dark, cx),
    );
    let mut dense_items = vec![ThreadHistoryItem::UserMessage {
        images: Vec::new(),
        item_id: "dense-user".to_owned(),
        text: "inspect the native material".to_owned(),
    }];
    dense_items.extend((0..20).map(|index| ThreadHistoryItem::Command {
        item_id: format!("dense-command-{index}"),
        command: if index == 4 {
            "python3 - <<'PY'\nfrom PIL import Image\nprint('done')\nPY".to_owned()
        } else {
            format!("cargo check --package fixture-{index}")
        },
        output: "Finished dev profile".to_owned(),
        status: CommandExecutionStatus::Completed,
        actions: Vec::new(),
        cwd: None,
        exit_code: None,
    }));
    let history = ThreadHistory {
        thread: ThreadSummary {
            thread_id: "dense-resume-thread".to_owned(),
            title: "Dense resume disclosure".to_owned(),
            preview: String::new(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            section: None,
            created_at: 1,
            updated_at: 2,
            recency_at: Some(2),
            activity: ThreadActivity::Idle,
        },
        turns: vec![
            ThreadTurn {
                turn_id: "dense-historical-turn".to_owned(),
                status: HistoryTurnStatus::Completed,
                items_view: HistoryItemDetail::Full,
                items: dense_items,
                started_at: Some(1_000),
                completed_at: Some(2_000),
                duration_ms: Some(1_000),
                error: None,
            },
            ThreadTurn {
                turn_id: "current-turn".to_owned(),
                status: HistoryTurnStatus::Completed,
                items_view: HistoryItemDetail::Full,
                items: vec![ThreadHistoryItem::UserMessage {
                    images: Vec::new(),
                    item_id: "current-user".to_owned(),
                    text: "continue".to_owned(),
                }],
                started_at: Some(3_000),
                completed_at: Some(4_000),
                duration_ms: Some(1_000),
                error: None,
            },
        ],
        next_turn_cursor: None,
        backwards_turn_cursor: None,
    };

    window.update(|home, _, cx| {
        home.composer_entity()
            .update(cx, |composer, cx| composer.hydrate_history(history, cx));
        home.expanded_tool_groups
            .insert("dense-command-0".to_owned());
    });
    window.draw();
    window.read(|home, _| {
        let scroll = home
            .tool_group_scroll_handles
            .get("dense-command-0")
            .expect("dense historical tool group scroll handle");
        // 20 fixed 21px rows, 19 four-pixel gaps, and the four-pixel top
        // inset produce 500px of content in the captured 224px viewport.
        assert_eq!(f32::from(scroll.max_offset().y), 276.0);
    });
}

#[test]
fn suggestion_press_uses_the_reference_scale_and_interruptible_transition() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| HomeView::new(ThemeMode::Dark, cx),
    );

    window.update(|home, window, cx| {
        home.set_suggestion_pressed(0, true, window, cx);
    });
    simulate_next_frame(&mut app, &window, 75);
    let pressed_midpoint = window.read(|home, _| home.suggestion_scale[0]);
    assert!(pressed_midpoint > SUGGESTION_PRESSED_SCALE && pressed_midpoint < 1.0);

    // Releasing halfway through must reverse from the rendered value,
    // rather than jumping to either endpoint.
    window.update(|home, window, cx| {
        home.set_suggestion_pressed(0, false, window, cx);
    });
    assert_eq!(
        window.read(|home, _| home.suggestion_scale[0]),
        pressed_midpoint
    );
    simulate_next_frame(&mut app, &window, 150);
    assert_eq!(window.read(|home, _| home.suggestion_scale[0]), 1.0);
    assert!(!window.read(|home, _| home.suggestion_animation_running));
}

#[test]
fn starting_a_prompt_does_not_render_a_synthetic_status_message() {
    assert_eq!(conversation_status(ConversationPhase::Starting), None);
    assert_eq!(
        conversation_status(ConversationPhase::Thinking),
        Some("正在思考")
    );
}

#[test]
fn event_notices_match_the_live_cdp_geometry() {
    assert_eq!(NOTICE_RADIUS, 20.0);
    assert_eq!(NOTICE_TEXT_SIZE, 13.0);
    assert_eq!(NOTICE_LINE_HEIGHT, 20.0);
    assert_eq!(NOTICE_ICON_SIZE, 18.0);
    assert_eq!(NOTICE_ERROR_GAP, 12.0);
    assert_eq!(NOTICE_WARNING_GAP, 16.0);
    assert_eq!(NOTICE_ERROR_CONTENT_GAP, 6.0);
    assert_eq!(NOTICE_WARNING_CONTENT_GAP, 8.0);
    assert_eq!(NOTICE_BUTTON_HEIGHT, 24.0);
}

#[test]
fn thinking_shimmer_matches_the_cdp_animation_geometry() {
    assert_eq!(THINKING_SHIMMER_DURATION, Duration::from_secs(1));
    assert_eq!(thinking_shimmer_progress(Duration::ZERO), 0.0);
    assert_eq!(thinking_shimmer_progress(Duration::from_millis(500)), 0.5);
    assert_eq!(thinking_shimmer_progress(Duration::from_secs(1)), 1.0);
    assert_eq!(thinking_shimmer_step(0.02), 0.0);
    assert_eq!(thinking_shimmer_step(1.0 / 48.0), 1.0 / 48.0);
    assert_eq!(
        thinking_shimmer_band_left(0.0, THINKING_SHIMMER_WIDTH),
        -28.0
    );
    assert_eq!(
        thinking_shimmer_band_left(1.0, THINKING_SHIMMER_WIDTH),
        70.0
    );
    assert_eq!(thinking_shimmer_alpha(0.0), 0.0);
    assert_eq!(thinking_shimmer_alpha(0.4), 0.75);
    assert_eq!(thinking_shimmer_alpha(0.6), 0.75);
    assert_eq!(thinking_shimmer_alpha(1.0), 0.0);
}

#[test]
fn reasoning_item_matches_the_desktop_geometry_and_localized_labels() {
    assert_eq!(REASONING_HEADER_HEIGHT, 21.0);
    assert_eq!(REASONING_TEXT_SIZE, 14.0);
    assert_eq!(REASONING_LINE_HEIGHT, 21.0);
    assert_eq!(REASONING_CHEVRON_SIZE, 14.0);
    assert_eq!(REASONING_BODY_MAX_HEIGHT, 140.0);
    assert_eq!(REASONING_TRANSITION_DURATION, Duration::from_millis(300));
    assert_eq!(reasoning_transition_ease(0.0), 0.0);
    assert_eq!(reasoning_transition_ease(1.0), 1.0);
    assert!(reasoning_transition_ease(0.5) > 0.9);
    assert_eq!(format_reasoning_elapsed(1), "1s");
    assert_eq!(format_reasoning_elapsed(29_000), "29s");
    assert_eq!(format_reasoning_elapsed(82_000), "1m 22s");
    assert_eq!(format_reasoning_elapsed(3_520_000), "58m 40s");

    let active = ReasoningActivityPresentation {
        item_id: "reasoning_1".into(),
        summary: vec![],
        content: vec![],
        started_at_ms: 1_000,
        completed_at_ms: None,
    };
    assert_eq!(reasoning_header_label(&active), "正在思考");

    let complete = ReasoningActivityPresentation {
        completed_at_ms: Some(30_000),
        ..active.clone()
    };
    assert_eq!(reasoning_header_label(&complete), "思考了 29s");
    let missing_elapsed = ReasoningActivityPresentation {
        completed_at_ms: Some(1_000),
        ..active
    };
    assert_eq!(reasoning_header_label(&missing_elapsed), "完成思考");
}

#[test]
fn active_reasoning_hides_the_streamed_summary_title_like_chatgpt() {
    assert_eq!(active_reasoning_body("  普通正文"), "普通正文");
    assert_eq!(
        active_reasoning_body("**检查实现**\n\n正在阅读协议"),
        "正在阅读协议"
    );
    assert_eq!(active_reasoning_body("**尚未闭合"), "");
}

#[test]
fn completed_reasoning_renders_the_summary_title_without_markdown_delimiters() {
    assert_eq!(
        completed_reasoning_body("**检查实现**\n\n正在阅读协议"),
        (Some("检查实现".into()), "正在阅读协议".into())
    );
    assert_eq!(
        completed_reasoning_body("普通正文"),
        (None, "普通正文".into())
    );
    assert_eq!(
        completed_reasoning_body("**尚未闭合"),
        (None, "**尚未闭合".into())
    );
}

#[test]
fn reasoning_disclosure_uses_one_toggle_path_for_pointer_and_keyboard_activation() {
    let mut app = TestApp::new();
    let home = app.new_entity(|cx| HomeView::new(ThemeMode::Dark, cx));
    let scroll_handle = gpui::ScrollHandle::new();

    app.update(|cx| {
        toggle_reasoning_item(&home, "reasoning_1", &scroll_handle, cx);
    });
    assert!(app.read_entity(&home, |home, _| {
        home.expanded_reasoning.contains("reasoning_1")
    }));

    app.update(|cx| {
        toggle_reasoning_item(&home, "reasoning_1", &scroll_handle, cx);
    });
    assert!(!app.read_entity(&home, |home, _| {
        home.expanded_reasoning.contains("reasoning_1")
    }));
}

#[test]
fn completed_standalone_reasoning_exposes_no_disclosure_target() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| HomeView::new(ThemeMode::Dark, cx),
    );

    window.update(|home, _, cx| home.set_reasoning_for_capture("completed-content", false, cx));
    window.draw();
    // This was the old standalone completed-reasoning hitbox. It must no
    // longer mount an interactive disclosure row.
    window.simulate_click(point(px(100.0), px(176.0)), MouseButton::Left);
    window.simulate_keystrokes("space");
    assert!(!window.read(|home, _| { home.expanded_reasoning.contains("reasoning-ui-capture") }));
}

#[test]
fn reasoning_disclosure_starts_settled_then_runs_the_reference_transition() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| HomeView::new(ThemeMode::Dark, cx),
    );

    window.update(|home, _, cx| home.set_reasoning_for_capture("active", false, cx));
    window.draw();
    window.read(|home, _| {
        let transition = home
            .reasoning_disclosure_transitions
            .get("reasoning-ui-capture")
            .unwrap();
        assert_eq!(transition.progress, 0.0);
        assert!(transition.started_at.is_none());
    });

    window.update(|home, _, cx| home.set_reasoning_for_capture("active-content", false, cx));
    window.draw();
    simulate_next_frame(&mut app, &window, 150);
    let midpoint = window.read(|home, _| {
        home.reasoning_disclosure_transitions
            .get("reasoning-ui-capture")
            .unwrap()
            .progress
    });
    assert!(midpoint > 0.9 && midpoint < 1.0);

    simulate_next_frame(&mut app, &window, 150);
    window.read(|home, _| {
        let transition = home
            .reasoning_disclosure_transitions
            .get("reasoning-ui-capture")
            .unwrap();
        assert_eq!(transition.progress, 1.0);
        assert!(transition.started_at.is_none());
        assert!(!home.reasoning_transition_running);
    });
}

#[test]
fn tool_group_disclosure_responds_to_real_pointer_and_keyboard_events() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| HomeView::new(ThemeMode::Dark, cx),
    );

    window.update(|home, _, cx| home.set_tool_group_for_capture(false, false, cx));
    window.draw();
    // The preamble starts at y=166 and is 22px tall. The stream's 16px
    // gap puts the 21px grouped-activity button at y=204..225.
    window.simulate_click(point(px(100.0), px(214.0)), MouseButton::Left);
    assert!(window.read(|home, _| { home.expanded_tool_groups.contains("tool-group-ui-capture") }));

    window.simulate_keystrokes("space");
    assert!(
        !window.read(|home, _| { home.expanded_tool_groups.contains("tool-group-ui-capture") })
    );
}

#[test]
fn image_view_disclosure_responds_to_real_pointer_and_keyboard_events() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| HomeView::new(ThemeMode::Dark, cx),
    );

    window.update(|home, _, cx| {
        home.set_image_view_for_capture(PathBuf::from("/tmp/image-view.png"), cx)
    });
    let preview_path = Arc::new(Mutex::new(None));
    let observed_preview_path = preview_path.clone();
    let home = window.root();
    let _observer = app.new_entity(|cx| {
        cx.subscribe(&home, move |_: &mut (), _, event: &OpenImagePreview, _| {
            *observed_preview_path.lock().unwrap() = Some(event.0.clone());
        })
        .detach();
    });
    window.draw();
    // The entire captured 21px disclosure row at y=204..225 is clickable.
    window.simulate_click(point(px(100.0), px(214.0)), MouseButton::Left);
    assert!(window.read(|home, _| { home.expanded_tool_groups.contains("image-view-ui-capture") }));

    window.simulate_keystrokes("space");
    assert!(
        window.read(|home, _| { !home.expanded_tool_groups.contains("image-view-ui-capture") })
    );

    window.simulate_keystrokes("enter");
    window.draw();
    window.simulate_click(point(px(100.0), px(250.0)), MouseButton::Left);
    assert_eq!(
        *preview_path.lock().unwrap(),
        Some(PathBuf::from("/tmp/image-view.png"))
    );
}

#[test]
fn uploaded_image_hitboxes_and_keyboard_keep_attachment_order() {
    use crate::theme::Theme;
    use gpui::{Entity, IntoElement, Window};
    struct Images {
        home: Entity<HomeView>,
        paths: Vec<PathBuf>,
    }
    impl gpui::Render for Images {
        fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
            super::messages::user_message_images(
                self.paths
                    .iter()
                    .cloned()
                    .map(crate::agent::UserMessageImage::Local)
                    .collect(),
                self.home.clone(),
                Theme::for_mode(ThemeMode::Dark),
            )
        }
    }
    let paths = ["assets/icons/folder.svg", "assets/icons/image-download.svg"]
        .map(|name| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(name));
    let mut app = TestApp::new();
    let home = app.new_entity(|cx| HomeView::new(ThemeMode::Dark, cx));
    let opened = Arc::new(Mutex::new(Vec::new()));
    let observed = opened.clone();
    let _observer = app.new_entity(|cx| {
        cx.subscribe(&home, move |_: &mut (), _, event: &OpenImagePreview, _| {
            observed.lock().unwrap().push(event.0.clone());
        })
        .detach();
    });
    let mut window = app.open_window_with_options(WindowOptions::default(), |_, _| Images {
        home,
        paths: paths.to_vec(),
    });
    window.draw();
    window.simulate_click(point(px(4.0), px(40.0)), MouseButton::Left);
    window.simulate_click(point(px(76.0), px(40.0)), MouseButton::Left);
    window.simulate_keystrokes("tab");
    window.simulate_keystrokes("space");
    window.simulate_keystrokes("enter");
    assert_eq!(
        *opened.lock().unwrap(),
        vec![
            paths[0].clone(),
            paths[0].clone(),
            paths[1].clone(),
            paths[1].clone()
        ]
    );
}

#[test]
fn computer_use_screenshot_bytes_override_incorrect_historical_mime() {
    assert_eq!(
        super::media::tool_image_format(&[0xff, 0xd8, 0xff, 0xe0], Some("image/png")),
        gpui::ImageFormat::Jpeg
    );
    assert_eq!(
        super::media::tool_image_format(b"\x89PNG\r\n\x1a\n", Some("image/jpeg")),
        gpui::ImageFormat::Png
    );
}

#[test]
fn generated_image_preview_and_failure_retry_respond_to_real_clicks() {
    let suffix = std::process::id();
    let path = std::env::temp_dir().join(format!("gpui-generated-card-{suffix}.png"));
    let png = base64::engine::general_purpose::STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
        .unwrap();
    std::fs::write(&path, png).unwrap();

    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| HomeView::new(ThemeMode::Dark, cx),
    );
    window.update(|home, _, cx| {
        home.set_image_generation_for_capture("completed", Some(path.clone()), cx)
    });
    let preview = Arc::new(Mutex::new(None));
    let observed_preview = preview.clone();
    let retries = Arc::new(Mutex::new(0usize));
    let observed_retries = retries.clone();
    let home = window.root();
    let _observer = app.new_entity(|cx| {
        cx.subscribe(&home, move |_: &mut (), _, event: &OpenImagePreview, _| {
            *observed_preview.lock().unwrap() = Some(event.0.clone());
        })
        .detach();
        cx.subscribe(&home, move |_: &mut (), _, _: &RetryImageGeneration, _| {
            *observed_retries.lock().unwrap() += 1;
        })
        .detach();
    });
    window.draw();
    // This point stays well inside the 480×480 preview in the fixed
    // 900×700 test viewport, even after the conversation follows its tail.
    window.simulate_click(point(px(220.0), px(320.0)), MouseButton::Left);
    assert_eq!(*preview.lock().unwrap(), Some(path.clone()));

    window.update(|home, _, cx| home.set_image_generation_for_capture("failed", None, cx));
    window.draw();
    // The retry control is 36px high; click its interior instead of an
    // antialiased border pixel so this remains a real pointer-path test.
    window.simulate_click(point(px(113.0), px(298.0)), MouseButton::Left);
    assert_eq!(*retries.lock().unwrap(), 1);

    std::fs::remove_file(path).unwrap();
}

#[test]
fn tool_group_disclosure_starts_settled_then_runs_the_reference_transition() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| HomeView::new(ThemeMode::Dark, cx),
    );

    window.update(|home, _, cx| home.set_tool_group_for_capture(false, false, cx));
    window.draw();
    window.read(|home, _| {
        let transition = home
            .tool_group_disclosure_transitions
            .get("tool-group-ui-capture")
            .unwrap();
        assert_eq!(transition.progress, 0.0);
        assert!(transition.started_at.is_none());
    });

    window.update(|home, _, cx| {
        home.expanded_tool_groups
            .insert("tool-group-ui-capture".to_owned());
        cx.notify();
    });
    window.draw();
    simulate_next_frame(&mut app, &window, 150);
    let midpoint = window.read(|home, _| {
        home.tool_group_disclosure_transitions
            .get("tool-group-ui-capture")
            .unwrap()
            .progress
    });
    assert!(midpoint > 0.9 && midpoint < 1.0);
    let chevron_midpoint = window.read(|home, _| {
        home.tool_group_disclosure_transitions
            .get("tool-group-ui-capture")
            .unwrap()
            .chevron_progress
    });
    assert!(chevron_midpoint > 0.7 && chevron_midpoint < 0.9);

    simulate_next_frame(&mut app, &window, 150);
    window.read(|home, _| {
        let transition = home
            .tool_group_disclosure_transitions
            .get("tool-group-ui-capture")
            .unwrap();
        assert_eq!(transition.progress, 1.0);
        assert_eq!(transition.chevron_progress, 1.0);
        assert!(transition.started_at.is_none());
        assert!(!home.tool_group_transition_running);
    });
}

#[test]
fn thinking_shimmer_timer_advances_and_loops_the_rendered_phase() {
    let mut app = TestApp::new();
    let home = app.new_entity(|cx| HomeView::new(ThemeMode::Dark, cx));

    app.update_entity(&home, |home, cx| home.start_thinking_shimmer(cx));
    assert_eq!(
        app.read_entity(&home, |home, _| home.thinking_shimmer_progress),
        0.0
    );

    app.advance_clock(THINKING_SHIMMER_FRAME_INTERVAL);
    app.run_until_parked();
    let progress = app.read_entity(&home, |home, _| home.thinking_shimmer_progress);
    assert!((progress - 1.0 / THINKING_SHIMMER_STEPS).abs() < 0.000_001);

    for _ in 1..THINKING_SHIMMER_STEPS as usize {
        app.advance_clock(THINKING_SHIMMER_FRAME_INTERVAL);
        app.run_until_parked();
    }
    assert_eq!(
        app.read_entity(&home, |home, _| home.thinking_shimmer_progress),
        1.0
    );

    app.advance_clock(THINKING_SHIMMER_FRAME_INTERVAL);
    app.run_until_parked();
    let wrapped_progress = app.read_entity(&home, |home, _| home.thinking_shimmer_progress);
    assert!((wrapped_progress - 1.0 / THINKING_SHIMMER_STEPS).abs() < 0.000_001);
    assert!(app.read_entity(&home, |home, _| home.thinking_shimmer_running));

    app.update_entity(&home, |home, cx| home.sync_thinking_shimmer(false, cx));
    app.advance_clock(THINKING_SHIMMER_FRAME_INTERVAL);
    app.run_until_parked();
    assert_eq!(
        app.read_entity(&home, |home, _| home.thinking_shimmer_progress),
        0.0
    );
    assert!(!app.read_entity(&home, |home, _| home.thinking_shimmer_running));
}

#[test]
fn user_bubble_uses_the_live_cdp_corner_radius() {
    assert_eq!(USER_MESSAGE_MAX_WIDTH_RATIO, 0.7);
    assert_eq!(USER_MESSAGE_HORIZONTAL_PADDING, 16.0);
    assert_eq!(USER_MESSAGE_VERTICAL_PADDING, 10.0);
    assert_eq!(USER_MESSAGE_TEXT_SIZE, 14.0);
    assert_eq!(USER_MESSAGE_LINE_HEIGHT, 22.75);
    assert_eq!(USER_MESSAGE_PARAGRAPH_GAP, 20.0);
    assert_eq!(USER_MESSAGE_BUBBLE_RADIUS, 22.0);
    assert_eq!(USER_MESSAGE_BUBBLE_SUPERELLIPSE, 1.5);
}

#[test]
fn user_bubble_paragraph_layout_preserves_hard_breaks_and_collapses_blank_runs() {
    assert_eq!(user_message_paragraphs("single line"), vec!["single line"]);
    assert_eq!(
        user_message_paragraphs("first\nsecond"),
        vec!["first\nsecond"]
    );
    assert_eq!(
        user_message_paragraphs("空白前\n\n\n空白后 Blank"),
        vec!["空白前", "空白后 Blank"]
    );
}

#[test]
fn side_chat_wrapped_user_rows_remeasure_when_the_panel_narrows() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(0.0), px(0.0)),
                size(px(700.0), px(850.0)),
            ))),
            ..Default::default()
        },
        |_, cx| {
            let mut home = HomeView::new(ThemeMode::Dark, cx);
            home.presentation = super::HomePresentation::SideChat;
            home
        },
    );
    window.update(|home, _, cx| home.submit_prompt_for_capture(
        "这次仅验证侧边聊天排版。请用两段中文说明侧边聊天用于独立提问，主对话保持原状。缩窄面板后每一行都应完整显示，回复应位于用户气泡下方。", cx));
    for _ in 0..3 {
        window.draw();
        app.run_until_parked();
    }
    let height = |window: &TestAppWindow<HomeView>| {
        window.read(|home, _| {
            let index = home
                .conversation_rows
                .iter()
                .position(|row| {
                    matches!(
                        row,
                        super::timeline::ConversationListRow::CurrentUser { .. }
                    )
                })
                .unwrap();
            home.conversation_list
                .bounds_for_item(index)
                .unwrap()
                .size
                .height
        })
    };
    let wide_height = height(&window);
    window.update(|_, window, cx| {
        window.resize(size(px(272.0), px(850.0)));
        window.bounds_changed(cx);
    });
    for _ in 0..3 {
        window.draw();
        app.run_until_parked();
    }
    let narrow_height = height(&window);
    assert!(
        narrow_height > wide_height + px(40.0),
        "wrapped text must increase its virtual row height: {wide_height:?} -> {narrow_height:?}; content width {}, list {:?}",
        window.read(|home, _| home.content_width),
        window.read(|home, _| home.conversation_list.viewport_bounds())
    );
    window.update(|_, window, cx| {
        window.resize(size(px(700.0), px(850.0)));
        window.bounds_changed(cx);
    });
    for _ in 0..3 {
        window.draw();
        app.run_until_parked();
    }
    assert!((f32::from(height(&window) - wide_height)).abs() < 1.0);
}

#[test]
fn response_action_icons_use_the_css_resolved_size() {
    assert_eq!(RESPONSE_ACTION_ICON_SIZE, 16.0);
}

#[test]
fn command_card_matches_the_live_cdp_geometry() {
    assert_eq!(COMMAND_ACTIVITY_ICON_SIZE, 16.0);
    assert_eq!(COMMAND_ACTIVITY_CONTENT_GAP, 6.0);
    assert_eq!(COMMAND_ACTIVITY_CHEVRON_SIZE, 14.0);
    assert_eq!(COMMAND_CARD_RADIUS, 12.5);
    assert_eq!(COMMAND_CARD_HEADER_SIZE, 13.0);
    assert_eq!(COMMAND_CARD_HEADER_LINE_HEIGHT, 18.5714);
    assert_eq!(COMMAND_CARD_TEXT_SIZE, 13.0);
    assert_eq!(COMMAND_CARD_LINE_HEIGHT, 19.5);
    assert_eq!(COMMAND_CARD_COMMAND_MAX_HEIGHT, 39.0);
    assert_eq!(COMMAND_CARD_OUTPUT_MAX_HEIGHT, 144.0);
    assert_eq!(COMMAND_CARD_STATUS_HEIGHT, 28.0);
}

#[test]
fn command_card_omits_one_terminal_line_ending() {
    assert_eq!(strip_terminal_line_ending("one\n"), "one");
    assert_eq!(strip_terminal_line_ending("one\r\n"), "one");
    assert_eq!(strip_terminal_line_ending("one\n\n"), "one\n");
    assert_eq!(strip_terminal_line_ending("one"), "one");
}

#[test]
fn assistant_footer_matches_the_live_cdp_geometry() {
    assert_eq!(RESPONSE_ACTION_FOOTER_OFFSET, 3.0);
    assert_eq!(RESPONSE_ACTION_FOOTER_HEIGHT, 26.0);
    assert_eq!(RESPONSE_ACTION_FOOTER_ELECTRON_SHIFT, -4.0);
    assert_eq!(RESPONSE_ACTION_GAP, 2.0);
    assert_eq!(RESPONSE_TIME_MARGIN, 6.0);
    assert_eq!(RESPONSE_ACTION_GAP + RESPONSE_TIME_MARGIN, 8.0);
    assert_eq!(RESPONSE_TIME_SIZE, 12.0);
    assert_eq!(RESPONSE_TIME_LINE_HEIGHT, 16.0);
}

#[test]
fn user_message_footer_matches_the_live_cdp_geometry() {
    assert_eq!(USER_MESSAGE_FOOTER_OFFSET, 4.0);
    assert_eq!(USER_MESSAGE_FOOTER_HEIGHT, 26.0);
    assert_eq!(USER_MESSAGE_FOOTER_SIDE_MARGIN, 4.0);
    assert_eq!(USER_MESSAGE_FOOTER_GAP, 8.0);
    assert_eq!(USER_MESSAGE_TIME_SIZE, 12.0);
    assert_eq!(USER_MESSAGE_TIME_LINE_HEIGHT, 16.0);
}

#[test]
fn user_message_copy_button_copies_the_submitted_prompt() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| HomeView::new(ThemeMode::Dark, cx),
    );

    window.update(|home, _, cx| home.submit_prompt_for_capture("clipboard", cx));
    window.draw();

    // The footer action is 26×26 in this fixed viewport. Hover first to
    // exercise the same pointer affordance a user sees before clicking.
    let copy_button_center = point(px(801.0), px(138.0));
    window.simulate_mouse_move(copy_button_center);
    window.simulate_click(copy_button_center, MouseButton::Left);

    assert_eq!(
        app.read_from_clipboard().and_then(|item| item.text()),
        Some("clipboard".to_owned())
    );
}

#[test]
fn approval_surface_owns_focus_and_drives_real_keyboard_events() {
    use crate::{
        components::approval::{ApprovalKeyboardFocus, ApprovalMenuItem, ApprovalVisualState},
        conversation::ConversationActivity,
    };

    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| HomeView::new(ThemeMode::Dark, cx),
    );

    window.update(|home, _, cx| home.set_approval_for_capture("command", "default", cx));
    window.draw();
    window.update(|home, window, cx| {
        assert!(home.approval_focus.is_focused(window));
        assert!(home.composer.read(cx).conversation_render_snapshot().5.iter().any(
            |activity| matches!(activity, ConversationActivity::Approval(model) if model.should_render())
        ));
    });

    window.simulate_keystrokes("shift-tab enter tab");
    window.read(|home, cx| {
        let activities = home.composer.read(cx).conversation_render_snapshot().5;
        let model = activities
            .iter()
            .find_map(|activity| {
                let ConversationActivity::Approval(model) = activity else {
                    return None;
                };
                Some(model)
            })
            .unwrap();
        assert_eq!(
            model.keyboard_focus,
            Some(ApprovalKeyboardFocus::MenuAllowOnce)
        );
        assert_eq!(
            model.visual_state,
            ApprovalVisualState::SplitMenu {
                focused: Some(ApprovalMenuItem::AllowOnce)
            }
        );
    });

    window.simulate_keystrokes("shift-tab escape");
    window.read(|home, cx| {
        let activities = home.composer.read(cx).conversation_render_snapshot().5;
        let model = activities
            .iter()
            .find_map(|activity| {
                let ConversationActivity::Approval(model) = activity else {
                    return None;
                };
                Some(model)
            })
            .unwrap();
        assert_eq!(
            model.keyboard_focus,
            Some(ApprovalKeyboardFocus::MenuToggle)
        );
        assert_eq!(model.visual_state, ApprovalVisualState::Default);
        assert!(model.should_render());
    });

    window.simulate_keystrokes("enter tab enter");
    window.read(|home, cx| {
        assert!(home.composer.read(cx).conversation_render_snapshot().5.iter().any(
            |activity| matches!(activity, ConversationActivity::Approval(model) if !model.should_render())
        ));
    });
    window.draw();
    window.update(|home, window, _| {
        assert!(!home.approval_focus.is_focused(window));
    });
}

#[test]
fn command_approval_replaces_the_bottom_composer_and_rejects_by_mouse() {
    use crate::conversation::ConversationActivity;

    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| HomeView::new(ThemeMode::Dark, cx),
    );

    window.update(|home, _, cx| home.set_approval_for_capture("command", "default", cx));
    window.draw();

    // The 736 px card is centered and pinned 16 px above the bottom.
    // Its reject button occupies the actions row around y=654 here. This
    // point was part of the Composer before the approval overlay moved.
    window.simulate_click(point(px(643.0), px(654.0)), MouseButton::Left);

    window.read(|home, cx| {
        assert!(home.composer.read(cx).conversation_render_snapshot().5.iter().any(
            |activity| matches!(activity, ConversationActivity::Approval(model) if !model.should_render())
        ));
    });
}

#[test]
fn file_approval_surface_drives_focus_enter_and_escape() {
    use crate::{
        components::file_change::{
            FileApprovalKeyboardFocus, FileApprovalMenuItem, FileApprovalVisualState,
        },
        conversation::ConversationActivity,
    };

    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| HomeView::new(ThemeMode::Dark, cx),
    );

    window.update(|home, _, cx| home.set_file_approval_for_capture("default", cx));
    window.draw();
    window.update(|home, window, _| assert!(home.approval_focus.is_focused(window)));

    // Shift-Tab focuses the split toggle, Enter opens it, and Tab moves
    // focus into the first evidence-backed menu row.
    window.simulate_keystrokes("shift-tab enter tab");
    window.read(|home, cx| {
        let activities = home.composer.read(cx).conversation_render_snapshot().5;
        let model = activities
            .iter()
            .find_map(|activity| {
                let ConversationActivity::FileApproval(model) = activity else {
                    return None;
                };
                Some(model)
            })
            .unwrap();
        assert_eq!(
            model.keyboard_focus,
            Some(FileApprovalKeyboardFocus::MenuAllowOnce)
        );
        assert_eq!(
            model.visual_state,
            FileApprovalVisualState::SplitMenu {
                focused: Some(FileApprovalMenuItem::AllowOnce)
            }
        );
    });

    window.simulate_keystroke("escape");
    window.read(|home, cx| {
        let activities = home.composer.read(cx).conversation_render_snapshot().5;
        let model = activities
            .iter()
            .find_map(|activity| {
                let ConversationActivity::FileApproval(model) = activity else {
                    return None;
                };
                Some(model)
            })
            .unwrap();
        assert_eq!(
            model.keyboard_focus,
            Some(FileApprovalKeyboardFocus::MenuToggle)
        );
        assert_eq!(model.visual_state, FileApprovalVisualState::Default);
        assert!(model.should_render());
    });

    window.simulate_keystrokes("enter tab enter");
    window.read(|home, cx| {
        assert!(home.composer.read(cx).conversation_render_snapshot().5.iter().any(
            |activity| matches!(activity, ConversationActivity::FileApproval(model) if !model.should_render())
        ));
    });
    window.draw();
    window.update(|home, window, _| assert!(!home.approval_focus.is_focused(window)));

    window.update(|home, _, cx| home.set_file_approval_for_capture("default", cx));
    window.draw();
    window.simulate_keystroke("escape");
    window.read(|home, cx| {
        assert!(home.composer.read(cx).conversation_render_snapshot().5.iter().any(
            |activity| matches!(activity, ConversationActivity::FileApproval(model) if !model.should_render())
        ));
    });
}

#[test]
fn permissions_approval_surface_drives_focus_menu_and_terminal_unmount() {
    use crate::{
        components::permissions_approval::{
            PermissionApprovalKeyboardFocus, PermissionApprovalMenuItem,
            PermissionApprovalVisualState,
        },
        conversation::ConversationActivity,
    };

    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| HomeView::new(ThemeMode::Dark, cx),
    );

    window
        .update(|home, _, cx| home.set_permissions_approval_for_capture("network", "default", cx));
    window.draw();
    window.update(|home, window, _| assert!(home.approval_focus.is_focused(window)));

    // The pending permission card participates in the same native focus
    // loop as command and file approvals: Shift-Tab reaches the split
    // toggle, Enter opens it, and Tab advances into the first menu row.
    window.simulate_keystrokes("shift-tab enter tab");
    window.read(|home, cx| {
        let activities = home.composer.read(cx).conversation_render_snapshot().5;
        let model = activities
            .iter()
            .find_map(|activity| {
                let ConversationActivity::PermissionsApproval(model) = activity else {
                    return None;
                };
                Some(model)
            })
            .expect("pending permissions approval");
        assert_eq!(
            model.keyboard_focus,
            Some(PermissionApprovalKeyboardFocus::MenuAllowOnce)
        );
        assert_eq!(
            model.visual_state,
            PermissionApprovalVisualState::Menu {
                focused: Some(PermissionApprovalMenuItem::AllowOnce)
            }
        );
    });

    window.simulate_keystroke("escape");
    window.read(|home, cx| {
        let activities = home.composer.read(cx).conversation_render_snapshot().5;
        let model = activities
            .iter()
            .find_map(|activity| {
                let ConversationActivity::PermissionsApproval(model) = activity else {
                    return None;
                };
                Some(model)
            })
            .expect("pending permissions approval");
        assert_eq!(
            model.keyboard_focus,
            Some(PermissionApprovalKeyboardFocus::MenuToggle)
        );
        assert_eq!(model.visual_state, PermissionApprovalVisualState::Default);
        assert!(model.should_render());
    });

    window.simulate_keystrokes("enter tab enter");
    window.read(|home, cx| {
        assert!(home.composer.read(cx).conversation_render_snapshot().5.iter().any(
            |activity| matches!(activity, ConversationActivity::PermissionsApproval(model) if !model.should_render())
        ));
    });
    window.draw();
    window.update(|home, window, _| assert!(!home.approval_focus.is_focused(window)));

    window
        .update(|home, _, cx| home.set_permissions_approval_for_capture("network", "default", cx));
    window.draw();
    window.simulate_keystroke("escape");
    window.read(|home, cx| {
        assert!(home.composer.read(cx).conversation_render_snapshot().5.iter().any(
            |activity| matches!(activity, ConversationActivity::PermissionsApproval(model) if !model.should_render())
        ));
    });
}

#[test]
fn file_change_disclosure_event_toggles_the_inline_diff() {
    use crate::components::file_change::FileChangeActivityEvent;

    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| HomeView::new(ThemeMode::Dark, cx),
    );

    window.update(|home, _, cx| home.set_file_change_for_capture("completed", cx));
    let item_id = window.read(|home, cx| {
        home.composer
            .read(cx)
            .conversation_render_snapshot()
            .5
            .iter()
            .find_map(|activity| {
                let ConversationActivity::FileChange(model) = activity else {
                    return None;
                };
                Some(model.item_id.clone())
            })
            .expect("completed fileChange activity")
    });

    window.update(|home, _, cx| {
        home.handle_file_change_activity_event(
            FileChangeActivityEvent::ToggleDetails {
                item_id: item_id.clone(),
            },
            cx,
        )
    });
    assert!(window.read(|home, _| home.expanded_commands.contains(&item_id)));
    window.update(|home, _, cx| {
        home.handle_file_change_activity_event(
            FileChangeActivityEvent::ToggleDetails {
                item_id: item_id.clone(),
            },
            cx,
        )
    });
    assert!(!window.read(|home, _| home.expanded_commands.contains(&item_id)));
}

#[test]
fn user_input_other_is_a_native_editor_and_tab_returns_to_form_navigation() {
    let mut app = TestApp::new();
    app.update(|cx| {
        cx.bind_keys([KeyBinding::new("enter", Submit, Some("PromptInput"))]);
    });
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| HomeView::new(ThemeMode::Dark, cx),
    );

    window.update(|home, window, cx| {
        home.set_user_input_for_capture("other-focus", cx);
        let focus = home.composer.read(cx).user_input_other_focus_handle(cx);
        window.focus(&focus, cx);
    });
    window.draw();
    window.simulate_input("我想喝茶。");
    window.read(|home, cx| {
        let composer = home.composer.read(cx);
        assert_eq!(composer.user_input_other_entity().read(cx).text(), "我想喝茶。");
        assert!(composer.conversation_render_snapshot().5.iter().any(|activity| {
            matches!(activity, ConversationActivity::UserInput(model) if model.other_answer == "我想喝茶。")
        }));
    });

    // Enter is handled by the PromptInput EntityInputHandler/action, not
    // by the card's top-level KeyDown character concatenation.
    window.simulate_keystroke("enter");
    window.read(|home, cx| {
        assert!(home.composer.read(cx).conversation_render_snapshot().5.iter().any(
            |activity| matches!(activity, ConversationActivity::UserInput(model) if model.status == UserInputRequestStatus::Submitting)
        ));
    });

    window.update(|home, window, cx| {
        home.set_user_input_for_capture("other-focus", cx);
        let focus = home.composer.read(cx).user_input_other_focus_handle(cx);
        window.focus(&focus, cx);
    });
    window.draw();
    window.simulate_keystroke("tab");
    window.update(|home, window, cx| {
        assert!(home.approval_focus.is_focused(window));
        assert!(home.composer.read(cx).conversation_render_snapshot().5.iter().any(
            |activity| matches!(activity, ConversationActivity::UserInput(model) if model.keyboard_focus == Some(UserInputKeyboardFocus::Skip))
        ));
    });
}
