//! Capture behavior and presentation for the prompt composer.

#[cfg(test)]
use crate::agent::AgentImageView;

use std::path::PathBuf;

use gpui::Context;

use super::{ComposerView, ConversationChanged, DictationState, PermissionMode};
use crate::{
    agent::{
        AgentCollaboration, AgentCollaborationStatus, AgentCollaborationTool,
        AgentCollaboratorState, AgentCollaboratorStatus, AgentContextCompaction,
        AgentImageGeneration, AgentImageGenerationFailure, AgentImageGenerationStatus,
        AgentMcpToolCall, AgentMcpToolCallStatus, CommandExecution, CommandExecutionAction,
        CommandExecutionStatus, LegacySubAgentActivityKind,
    },
    components::{
        approval::{
            ApprovalCardStatus, ApprovalCardViewModel, ApprovalMenuItem,
            ApprovalRequestPresentation, ApprovalVisualState,
        },
        file_change::{
            FileApprovalStatus, captured_file_approval_fixture,
            captured_file_change_activity_fixture,
        },
        permissions_approval::{
            PermissionApprovalKeyboardFocus, PermissionApprovalMenuItem,
            PermissionApprovalPresentation, PermissionApprovalStatus,
            PermissionApprovalVisualState, PermissionPathAccess, PermissionPathRequest,
        },
        user_input_request::{
            UserInputKeyboardFocus, UserInputOptionPresentation, UserInputQuestionPresentation,
            UserInputRequestPresentation, UserInputRequestStatus, UserInputVisualState,
            captured_multi_question_fixture,
        },
    },
    conversation::{ConversationActivity, ConversationPhase, ReasoningActivityPresentation},
    theme::ThemeMode,
};

impl ComposerView {
    #[cfg(feature = "screenshot")]
    pub fn model_catalog_ready_for_capture(&self) -> Result<bool, String> {
        if cfg!(test) {
            return Ok(true);
        }
        if let Some(error) = &self.conversation.model_catalog_error {
            return Err(format!("模型目录加载失败：{error}"));
        }
        Ok(!self.conversation.models.is_empty())
    }
    pub fn enable_permission_ui_for_capture(&mut self, cx: &mut Context<Self>) {
        self.permission_ui_enabled = true;
        cx.notify();
    }
    pub fn set_permission_mode_for_capture(&mut self, mode: &str, cx: &mut Context<Self>) {
        self.permission_ui_enabled = true;
        self.permission_mode = match mode {
            "request" => PermissionMode::Request,
            "assist" => PermissionMode::Assist,
            "custom" => PermissionMode::Custom,
            _ => PermissionMode::Full,
        };
        self.permission_menu_open = false;
        self.permission_menu_keyboard_focus = false;
        cx.notify();
    }
    pub fn open_permission_menu_for_capture(&mut self, cx: &mut Context<Self>) {
        self.permission_ui_enabled = true;
        self.menu_open = false;
        self.submenu = None;
        self.permission_menu_keyboard_focus = false;
        self.permission_menu_open = true;
        cx.notify();
    }
    pub fn set_permission_menu_capture_state(&mut self, state: &str, cx: &mut Context<Self>) {
        self.open_permission_menu_for_capture(cx);
        if matches!(state, "request-hover" | "request-focus") {
            self.permission_menu_focused_item = 0;
            self.permission_menu_keyboard_focus = true;
            cx.notify();
        }
    }
    pub fn set_dictation_state_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.dictation_state = match state {
            "recording" => DictationState::Recording,
            "transcribing" => DictationState::Transcribing,
            _ => DictationState::Idle,
        };
        cx.notify();
    }
    pub fn submit_prompt_for_capture(&mut self, prompt: &str, cx: &mut Context<Self>) {
        self.submit_prompt(prompt.to_owned(), cx);
    }
    pub fn set_command_tool_for_capture(&mut self, running: bool, cx: &mut Context<Self>) {
        let item_id = "exec-command-ui-capture".to_owned();
        let command = "printf 'SHELLPIXEL20260830\\n'".to_owned();
        self.conversation.user_message = Some(
            "请使用终端执行 printf 'SHELLPIXEL20260830\\n'，等待命令执行完成后告诉我输出。"
                .to_owned(),
        );
        self.conversation.user_message_time = Some("21:45".to_owned());
        self.conversation.assistant_message = if running {
            "我现在执行这条命令，完成后原样告诉你输出。".to_owned()
        } else {
            "我现在执行这条命令，完成后原样告诉你输出。输出为：SHELLPIXEL20260830".to_owned()
        };
        self.conversation.assistant_message_time = (!running).then(|| "21:45".to_owned());
        self.conversation.phase = if running {
            ConversationPhase::Streaming
        } else {
            ConversationPhase::Complete
        };
        self.conversation.activities = vec![
            ConversationActivity::AssistantMessage {
                item_id: "msg-command-preamble".to_owned(),
                text: "我现在执行这条命令，完成后原样告诉你输出。".to_owned(),
            },
            ConversationActivity::Command(CommandExecution {
                id: item_id,
                command,
                actions: Vec::new(),
                cwd: "/path/to/project".to_owned(),
                output: "SHELLPIXEL20260830\n".to_owned(),
                terminal_process_id: None,
                status: if running {
                    CommandExecutionStatus::InProgress
                } else {
                    CommandExecutionStatus::Completed
                },
                exit_code: (!running).then_some(0),
            }),
        ];
        if !running {
            self.conversation
                .activities
                .push(ConversationActivity::AssistantMessage {
                    item_id: "msg-command-final".to_owned(),
                    text: "输出为：\n\nSHELLPIXEL20260830".to_owned(),
                });
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }
    pub fn set_context_compaction_for_capture(&mut self, running: bool, cx: &mut Context<Self>) {
        self.conversation.user_message = Some("请压缩当前聊天的上下文。".to_owned());
        self.conversation.user_message_time = Some("19:19".to_owned());
        self.conversation.assistant_message.clear();
        self.conversation.assistant_message_time = None;
        self.conversation.phase = if running {
            ConversationPhase::Streaming
        } else {
            ConversationPhase::Complete
        };
        self.conversation.activities = vec![ConversationActivity::ContextCompaction(
            AgentContextCompaction {
                id: "context-compaction-ui-capture".to_owned(),
                completed: !running,
            },
        )];
        cx.emit(ConversationChanged);
        cx.notify();
    }
    pub fn set_collaboration_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        let (status, collaborator_status, legacy_kind) = match state {
            "completed" | "success" => (
                AgentCollaborationStatus::Completed,
                AgentCollaboratorStatus::Completed,
                LegacySubAgentActivityKind::Completed,
            ),
            "failed" => (
                AgentCollaborationStatus::Failed,
                AgentCollaboratorStatus::Errored,
                LegacySubAgentActivityKind::Completed,
            ),
            "interrupted" => (
                AgentCollaborationStatus::Interrupted,
                AgentCollaboratorStatus::Interrupted,
                LegacySubAgentActivityKind::Interrupted,
            ),
            _ => (
                AgentCollaborationStatus::InProgress,
                AgentCollaboratorStatus::Running,
                LegacySubAgentActivityKind::Started,
            ),
        };
        let thread_id = "01a06b7a-14c2-73b3-9c62-b29e27bd8689".to_owned();
        self.conversation.user_message = None;
        self.conversation.user_message_time = None;
        self.conversation.assistant_message.clear();
        self.conversation.assistant_message_time = None;
        self.conversation.phase = if status == AgentCollaborationStatus::InProgress {
            ConversationPhase::Streaming
        } else {
            ConversationPhase::Complete
        };
        self.conversation.activities =
            vec![ConversationActivity::Collaboration(AgentCollaboration {
                id: "collaboration-ui-capture".to_owned(),
                tool: if status == AgentCollaborationStatus::Failed {
                    AgentCollaborationTool::SpawnAgent
                } else {
                    AgentCollaborationTool::LegacyActivity
                },
                status,
                sender_thread_id: if status == AgentCollaborationStatus::Failed {
                    "parent-thread".to_owned()
                } else {
                    Default::default()
                },
                receiver_thread_ids: vec![thread_id.clone()],
                agents_states: std::collections::BTreeMap::from([(
                    thread_id,
                    AgentCollaboratorState {
                        status: collaborator_status,
                        message: (status == AgentCollaborationStatus::Failed)
                            .then(|| "Agent failed while collecting evidence.".to_owned()),
                        name: None,
                    },
                )]),
                prompt: (status == AgentCollaborationStatus::Failed)
                    .then(|| "Collab evidence probe".to_owned()),
                model: (status == AgentCollaborationStatus::Failed).then(|| "gpt-5.4".to_owned()),
                reasoning_effort: (status == AgentCollaborationStatus::Failed)
                    .then(|| "high".to_owned()),
                legacy_agent_path: (status != AgentCollaborationStatus::Failed)
                    .then(|| "/root/collab_evidence_probe".to_owned()),
                legacy_kind: (status != AgentCollaborationStatus::Failed).then_some(legacy_kind),
            })];
        cx.emit(ConversationChanged);
        cx.notify();
    }
    pub fn set_mcp_tool_call_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        let status = match state {
            "running" => AgentMcpToolCallStatus::InProgress,
            "failed" => AgentMcpToolCallStatus::Failed,
            _ => AgentMcpToolCallStatus::Completed,
        };
        self.conversation.user_message = Some(
            "必须调用 Codex App 的 get_usage_limits MCP 工具读取当前账户用量；不要根据记忆回答。工具完成后只用一句中文报告剩余额度。"
                .to_owned(),
        );
        self.conversation.user_message_time = Some("16:15".to_owned());
        self.conversation.assistant_message = if status == AgentMcpToolCallStatus::Completed {
            "我现在直接读取 Codex App 中当前账户的实时用量。\n\n当前 Codex 通用额度剩余 29%。"
                .to_owned()
        } else {
            "我现在直接读取 Codex App 中当前账户的实时用量。".to_owned()
        };
        self.conversation.assistant_message_time =
            (status != AgentMcpToolCallStatus::InProgress).then(|| "16:15".to_owned());
        self.conversation.phase = if status == AgentMcpToolCallStatus::InProgress {
            ConversationPhase::Streaming
        } else {
            ConversationPhase::Complete
        };
        self.conversation.activities = vec![
            ConversationActivity::AssistantMessage {
                item_id: "msg-mcp-preamble".to_owned(),
                text: "我现在直接读取 Codex App 中当前账户的实时用量。".to_owned(),
            },
            ConversationActivity::McpToolCall(Box::from(AgentMcpToolCall {
                id: "exec-mcp-ui-capture".to_owned(),
                server: "codex_app".to_owned(),
                tool: "get_usage_limits".to_owned(),
                status,
                arguments: serde_json::json!({}),
                app_context: None,
                plugin_id: None,
                result: (status == AgentMcpToolCallStatus::Completed).then(|| {
                    serde_json::json!({
                        "content": [{"type": "text", "text": "remaining: 29"}],
                        "structuredContent": null,
                        "_meta": null
                    })
                }),
                error: (status == AgentMcpToolCallStatus::Failed)
                    .then(|| "Declined by user".to_owned()),
                legacy_resource_uri: None,
                read_only_hint: None,
                duration_ms: (status != AgentMcpToolCallStatus::InProgress).then_some(1535),
                progress: if status == AgentMcpToolCallStatus::InProgress {
                    vec!["Reading limits".to_owned()]
                } else {
                    Default::default()
                },
            })),
        ];
        if status == AgentMcpToolCallStatus::Completed {
            self.conversation
                .activities
                .push(ConversationActivity::AssistantMessage {
                    item_id: "msg-mcp-final".to_owned(),
                    text: "当前 Codex 通用额度剩余 29%。".to_owned(),
                });
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }
    #[cfg(test)]
    pub fn set_image_view_for_capture(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.conversation.user_message = Some("请查看这张图像。".to_owned());
        self.conversation.user_message_time = Some("21:45".to_owned());
        self.conversation.assistant_message.clear();
        self.conversation.assistant_message_time = Some("21:45".to_owned());
        self.conversation.phase = ConversationPhase::Complete;
        self.conversation.activities = vec![
            ConversationActivity::AssistantMessage {
                item_id: "msg-image-preamble".to_owned(),
                text: "我来查看这张图像。".to_owned(),
            },
            ConversationActivity::ImageView(AgentImageView {
                id: "image-view-ui-capture".to_owned(),
                path,
            }),
        ];
        cx.emit(ConversationChanged);
        cx.notify();
    }
    pub fn set_image_generation_for_capture(
        &mut self,
        state: &str,
        path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.conversation.user_message = Some(
            "请务必调用图像生成工具生成一张 1024×1024 的正方形图片：纯白背景中央是一架红色纸飞机，极简扁平插画，无文字。只生成一张图。"
                .to_owned(),
        );
        self.conversation.user_message_time = None;
        self.conversation.assistant_message_time = None;
        self.conversation.phase = if state == "running" {
            ConversationPhase::Streaming
        } else {
            ConversationPhase::Complete
        };
        let status = match state {
            "running" => AgentImageGenerationStatus::InProgress,
            "failed" => AgentImageGenerationStatus::Failed,
            _ => AgentImageGenerationStatus::Completed,
        };
        let failure =
            (state == "failed").then(|| AgentImageGenerationFailure::UsageLimitExceeded {
                limit_id: "image_generation".to_owned(),
                resets_at: Some(1_788_566_400),
            });
        let load_error =
            (state == "load-error").then(|| "生成的图像文件不存在，请重试。".to_owned());
        let mut activities = Vec::new();
        if status == AgentImageGenerationStatus::Completed && load_error.is_none() {
            activities.push(ConversationActivity::AssistantMessage {
                item_id: "msg-image-generation-final".to_owned(),
                text: "已生成并校准为 **1024×1024 PNG**，仅一张图。".to_owned(),
            });
        }
        activities.push(ConversationActivity::ImageGeneration(
            AgentImageGeneration {
                id: "image-generation-ui-capture".to_owned(),
                status,
                revised_prompt: Some(
                    "纯白背景中央的一架红色纸飞机，极简扁平插画，无文字。".to_owned(),
                ),
                path,
                dimensions: (status == AgentImageGenerationStatus::Completed)
                    .then_some((1024, 1024)),
                transparent_background: Some(false),
                failure,
                load_error,
            },
        ));
        self.conversation.assistant_message = if status == AgentImageGenerationStatus::Completed {
            "已生成并校准为 1024×1024 PNG，仅一张图。".to_owned()
        } else {
            String::new()
        };
        self.conversation.activities = activities;
        cx.emit(ConversationChanged);
        cx.notify();
    }
    pub fn set_tool_group_for_capture(&mut self, running: bool, cx: &mut Context<Self>) {
        let completed_status = CommandExecutionStatus::Completed;
        let final_status = if running {
            CommandExecutionStatus::InProgress
        } else {
            completed_status
        };
        let completed_exit = Some(0);
        let final_exit = (!running).then_some(0);
        let commands = vec![
            CommandExecution {
                id: "tool-group-read-1".to_owned(),
                command: "sed -n '1,240p' src/agent/codex/manager.rs".to_owned(),
                actions: vec![CommandExecutionAction::Read {
                    command: "sed -n '1,240p' src/agent/codex/manager.rs".to_owned(),
                    name: "manager.rs".to_owned(),
                    path: "src/agent/codex/manager.rs".to_owned(),
                }],
                cwd: "/Users/zp/Desktop/GPUI".to_owned(),
                output: "use std::sync::Arc;\n".to_owned(),
                terminal_process_id: None,
                status: completed_status,
                exit_code: completed_exit,
            },
            CommandExecution {
                id: "tool-group-read-2".to_owned(),
                command: "sed -n '240,520p' src/agent/codex/manager.rs".to_owned(),
                actions: vec![CommandExecutionAction::Read {
                    command: "sed -n '240,520p' src/agent/codex/manager.rs".to_owned(),
                    name: "manager.rs".to_owned(),
                    path: "src/agent/codex/manager.rs".to_owned(),
                }],
                cwd: "/Users/zp/Desktop/GPUI".to_owned(),
                output: "impl CodexAppServerManager {\n".to_owned(),
                terminal_process_id: None,
                status: completed_status,
                exit_code: completed_exit,
            },
            CommandExecution {
                id: "tool-group-read-3".to_owned(),
                command: "sed -n '520,780p' src/agent/codex/manager.rs".to_owned(),
                actions: vec![CommandExecutionAction::Read {
                    command: "sed -n '520,780p' src/agent/codex/manager.rs".to_owned(),
                    name: "manager.rs".to_owned(),
                    path: "src/agent/codex/manager.rs".to_owned(),
                }],
                cwd: "/Users/zp/Desktop/GPUI".to_owned(),
                output: "}\n".to_owned(),
                terminal_process_id: None,
                status: completed_status,
                exit_code: completed_exit,
            },
            CommandExecution {
                id: "tool-group-read-4".to_owned(),
                command: "sed -n '780,1040p' src/agent/codex/manager.rs".to_owned(),
                actions: vec![CommandExecutionAction::Read {
                    command: "sed -n '780,1040p' src/agent/codex/manager.rs".to_owned(),
                    name: "manager.rs".to_owned(),
                    path: "src/agent/codex/manager.rs".to_owned(),
                }],
                cwd: "/Users/zp/Desktop/GPUI".to_owned(),
                output: "impl Drop for AppServerProcess {\n".to_owned(),
                terminal_process_id: None,
                status: completed_status,
                exit_code: completed_exit,
            },
            CommandExecution {
                id: "tool-group-read-5".to_owned(),
                command: "sed -n '1040,1260p' src/agent/codex/manager.rs".to_owned(),
                actions: vec![CommandExecutionAction::Read {
                    command: "sed -n '1040,1260p' src/agent/codex/manager.rs".to_owned(),
                    name: "manager.rs".to_owned(),
                    path: "src/agent/codex/manager.rs".to_owned(),
                }],
                cwd: "/Users/zp/Desktop/GPUI".to_owned(),
                output: "}\n".to_owned(),
                terminal_process_id: None,
                status: completed_status,
                exit_code: completed_exit,
            },
            CommandExecution {
                id: "tool-group-search-1".to_owned(),
                command: "rg -n 'Command::new' src".to_owned(),
                actions: vec![CommandExecutionAction::Search {
                    command: "rg -n 'Command::new' src".to_owned(),
                    path: Some("src".to_owned()),
                    query: Some("Command::new".to_owned()),
                }],
                cwd: "/Users/zp/Desktop/GPUI".to_owned(),
                output: "src/agent/codex.rs:42\n".to_owned(),
                terminal_process_id: None,
                status: completed_status,
                exit_code: completed_exit,
            },
            CommandExecution {
                id: "tool-group-search-2".to_owned(),
                command: "rg -n 'thread/(list|read)' src".to_owned(),
                actions: vec![CommandExecutionAction::Search {
                    command: "rg -n 'thread/(list|read)' src".to_owned(),
                    path: Some("src".to_owned()),
                    query: Some("thread/(list|read)".to_owned()),
                }],
                cwd: "/Users/zp/Desktop/GPUI".to_owned(),
                output: "src/agent/codex.rs:84\n".to_owned(),
                terminal_process_id: None,
                status: completed_status,
                exit_code: completed_exit,
            },
            CommandExecution {
                id: "tool-group-search-3".to_owned(),
                command: "rg -n 'spawn|current_dir|home' src".to_owned(),
                actions: vec![CommandExecutionAction::Search {
                    command: "rg -n 'spawn|current_dir|home' src".to_owned(),
                    path: Some("src".to_owned()),
                    query: Some("spawn|current_dir|home".to_owned()),
                }],
                cwd: "/Users/zp/Desktop/GPUI".to_owned(),
                output: "src/agent/codex/manager.rs:118\n".to_owned(),
                terminal_process_id: None,
                status: completed_status,
                exit_code: completed_exit,
            },
            CommandExecution {
                id: "tool-group-run-1".to_owned(),
                command: "find . -maxdepth 2 -type d | sort".to_owned(),
                actions: vec![CommandExecutionAction::Unknown {
                    command: "find . -maxdepth 2 -type d | sort".to_owned(),
                }],
                cwd: "/Users/zp/Desktop/GPUI".to_owned(),
                output: ".\n./src\n./tests\n".to_owned(),
                terminal_process_id: None,
                status: completed_status,
                exit_code: completed_exit,
            },
            CommandExecution {
                id: "tool-group-run-2".to_owned(),
                command: "cargo test --quiet".to_owned(),
                actions: vec![CommandExecutionAction::Unknown {
                    command: "cargo test --quiet".to_owned(),
                }],
                cwd: "/Users/zp/Desktop/GPUI".to_owned(),
                output: "running 192 tests\n".to_owned(),
                terminal_process_id: None,
                status: final_status,
                exit_code: final_exit,
            },
        ];

        self.conversation.user_message = Some("深入分析当前项目".to_owned());
        self.conversation.user_message_time = Some("20:27".to_owned());
        self.conversation.phase = if running {
            ConversationPhase::Streaming
        } else {
            ConversationPhase::Complete
        };
        self.conversation.assistant_message = if !running {
            "分析完成，关键链路已经核对。".to_owned()
        } else {
            Default::default()
        };
        self.conversation.assistant_message_time = (!running).then(|| "20:28".to_owned());
        self.conversation.activities = vec![
            ConversationActivity::AssistantMessage {
                item_id: "tool-group-preamble".to_owned(),
                text: "我会从仓库结构、核心运行链路和协议适配层逐项核对。".to_owned(),
            },
            ConversationActivity::Reasoning(ReasoningActivityPresentation {
                item_id: "tool-group-ui-capture".to_owned(),
                summary: vec!["Identifying concurrency and resource risks".to_owned()],
                content: Vec::new(),
                started_at_ms: 1_000,
                completed_at_ms: (!running).then_some(3_000),
            }),
        ];
        self.conversation
            .activities
            .extend(commands.into_iter().map(ConversationActivity::Command));
        if !running {
            self.conversation
                .activities
                .push(ConversationActivity::AssistantMessage {
                    item_id: "tool-group-final".to_owned(),
                    text: "分析完成，关键链路已经核对。".to_owned(),
                });
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }
    pub fn set_reasoning_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        let active = state.starts_with("active");
        let with_content = state.ends_with("content");
        self.conversation.user_message = Some("请分析当前实现并给出结论。".to_owned());
        self.conversation.user_message_time = Some("18:27".to_owned());
        self.conversation.phase = if active {
            ConversationPhase::Thinking
        } else {
            ConversationPhase::Complete
        };
        self.conversation.assistant_message = if !active {
            "实现已经核对完成。".to_owned()
        } else {
            Default::default()
        };
        self.conversation.assistant_message_time = (!active).then(|| "18:28".to_owned());
        let summary = if with_content {
            vec![
                "检查实现".to_owned(),
                "正在比对桌面 ChatGPT 的推理组件与协议事件。".to_owned(),
            ]
        } else {
            Vec::new()
        };
        self.conversation.activities = vec![ConversationActivity::Reasoning(
            ReasoningActivityPresentation {
                item_id: "reasoning-ui-capture".to_owned(),
                summary,
                content: Vec::new(),
                started_at_ms: 1_000,
                completed_at_ms: (!active).then_some(30_000),
            },
        )];
        if !active {
            self.conversation
                .activities
                .push(ConversationActivity::AssistantMessage {
                    item_id: "reasoning-capture-answer".to_owned(),
                    text: self.conversation.assistant_message.clone(),
                });
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }
    pub fn set_approval_for_capture(&mut self, kind: &str, state: &str, cx: &mut Context<Self>) {
        let resolved_capture = state == "resolved";
        self.approval_resolved_capture = resolved_capture;
        let (command, command_reason) = if self.mode == ThemeMode::Light {
            (
                "curl -I https://iana.org",
                "是否允许我仅在终端运行命令 `curl -I https://iana.org`？",
            )
        } else {
            (
                "curl -I https://example.com",
                "是否允许我仅运行命令 `curl -I https://example.com`？",
            )
        };
        let request = match kind {
            "network" => ApprovalRequestPresentation::network(
                "example.com",
                Some(command.to_owned()),
                Some("是否允许 ChatGPT 连接到 example.com？".to_owned()),
            ),
            _ => ApprovalRequestPresentation::command(command, Some(command_reason.to_owned())),
        };
        let mut approval = ApprovalCardViewModel::pending("approval-ui-capture", request);
        approval.visual_state = match state {
            "approve-hover" => ApprovalVisualState::ApproveHovered,
            "decline-hover" => ApprovalVisualState::DeclineHovered,
            "options" => ApprovalVisualState::SplitMenu { focused: None },
            "options-focus" => ApprovalVisualState::SplitMenu {
                focused: Some(ApprovalMenuItem::AllowOnce),
            },
            _ => ApprovalVisualState::Default,
        };
        if matches!(state, "approved" | "declined" | "resolved") {
            approval.status = ApprovalCardStatus::Resolved;
        }

        self.conversation.user_message = Some(
            "请只执行命令 curl -I https://example.com，等待我的批准，不要采取其他行动。".to_owned(),
        );
        self.conversation.user_message_time = Some("16:27".to_owned());
        self.conversation.assistant_message = if resolved_capture {
            "命令未执行：你拒绝了批准。未采取其他行动。".to_owned()
        } else {
            "我将只申请运行该命令，并等待你的批准。".to_owned()
        };
        self.conversation.assistant_message_time = resolved_capture.then(|| "16:28".to_owned());
        self.conversation.phase = if resolved_capture {
            // CDP 12 is the independently captured, stable resolved fixture:
            // the declined turn is complete, the approval card is unmounted,
            // and the composer has returned to its ordinary send state.
            ConversationPhase::Complete
        } else {
            ConversationPhase::Streaming
        };
        if resolved_capture {
            self.permission_mode = PermissionMode::Request;
            self.conversation.selected_model = "5.6 Sol".to_owned();
            self.conversation.actual_model = Some("5.6 Sol".to_owned());
            self.conversation.selected_effort = "ultra".to_owned();
            self.conversation.selected_service_tier = Some("priority".to_owned());
        }
        self.conversation.activities = vec![
            ConversationActivity::AssistantMessage {
                item_id: "msg-approval-preamble".to_owned(),
                text: self.conversation.assistant_message.clone(),
            },
            ConversationActivity::Approval(approval),
        ];
        cx.emit(ConversationChanged);
        cx.notify();
    }
    pub fn set_file_approval_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        let approval = captured_file_approval_fixture(self.mode, state);
        let resolved = approval.status == FileApprovalStatus::Resolved;
        let immediate = state == "declined-immediate";
        let (path, request, pending_message, resolved_message, time) = match self.mode {
            ThemeMode::Light => (
                "/Users/zp/Desktop/codex-cdp-file-approval-probe.txt",
                "请仅使用文件修改工具在 /Users/zp/Desktop/codex-cdp-file-approval-probe.txt 新建文件，内容为 PROBE；必须等待我的批准，不要使用终端命令或其他方式。",
                "我将仅通过文件修改工具申请创建该文件，并等待你的批准。",
                "文件未创建：批准被拒绝。未使用终端命令或其他方式。",
                "16:31",
            ),
            ThemeMode::Dark => (
                "/Users/zp/Desktop/codex-cdp-file-approval-dark-probe.txt",
                "请仅使用文件修改工具在 /Users/zp/Desktop/codex-cdp-file-approval-dark-probe.txt 新建文件，内容为 DARK_PROBE；请直接发起系统审批，不要先向我文字确认，不要使用终端。",
                "正在直接发起文件修改系统审批。",
                "系统审批被拒绝，文件未创建。未使用终端。",
                "16:36",
            ),
        };
        debug_assert_eq!(approval.files[0].path, path);

        self.conversation.user_message = Some(request.to_owned());
        self.conversation.user_message_time = Some(time.to_owned());
        self.conversation.assistant_message = if resolved && !immediate {
            resolved_message.to_owned()
        } else {
            pending_message.to_owned()
        };
        self.conversation.assistant_message_time =
            (resolved && !immediate).then(|| match self.mode {
                ThemeMode::Light => "16:32".to_owned(),
                ThemeMode::Dark => "16:37".to_owned(),
            });
        self.conversation.phase = if resolved && !immediate {
            ConversationPhase::Complete
        } else {
            ConversationPhase::Streaming
        };
        self.conversation.activities = vec![
            ConversationActivity::AssistantMessage {
                item_id: if resolved && !immediate {
                    "msg-file-approval-resolved".to_owned()
                } else {
                    "msg-file-approval-preamble".to_owned()
                },
                text: self.conversation.assistant_message.clone(),
            },
            ConversationActivity::FileApproval(approval),
        ];
        cx.emit(ConversationChanged);
        cx.notify();
    }
    pub fn set_permissions_approval_for_capture(
        &mut self,
        kind: &str,
        state: &str,
        cx: &mut Context<Self>,
    ) {
        let mut approval = match kind {
            "filesystem" => PermissionApprovalPresentation::file_system(
                "permissions-ui-capture",
                vec![PermissionPathRequest::new(
                    "/Users/zp/Downloads",
                    PermissionPathAccess::Read,
                )],
                Some("Inspect downloaded fixtures needed by this task.".to_owned()),
            ),
            "combined" => PermissionApprovalPresentation::combined(
                "permissions-ui-capture",
                vec![
                    PermissionPathRequest::new("/Users/zp/Downloads", PermissionPathAccess::Read),
                    PermissionPathRequest::new(
                        "/Users/zp/Desktop/GPUI",
                        PermissionPathAccess::Write,
                    ),
                ],
                Some("Download a fixture and store the generated result.".to_owned()),
            ),
            _ => PermissionApprovalPresentation::network(
                "permissions-ui-capture",
                Some("Connect to example.com to verify the integration.".to_owned()),
            ),
        };
        approval.visual_state = match state {
            "approve-hover" => PermissionApprovalVisualState::AllowHovered,
            "decline-hover" => PermissionApprovalVisualState::DeclineHovered,
            "options" => PermissionApprovalVisualState::Menu { focused: None },
            "options-focus" => PermissionApprovalVisualState::Menu {
                focused: Some(PermissionApprovalMenuItem::AllowOnce),
            },
            _ => PermissionApprovalVisualState::Default,
        };
        approval.keyboard_focus = match state {
            "approve-focus" => Some(PermissionApprovalKeyboardFocus::AllowOnce),
            "decline-focus" => Some(PermissionApprovalKeyboardFocus::Decline),
            "options-focus" => Some(PermissionApprovalKeyboardFocus::MenuAllowOnce),
            _ => None,
        };
        approval.status = match state {
            "approved" => PermissionApprovalStatus::Approved,
            "declined" => PermissionApprovalStatus::Declined,
            "resolved" => PermissionApprovalStatus::Resolved,
            _ => PermissionApprovalStatus::Pending,
        };

        self.conversation.user_message = None;
        self.conversation.user_message_time = None;
        self.conversation.assistant_message.clear();
        self.conversation.assistant_message_time = None;
        self.conversation.phase = if approval.should_render() {
            ConversationPhase::Streaming
        } else {
            ConversationPhase::Complete
        };
        self.conversation.activities = vec![ConversationActivity::PermissionsApproval(approval)];
        cx.emit(ConversationChanged);
        cx.notify();
    }
    pub fn set_file_change_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        let activity = captured_file_change_activity_fixture(state);
        self.conversation.user_message = Some(
            "请仅使用文件修改工具在 /tmp/chatgpt-cdp-file-approval-approved.txt 新建文件，内容为 APPROVED_PROBE；必须等待我的批准，不要使用终端命令或其他方式。"
                .to_owned(),
        );
        self.conversation.user_message_time = Some("16:32".to_owned());
        self.conversation.assistant_message =
            "已创建 /tmp/chatgpt-cdp-file-approval-approved.txt，内容为 APPROVED_PROBE。未使用终端命令或其他方式。"
                .to_owned();
        self.conversation.assistant_message_time = Some("16:33".to_owned());
        self.conversation.phase = ConversationPhase::Complete;
        self.conversation.activities = vec![
            ConversationActivity::AssistantMessage {
                item_id: "msg-file-change-completed".to_owned(),
                text: self.conversation.assistant_message.clone(),
            },
            ConversationActivity::FileChange(activity),
        ];
        cx.emit(ConversationChanged);
        cx.notify();
    }
    pub fn set_user_input_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        let multi_fixture = state.starts_with("multi-");
        let skip_fixture = state.starts_with("skip-");
        let shape_fixture = state.starts_with("shape-");
        let other_fixture = state.starts_with("other-");
        let keyboard_fixture = state.starts_with("keyboard-");
        let question = if multi_fixture {
            None
        } else if skip_fixture {
            Some(UserInputQuestionPresentation::single_choice(
                "continue",
                "是否继续？",
                vec![
                    UserInputOptionPresentation::recommended(
                        "继续",
                        Some("选择继续后保持当前流程进行。".to_owned()),
                    ),
                    UserInputOptionPresentation::new(
                        "停止",
                        Some("选择停止后结束当前流程。".to_owned()),
                    ),
                ],
            ))
        } else if shape_fixture {
            Some(UserInputQuestionPresentation::single_choice(
                "shape",
                "请选择一种形状。",
                vec![
                    UserInputOptionPresentation::recommended(
                        "圆形",
                        Some("选择圆形作为你的单选答案。".to_owned()),
                    ),
                    UserInputOptionPresentation::new(
                        "方形",
                        Some("选择方形作为你的单选答案。".to_owned()),
                    ),
                ],
            ))
        } else if other_fixture {
            if state == "other-focus" {
                Some(UserInputQuestionPresentation::single_choice(
                    "transport",
                    "请选择一种交通工具。",
                    vec![
                        UserInputOptionPresentation::recommended(
                            "火车",
                            Some("选择乘坐火车。".to_owned()),
                        ),
                        UserInputOptionPresentation::new("飞机", Some("选择乘坐飞机。".to_owned())),
                    ],
                ))
            } else {
                // CDP 97's typed Other path was captured from the independently
                // restarted drink fixture used by 92–98.
                Some(UserInputQuestionPresentation::single_choice(
                    "drink",
                    "请选择一种饮料。",
                    vec![
                        UserInputOptionPresentation::recommended("水", Some("选择水。".to_owned())),
                        UserInputOptionPresentation::new("咖啡", Some("选择咖啡。".to_owned())),
                    ],
                ))
            }
        } else if keyboard_fixture {
            Some(UserInputQuestionPresentation::single_choice(
                "drink",
                "请选择一种饮料。",
                vec![
                    UserInputOptionPresentation::recommended("水", Some("选择水。".to_owned())),
                    UserInputOptionPresentation::new("咖啡", Some("选择咖啡。".to_owned())),
                ],
            ))
        } else {
            Some(UserInputQuestionPresentation::single_choice(
                "color",
                "请选择一种颜色。",
                vec![
                    UserInputOptionPresentation::recommended(
                        "红色",
                        Some("选择红色作为你的单选答案。".to_owned()),
                    ),
                    UserInputOptionPresentation::new(
                        "蓝色",
                        Some("选择蓝色作为你的单选答案。".to_owned()),
                    ),
                ],
            ))
        };
        let mut request = if multi_fixture {
            captured_multi_question_fixture(self.mode, state)
        } else {
            UserInputRequestPresentation::pending(
                "user-input-ui-capture",
                vec![question.expect("single-question capture fixture")],
            )
        };
        if !multi_fixture {
            request.visual_state = match state {
                "option-hover" => UserInputVisualState::option_active(1),
                "option-focus" => UserInputVisualState::option_focused(1, 0),
                "skip-hover" => UserInputVisualState::skip_hovered(),
                "keyboard-arrow-selected" => UserInputVisualState::option_focused(1, 0),
                _ => UserInputVisualState::option_active(0),
            };
            request.keyboard_focus = match state {
                "keyboard-dismiss-focus" => Some(UserInputKeyboardFocus::Dismiss),
                "keyboard-option-focus" | "keyboard-arrow-selected" => {
                    Some(UserInputKeyboardFocus::Option(0))
                }
                "other-focus" | "other-text" => Some(UserInputKeyboardFocus::Other),
                _ => None,
            };
            if state == "keyboard-arrow-selected" {
                request.selected_option_index = Some(1);
            }
            if other_fixture {
                // CDP 90/97 show that choosing Other clears the checked radio and
                // removes the option activity fill/submit arrow.
                request.selected_option_index = None;
                request.visual_state.active_option_index = None;
            }
            if state == "other-text" {
                request.other_answer = "我想喝茶。".to_owned();
            }
            request.status = match state {
                "submitting" | "skip-submitting" | "shape-submitting" => {
                    UserInputRequestStatus::Submitting
                }
                "resolved" | "skip-resolved" | "shape-resolved" => UserInputRequestStatus::Resolved,
                _ => UserInputRequestStatus::Pending,
            };
            if matches!(
                state,
                "submitting"
                    | "resolved"
                    | "skip-submitting"
                    | "skip-resolved"
                    | "shape-submitting"
                    | "shape-resolved"
            ) {
                request.selected_option_index = Some(1);
            }
        }

        self.conversation.user_message = Some(if multi_fixture {
            "请只通过请求用户输入表单依次询问界面主色和图标形状，并等待我的表单操作。".to_owned()
        } else if skip_fixture {
            "请直接调用请求用户输入表单，提一个单选问题：问题“是否继续？”，选项“继续”和“停止”，等待我的表单操作。".to_owned()
        } else if shape_fixture {
            "请直接调用请求用户输入表单，提一个单选问题：“选择形状”，选项“圆形”和“方形”，等待我的表单选择。".to_owned()
        } else if other_fixture {
            "请仅调用请求用户输入表单，提一个单选问题：标题“选择交通工具”，问题“请选择一种交通工具。”，选项“火车”和“飞机”，等待我的表单操作。不要修改文件、不要执行命令，也不要在普通回复中提问。".to_owned()
        } else if keyboard_fixture {
            "请仅调用请求用户输入表单，提一个单选问题：标题“选择饮料”，问题“请选择一种饮料。”，选项“水”和“咖啡”，等待我的表单操作。".to_owned()
        } else {
            "请直接调用请求用户输入/提问表单能力，向我提一个单选问题：标题“选择颜色”，选项“红色”和“蓝色”。不要在普通回复中提问，必须等待我的表单回答。".to_owned()
        });
        self.conversation.user_message_time = Some("16:43".to_owned());
        self.conversation.assistant_message.clear();
        self.conversation.assistant_message_time = None;
        self.conversation.phase = ConversationPhase::Streaming;
        self.conversation.activities = vec![ConversationActivity::UserInput(request)];
        if let Some((placeholder, secret, answer)) =
            self.conversation.activities.iter().find_map(|activity| {
                let ConversationActivity::UserInput(model) = activity else {
                    return None;
                };
                model.current_question().map(|question| {
                    (
                        question.other_placeholder.clone(),
                        question.is_secret,
                        model.other_answer.clone(),
                    )
                })
            })
        {
            self.user_input_other_input.update(cx, |input, cx| {
                input.configure_inline_other(placeholder, secret, cx);
                input.set_text_silently(answer, cx);
            });
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }
}
