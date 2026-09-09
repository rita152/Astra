use std::{
    collections::HashSet,
    io::{Cursor, Error as IoError, ErrorKind, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};

use base64::Engine as _;
use serde_json::{Value, json};

use super::{
    AgentAccountRateLimits, AgentBackend, AgentCommandApprovalChoice, AgentConfigWarning,
    AgentCreditsSnapshot, AgentEvent, AgentFileChangeStatus, AgentImageGenerationFailure,
    AgentImageGenerationStatus, AgentImageView, AgentInterruptControl, AgentInterruptHandle,
    AgentInterruptOutcome, AgentMcpServerStartupFailureReason, AgentMcpServerStartupState,
    AgentMcpServerStartupStatus, AgentMcpToolCall, AgentMcpToolCallStatus, AgentOptionalField,
    AgentPermissionMode, AgentPermissionsApprovalChoice, AgentRateLimitWindow, AgentReasoning,
    AgentRequest, AgentServerRequestFailureKind, AgentServerRequestId, AgentServerRequestKind,
    AgentServerRequestMetadata, AgentThreadActiveFlag, AgentThreadSettings, AgentThreadStatus,
    AgentThreadStatusState, AgentThreadTokenUsage, AgentTokenUsageBreakdown,
    AgentUserInputResponse, AppServerProcess, CodexAppServerBackend, CodexTurnSession,
    INITIALIZE_ID, MODEL_LIST_PAGE_SIZE, TurnOutcome, UNDEFINED_METHOD_PARAMS_LIMIT,
    cleanup_pending_server_requests, drive_model_catalog, drive_permission_profiles, drive_session,
    drive_thread_settings_update, ensure_server_method_is_defined, finish_prompt_session,
    handle_server_request_resolved, parse_agent_notification, respond_to_server_request_on_session,
    run_model_catalog_process, thread_settings_update_request, wait_for_response,
};
use crate::agent::AgentUserInputAnswer;

struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
        Err(IoError::new(
            ErrorKind::BrokenPipe,
            "fixture JSON-RPC write failure",
        ))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn command_approval_request(id: Value) -> Value {
    command_approval_request_for(id, "thr_1", "turn_1")
}

fn current_command_approval_request(id: Value) -> Value {
    let mut request = command_approval_request(id);
    request["params"]["availableDecisions"][2] = json!("cancel");
    request
}

fn command_approval_request_for(id: Value, thread_id: &str, turn_id: &str) -> Value {
    json!({
        "id": id,
        "method": "item/commandExecution/requestApproval",
        "params": {
            "kind": "command",
            "threadId": thread_id,
            "turnId": turn_id,
            "itemId": "item_1",
            "startedAtMs": 1_777_777_777_000_i64,
            "environmentId": null,
            "reason": "需要读取版本",
            "command": "git --version",
            "cwd": "/tmp",
            "commandActions": [{"type":"unknown","command":"git --version"}],
            "proposedExecpolicyAmendment": ["git", "--version"],
            "availableDecisions": [
                "accept",
                {"acceptWithExecpolicyAmendment":{"execpolicy_amendment":["git","--version"]}},
                "decline"
            ]
        }
    })
}

fn user_input_request(id: Value) -> Value {
    json!({
        "id": id,
        "method": "item/tool/requestUserInput",
        "params": {
            "threadId": "thr_1",
            "turnId": "turn_1",
            "itemId": "tool_1",
            "questions": [
                {
                    "id": "color",
                    "header": "Color",
                    "question": "Choose colors",
                    "isOther": true,
                    "isSecret": false,
                    "options": [
                        {"label": "red", "description": "Warm"},
                        {"label": "blue", "description": "Cool"}
                    ]
                },
                {
                    "id": "token",
                    "header": "Token",
                    "question": "Enter the token",
                    "isOther": true,
                    "isSecret": true,
                    "options": null
                }
            ],
            "isBlocking": true,
            "autoResolutionMs": 1500
        }
    })
}

fn permissions_approval_request(id: Value) -> Value {
    json!({
        "id": id,
        "method": "item/permissions/requestApproval",
        "params": {
            "threadId": "thr_1",
            "turnId": "turn_1",
            "itemId": "permissions_1",
            "environmentId": "env_1",
            "startedAtMs": 1_777_777_777_000_i64,
            "cwd": "/workspace/project",
            "reason": "Read fixtures and contact the network",
            "permissions": {
                "fileSystem": {
                    "read": ["/legacy/read"],
                    "write": null,
                    "globScanMaxDepth": 4,
                    "entries": [
                        {"access":"read","path":{"type":"path","path":"/workspace/input"}},
                        {"access":"write","path":{"type":"glob_pattern","pattern":"/workspace/out/**"}},
                        {"access":"deny","path":{"type":"special","value":{"kind":"project_roots","subpath":"private"}}},
                        {"access":"read","path":{"type":"special","value":{"kind":"root"}}},
                        {"access":"read","path":{"type":"special","value":{"kind":"minimal"}}},
                        {"access":"read","path":{"type":"special","value":{"kind":"tmpdir"}}},
                        {"access":"read","path":{"type":"special","value":{"kind":"slash_tmp"}}},
                        {"access":"read","path":{"type":"special","value":{"kind":"unknown","path":"/unknown","subpath":"child"}}}
                    ]
                },
                "network": {"enabled": true}
            }
        }
    })
}

fn take_session_output(session: &CodexTurnSession<Vec<u8>>) -> Vec<u8> {
    let mut writer = session.writer.lock().unwrap();
    std::mem::take(writer.as_mut().unwrap())
}

fn turn_item_message(method: &str, item: Value) -> Value {
    let mut message = json!({
        "method": method,
        "params": {
            "threadId": "thr_1",
            "turnId": "turn_1",
            "item": item
        }
    });
    match method {
        "item/started" => message["params"]["startedAtMs"] = json!(1_000),
        "item/completed" => message["params"]["completedAtMs"] = json!(2_250),
        _ => {}
    }
    message
}

fn thread_token_usage_message(thread_id: &str, turn_id: &str) -> Value {
    json!({
        "method": "thread/tokenUsage/updated",
        "params": {
            "threadId": thread_id,
            "turnId": turn_id,
            "tokenUsage": {
                "total": {
                    "totalTokens": 16_221,
                    "inputTokens": 16_207,
                    "cachedInputTokens": 11_008,
                    "cacheWriteInputTokens": 0,
                    "outputTokens": 14,
                    "reasoningOutputTokens": 0
                },
                "last": {
                    "totalTokens": 16_221,
                    "inputTokens": 16_207,
                    "cachedInputTokens": 11_008,
                    "cacheWriteInputTokens": 0,
                    "outputTokens": 14,
                    "reasoningOutputTokens": 0
                },
                "modelContextWindow": 258_400
            }
        }
    })
}

fn account_rate_limits_message() -> Value {
    json!({
        "method": "account/rateLimits/updated",
        "params": {
            "rateLimits": {
                "limitId": "codex",
                "limitName": null,
                "primary": {
                    "usedPercent": 15,
                    "windowDurationMins": 10_080,
                    "resetsAt": 1_788_752_152_i64
                },
                "secondary": null,
                "credits": {
                    "hasCredits": false,
                    "unlimited": false,
                    "balance": "0"
                },
                "individualLimit": null,
                "spendControlReached": null,
                "planType": "pro",
                "rateLimitReachedType": null
            }
        }
    })
}

fn assert_turn_message_fails(message: &Value, expected: &[&str]) -> String {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let mut streamed_text = false;
    let error = super::process_turn_message(
        &session,
        message,
        "thr_1",
        "turn_1",
        &tx,
        &mut streamed_text,
    )
    .unwrap_err()
    .to_string();
    assert!(
        rx.try_recv().is_err(),
        "unexpected event for {message}: {error}"
    );
    for fragment in expected {
        assert!(
            error.contains(fragment),
            "error for {message} did not contain `{fragment}`: {error}"
        );
    }
    error
}

fn command_execution_item(status: &str) -> Value {
    json!({
        "type": "commandExecution",
        "id": "exec_1",
        "command": "/bin/zsh -lc pwd",
        "commandActions": [{"type": "unknown", "command": "pwd"}],
        "cwd": "/tmp/project",
        "status": status,
        "aggregatedOutput": null,
        "exitCode": null
    })
}

fn turn_start_for_mode(mode: AgentPermissionMode, cwd: PathBuf) -> Value {
    let input = concat!(
        "{\"id\":1,\"result\":{}}\n",
        "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_permissions\"}}}\n",
        "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_permissions\"}}}\n",
        "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thr_permissions\",\"turn\":{\"id\":\"turn_permissions\",\"status\":\"completed\"}}}\n"
    );
    let mut reader = Cursor::new(input.as_bytes());
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, _rx) = async_channel::unbounded();
    drive_session(
        &mut reader,
        &session,
        &AgentRequest {
            client_message_id: None,
            prompt: "permission probe".into(),
            cwd,
            project_id: None,
            thread_id: None,
            model: "gpt-test".into(),
            effort: "medium".into(),
            service_tier: None,
            permission_mode: mode,
            context: Default::default(),
        },
        &tx,
    )
    .unwrap();
    String::from_utf8(take_session_output(&session))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|message| message.get("method").and_then(Value::as_str) == Some("turn/start"))
        .unwrap()
}

#[test]
fn permission_mode_requests_match_the_four_protocol_shapes() {
    let cwd = PathBuf::from("/tmp/project");
    let request =
        thread_settings_update_request(7, "thr_1", &cwd, AgentPermissionMode::Request).unwrap();
    assert_eq!(
        request.pointer("/params/approvalPolicy"),
        Some(&json!("on-request"))
    );
    assert_eq!(
        request.pointer("/params/approvalsReviewer"),
        Some(&json!("user"))
    );
    assert_eq!(
        request.pointer("/params/permissions"),
        Some(&json!(":workspace"))
    );
    assert!(request.pointer("/params/sandboxPolicy").is_none());

    let assist =
        thread_settings_update_request(7, "thr_1", &cwd, AgentPermissionMode::Assist).unwrap();
    assert_eq!(
        assist.pointer("/params/approvalsReviewer"),
        Some(&json!("guardian_subagent"))
    );
    assert_eq!(
        assist.pointer("/params/permissions"),
        Some(&json!(":workspace"))
    );

    let full = thread_settings_update_request(7, "thr_1", &cwd, AgentPermissionMode::Full).unwrap();
    assert_eq!(
        full.pointer("/params/approvalPolicy"),
        Some(&json!("never"))
    );
    assert_eq!(
        full.pointer("/params/approvalsReviewer"),
        Some(&json!("user"))
    );
    assert_eq!(
        full.pointer("/params/permissions"),
        Some(&json!(":danger-full-access"))
    );

    let temp = std::env::temp_dir().join(format!("gpui-permission-custom-{}", std::process::id()));
    std::fs::create_dir_all(temp.join(".codex")).unwrap();
    std::fs::write(
        temp.join(".codex/config.toml"),
        "sandbox_mode = \"danger-full-access\"\n",
    )
    .unwrap();
    let custom =
        thread_settings_update_request(7, "thr_1", &temp, AgentPermissionMode::Custom).unwrap();
    assert_eq!(
        custom.pointer("/params/approvalPolicy"),
        Some(&json!("on-request"))
    );
    assert_eq!(
        custom.pointer("/params/approvalsReviewer"),
        Some(&json!("user"))
    );
    assert_eq!(
        custom.pointer("/params/sandboxPolicy/type"),
        Some(&json!("dangerFullAccess"))
    );
    assert!(custom.pointer("/params/permissions").is_none());
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn first_turn_carries_each_permission_mode_and_assist_uses_effective_reviewer() {
    let request = turn_start_for_mode(AgentPermissionMode::Request, PathBuf::from("/tmp/project"));
    assert_eq!(
        request.pointer("/params/approvalPolicy"),
        Some(&json!("on-request"))
    );
    assert_eq!(
        request.pointer("/params/approvalsReviewer"),
        Some(&json!("user"))
    );
    assert_eq!(
        request.pointer("/params/sandboxPolicy/type"),
        Some(&json!("workspaceWrite"))
    );
    assert_eq!(request.pointer("/params/permissions"), Some(&Value::Null));
    assert_eq!(
        request.pointer("/params/runtimeWorkspaceRoots"),
        Some(&Value::Null)
    );

    let assist = turn_start_for_mode(AgentPermissionMode::Assist, PathBuf::from("/tmp/project"));
    assert_eq!(
        assist.pointer("/params/approvalsReviewer"),
        Some(&json!("auto_review"))
    );

    let full = turn_start_for_mode(AgentPermissionMode::Full, PathBuf::from("/tmp/project"));
    assert_eq!(
        full.pointer("/params/permissions"),
        Some(&json!(":danger-full-access"))
    );
    assert_eq!(full.pointer("/params/sandboxPolicy"), Some(&Value::Null));
    assert!(
        full.pointer("/params/runtimeWorkspaceRoots")
            .is_some_and(Value::is_array)
    );

    let temp = std::env::temp_dir().join(format!(
        "gpui-permission-turn-custom-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(temp.join(".codex")).unwrap();
    std::fs::write(
        temp.join(".codex/config.toml"),
        "sandbox_mode = \"danger-full-access\"\n",
    )
    .unwrap();
    let custom = turn_start_for_mode(AgentPermissionMode::Custom, temp.clone());
    assert_eq!(
        custom.pointer("/params/sandboxPolicy/type"),
        Some(&json!("dangerFullAccess"))
    );
    assert_eq!(custom.pointer("/params/permissions"), Some(&Value::Null));
    assert!(
        custom
            .pointer("/params/runtimeWorkspaceRoots")
            .is_some_and(Value::is_array)
    );
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn effective_permission_notification_preserves_auto_review_and_profile() {
    let message = json!({
        "method": "thread/settings/updated",
        "params": { "threadId": "thr_1", "threadSettings": {
            "model": "gpt-test", "effort": "medium", "serviceTier": null, "cwd": "/tmp/project",
            "approvalPolicy": "on-request", "approvalsReviewer": "auto_review",
            "sandboxPolicy": { "type": "workspaceWrite", "writableRoots": ["/tmp/project"] },
            "activePermissionProfile": { "id": ":workspace", "extends": null }
        }}
    });
    let Some(AgentEvent::ThreadSettingsUpdated(settings)) =
        parse_agent_notification(&message).unwrap()
    else {
        panic!("expected settings event");
    };
    let permissions = settings.permissions.unwrap();
    assert_eq!(permissions.approvals_reviewer, "auto_review");
    assert_eq!(permissions.approval_policy, "on-request");
    assert_eq!(
        permissions.active_permission_profile.unwrap().id,
        ":workspace"
    );
    assert_eq!(
        permissions.sandbox_policy.unwrap()["type"],
        "workspaceWrite"
    );
}

#[test]
fn settings_rpc_failure_returns_error_without_an_effective_update() {
    let input = concat!(
        "{\"id\":1,\"result\":{}}\n",
        "{\"id\":2,\"error\":{\"code\":-32602,\"message\":\"invalid permissions\"}}\n"
    );
    let mut reader = Cursor::new(input.as_bytes());
    let mut writer = Vec::new();
    let error = drive_thread_settings_update(
        &mut reader,
        &mut writer,
        "thr_1",
        Path::new("/tmp/project"),
        AgentPermissionMode::Assist,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("invalid permissions"));
}

#[test]
fn existing_thread_switch_waits_for_and_returns_effective_settings() {
    let input = concat!(
        "{\"id\":1,\"result\":{}}\n",
        "{\"id\":2,\"result\":{}}\n",
        "{\"method\":\"thread/settings/updated\",\"params\":{\"threadId\":\"thr_1\",\"threadSettings\":{\"model\":\"gpt-test\",\"effort\":\"medium\",\"serviceTier\":null,\"cwd\":\"/tmp/project\",\"approvalPolicy\":\"on-request\",\"approvalsReviewer\":\"auto_review\",\"sandboxPolicy\":{\"type\":\"workspaceWrite\"},\"activePermissionProfile\":{\"id\":\":workspace\",\"extends\":null}}}}\n"
    );
    let mut reader = Cursor::new(input.as_bytes());
    let mut writer = Vec::new();
    let settings = drive_thread_settings_update(
        &mut reader,
        &mut writer,
        "thr_1",
        Path::new("/tmp/project"),
        AgentPermissionMode::Assist,
    )
    .unwrap();
    let effective = settings.permissions.unwrap();
    assert_eq!(effective.approvals_reviewer, "auto_review");
    let sent = String::from_utf8(writer).unwrap();
    let update: Value = sent
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .find(|message: &Value| {
            message.get("method").and_then(Value::as_str) == Some("thread/settings/update")
        })
        .unwrap();
    assert_eq!(
        update.pointer("/params/approvalsReviewer"),
        Some(&json!("guardian_subagent"))
    );
}

#[test]
fn permission_profile_list_maps_available_profiles() {
    let input = concat!(
        "{\"id\":1,\"result\":{}}\n",
        "{\"id\":2,\"result\":{\"data\":[{\"id\":\":workspace\",\"allowed\":true,\"extends\":null}],\"nextCursor\":null}}\n"
    );
    let mut reader = Cursor::new(input.as_bytes());
    let mut writer = Vec::new();
    let profiles =
        drive_permission_profiles(&mut reader, &mut writer, Path::new("/tmp/project")).unwrap();
    assert_eq!(
        profiles,
        vec![super::AgentPermissionProfile {
            id: ":workspace".into(),
            allowed: true,
            extends: None
        }]
    );
    let sent = String::from_utf8(writer).unwrap();
    assert!(sent.contains("\"method\":\"permissionProfile/list\""));
}

#[test]
fn drives_one_complete_prompt_and_normalizes_stream_events() {
    let input = concat!(
        "{\"id\":1,\"result\":{}}\n",
        "{\"method\":\"thread/status/changed\",\"params\":{\"threadId\":\"thr_1\",\"status\":{\"type\":\"active\",\"activeFlags\":[]}}}\n",
        "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_1\"}}}\n",
        "{\"method\":\"thread/started\",\"params\":{\"thread\":{\"id\":\"thr_1\",\"sessionId\":\"thr_1\",\"ephemeral\":false,\"turns\":[]}}}\n",
        "{\"method\":\"mcpServer/startupStatus/updated\",\"params\":{\"threadId\":\"thr_1\",\"name\":\"codex_apps\",\"status\":\"starting\",\"error\":null,\"failureReason\":null}}\n",
        "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_1\",\"turn\":{\"id\":\"turn_1\",\"items\":[],\"status\":\"inProgress\"}}}\n",
        "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_1\"}}}\n",
        "{\"method\":\"item/started\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"item\":{\"type\":\"userMessage\",\"id\":\"user_1\",\"clientId\":null,\"content\":[{\"type\":\"text\",\"text\":\"打个招呼\",\"text_elements\":[]}]}}}\n",
        "{\"method\":\"item/completed\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"item\":{\"type\":\"userMessage\",\"id\":\"user_1\",\"clientId\":null,\"content\":[{\"type\":\"text\",\"text\":\"打个招呼\",\"text_elements\":[]}]}}}\n",
        "{\"method\":\"item/started\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"item\":{\"type\":\"agentMessage\",\"id\":\"msg_1\",\"text\":\"\"}}}\n",
        "{\"method\":\"item/agentMessage/delta\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"itemId\":\"msg_1\",\"delta\":\"你好\"}}\n",
        "{\"method\":\"item/agentMessage/delta\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"itemId\":\"msg_1\",\"delta\":\"！\"}}\n",
        "{\"method\":\"item/started\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"item\":{\"type\":\"commandExecution\",\"id\":\"exec_1\",\"command\":\"/bin/zsh -lc pwd\",\"cwd\":\"/tmp/project\",\"status\":\"inProgress\",\"commandActions\":[{\"type\":\"unknown\",\"command\":\"pwd\"}],\"aggregatedOutput\":null,\"exitCode\":null}}}\n",
        "{\"method\":\"item/commandExecution/outputDelta\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"itemId\":\"exec_1\",\"delta\":\"/tmp/project\\n\"}}\n",
        "{\"method\":\"item/completed\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"item\":{\"type\":\"commandExecution\",\"id\":\"exec_1\",\"command\":\"/bin/zsh -lc pwd\",\"cwd\":\"/tmp/project\",\"status\":\"completed\",\"commandActions\":[{\"type\":\"unknown\",\"command\":\"pwd\"}],\"aggregatedOutput\":\"/tmp/project\\n\",\"exitCode\":0}}}\n",
        "{\"method\":\"thread/tokenUsage/updated\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"tokenUsage\":{\"total\":{\"totalTokens\":16221,\"inputTokens\":16207,\"cachedInputTokens\":11008,\"cacheWriteInputTokens\":0,\"outputTokens\":14,\"reasoningOutputTokens\":0},\"last\":{\"totalTokens\":16221,\"inputTokens\":16207,\"cachedInputTokens\":11008,\"cacheWriteInputTokens\":0,\"outputTokens\":14,\"reasoningOutputTokens\":0},\"modelContextWindow\":258400}}}\n",
        "{\"method\":\"account/rateLimits/updated\",\"params\":{\"rateLimits\":{\"limitId\":\"codex\",\"limitName\":null,\"primary\":{\"usedPercent\":15,\"windowDurationMins\":10080,\"resetsAt\":1788752152},\"secondary\":null,\"credits\":{\"hasCredits\":false,\"unlimited\":false,\"balance\":\"0\"},\"individualLimit\":null,\"spendControlReached\":null,\"planType\":\"pro\",\"rateLimitReachedType\":null}}}\n",
        "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thr_1\",\"turn\":{\"id\":\"turn_1\",\"status\":\"completed\"}}}\n"
    );
    let mut reader = Cursor::new(input.as_bytes());
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let outcome = drive_session(
        &mut reader,
        &session,
        &AgentRequest {
            client_message_id: None,
            prompt: "打个招呼".into(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            thread_id: None,
            model: "gpt-test".into(),
            effort: "high".into(),
            service_tier: Some("priority".into()),
            permission_mode: AgentPermissionMode::Full,
            context: Default::default(),
        },
        &tx,
    )
    .unwrap();
    assert_eq!(outcome, TurnOutcome::Completed);
    tx.send_blocking(outcome.into_event()).unwrap();
    drop(tx);

    let mut received = Vec::new();
    while let Ok(event) = rx.try_recv() {
        received.push(event);
    }
    assert_eq!(
        received,
        vec![
            AgentEvent::ThreadStatusChanged(AgentThreadStatus {
                thread_id: "thr_1".into(),
                state: AgentThreadStatusState::Active {
                    active_flags: Vec::new(),
                },
            }),
            AgentEvent::ThreadCreated {
                thread_id: "thr_1".into()
            },
            AgentEvent::McpServerStartupStatusUpdated(AgentMcpServerStartupStatus {
                thread_id: Some("thr_1".into()),
                name: "codex_apps".into(),
                state: AgentMcpServerStartupState::Starting,
                error: None,
                failure_reason: None,
            }),
            AgentEvent::Started,
            AgentEvent::UserMessage {
                item_id: "user_1".into(),
                client_message_id: None,
                text: "打个招呼".into(),
                images: vec![]
            },
            AgentEvent::AssistantMessageStarted {
                item_id: "msg_1".into(),
            },
            AgentEvent::TextDelta("你好".into()),
            AgentEvent::TextDelta("！".into()),
            AgentEvent::CommandStarted(super::CommandExecution {
                id: "exec_1".into(),
                command: "pwd".into(),
                actions: vec![super::CommandExecutionAction::Unknown {
                    command: "pwd".into(),
                }],
                cwd: "/tmp/project".into(),
                output: String::new(),
                terminal_process_id: None,
                status: super::CommandExecutionStatus::InProgress,
                exit_code: None,
            }),
            AgentEvent::CommandOutputDelta {
                item_id: "exec_1".into(),
                delta: "/tmp/project\n".into(),
            },
            AgentEvent::CommandCompleted(super::CommandExecution {
                id: "exec_1".into(),
                command: "pwd".into(),
                actions: vec![super::CommandExecutionAction::Unknown {
                    command: "pwd".into(),
                }],
                cwd: "/tmp/project".into(),
                output: "/tmp/project\n".into(),
                terminal_process_id: None,
                status: super::CommandExecutionStatus::Completed,
                exit_code: Some(0),
            }),
            AgentEvent::ThreadTokenUsageUpdated(AgentThreadTokenUsage {
                thread_id: "thr_1".into(),
                turn_id: "turn_1".into(),
                total: AgentTokenUsageBreakdown {
                    total_tokens: 16_221,
                    input_tokens: 16_207,
                    cached_input_tokens: 11_008,
                    cache_write_input_tokens: 0,
                    output_tokens: 14,
                    reasoning_output_tokens: 0,
                },
                last: AgentTokenUsageBreakdown {
                    total_tokens: 16_221,
                    input_tokens: 16_207,
                    cached_input_tokens: 11_008,
                    cache_write_input_tokens: 0,
                    output_tokens: 14,
                    reasoning_output_tokens: 0,
                },
                model_context_window: Some(258_400),
            }),
            AgentEvent::AccountRateLimitsUpdated(AgentAccountRateLimits {
                limit_id: Some("codex".into()),
                limit_name: None,
                primary: Some(AgentRateLimitWindow {
                    used_percent: 15,
                    window_duration_mins: Some(10_080),
                    resets_at: Some(1_788_752_152),
                }),
                secondary: None,
                credits: Some(AgentCreditsSnapshot {
                    has_credits: false,
                    unlimited: false,
                    balance: Some("0".into()),
                }),
                individual_limit: None,
                spend_control_reached: None,
                plan_type: Some("pro".into()),
                rate_limit_reached_type: None,
            }),
            AgentEvent::Completed,
        ]
    );

    let sent = String::from_utf8(take_session_output(&session)).unwrap();
    assert!(sent.contains("\"method\":\"initialize\""));
    assert!(sent.contains("\"method\":\"initialized\""));
    assert!(sent.contains("\"method\":\"thread/start\""));
    assert!(sent.contains("\"method\":\"turn/start\""));
    assert!(sent.contains("\"threadId\":\"thr_1\""));

    let sent_messages: Vec<serde_json::Value> = sent
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let sent_methods: Vec<_> = sent_messages
        .iter()
        .filter_map(|message| message.get("method").and_then(Value::as_str))
        .collect();
    assert_eq!(
        sent_methods,
        vec!["initialize", "initialized", "thread/start", "turn/start"]
    );
    let initialize = sent_messages
        .iter()
        .find(|message| {
            message.get("method").and_then(|value| value.as_str()) == Some("initialize")
        })
        .unwrap();
    assert_eq!(
        initialize.pointer("/params/capabilities/experimentalApi"),
        Some(&json!(true))
    );
    assert_eq!(
        initialize.pointer("/params/capabilities/requestAttestation"),
        Some(&json!(false))
    );
    let thread_start = sent_messages
        .iter()
        .find(|message| {
            message.get("method").and_then(|value| value.as_str()) == Some("thread/start")
        })
        .unwrap();
    assert_eq!(
        thread_start
            .pointer("/params/model")
            .and_then(|value| value.as_str()),
        Some("gpt-test")
    );
    assert_eq!(
        thread_start
            .pointer("/params/serviceTier")
            .and_then(|value| value.as_str()),
        Some("priority")
    );
    assert!(thread_start.pointer("/params/approvalPolicy").is_none());
    assert!(thread_start.pointer("/params/sandbox").is_none());
    let turn_start = sent_messages
        .iter()
        .find(|message| {
            message.get("method").and_then(|value| value.as_str()) == Some("turn/start")
        })
        .unwrap();
    assert_eq!(
        turn_start
            .pointer("/params/model")
            .and_then(|value| value.as_str()),
        Some("gpt-test")
    );
    assert_eq!(
        turn_start
            .pointer("/params/effort")
            .and_then(|value| value.as_str()),
        Some("high")
    );
    assert_eq!(
        turn_start
            .pointer("/params/serviceTier")
            .and_then(|value| value.as_str()),
        Some("priority")
    );
    assert_eq!(
        turn_start.pointer("/params/approvalPolicy"),
        Some(&json!("never"))
    );
    assert_eq!(
        turn_start.pointer("/params/approvalsReviewer"),
        Some(&json!("user"))
    );
    assert_eq!(
        turn_start.pointer("/params/sandboxPolicy"),
        Some(&Value::Null)
    );
    assert_eq!(
        turn_start.pointer("/params/permissions"),
        Some(&json!(":danger-full-access"))
    );
    assert!(
        turn_start
            .pointer("/params/runtimeWorkspaceRoots")
            .is_some_and(Value::is_array)
    );
}

#[test]
fn existing_thread_resumes_before_turn_start() {
    let input = concat!(
        "{\"id\":1,\"result\":{}}\n",
        "{\"method\":\"thread/started\",\"params\":{\"thread\":{\"id\":\"thr_existing\"}}}\n",
        "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_existing\"}}}\n",
        "{\"method\":\"thread/goal/cleared\",\"params\":{\"threadId\":\"thr_existing\"}}\n",
        "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_existing\",\"turn\":{\"id\":\"turn_next\",\"items\":[],\"status\":\"inProgress\"}}}\n",
        "{\"method\":\"item/agentMessage/delta\",\"params\":{\"threadId\":\"thr_existing\",\"turnId\":\"turn_next\",\"itemId\":\"msg_next\",\"delta\":\"继续\"}}\n",
        "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_next\"}}}\n",
        "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thr_existing\",\"turn\":{\"id\":\"turn_next\",\"status\":\"completed\"}}}\n"
    );
    let mut reader = Cursor::new(input.as_bytes());
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();

    let outcome = drive_session(
        &mut reader,
        &session,
        &AgentRequest {
            client_message_id: None,
            prompt: "继续对话".into(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            thread_id: Some("thr_existing".into()),
            model: "gpt-test".into(),
            effort: "high".into(),
            service_tier: Some("priority".into()),
            permission_mode: AgentPermissionMode::Request,
            context: Default::default(),
        },
        &tx,
    )
    .unwrap();
    assert_eq!(outcome, TurnOutcome::Completed);
    tx.send_blocking(outcome.into_event()).unwrap();
    drop(tx);

    let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert_eq!(
        events,
        vec![
            AgentEvent::Started,
            AgentEvent::TextDelta("继续".into()),
            AgentEvent::Completed,
        ]
    );

    let sent_messages: Vec<Value> = String::from_utf8(take_session_output(&session))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let sent_methods: Vec<_> = sent_messages
        .iter()
        .filter_map(|message| message.get("method").and_then(Value::as_str))
        .collect();
    assert_eq!(
        sent_methods,
        vec!["initialize", "initialized", "thread/resume", "turn/start"]
    );
    let resume = sent_messages
        .iter()
        .find(|message| message.get("method").and_then(Value::as_str) == Some("thread/resume"))
        .unwrap();
    assert_eq!(resume.get("id"), Some(&json!(2)));
    assert_eq!(
        resume.get("params"),
        Some(&json!({"threadId":"thr_existing","excludeTurns":true}))
    );

    let turn_start = sent_messages
        .iter()
        .find(|message| message.get("method").and_then(Value::as_str) == Some("turn/start"))
        .unwrap();
    assert_eq!(
        turn_start.pointer("/params/threadId"),
        Some(&json!("thr_existing"))
    );
    assert_eq!(
        turn_start.pointer("/params/model"),
        Some(&json!("gpt-test"))
    );
    assert_eq!(turn_start.pointer("/params/effort"), Some(&json!("high")));
    assert_eq!(
        turn_start.pointer("/params/serviceTier"),
        Some(&json!("priority"))
    );
    for field in [
        "approvalPolicy",
        "approvalsReviewer",
        "sandboxPolicy",
        "permissions",
        "runtimeWorkspaceRoots",
    ] {
        assert!(turn_start.pointer(&format!("/params/{field}")).is_none());
    }
}

#[test]
fn resume_goal_cleared_requires_a_matching_string_thread_id() {
    for (params, expected_error) in [
        (json!({}), "缺少字符串字段 params.threadId"),
        (json!({"threadId": 7}), "必须是字符串"),
        (
            json!({"threadId": "thr_other"}),
            "与当前 resume thread `thr_existing` 不一致",
        ),
    ] {
        let input = format!(
            "{}\n{}\n{}\n",
            json!({"id": 1, "result": {}}),
            json!({"method": "thread/goal/cleared", "params": params}),
            json!({"id": 2, "result": {"thread": {"id": "thr_existing"}}}),
        );
        let mut reader = Cursor::new(input.into_bytes());
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();

        let error = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                client_message_id: None,
                prompt: "继续对话".into(),
                cwd: PathBuf::from("/tmp/project"),
                project_id: None,
                thread_id: Some("thr_existing".into()),
                model: "gpt-test".into(),
                effort: "medium".into(),
                service_tier: None,
                permission_mode: AgentPermissionMode::Full,
                context: Default::default(),
            },
            &tx,
        )
        .unwrap_err();
        let error = format!("{error:#}");

        assert!(error.contains("thread/goal/cleared"), "{error}");
        assert!(error.contains(expected_error), "{error}");
        let sent_methods: Vec<_> = String::from_utf8(take_session_output(&session))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .filter_map(|message| {
                message
                    .get("method")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .collect();
        assert_eq!(
            sent_methods,
            vec!["initialize", "initialized", "thread/resume"]
        );
        assert!(rx.try_recv().is_err());
    }
}

#[test]
fn goal_cleared_after_resumed_turn_start_fails_fast() {
    let input = concat!(
        "{\"id\":1,\"result\":{}}\n",
        "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_existing\"}}}\n",
        "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_next\"}}}\n",
        "{\"method\":\"thread/goal/cleared\",\"params\":{\"threadId\":\"thr_existing\"}}\n"
    );
    let mut reader = Cursor::new(input.as_bytes());
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();

    let error = drive_session(
        &mut reader,
        &session,
        &AgentRequest {
            client_message_id: None,
            prompt: "继续对话".into(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            thread_id: Some("thr_existing".into()),
            model: "gpt-test".into(),
            effort: "medium".into(),
            service_tier: None,
            permission_mode: AgentPermissionMode::Full,
            context: Default::default(),
        },
        &tx,
    )
    .unwrap_err();
    let error = format!("{error:#}");

    assert!(error.contains("未定义"), "{error}");
    assert!(error.contains("thread/goal/cleared"), "{error}");
    let sent = String::from_utf8(take_session_output(&session)).unwrap();
    assert!(sent.contains("\"method\":\"turn/start\""));
    assert!(rx.try_recv().is_err());
}

#[test]
fn thread_started_must_match_the_canonical_thread_id_in_either_order() {
    let cases = [
        (
            concat!(
                "{\"id\":1,\"result\":{}}\n",
                "{\"method\":\"thread/started\",\"params\":{\"thread\":{\"id\":\"thr_wrong\"}}}\n",
                "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_expected\"}}}\n"
            ),
            "thread/start 通知与响应不一致",
        ),
        (
            concat!(
                "{\"id\":1,\"result\":{}}\n",
                "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_expected\"}}}\n",
                "{\"method\":\"thread/started\",\"params\":{\"thread\":{\"id\":\"thr_wrong\"}}}\n"
            ),
            "turn/start 失败",
        ),
    ];

    for (input, expected_context) in cases {
        let mut reader = Cursor::new(input.as_bytes());
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, _rx) = async_channel::unbounded();
        let error = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                client_message_id: None,
                prompt: "检查生命周期关联".into(),
                cwd: PathBuf::from("/tmp/project"),
                project_id: None,
                thread_id: None,
                model: "gpt-test".into(),
                effort: "medium".into(),
                service_tier: None,
                permission_mode: AgentPermissionMode::Full,
                context: Default::default(),
            },
            &tx,
        )
        .unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains(expected_context), "{message}");
        assert!(message.contains("thr_wrong"), "{message}");
        assert!(message.contains("thr_expected"), "{message}");
    }
}

#[test]
fn deferred_turn_is_not_forwarded_when_turn_start_fails() {
    let input = concat!(
        "{\"id\":1,\"result\":{}}\n",
        "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_existing\",\"turns\":[]}}}\n",
        "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_existing\",\"turn\":{\"id\":\"turn_auto\",\"items\":[],\"status\":\"inProgress\"}}}\n",
        "{\"id\":3,\"error\":{\"code\":-32600,\"message\":\"thread already has an active turn\"}}\n"
    );
    let mut reader = Cursor::new(input.as_bytes());
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();

    let result = drive_session(
        &mut reader,
        &session,
        &AgentRequest {
            client_message_id: None,
            prompt: "继续对话".into(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            thread_id: Some("thr_existing".into()),
            model: "gpt-test".into(),
            effort: "medium".into(),
            service_tier: None,
            permission_mode: AgentPermissionMode::Full,
            context: Default::default(),
        },
        &tx,
    );
    let terminal = finish_prompt_session(&session, result);

    let AgentEvent::Failed(message) = terminal else {
        panic!("expected active-turn failure");
    };
    assert!(message.contains("turn/start 失败"));
    assert!(message.contains("thread already has an active turn"));
    assert!(rx.try_recv().is_err());
    assert!(session.writer.lock().unwrap().is_none());
}

#[test]
fn deferred_batch_is_atomic_when_a_later_notification_mismatches() {
    let input = concat!(
        "{\"id\":1,\"result\":{}}\n",
        "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_existing\",\"turns\":[]}}}\n",
        "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_existing\",\"turn\":{\"id\":\"turn_user\",\"items\":[],\"status\":\"inProgress\"}}}\n",
        "{\"method\":\"error\",\"params\":{\"threadId\":\"thr_existing\",\"turnId\":\"turn_auto\",\"error\":{\"message\":\"old turn\",\"additionalDetails\":null},\"willRetry\":false}}\n",
        "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_user\"}}}\n"
    );
    let mut reader = Cursor::new(input.as_bytes());
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();

    let result = drive_session(
        &mut reader,
        &session,
        &AgentRequest {
            client_message_id: None,
            prompt: "继续对话".into(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            thread_id: Some("thr_existing".into()),
            model: "gpt-test".into(),
            effort: "medium".into(),
            service_tier: None,
            permission_mode: AgentPermissionMode::Full,
            context: Default::default(),
        },
        &tx,
    );
    let terminal = finish_prompt_session(&session, result);

    let AgentEvent::Failed(message) = terminal else {
        panic!("expected the deferred batch to fail atomically");
    };
    assert!(message.contains("属于其他 turn"));
    assert!(message.contains("turn_auto"));
    assert!(message.contains("turn_user"));
    assert!(rx.try_recv().is_err());
    assert!(session.writer.lock().unwrap().is_none());
}

#[test]
fn active_goal_approval_is_discarded_when_turn_start_fails() {
    let approval = command_approval_request_for(json!(77), "thr_existing", "turn_auto");
    let input = format!(
        "{{\"id\":1,\"result\":{{}}}}\n\
         {{\"id\":2,\"result\":{{\"thread\":{{\"id\":\"thr_existing\"}}}}}}\n\
         {approval}\n\
         {{\"id\":3,\"error\":{{\"code\":-32600,\"message\":\"thread already has an active turn\"}}}}\n"
    );
    let mut reader = Cursor::new(input.as_bytes());
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();

    let result = drive_session(
        &mut reader,
        &session,
        &AgentRequest {
            client_message_id: None,
            prompt: "继续对话".into(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            thread_id: Some("thr_existing".into()),
            model: "gpt-test".into(),
            effort: "medium".into(),
            service_tier: None,
            permission_mode: AgentPermissionMode::Full,
            context: Default::default(),
        },
        &tx,
    );
    let sent: Vec<Value> = String::from_utf8(take_session_output(&session))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let terminal = finish_prompt_session(&session, result);

    let AgentEvent::Failed(message) = terminal else {
        panic!("expected turn/start failure");
    };
    assert!(message.contains("thread already has an active turn"));
    assert!(rx.try_recv().is_err());
    assert!(session.pending_approval_snapshot().is_empty());
    assert!(
        !sent
            .iter()
            .any(|message| message.get("id") == Some(&json!(77)))
    );
}

#[test]
fn deferred_started_approval_and_resolution_keep_wire_order() {
    let approval = command_approval_request_for(json!(77), "thr_existing", "turn_user");
    let input = format!(
        "{{\"id\":1,\"result\":{{}}}}\n\
         {{\"id\":2,\"result\":{{\"thread\":{{\"id\":\"thr_existing\"}}}}}}\n\
         {{\"method\":\"turn/started\",\"params\":{{\"threadId\":\"thr_existing\",\"turn\":{{\"id\":\"turn_user\",\"items\":[],\"status\":\"inProgress\"}}}}}}\n\
         {approval}\n\
         {{\"method\":\"serverRequest/resolved\",\"params\":{{\"threadId\":\"thr_existing\",\"requestId\":77}}}}\n\
         {{\"id\":3,\"result\":{{\"turn\":{{\"id\":\"turn_user\"}}}}}}\n\
         {{\"method\":\"turn/completed\",\"params\":{{\"threadId\":\"thr_existing\",\"turn\":{{\"id\":\"turn_user\",\"status\":\"completed\"}}}}}}\n"
    );
    let mut reader = Cursor::new(input.as_bytes());
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();

    let outcome = drive_session(
        &mut reader,
        &session,
        &AgentRequest {
            client_message_id: None,
            prompt: "继续对话".into(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            thread_id: Some("thr_existing".into()),
            model: "gpt-test".into(),
            effort: "medium".into(),
            service_tier: None,
            permission_mode: AgentPermissionMode::Full,
            context: Default::default(),
        },
        &tx,
    )
    .unwrap();
    tx.send_blocking(outcome.into_event()).unwrap();
    drop(tx);

    assert_eq!(rx.try_recv().unwrap(), AgentEvent::Started);
    let AgentEvent::CommandApprovalRequested { request, .. } = rx.try_recv().unwrap() else {
        panic!("expected deferred approval after Started");
    };
    assert_eq!(request.request_id, AgentServerRequestId::Number(77));
    assert_eq!(
        rx.try_recv().unwrap(),
        AgentEvent::ServerRequestResolved {
            request: AgentServerRequestMetadata {
                request_id: AgentServerRequestId::Number(77),
                thread_id: "thr_existing".into(),
                turn_id: "turn_user".into(),
                item_id: "item_1".into(),
                kind: AgentServerRequestKind::CommandApproval,
            }
        }
    );
    assert_eq!(rx.try_recv().unwrap(), AgentEvent::Completed);
    assert!(rx.try_recv().is_err());
    assert!(session.pending_approval_snapshot().is_empty());
}

#[test]
fn live_item_event_must_match_the_active_turn() {
    let input = concat!(
        "{\"id\":1,\"result\":{}}\n",
        "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_existing\"}}}\n",
        "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_user\"}}}\n",
        "{\"method\":\"item/agentMessage/delta\",\"params\":{\"threadId\":\"thr_existing\",\"turnId\":\"turn_old\",\"itemId\":\"msg_old\",\"delta\":\"旧内容\"}}\n"
    );
    let mut reader = Cursor::new(input.as_bytes());
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();

    let result = drive_session(
        &mut reader,
        &session,
        &AgentRequest {
            client_message_id: None,
            prompt: "继续对话".into(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            thread_id: Some("thr_existing".into()),
            model: "gpt-test".into(),
            effort: "medium".into(),
            service_tier: None,
            permission_mode: AgentPermissionMode::Full,
            context: Default::default(),
        },
        &tx,
    );
    let terminal = finish_prompt_session(&session, result);

    let AgentEvent::Failed(message) = terminal else {
        panic!("expected mismatched item event to fail");
    };
    assert!(message.contains("item/agentMessage/delta"));
    assert!(message.contains("turn_old"));
    assert!(message.contains("turn_user"));
    assert!(rx.try_recv().is_err());
}

#[test]
fn completed_turn_can_finish_before_turn_start_response() {
    let input = concat!(
        "{\"id\":1,\"result\":{}}\n",
        "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_fast\"}}}\n",
        "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_fast\",\"turn\":{\"id\":\"turn_fast\",\"items\":[],\"status\":\"inProgress\"}}}\n",
        "{\"method\":\"item/agentMessage/delta\",\"params\":{\"threadId\":\"thr_fast\",\"turnId\":\"turn_fast\",\"itemId\":\"msg_fast\",\"delta\":\"完成\"}}\n",
        "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thr_fast\",\"turn\":{\"id\":\"turn_fast\",\"status\":\"completed\"}}}\n",
        "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_fast\"}}}\n"
    );
    let mut reader = Cursor::new(input.as_bytes());
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();

    let outcome = drive_session(
        &mut reader,
        &session,
        &AgentRequest {
            client_message_id: None,
            prompt: "快速完成".into(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            thread_id: None,
            model: "gpt-test".into(),
            effort: "medium".into(),
            service_tier: None,
            permission_mode: AgentPermissionMode::Full,
            context: Default::default(),
        },
        &tx,
    )
    .unwrap();
    assert_eq!(outcome, TurnOutcome::Completed);
    tx.send_blocking(outcome.into_event()).unwrap();
    drop(tx);

    assert_eq!(
        std::iter::from_fn(|| rx.try_recv().ok()).collect::<Vec<_>>(),
        vec![
            AgentEvent::ThreadCreated {
                thread_id: "thr_fast".into()
            },
            AgentEvent::Started,
            AgentEvent::TextDelta("完成".into()),
            AgentEvent::Completed,
        ]
    );
}

#[test]
fn resume_rpc_error_fails_closed_without_starting_or_turning() {
    let input = concat!(
        "{\"id\":1,\"result\":{}}\n",
        "{\"id\":2,\"error\":{\"code\":-32600,\"message\":\"no rollout found for thread id thr_missing\"}}\n"
    );
    let mut reader = Cursor::new(input.as_bytes());
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let result = drive_session(
        &mut reader,
        &session,
        &AgentRequest {
            client_message_id: None,
            prompt: "继续对话".into(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            thread_id: Some("thr_missing".into()),
            model: "gpt-test".into(),
            effort: "medium".into(),
            service_tier: None,
            permission_mode: AgentPermissionMode::Full,
            context: Default::default(),
        },
        &tx,
    );
    let sent_messages: Vec<Value> = String::from_utf8(take_session_output(&session))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let sent_methods: Vec<_> = sent_messages
        .iter()
        .filter_map(|message| message.get("method").and_then(Value::as_str))
        .collect();
    assert_eq!(
        sent_methods,
        vec!["initialize", "initialized", "thread/resume"]
    );

    let terminal = finish_prompt_session(&session, result);
    let AgentEvent::Failed(message) = terminal else {
        panic!("expected resume failure");
    };
    assert!(message.contains("thread/resume `thr_missing` 失败"));
    assert!(message.contains("no rollout found for thread id thr_missing"));
    assert!(session.writer.lock().unwrap().is_none());
    assert_eq!(session.snapshot(), (None, None, false, false, true));
    assert!(rx.try_recv().is_err());
}

#[test]
fn resume_response_requires_the_requested_thread_id() {
    for (resume_response, expected_error) in [
        (
            json!({"id":2,"result":{"thread":{}}}),
            "thread/resume 响应缺少字符串 result.thread.id",
        ),
        (
            json!({"id":2,"result":{"thread":{"id":"thr_other"}}}),
            "thread/resume 响应的 thread id `thr_other` 与请求的 `thr_existing` 不一致",
        ),
    ] {
        let input = format!("{}\n{}\n", json!({"id":1,"result":{}}), resume_response);
        let mut reader = Cursor::new(input.into_bytes());
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        let result = drive_session(
            &mut reader,
            &session,
            &AgentRequest {
                client_message_id: None,
                prompt: "继续对话".into(),
                cwd: PathBuf::from("/tmp/project"),
                project_id: None,
                thread_id: Some("thr_existing".into()),
                model: "gpt-test".into(),
                effort: "medium".into(),
                service_tier: None,
                permission_mode: AgentPermissionMode::Full,
                context: Default::default(),
            },
            &tx,
        );
        let sent_messages: Vec<Value> = String::from_utf8(take_session_output(&session))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(
            sent_messages
                .iter()
                .filter_map(|message| message.get("method").and_then(Value::as_str))
                .collect::<Vec<_>>(),
            vec!["initialize", "initialized", "thread/resume"]
        );

        let terminal = finish_prompt_session(&session, result);
        let AgentEvent::Failed(message) = terminal else {
            panic!("expected malformed resume response to fail");
        };
        assert!(
            message.contains(expected_error),
            "unexpected error: {message}"
        );
        assert!(session.writer.lock().unwrap().is_none());
        assert!(rx.try_recv().is_err());
    }
}

#[cfg(unix)]
#[test]
fn resume_failure_cleanup_reaps_the_app_server_process() {
    let process = Arc::new(AppServerProcess::new(
        Command::new("sleep").arg("30").spawn().unwrap(),
    ));
    let session = Arc::new(CodexTurnSession::new(Vec::new(), Some(process.clone())));
    let mut reader = Cursor::new(
        concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"id\":2,\"error\":{\"code\":-32600,\"message\":\"resume failed\"}}\n"
        )
        .as_bytes(),
    );
    let (tx, _rx) = async_channel::unbounded();

    let result = drive_session(
        &mut reader,
        &session,
        &AgentRequest {
            client_message_id: None,
            prompt: "继续对话".into(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            thread_id: Some("thr_existing".into()),
            model: "gpt-test".into(),
            effort: "medium".into(),
            service_tier: None,
            permission_mode: AgentPermissionMode::Full,
            context: Default::default(),
        },
        &tx,
    );
    let terminal = finish_prompt_session(&session, result);

    let AgentEvent::Failed(message) = terminal else {
        panic!("expected resume failure");
    };
    assert!(message.contains("resume failed"));
    assert!(process.is_reaped());
    assert!(process.child.lock().unwrap().is_none());
    assert!(session.writer.lock().unwrap().is_none());
}

#[test]
fn pending_interrupt_uses_the_active_thread_and_turn_and_waits_for_terminal_status() {
    let input = concat!(
        "{\"id\":1,\"result\":{}}\n",
        "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_interrupt\"}}}\n",
        "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_interrupt\",\"turn\":{\"id\":\"turn_interrupt\",\"items\":[],\"status\":\"inProgress\"}}}\n",
        "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_interrupt\"}}}\n",
        "{\"id\":4,\"result\":{}}\n",
        "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thr_interrupt\",\"turn\":{\"id\":\"turn_interrupt\",\"status\":\"interrupted\"}}}\n"
    );
    let mut reader = Cursor::new(input.as_bytes());
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();

    assert_eq!(
        session.request_interrupt_inner().unwrap(),
        AgentInterruptOutcome::Requested
    );
    assert_eq!(session.snapshot(), (None, None, true, false, false));

    let outcome = drive_session(
        &mut reader,
        &session,
        &AgentRequest {
            client_message_id: None,
            prompt: "interrupt me".into(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            thread_id: None,
            model: "gpt-test".into(),
            effort: "medium".into(),
            service_tier: None,
            permission_mode: AgentPermissionMode::Full,
            context: Default::default(),
        },
        &tx,
    )
    .unwrap();
    assert_eq!(outcome, TurnOutcome::Interrupted);
    assert_eq!(
        session.snapshot(),
        (
            Some("thr_interrupt".into()),
            Some("turn_interrupt".into()),
            true,
            true,
            true,
        )
    );

    tx.send_blocking(outcome.into_event()).unwrap();
    drop(tx);
    assert_eq!(
        rx.try_recv().unwrap(),
        AgentEvent::ThreadCreated {
            thread_id: "thr_interrupt".into()
        }
    );
    assert_eq!(rx.try_recv().unwrap(), AgentEvent::Started);
    assert_eq!(rx.try_recv().unwrap(), AgentEvent::Interrupted);
    assert!(rx.try_recv().is_err());

    let sent: Vec<Value> = String::from_utf8(take_session_output(&session))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let interrupts: Vec<_> = sent
        .iter()
        .filter(|message| message.get("method").and_then(Value::as_str) == Some("turn/interrupt"))
        .collect();
    assert_eq!(interrupts.len(), 1);
    assert_eq!(interrupts[0].get("id").and_then(Value::as_u64), Some(4));
    assert_eq!(
        interrupts[0]
            .pointer("/params/threadId")
            .and_then(Value::as_str),
        Some("thr_interrupt")
    );
    assert_eq!(
        interrupts[0]
            .pointer("/params/turnId")
            .and_then(Value::as_str),
        Some("turn_interrupt")
    );
}

#[test]
fn duplicate_and_finished_interrupts_do_not_write_again() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    session
        .activate_turn("thr_1".into(), "turn_1".into())
        .unwrap();

    assert_eq!(
        session.request_interrupt_inner().unwrap(),
        AgentInterruptOutcome::Requested
    );
    assert_eq!(
        session.request_interrupt_inner().unwrap(),
        AgentInterruptOutcome::AlreadyRequested
    );
    session.mark_terminal();
    assert_eq!(
        session.request_interrupt_inner().unwrap(),
        AgentInterruptOutcome::AlreadyFinished
    );

    let sent = String::from_utf8(take_session_output(&session)).unwrap();
    assert_eq!(sent.matches("\"method\":\"turn/interrupt\"").count(), 1);
}

#[cfg(unix)]
#[test]
fn abandoned_session_kills_and_reaps_its_child_process() {
    let process = Arc::new(AppServerProcess::new(
        Command::new("sleep").arg("30").spawn().unwrap(),
    ));
    let session = Arc::new(CodexTurnSession::new(Vec::new(), Some(process.clone())));
    let control: Arc<dyn AgentInterruptControl> = session.clone();
    let handle = AgentInterruptHandle::new(control);

    drop(handle);
    session.finish().unwrap();

    assert!(process.is_reaped());
    assert!(process.child.lock().unwrap().is_none());
    assert!(session.writer.lock().unwrap().is_none());
}

#[test]
fn model_catalog_accumulates_pages_and_maps_defaults_and_options() {
    let input = concat!(
        "{\"id\":1,\"result\":{}}\n",
        "{\"method\":\"remoteControl/status/changed\",\"params\":{\"status\":\"disabled\",\"serverName\":\"test-host\",\"installationId\":\"install-1\",\"environmentId\":null}}\n",
        "{\"method\":\"mcpServer/startupStatus/updated\",\"params\":{\"threadId\":null,\"name\":\"codex_apps\",\"status\":\"ready\",\"error\":null,\"failureReason\":null}}\n",
        "{\"id\":2,\"result\":{\"data\":[",
        "{\"id\":\"hidden\",\"model\":\"hidden\",\"displayName\":\"Hidden\",\"description\":\"hidden\",\"hidden\":true,\"supportedReasoningEfforts\":[{\"reasoningEffort\":\"low\",\"description\":\"Low\"}],\"defaultReasoningEffort\":\"low\",\"isDefault\":false},",
        "{\"id\":\"model-a\",\"model\":\"model-a-wire\",\"displayName\":\"Model A\",\"description\":\"First page\",\"hidden\":false,\"supportedReasoningEfforts\":[{\"reasoningEffort\":\"low\",\"description\":\"Low\"}],\"defaultReasoningEffort\":\"low\",\"serviceTiers\":[],\"defaultServiceTier\":null,\"isDefault\":false}],\"nextCursor\":\"page-2\"}}\n",
        "{\"id\":3,\"result\":{\"data\":[{\"id\":\"model-b\",\"model\":\"model-b-wire\",\"displayName\":\"Model B\",\"description\":\"Second page\",\"hidden\":false,\"supportedReasoningEfforts\":[{\"reasoningEffort\":\"medium\",\"description\":\"Balanced\"},{\"reasoningEffort\":\"high\",\"description\":\"Deep\"}],\"defaultReasoningEffort\":\"medium\",\"serviceTiers\":[{\"id\":\"priority\",\"name\":\"Fast\",\"description\":\"Lower latency\"}],\"defaultServiceTier\":\"priority\",\"isDefault\":true}],\"nextCursor\":null}}\n"
    );
    let mut reader = Cursor::new(input.as_bytes());
    let mut output = Vec::new();

    let catalog = drive_model_catalog(&mut reader, &mut output).unwrap();
    assert_eq!(catalog.models.len(), 2);
    assert_eq!(catalog.models[0].id, "model-a");
    assert_eq!(catalog.models[0].model, "model-a-wire");
    assert_eq!(catalog.models[1].display_name, "Model B");
    assert!(catalog.models[1].is_default);
    assert_eq!(catalog.models[1].default_reasoning_effort, "medium");
    assert_eq!(catalog.models[1].service_tiers[0].id, "priority");
    assert_eq!(
        catalog.models[1].default_service_tier.as_deref(),
        Some("priority")
    );

    let sent = String::from_utf8(output).unwrap();
    let requests: Vec<serde_json::Value> = sent
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|message: &serde_json::Value| {
            message.get("method").and_then(|value| value.as_str()) == Some("model/list")
        })
        .collect();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].pointer("/params/cursor"), Some(&json!(null)));
    assert_eq!(
        requests[0]
            .pointer("/params/limit")
            .and_then(|value| value.as_u64()),
        Some(u64::from(MODEL_LIST_PAGE_SIZE))
    );
    assert_eq!(
        requests[1]
            .pointer("/params/cursor")
            .and_then(|value| value.as_str()),
        Some("page-2")
    );
}

#[test]
fn thread_started_is_a_validated_lifecycle_notification() {
    ensure_server_method_is_defined(&json!({
        "method": "thread/started",
        "params": {
            "thread": {
                "id": "thr_1",
                "sessionId": "thr_1",
                "ephemeral": false,
                "turns": []
            }
        }
    }))
    .unwrap();

    for thread in [json!({}), json!({"id": null}), json!({"id": 7})] {
        let error = ensure_server_method_is_defined(&json!({
            "method": "thread/started",
            "params": {"thread": thread}
        }))
        .unwrap_err()
        .to_string();
        assert!(error.contains("thread/started"), "{error}");
        assert!(error.contains("params.thread.id"), "{error}");
    }
}

#[test]
fn remote_control_status_changed_is_a_validated_connection_notification() {
    ensure_server_method_is_defined(&json!({
        "method": "remoteControl/status/changed",
        "params": {
            "status": "connected",
            "serverName": "test-host",
            "installationId": "install-1",
            "environmentId": "environment-1"
        }
    }))
    .unwrap();

    for params in [
        json!({
            "status": "future-status",
            "serverName": "test-host",
            "installationId": "install-1",
            "environmentId": null
        }),
        json!({
            "status": "disabled",
            "serverName": "test-host",
            "installationId": "install-1"
        }),
    ] {
        let error = ensure_server_method_is_defined(&json!({
            "method": "remoteControl/status/changed",
            "params": params
        }))
        .unwrap_err()
        .to_string();
        assert!(error.contains("remoteControl/status/changed"), "{error}");
    }
}

#[test]
fn mcp_server_startup_status_is_validated_and_normalized() {
    let starting = json!({
        "method": "mcpServer/startupStatus/updated",
        "params": {
            "threadId": "thr_1",
            "name": "codex_apps",
            "status": "starting",
            "error": null,
            "failureReason": null
        }
    });
    ensure_server_method_is_defined(&starting).unwrap();
    assert_eq!(
        parse_agent_notification(&starting).unwrap(),
        Some(AgentEvent::McpServerStartupStatusUpdated(
            AgentMcpServerStartupStatus {
                thread_id: Some("thr_1".into()),
                name: "codex_apps".into(),
                state: AgentMcpServerStartupState::Starting,
                error: None,
                failure_reason: None,
            }
        ))
    );

    for (status, state) in [
        ("ready", AgentMcpServerStartupState::Ready),
        ("cancelled", AgentMcpServerStartupState::Cancelled),
    ] {
        assert_eq!(
            parse_agent_notification(&json!({
                "method": "mcpServer/startupStatus/updated",
                "params": {"name": "codex_apps", "status": status}
            }))
            .unwrap(),
            Some(AgentEvent::McpServerStartupStatusUpdated(
                AgentMcpServerStartupStatus {
                    thread_id: None,
                    name: "codex_apps".into(),
                    state,
                    error: None,
                    failure_reason: None,
                }
            ))
        );
    }

    let failed = json!({
        "method": "mcpServer/startupStatus/updated",
        "params": {
            "threadId": null,
            "name": "remote_tools",
            "status": "failed",
            "error": "OAuth token expired",
            "failureReason": "reauthenticationRequired"
        }
    });
    assert_eq!(
        parse_agent_notification(&failed).unwrap(),
        Some(AgentEvent::McpServerStartupStatusUpdated(
            AgentMcpServerStartupStatus {
                thread_id: None,
                name: "remote_tools".into(),
                state: AgentMcpServerStartupState::Failed,
                error: Some("OAuth token expired".into()),
                failure_reason: Some(AgentMcpServerStartupFailureReason::ReauthenticationRequired),
            }
        ))
    );

    for params in [
        json!({
            "threadId": "thr_1",
            "name": "codex_apps",
            "status": "future-status",
            "error": null,
            "failureReason": null
        }),
        json!({
            "threadId": "thr_1",
            "name": "codex_apps",
            "status": "failed",
            "error": null,
            "failureReason": "future-reason"
        }),
        json!({
            "threadId": 7,
            "name": "codex_apps",
            "status": "ready",
            "error": null,
            "failureReason": null
        }),
    ] {
        let error = ensure_server_method_is_defined(&json!({
            "method": "mcpServer/startupStatus/updated",
            "params": params
        }))
        .unwrap_err()
        .to_string();
        assert!(error.contains("mcpServer/startupStatus/updated"), "{error}");
    }
}

#[test]
fn thread_status_changed_is_validated_and_normalized() {
    let active = json!({
        "method": "thread/status/changed",
        "params": {
            "threadId": "thr_1",
            "status": {
                "type": "active",
                "activeFlags": ["waitingOnApproval", "waitingOnUserInput"]
            }
        }
    });
    ensure_server_method_is_defined(&active).unwrap();
    assert_eq!(
        parse_agent_notification(&active).unwrap(),
        Some(AgentEvent::ThreadStatusChanged(AgentThreadStatus {
            thread_id: "thr_1".into(),
            state: AgentThreadStatusState::Active {
                active_flags: vec![
                    AgentThreadActiveFlag::WaitingOnApproval,
                    AgentThreadActiveFlag::WaitingOnUserInput,
                ],
            },
        }))
    );

    for (raw_state, state) in [
        ("notLoaded", AgentThreadStatusState::NotLoaded),
        ("idle", AgentThreadStatusState::Idle),
        ("systemError", AgentThreadStatusState::SystemError),
    ] {
        assert_eq!(
            parse_agent_notification(&json!({
                "method": "thread/status/changed",
                "params": {
                    "threadId": "thr_1",
                    "status": {"type": raw_state}
                }
            }))
            .unwrap(),
            Some(AgentEvent::ThreadStatusChanged(AgentThreadStatus {
                thread_id: "thr_1".into(),
                state,
            }))
        );
    }

    for params in [
        json!({"threadId": 7, "status": {"type": "idle"}}),
        json!({"threadId": "thr_1", "status": null}),
        json!({"threadId": "thr_1", "status": {}}),
        json!({"threadId": "thr_1", "status": {"type": "future-status"}}),
        json!({"threadId": "thr_1", "status": {"type": "active"}}),
        json!({
            "threadId": "thr_1",
            "status": {"type": "active", "activeFlags": "waitingOnApproval"}
        }),
        json!({
            "threadId": "thr_1",
            "status": {"type": "active", "activeFlags": ["future-flag"]}
        }),
        json!({
            "threadId": "thr_1",
            "status": {"type": "active", "activeFlags": [7]}
        }),
    ] {
        let error = ensure_server_method_is_defined(&json!({
            "method": "thread/status/changed",
            "params": params
        }))
        .unwrap_err()
        .to_string();
        assert!(error.contains("thread/status/changed"), "{error}");
    }
}

#[test]
fn thread_token_usage_updated_is_validated_and_normalized() {
    let update = thread_token_usage_message("thr_1", "turn_1");
    ensure_server_method_is_defined(&update).unwrap();
    assert_eq!(
        parse_agent_notification(&update).unwrap(),
        Some(AgentEvent::ThreadTokenUsageUpdated(AgentThreadTokenUsage {
            thread_id: "thr_1".into(),
            turn_id: "turn_1".into(),
            total: AgentTokenUsageBreakdown {
                total_tokens: 16_221,
                input_tokens: 16_207,
                cached_input_tokens: 11_008,
                cache_write_input_tokens: 0,
                output_tokens: 14,
                reasoning_output_tokens: 0,
            },
            last: AgentTokenUsageBreakdown {
                total_tokens: 16_221,
                input_tokens: 16_207,
                cached_input_tokens: 11_008,
                cache_write_input_tokens: 0,
                output_tokens: 14,
                reasoning_output_tokens: 0,
            },
            model_context_window: Some(258_400),
        }))
    );

    let without_optional_fields = json!({
        "method": "thread/tokenUsage/updated",
        "params": {
            "threadId": "thr_1",
            "turnId": "turn_1",
            "tokenUsage": {
                "total": {
                    "totalTokens": 10,
                    "inputTokens": 8,
                    "cachedInputTokens": 2,
                    "outputTokens": 2,
                    "reasoningOutputTokens": 1
                },
                "last": {
                    "totalTokens": 4,
                    "inputTokens": 3,
                    "cachedInputTokens": 1,
                    "outputTokens": 1,
                    "reasoningOutputTokens": 0
                }
            }
        }
    });
    let Some(AgentEvent::ThreadTokenUsageUpdated(usage)) =
        parse_agent_notification(&without_optional_fields).unwrap()
    else {
        panic!("expected token usage event");
    };
    assert_eq!(usage.total.cache_write_input_tokens, 0);
    assert_eq!(usage.last.cache_write_input_tokens, 0);
    assert_eq!(usage.model_context_window, None);

    let mut explicit_null_context = without_optional_fields.clone();
    explicit_null_context["params"]["tokenUsage"]["modelContextWindow"] = Value::Null;
    let Some(AgentEvent::ThreadTokenUsageUpdated(usage)) =
        parse_agent_notification(&explicit_null_context).unwrap()
    else {
        panic!("expected token usage event");
    };
    assert_eq!(usage.model_context_window, None);

    for params in [
        json!({
            "threadId": 7,
            "turnId": "turn_1",
            "tokenUsage": update["params"]["tokenUsage"].clone()
        }),
        json!({
            "threadId": "thr_1",
            "tokenUsage": update["params"]["tokenUsage"].clone()
        }),
        json!({"threadId": "thr_1", "turnId": "turn_1", "tokenUsage": null}),
        json!({
            "threadId": "thr_1",
            "turnId": "turn_1",
            "tokenUsage": {
                "total": {},
                "last": update["params"]["tokenUsage"]["last"].clone()
            }
        }),
        json!({
            "threadId": "thr_1",
            "turnId": "turn_1",
            "tokenUsage": {
                "total": update["params"]["tokenUsage"]["total"].clone(),
                "last": {
                    "totalTokens": 4,
                    "inputTokens": 3,
                    "cachedInputTokens": 1,
                    "outputTokens": "1",
                    "reasoningOutputTokens": 0
                }
            }
        }),
        json!({
            "threadId": "thr_1",
            "turnId": "turn_1",
            "tokenUsage": {
                "total": update["params"]["tokenUsage"]["total"].clone(),
                "last": update["params"]["tokenUsage"]["last"].clone(),
                "modelContextWindow": "258400"
            }
        }),
    ] {
        let error = ensure_server_method_is_defined(&json!({
            "method": "thread/tokenUsage/updated",
            "params": params
        }))
        .unwrap_err()
        .to_string();
        assert!(error.contains("thread/tokenUsage/updated"), "{error}");
        assert!(error.contains("schema"), "{error}");
    }
}

#[test]
fn thread_token_usage_update_must_match_the_active_turn() {
    let message = thread_token_usage_message("thr_1", "turn_old");
    assert_turn_message_fails(
        &message,
        &["thread/tokenUsage/updated", "thr_1", "turn_old", "turn_1"],
    );
}

#[test]
fn account_rate_limits_updated_is_validated_and_normalized() {
    let update = account_rate_limits_message();
    ensure_server_method_is_defined(&update).unwrap();
    assert_eq!(
        parse_agent_notification(&update).unwrap(),
        Some(AgentEvent::AccountRateLimitsUpdated(
            AgentAccountRateLimits {
                limit_id: Some("codex".into()),
                limit_name: None,
                primary: Some(AgentRateLimitWindow {
                    used_percent: 15,
                    window_duration_mins: Some(10_080),
                    resets_at: Some(1_788_752_152),
                }),
                secondary: None,
                credits: Some(AgentCreditsSnapshot {
                    has_credits: false,
                    unlimited: false,
                    balance: Some("0".into()),
                }),
                individual_limit: None,
                spend_control_reached: None,
                plan_type: Some("pro".into()),
                rate_limit_reached_type: None,
            }
        ))
    );

    assert_eq!(
        parse_agent_notification(&json!({
            "method": "account/rateLimits/updated",
            "params": {"rateLimits": {}}
        }))
        .unwrap(),
        Some(AgentEvent::AccountRateLimitsUpdated(
            AgentAccountRateLimits::default()
        ))
    );

    for plan_type in [
        "free",
        "go",
        "plus",
        "pro",
        "prolite",
        "team",
        "self_serve_business_prolite",
        "self_serve_business_usage_based",
        "business",
        "ent26",
        "enterprise_cbp_automation",
        "enterprise_cbp_usage_based",
        "enterprise",
        "edu",
        "edu_plus",
        "edu_pro",
        "unknown",
    ] {
        ensure_server_method_is_defined(&json!({
            "method": "account/rateLimits/updated",
            "params": {"rateLimits": {"planType": plan_type}}
        }))
        .unwrap();
    }
    for reached_type in [
        "rate_limit_reached",
        "workspace_owner_credits_depleted",
        "workspace_member_credits_depleted",
        "workspace_owner_usage_limit_reached",
        "workspace_member_usage_limit_reached",
    ] {
        ensure_server_method_is_defined(&json!({
            "method": "account/rateLimits/updated",
            "params": {"rateLimits": {"rateLimitReachedType": reached_type}}
        }))
        .unwrap();
    }

    for params in [
        json!({}),
        json!({"rateLimits": null}),
        json!({"rateLimits": {"primary": {}}}),
        json!({"rateLimits": {"primary": {"usedPercent": 2_147_483_648_i64}}}),
        json!({"rateLimits": {"primary": {"usedPercent": 15, "resetsAt": "soon"}}}),
        json!({"rateLimits": {"credits": {"hasCredits": false}}}),
        json!({"rateLimits": {"individualLimit": {
            "limit": "100", "remainingPercent": 75, "resetsAt": 1_788_752_152_i64
        }}}),
        json!({"rateLimits": {"spendControlReached": "false"}}),
    ] {
        let error = ensure_server_method_is_defined(&json!({
            "method": "account/rateLimits/updated",
            "params": params
        }))
        .unwrap_err()
        .to_string();
        assert!(error.contains("account/rateLimits/updated"), "{error}");
        assert!(error.contains("schema"), "{error}");
    }

    for (field, value) in [
        ("planType", "future-plan"),
        ("rateLimitReachedType", "future-limit"),
    ] {
        let error = ensure_server_method_is_defined(&json!({
            "method": "account/rateLimits/updated",
            "params": {"rateLimits": {(field): value}}
        }))
        .unwrap_err()
        .to_string();
        assert!(error.contains("account/rateLimits/updated"), "{error}");
        assert!(error.contains(field), "{error}");
        assert!(error.contains(value), "{error}");
    }
}

#[test]
fn unintegrated_notifications_remain_fail_fast() {
    for method in [
        "deprecationNotice",
        "thread/goal/updated",
        "thread/goal/cleared",
        "protocol/arbitraryFutureNotification",
    ] {
        let error = ensure_server_method_is_defined(&json!({
            "method": method,
            "params": { "probe": true }
        }))
        .unwrap_err()
        .to_string();
        assert!(error.contains("未定义"), "{method}: {error}");
        assert!(error.contains(method), "{method}: {error}");
    }
}

#[test]
fn user_facing_methods_are_defined() {
    for method in [
        "turn/started",
        "error",
        "turn/completed",
        "thread/settings/updated",
        "warning",
        "configWarning",
    ] {
        ensure_server_method_is_defined(&json!({
            "method": method,
            "params": {}
        }))
        .unwrap();
    }
}

#[test]
fn user_facing_notifications_are_normalized_without_ending_the_turn() {
    let input = concat!(
        "{\"method\":\"configWarning\",\"params\":{\"summary\":\"配置值已弃用\",\"details\":\"请迁移到新键\",\"path\":\"/tmp/project/config.toml\",\"range\":{\"start\":{\"line\":8,\"column\":4},\"end\":{\"line\":8,\"column\":12}}}}\n",
        "{\"id\":1,\"result\":{}}\n",
        "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_notices\"}}}\n",
        "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_notices\",\"turn\":{\"id\":\"turn_notices\",\"items\":[],\"status\":\"inProgress\"}}}\n",
        "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_notices\"}}}\n",
        "{\"method\":\"thread/settings/updated\",\"params\":{\"threadId\":\"thr_notices\",\"threadSettings\":{\"model\":\"model-b\",\"effort\":\"high\",\"serviceTier\":\"priority\",\"cwd\":\"/tmp/project/updated\"}}}\n",
        "{\"method\":\"warning\",\"params\":{\"threadId\":null,\"message\":\"上下文窗口即将用尽\"}}\n",
        "{\"method\":\"error\",\"params\":{\"threadId\":\"thr_notices\",\"turnId\":\"turn_notices\",\"error\":{\"message\":\"连接暂时中断\",\"additionalDetails\":\"2 秒后重试\"},\"willRetry\":true}}\n",
        "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thr_notices\",\"turn\":{\"id\":\"turn_notices\",\"status\":\"completed\"}}}\n"
    );
    let mut reader = Cursor::new(input.as_bytes());
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();

    let outcome = drive_session(
        &mut reader,
        &session,
        &AgentRequest {
            client_message_id: None,
            prompt: "probe notices".into(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            thread_id: None,
            model: "model-a".into(),
            effort: "medium".into(),
            service_tier: None,
            permission_mode: AgentPermissionMode::Full,
            context: Default::default(),
        },
        &tx,
    )
    .unwrap();
    assert_eq!(outcome, TurnOutcome::Completed);
    tx.send_blocking(outcome.into_event()).unwrap();
    drop(tx);

    let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert_eq!(
        events,
        vec![
            AgentEvent::ConfigWarning(AgentConfigWarning {
                summary: "配置值已弃用".into(),
                details: Some("请迁移到新键".into()),
                path: Some("/tmp/project/config.toml".into()),
                line: Some(8),
                column: Some(4),
            }),
            AgentEvent::ThreadCreated {
                thread_id: "thr_notices".into()
            },
            AgentEvent::Started,
            AgentEvent::ThreadSettingsUpdated(AgentThreadSettings {
                model: "model-b".into(),
                effort: Some("high".into()),
                service_tier: Some("priority".into()),
                cwd: "/tmp/project/updated".into(),
                permissions: None,
            }),
            AgentEvent::Warning {
                message: "上下文窗口即将用尽".into(),
            },
            AgentEvent::Error {
                message: "连接暂时中断".into(),
                details: Some("2 秒后重试".into()),
                will_retry: true,
            },
            AgentEvent::Completed,
        ]
    );
}

#[test]
fn failed_turn_completion_is_the_terminal_event_and_keeps_error_details() {
    let input = concat!(
        "{\"id\":1,\"result\":{}}\n",
        "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_failed\"}}}\n",
        "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_failed\"}}}\n",
        "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_failed\",\"turn\":{\"id\":\"turn_failed\",\"items\":[],\"status\":\"inProgress\"}}}\n",
        "{\"method\":\"error\",\"params\":{\"threadId\":\"thr_failed\",\"turnId\":\"turn_failed\",\"error\":{\"message\":\"模型请求失败\",\"additionalDetails\":\"上游返回 503\"},\"willRetry\":false}}\n",
        "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thr_failed\",\"turn\":{\"id\":\"turn_failed\",\"status\":\"failed\",\"error\":{\"message\":\"模型请求失败\",\"additionalDetails\":\"上游返回 503\"}}}}\n"
    );
    let mut reader = Cursor::new(input.as_bytes());
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();

    let outcome = drive_session(
        &mut reader,
        &session,
        &AgentRequest {
            client_message_id: None,
            prompt: "fail".into(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            thread_id: None,
            model: "model-a".into(),
            effort: "medium".into(),
            service_tier: None,
            permission_mode: AgentPermissionMode::Full,
            context: Default::default(),
        },
        &tx,
    )
    .unwrap();
    assert_eq!(
        outcome,
        TurnOutcome::Failed("模型请求失败\n上游返回 503".into())
    );
    tx.send_blocking(outcome.into_event()).unwrap();
    drop(tx);

    let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert_eq!(
        events,
        vec![
            AgentEvent::ThreadCreated {
                thread_id: "thr_failed".into()
            },
            AgentEvent::Started,
            AgentEvent::Error {
                message: "模型请求失败".into(),
                details: Some("上游返回 503".into()),
                will_retry: false,
            },
            AgentEvent::Failed("模型请求失败\n上游返回 503".into()),
        ]
    );
    assert!(session.snapshot().4);
}

#[test]
fn non_turn_connection_does_not_silently_drop_visible_notifications() {
    let mut reader =
        Cursor::new(b"{\"method\":\"warning\",\"params\":{\"message\":\"visible warning\"}}\n");
    let mut output = Vec::new();

    let error = wait_for_response(&mut reader, &mut output, INITIALIZE_ID, None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("需要可见 UI 承接"));
    assert!(error.contains("warning"));
}

#[test]
fn account_rate_limits_update_is_validated_on_an_app_scoped_connection() {
    let input = format!(
        "{}\n{{\"id\":1,\"result\":{{}}}}\n",
        account_rate_limits_message()
    );
    let mut reader = Cursor::new(input.as_bytes());
    let mut output = Vec::new();

    let response = wait_for_response(&mut reader, &mut output, INITIALIZE_ID, None).unwrap();
    assert_eq!(response.pointer("/id"), Some(&json!(INITIALIZE_ID)));
    assert!(output.is_empty());
}

#[test]
fn unknown_method_error_contains_its_kind_name_and_params() {
    let error = ensure_server_method_is_defined(&json!({
        "method": "item/futureTool/progress",
        "params": {
            "itemId": "future_1",
            "progress": 0.5
        }
    }))
    .unwrap_err()
    .to_string();

    assert!(error.contains("未定义"));
    assert!(error.contains("通知"));
    assert!(error.contains("item/futureTool/progress"));
    assert!(error.contains("future_1"));
    assert!(error.contains("progress"));
}

#[test]
fn unknown_method_payload_is_truncated_on_a_utf8_boundary() {
    let error = ensure_server_method_is_defined(&json!({
        "method": "item/future/hugeDelta",
        "params": { "delta": "中".repeat(UNDEFINED_METHOD_PARAMS_LIMIT + 500) }
    }))
    .unwrap_err()
    .to_string();

    assert!(error.contains("item/future/hugeDelta"));
    assert!(error.ends_with('…'));
    assert!(error.chars().count() < UNDEFINED_METHOD_PARAMS_LIMIT + 100);
}

#[test]
fn handshake_wait_rejects_unknown_methods_instead_of_skipping_them() {
    let mut reader = Cursor::new(
        b"{\"method\":\"protocol/futureHandshake\",\"params\":{\"phase\":\"initialize\"}}\n",
    );
    let mut output = Vec::new();

    let error = wait_for_response(&mut reader, &mut output, INITIALIZE_ID, None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("protocol/futureHandshake"));
    assert!(error.contains("initialize"));
}

#[test]
fn unknown_server_request_is_replied_to_and_reported_locally() {
    let mut reader = Cursor::new(
        b"{\"id\":99,\"method\":\"item/futureApproval/request\",\"params\":{\"reason\":\"probe\"}}\n",
    );
    let mut output = Vec::new();

    let error = wait_for_response(&mut reader, &mut output, INITIALIZE_ID, None)
        .unwrap_err()
        .to_string();
    let response = String::from_utf8(output).unwrap();

    assert!(error.contains("请求"));
    assert!(error.contains("item/futureApproval/request"));
    assert!(response.contains("\"id\":99"));
    assert!(response.contains("\"code\":-32601"));
}

#[test]
fn command_approval_enters_live_ui_and_replies_exactly_once() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let message = command_approval_request(json!(77));

    respond_to_server_request_on_session(&session, &message, &tx).unwrap();
    assert!(take_session_output(&session).is_empty());
    let pending = session.pending_approval_snapshot();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].0, AgentServerRequestId::Number(77));
    assert_eq!(pending[0].1, message["params"]);
    assert!(!pending[0].2);

    let event = rx.try_recv().unwrap();
    let AgentEvent::CommandApprovalRequested { request, responder } = event else {
        panic!("expected command approval event");
    };
    assert_eq!(request.request_id, AgentServerRequestId::Number(77));
    assert_eq!(request.command, "git --version");
    assert!(
        request
            .available_decisions
            .contains(&AgentCommandApprovalChoice::Accept)
    );
    assert!(
        request
            .available_decisions
            .contains(&AgentCommandApprovalChoice::Decline)
    );
    assert!(
        !request
            .available_decisions
            .contains(&AgentCommandApprovalChoice::Cancel)
    );
    assert!(request.available_decisions.iter().any(|choice| matches!(
        choice,
        AgentCommandApprovalChoice::AcceptWithExecpolicyAmendment(_)
    )));

    responder
        .respond(AgentCommandApprovalChoice::Accept)
        .unwrap();
    let response: Value = serde_json::from_slice(&take_session_output(&session)).unwrap();
    assert_eq!(response, json!({"id":77,"result":{"decision":"accept"}}));
    let duplicate = responder
        .respond(AgentCommandApprovalChoice::Decline)
        .unwrap_err();
    assert!(duplicate.contains("拒绝重复 decision"));
    assert!(take_session_output(&session).is_empty());
}

#[test]
fn current_cancel_advertisement_preserves_interrupt_decision() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let message = current_command_approval_request(json!(78));

    respond_to_server_request_on_session(&session, &message, &tx).unwrap();
    let event = rx.try_recv().unwrap();
    let AgentEvent::CommandApprovalRequested { request, responder } = event else {
        panic!("expected command approval event");
    };
    assert!(
        request
            .available_decisions
            .contains(&AgentCommandApprovalChoice::Accept)
    );
    assert!(
        !request
            .available_decisions
            .contains(&AgentCommandApprovalChoice::Decline)
    );
    assert!(
        request
            .available_decisions
            .contains(&AgentCommandApprovalChoice::Cancel)
    );

    responder
        .respond(AgentCommandApprovalChoice::Cancel)
        .unwrap();
    let response: Value = serde_json::from_slice(&take_session_output(&session)).unwrap();
    assert_eq!(response, json!({"id":78,"result":{"decision":"cancel"}}));
}

#[test]
fn command_approval_preserves_string_ids_and_raw_execpolicy_decision() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    respond_to_server_request_on_session(&session, &command_approval_request(json!("77")), &tx)
        .unwrap();
    respond_to_server_request_on_session(&session, &command_approval_request(json!(77)), &tx)
        .unwrap();
    assert_eq!(session.pending_approval_snapshot().len(), 2);

    let first = rx.try_recv().unwrap();
    let AgentEvent::CommandApprovalRequested { request, responder } = first else {
        panic!("expected command approval event");
    };
    assert_eq!(
        request.request_id,
        AgentServerRequestId::String("77".into())
    );
    responder
        .respond(AgentCommandApprovalChoice::AcceptWithExecpolicyAmendment(
            vec!["git".into(), "--version".into()],
        ))
        .unwrap();
    let response: Value = serde_json::from_slice(&take_session_output(&session)).unwrap();
    assert_eq!(response["id"], json!("77"));
    assert_eq!(
        response["result"]["decision"],
        json!({"acceptWithExecpolicyAmendment":{"execpolicy_amendment":["git","--version"]}})
    );

    let second = rx.try_recv().unwrap();
    let AgentEvent::CommandApprovalRequested { request, responder } = second else {
        panic!("expected second command approval event");
    };
    assert_eq!(request.request_id, AgentServerRequestId::Number(77));
    responder
        .respond(AgentCommandApprovalChoice::Decline)
        .unwrap();
    let response: Value = serde_json::from_slice(&take_session_output(&session)).unwrap();
    assert_eq!(response, json!({"id":77,"result":{"decision":"decline"}}));
}

#[test]
fn user_input_request_preserves_all_questions_answers_and_original_id_once() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let message = user_input_request(json!("user-input-7"));
    respond_to_server_request_on_session(&session, &message, &tx).unwrap();

    let AgentEvent::UserInputRequested { request, responder } = rx.try_recv().unwrap() else {
        panic!("expected user input event");
    };
    assert_eq!(
        request.request_id,
        AgentServerRequestId::String("user-input-7".into())
    );
    assert_eq!(request.thread_id, "thr_1");
    assert_eq!(request.turn_id, "turn_1");
    assert_eq!(request.item_id, "tool_1");
    assert!(request.is_blocking);
    assert_eq!(request.auto_resolution_ms, Some(1500));
    assert_eq!(request.questions.len(), 2);
    assert_eq!(request.questions[0].header, "Color");
    assert_eq!(request.questions[0].options.len(), 2);
    assert!(request.questions[0].allows_other);
    assert!(request.questions[1].is_secret);

    let response = AgentUserInputResponse {
        answers: vec![
            AgentUserInputAnswer {
                question_id: "color".into(),
                answers: vec!["red".into(), "blue".into(), "custom shade".into()],
            },
            AgentUserInputAnswer {
                question_id: "token".into(),
                answers: vec!["super-secret-value".into()],
            },
        ],
    };
    let debug = format!("{response:?}");
    assert!(!debug.contains("super-secret-value"));
    assert!(debug.contains("<redacted>"));
    responder.respond(response).unwrap();
    let wire: Value = serde_json::from_slice(&take_session_output(&session)).unwrap();
    assert_eq!(wire["id"], json!("user-input-7"));
    assert_eq!(
        wire["result"],
        json!({
            "answers": {
                "color": {"answers": ["red", "blue", "custom shade"]},
                "token": {"answers": ["super-secret-value"]}
            }
        })
    );

    let duplicate = responder
        .respond(AgentUserInputResponse::default())
        .unwrap_err();
    assert!(duplicate.contains("拒绝重复 answers"));
    assert!(!duplicate.contains("super-secret-value"));
    assert!(take_session_output(&session).is_empty());
}

#[test]
fn current_tool_request_user_input_alias_uses_the_same_response_contract() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let mut message = user_input_request(json!(78));
    message["method"] = json!("tool/requestUserInput");
    respond_to_server_request_on_session(&session, &message, &tx).unwrap();

    let AgentEvent::UserInputRequested { request, responder } = rx.try_recv().unwrap() else {
        panic!("expected user input event");
    };
    assert_eq!(request.item_id, "tool_1");
    responder
        .respond(AgentUserInputResponse {
            answers: vec![AgentUserInputAnswer {
                question_id: "color".into(),
                answers: vec!["red".into()],
            }],
        })
        .unwrap();
    let wire: Value = serde_json::from_slice(&take_session_output(&session)).unwrap();
    assert_eq!(wire["id"], 78);
    assert_eq!(
        wire["result"]["answers"]["color"]["answers"],
        json!(["red"])
    );
}

#[test]
fn user_input_invalid_params_receive_minus_32602_without_creating_pending_ui() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let mut message = user_input_request(json!(81));
    message["params"]["questions"][0]["isSecret"] = json!("yes");

    let error = respond_to_server_request_on_session(&session, &message, &tx)
        .unwrap_err()
        .to_string();
    assert!(error.contains("isSecret"));
    assert!(rx.try_recv().is_err());
    assert!(session.pending_server_request_snapshot().is_empty());
    let wire: Value = serde_json::from_slice(&take_session_output(&session)).unwrap();
    assert_eq!(wire["id"], json!(81));
    assert_eq!(wire["error"]["code"], json!(-32602));
}

#[test]
fn permissions_approval_preserves_structured_permissions_and_maps_all_scopes() {
    for (id, choice, expected_scope, expected_permissions) in [
        (
            91,
            AgentPermissionsApprovalChoice::AllowOnce,
            "turn",
            Some(permissions_approval_request(json!(91))["params"]["permissions"].clone()),
        ),
        (
            92,
            AgentPermissionsApprovalChoice::AllowForSession,
            "session",
            Some(permissions_approval_request(json!(92))["params"]["permissions"].clone()),
        ),
        (93, AgentPermissionsApprovalChoice::Decline, "turn", None),
    ] {
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        respond_to_server_request_on_session(
            &session,
            &permissions_approval_request(json!(id)),
            &tx,
        )
        .unwrap();
        let AgentEvent::PermissionsApprovalRequested { request, responder } =
            rx.try_recv().unwrap()
        else {
            panic!("expected permissions approval event");
        };
        assert_eq!(request.cwd, "/workspace/project");
        assert_eq!(
            request.reason.as_deref(),
            Some("Read fixtures and contact the network")
        );
        assert_eq!(request.environment_id.as_deref(), Some("env_1"));
        responder.respond(choice).unwrap();
        let wire: Value = serde_json::from_slice(&take_session_output(&session)).unwrap();
        assert_eq!(wire["id"], json!(id));
        assert_eq!(wire["result"]["scope"], json!(expected_scope));
        assert!(wire["result"].get("strictAutoReview").is_none());
        match expected_permissions {
            Some(permissions) => assert_eq!(wire["result"]["permissions"], permissions),
            None => assert_eq!(wire["result"]["permissions"], json!({})),
        }
        let duplicate = responder.respond(choice).unwrap_err();
        assert!(duplicate.contains("拒绝重复 decision"));
        assert!(take_session_output(&session).is_empty());
    }
}

#[test]
fn permissions_file_network_and_mixed_profiles_are_all_parsed() {
    for (id, file_system, network) in [(94, true, false), (95, false, true), (96, true, true)] {
        let mut message = permissions_approval_request(json!(id));
        if !file_system {
            message["params"]["permissions"]["fileSystem"] = Value::Null;
        }
        if !network {
            message["params"]["permissions"]["network"] = Value::Null;
        }
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        respond_to_server_request_on_session(&session, &message, &tx).unwrap();
        let AgentEvent::PermissionsApprovalRequested { request, .. } = rx.try_recv().unwrap()
        else {
            panic!("expected permissions approval event");
        };
        assert_eq!(
            matches!(
                request.permissions.file_system,
                AgentOptionalField::Value(_)
            ),
            file_system
        );
        assert_eq!(
            matches!(request.permissions.network, AgentOptionalField::Value(_)),
            network
        );
    }
}

#[test]
fn permissions_invalid_params_receive_minus_32602() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let mut message = permissions_approval_request(json!(97));
    message["params"]["permissions"]["fileSystem"]["entries"][0]["path"]["type"] =
        json!("future_path");
    let error = respond_to_server_request_on_session(&session, &message, &tx)
        .unwrap_err()
        .to_string();
    assert!(error.contains("future_path"));
    assert!(rx.try_recv().is_err());
    let wire: Value = serde_json::from_slice(&take_session_output(&session)).unwrap();
    assert_eq!(wire["error"]["code"], json!(-32602));
}

#[test]
fn file_change_request_without_required_timestamp_is_rejected() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let error = respond_to_server_request_on_session(
        &session,
        &json!({
            "id":78,"method":"item/fileChange/requestApproval",
            "params":{"threadId":"thr_1","turnId":"turn_1","itemId":"item_1"}
        }),
        &tx,
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("startedAtMs"));
    let response: Value = serde_json::from_slice(&take_session_output(&session)).unwrap();
    assert_eq!(response["id"], 78);
    assert_eq!(response["error"]["code"], -32602);
    assert!(rx.try_recv().is_err());
}

#[test]
fn server_request_resolved_clears_only_the_matching_pending_request() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    for id in [json!(77), json!("77")] {
        respond_to_server_request_on_session(&session, &command_approval_request(id), &tx).unwrap();
        rx.try_recv().unwrap();
    }

    let resolved = json!({
        "method":"serverRequest/resolved",
        "params":{"threadId":"thr_1","requestId":77}
    });
    handle_server_request_resolved(&session, &resolved, &tx).unwrap();
    assert_eq!(session.pending_approval_snapshot().len(), 1);
    assert_eq!(
        session.pending_approval_snapshot()[0].0,
        AgentServerRequestId::String("77".into())
    );
    assert_eq!(
        rx.try_recv().unwrap(),
        AgentEvent::ServerRequestResolved {
            request: AgentServerRequestMetadata {
                request_id: AgentServerRequestId::Number(77),
                thread_id: "thr_1".into(),
                turn_id: "turn_1".into(),
                item_id: "item_1".into(),
                kind: AgentServerRequestKind::CommandApproval,
            }
        }
    );
    ensure_server_method_is_defined(&resolved).unwrap();
}

#[test]
fn all_three_server_request_kinds_follow_response_then_resolved_and_duplicate_is_idempotent() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();

    respond_to_server_request_on_session(&session, &command_approval_request(json!(101)), &tx)
        .unwrap();
    let AgentEvent::CommandApprovalRequested {
        responder: command, ..
    } = rx.try_recv().unwrap()
    else {
        panic!("expected command approval");
    };
    command.respond(AgentCommandApprovalChoice::Accept).unwrap();

    respond_to_server_request_on_session(&session, &user_input_request(json!(102)), &tx).unwrap();
    let AgentEvent::UserInputRequested {
        responder: user_input,
        ..
    } = rx.try_recv().unwrap()
    else {
        panic!("expected user input");
    };
    user_input
        .respond(AgentUserInputResponse {
            answers: vec![AgentUserInputAnswer {
                question_id: "color".into(),
                answers: vec!["red".into()],
            }],
        })
        .unwrap();

    respond_to_server_request_on_session(&session, &permissions_approval_request(json!(103)), &tx)
        .unwrap();
    let AgentEvent::PermissionsApprovalRequested {
        responder: permissions,
        ..
    } = rx.try_recv().unwrap()
    else {
        panic!("expected permissions approval");
    };
    permissions
        .respond(AgentPermissionsApprovalChoice::AllowOnce)
        .unwrap();
    take_session_output(&session);

    assert_eq!(session.pending_server_request_snapshot().len(), 3);
    for (request_id, expected_kind) in [
        (101, AgentServerRequestKind::CommandApproval),
        (102, AgentServerRequestKind::UserInput),
        (103, AgentServerRequestKind::PermissionsApproval),
    ] {
        let resolved = json!({
            "method": "serverRequest/resolved",
            "params": {"threadId": "thr_1", "requestId": request_id}
        });
        handle_server_request_resolved(&session, &resolved, &tx).unwrap();
        let AgentEvent::ServerRequestResolved { request } = rx.try_recv().unwrap() else {
            panic!("expected resolved event");
        };
        assert_eq!(request.request_id, AgentServerRequestId::Number(request_id));
        assert_eq!(request.kind, expected_kind);
        assert_eq!(request.thread_id, "thr_1");
        assert_eq!(request.turn_id, "turn_1");

        handle_server_request_resolved(&session, &resolved, &tx).unwrap();
        assert!(
            rx.try_recv().is_err(),
            "duplicate resolved must be idempotent"
        );
    }
    assert!(session.pending_server_request_snapshot().is_empty());
}

#[test]
fn resolved_rejects_wrong_thread_and_unknown_request_without_clearing_pending() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    respond_to_server_request_on_session(&session, &user_input_request(json!(111)), &tx).unwrap();
    rx.try_recv().unwrap();

    let wrong_thread = json!({
        "method": "serverRequest/resolved",
        "params": {"threadId": "thr_wrong", "requestId": 111}
    });
    let error = handle_server_request_resolved(&session, &wrong_thread, &tx)
        .unwrap_err()
        .to_string();
    assert!(error.contains("不一致"));
    assert_eq!(session.pending_server_request_snapshot().len(), 1);

    let unknown = json!({
        "method": "serverRequest/resolved",
        "params": {"threadId": "thr_1", "requestId": 999}
    });
    let error = handle_server_request_resolved(&session, &unknown, &tx)
        .unwrap_err()
        .to_string();
    assert!(error.contains("未知 request"));
    assert_eq!(session.pending_server_request_snapshot().len(), 1);
}

#[test]
fn mismatched_server_request_turn_returns_minus_32602_and_no_ui_event() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let mut message = user_input_request(json!(112));
    message["params"]["turnId"] = json!("turn_wrong");
    let mut streamed_text = false;
    let error = super::process_turn_message(
        &session,
        &message,
        "thr_1",
        "turn_1",
        &tx,
        &mut streamed_text,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("属于其他 turn"));
    assert!(rx.try_recv().is_err());
    assert!(session.pending_server_request_snapshot().is_empty());
    let wire: Value = serde_json::from_slice(&take_session_output(&session)).unwrap();
    assert_eq!(wire["id"], json!(112));
    assert_eq!(wire["error"]["code"], json!(-32602));
}

#[test]
fn turn_end_drains_all_pending_responders_and_reports_visible_terminal_states() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    respond_to_server_request_on_session(&session, &command_approval_request(json!(121)), &tx)
        .unwrap();
    let AgentEvent::CommandApprovalRequested {
        responder: command, ..
    } = rx.try_recv().unwrap()
    else {
        panic!("expected command approval");
    };
    respond_to_server_request_on_session(&session, &user_input_request(json!(122)), &tx).unwrap();
    let AgentEvent::UserInputRequested {
        responder: user_input,
        ..
    } = rx.try_recv().unwrap()
    else {
        panic!("expected user input");
    };
    respond_to_server_request_on_session(&session, &permissions_approval_request(json!(123)), &tx)
        .unwrap();
    let AgentEvent::PermissionsApprovalRequested {
        responder: permissions,
        ..
    } = rx.try_recv().unwrap()
    else {
        panic!("expected permissions approval");
    };

    cleanup_pending_server_requests(&session, &Ok(TurnOutcome::Interrupted), &tx).unwrap();
    let mut kinds = HashSet::new();
    for _ in 0..3 {
        let AgentEvent::ServerRequestFailed {
            request,
            kind,
            message,
        } = rx.try_recv().unwrap()
        else {
            panic!("expected pending cleanup event");
        };
        assert_eq!(kind, AgentServerRequestFailureKind::Cancelled);
        assert!(message.contains("已取消"));
        kinds.insert(request.kind);
    }
    assert_eq!(kinds.len(), 3);
    assert!(session.pending_server_request_snapshot().is_empty());
    assert!(
        command
            .respond(AgentCommandApprovalChoice::Decline)
            .is_err()
    );
    assert!(
        user_input
            .respond(AgentUserInputResponse::default())
            .is_err()
    );
    assert!(
        permissions
            .respond(AgentPermissionsApprovalChoice::Decline)
            .is_err()
    );
}

#[test]
fn normal_completion_with_unresolved_request_is_a_protocol_consistency_error() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    respond_to_server_request_on_session(&session, &user_input_request(json!(131)), &tx).unwrap();
    rx.try_recv().unwrap();
    let error = cleanup_pending_server_requests(&session, &Ok(TurnOutcome::Completed), &tx)
        .unwrap_err()
        .to_string();
    assert!(error.contains("未 resolved"));
    let AgentEvent::ServerRequestFailed { kind, .. } = rx.try_recv().unwrap() else {
        panic!("expected cleanup failure event");
    };
    assert_eq!(kind, AgentServerRequestFailureKind::Failed);
}

#[test]
fn closed_writer_and_failed_write_are_explicit_and_still_one_shot() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    respond_to_server_request_on_session(&session, &user_input_request(json!(141)), &tx).unwrap();
    let AgentEvent::UserInputRequested { responder, .. } = rx.try_recv().unwrap() else {
        panic!("expected user input");
    };
    session.close_writer();
    let error = responder
        .respond(AgentUserInputResponse::default())
        .unwrap_err();
    assert!(error.contains("连接已经关闭"));
    let duplicate = responder
        .respond(AgentUserInputResponse::default())
        .unwrap_err();
    assert!(duplicate.contains("拒绝重复 answers"));

    let session = Arc::new(CodexTurnSession::new(FailingWriter, None));
    let (tx, rx) = async_channel::unbounded();
    respond_to_server_request_on_session(&session, &user_input_request(json!(142)), &tx).unwrap();
    let AgentEvent::UserInputRequested { responder, .. } = rx.try_recv().unwrap() else {
        panic!("expected user input");
    };
    let failure = responder
        .respond(AgentUserInputResponse {
            answers: vec![AgentUserInputAnswer {
                question_id: "token".into(),
                answers: vec!["never-log-this-secret".into()],
            }],
        })
        .unwrap_err();
    assert!(failure.contains("fixture JSON-RPC write failure"));
    assert!(!failure.contains("never-log-this-secret"));
    assert!(
        responder
            .respond(AgentUserInputResponse::default())
            .unwrap_err()
            .contains("拒绝重复 answers")
    );
}

#[test]
#[ignore = "requires a logged-in local Codex CLI and makes one model request"]
fn real_cli_safe_network_command_accept_once_round_trip() {
    let catalog = run_model_catalog_process().unwrap();
    let model = catalog
        .models
        .iter()
        .find(|model| model.is_default)
        .unwrap_or(&catalog.models[0]);
    let run = CodexAppServerBackend::new().run_prompt(AgentRequest {
        client_message_id: None,
        prompt: "Use the shell to run exactly `curl -I https://example.com` and no other command. Request approval for network access, then wait for my decision.".into(),
        cwd: std::env::current_dir().unwrap(),
        project_id: None,
        thread_id: None,
        model: model.model.clone(),
        effort: model.default_reasoning_effort.clone(),
        service_tier: model.default_service_tier.clone(),
        permission_mode: AgentPermissionMode::Request,
        context: Default::default(),
    });
    let (events, _interrupt) = run.into_parts();
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut approval_id = None;
    let mut resolved = false;
    let mut completed = false;
    while Instant::now() < deadline && !completed {
        match events.try_recv() {
            Ok(AgentEvent::CommandApprovalRequested { request, responder }) => {
                eprintln!("real command approval: {request:#?}");
                assert!(
                    request
                        .available_decisions
                        .contains(&AgentCommandApprovalChoice::Accept)
                );
                approval_id = Some(request.request_id);
                responder
                    .respond(AgentCommandApprovalChoice::Accept)
                    .unwrap();
            }
            Ok(AgentEvent::ServerRequestResolved { request }) => {
                assert_eq!(Some(&request.request_id), approval_id.as_ref());
                resolved = true;
            }
            Ok(AgentEvent::Completed) => completed = true,
            Ok(AgentEvent::Failed(error)) => panic!("real CLI turn failed: {error}"),
            Ok(_) | Err(async_channel::TryRecvError::Empty) => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(async_channel::TryRecvError::Closed) => break,
        }
    }
    assert!(
        approval_id.is_some(),
        "real CLI did not request command approval"
    );
    assert!(resolved, "real CLI did not emit serverRequest/resolved");
    assert!(completed, "real CLI turn did not complete after accept");
}

#[test]
fn file_change_item_patch_and_turn_diff_emit_typed_events() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let mut streamed_text = false;
    let change = json!({
        "path": "/tmp/example.txt",
        "kind": { "type": "update", "move_path": null },
        "diff": "@@ -1 +1 @@\n-old\n+new\n"
    });

    for (method, status, expected_status) in [
        (
            "item/started",
            "inProgress",
            AgentFileChangeStatus::InProgress,
        ),
        (
            "item/completed",
            "completed",
            AgentFileChangeStatus::Completed,
        ),
    ] {
        let message = turn_item_message(
            method,
            json!({
                "type": "fileChange",
                "id": "file_1",
                "status": status,
                "changes": [change.clone()]
            }),
        );
        assert_eq!(
            super::process_turn_message(
                &session,
                &message,
                "thr_1",
                "turn_1",
                &tx,
                &mut streamed_text,
            )
            .unwrap(),
            None
        );
        let AgentEvent::FileChangeUpdated(file_change) = rx.try_recv().unwrap() else {
            panic!("expected file change event");
        };
        assert_eq!(file_change.id, "file_1");
        assert_eq!(file_change.status, expected_status);
        assert_eq!(file_change.changes.len(), 1);
    }

    let patch = json!({
        "method": "item/fileChange/patchUpdated",
        "params": {
            "threadId": "thr_1",
            "turnId": "turn_1",
            "itemId": "file_1",
            "changes": [change]
        }
    });
    super::process_turn_message(&session, &patch, "thr_1", "turn_1", &tx, &mut streamed_text)
        .unwrap();
    assert!(matches!(
        rx.try_recv().unwrap(),
        AgentEvent::FileChangePatchUpdated { item_id, changes }
            if item_id == "file_1" && changes.len() == 1
    ));

    let turn_diff = json!({
        "method": "turn/diff/updated",
        "params": {
            "threadId": "thr_1",
            "turnId": "turn_1",
            "diff": "diff --git a/example.txt b/example.txt\n@@ -1 +1 @@\n-old\n+new\n"
        }
    });
    super::process_turn_message(
        &session,
        &turn_diff,
        "thr_1",
        "turn_1",
        &tx,
        &mut streamed_text,
    )
    .unwrap();
    assert!(matches!(
        rx.try_recv().unwrap(),
        AgentEvent::TurnDiffUpdated { diff } if diff.contains("example.txt")
    ));
}

#[test]
fn future_item_type_fails_fast_for_started_and_completed() {
    for method in ["item/started", "item/completed"] {
        let message = turn_item_message(
            method,
            json!({"type": "futureItem", "id": "future_1", "payload": "probe"}),
        );
        assert_turn_message_fails(
            &message,
            &[method, "futureItem", "future_1", "thr_1", "turn_1", "probe"],
        );
    }
}

#[test]
fn item_started_user_message_is_validated_without_duplicate_ui_event() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let mut streamed_text = false;
    let message = json!({
        "method": "item/started",
        "params": {
            "item": {
                "type": "userMessage",
                "id": "user_1",
                "clientId": null,
                "content": [{
                    "type": "text",
                    "text": "hello",
                    "text_elements": []
                }]
            },
            "threadId": "thr_1",
            "turnId": "turn_1",
            "startedAtMs": 1
        },
        "emittedAtMs": 1
    });

    let outcome = super::process_turn_message(
        &session,
        &message,
        "thr_1",
        "turn_1",
        &tx,
        &mut streamed_text,
    )
    .unwrap();

    assert_eq!(outcome, None);
    assert!(!streamed_text);
    assert!(matches!(
        rx.try_recv().unwrap(),
        AgentEvent::UserMessage { .. }
    ));
    super::process_turn_message(
        &session,
        &message,
        "thr_1",
        "turn_1",
        &tx,
        &mut streamed_text,
    )
    .unwrap();
    assert!(
        rx.try_recv().is_err(),
        "an identical event is emitted only once"
    );
}

#[test]
fn user_message_item_lifecycle_schema_errors_fail_fast() {
    let cases = [
        (json!({"type":"userMessage","content":[]}), "item.id"),
        (json!({"type":"userMessage","id":"user_1"}), "item.content"),
        (
            json!({"type":"userMessage","id":"user_1","clientId":1,"content":[]}),
            "item.clientId",
        ),
        (
            json!({"type":"userMessage","id":"user_1","content":["hello"]}),
            "content[0]",
        ),
        (
            json!({"type":"userMessage","id":"user_1","content":[{"text":"hello"}]}),
            "content[0] item.type",
        ),
        (
            json!({"type":"userMessage","id":"user_1","content":[{"type":"text"}]}),
            "content[0] item.text",
        ),
        (
            json!({"type":"userMessage","id":"user_1","content":[{"type":"text","text":"hello","text_elements":1}]}),
            "text_elements",
        ),
        (
            json!({"type":"userMessage","id":"user_1","content":[{"type":"localImage"}]}),
            "item.path",
        ),
        (
            json!({"type":"userMessage","id":"user_1","content":[{"type":"image","url":"https://example.com/image.png","detail":"medium"}]}),
            "item.content[0].detail",
        ),
        (
            json!({"type":"userMessage","id":"user_1","content":[{"type":"futureInput","value":"probe"}]}),
            "不在当前协议 schema",
        ),
    ];

    for method in ["item/started", "item/completed"] {
        for (item, expected) in &cases {
            let message = turn_item_message(method, item.clone());
            assert_turn_message_fails(&message, &[method, "userMessage", *expected]);
        }
    }
}

#[test]
fn user_message_attachment_lifecycle_matches_generated_schema() {
    let content = json!([
        {"type":"text","text":"附件 + \\*\\*Markdown\\*\\* + 中English\n","text_elements":[]},
        {"type":"image","url":"https://example.com/image.png","detail":"high"},
        {"type":"localImage","path":"/tmp/capture.png","detail":null},
        {"type":"audio","url":"data:audio/wav;base64,AA=="},
        {"type":"localAudio","path":"/tmp/capture.wav"},
        {"type":"skill","name":"example","path":"/tmp/example/SKILL.md"},
        {"type":"mention","name":"source.rs","path":"/tmp/source.rs"}
    ]);

    for method in ["item/started", "item/completed"] {
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        let mut streamed_text = false;
        let message = turn_item_message(
            method,
            json!({
                "type": "userMessage",
                "id": "user_attachment_1",
                "clientId": null,
                "content": content.clone()
            }),
        );

        assert_eq!(
            super::process_turn_message(
                &session,
                &message,
                "thr_1",
                "turn_1",
                &tx,
                &mut streamed_text,
            )
            .unwrap(),
            None
        );
        assert!(matches!(
            rx.try_recv().unwrap(),
            AgentEvent::UserMessage { .. }
        ));
        super::process_turn_message(
            &session,
            &message,
            "thr_1",
            "turn_1",
            &tx,
            &mut streamed_text,
        )
        .unwrap();
        assert!(
            rx.try_recv().is_err(),
            "an identical event is emitted only once"
        );
    }
}

#[test]
fn item_completed_user_message_is_validated_without_duplicate_ui_event() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let mut streamed_text = false;
    let message = turn_item_message(
        "item/completed",
        json!({
            "type": "userMessage",
            "id": "user_1",
            "clientId": null,
            "content": [{"type":"text","text":"hello","text_elements":[]}]
        }),
    );

    let outcome = super::process_turn_message(
        &session,
        &message,
        "thr_1",
        "turn_1",
        &tx,
        &mut streamed_text,
    )
    .unwrap();

    assert_eq!(outcome, None);
    assert!(!streamed_text);
    assert!(matches!(
        rx.try_recv().unwrap(),
        AgentEvent::UserMessage { .. }
    ));
    super::process_turn_message(
        &session,
        &message,
        "thr_1",
        "turn_1",
        &tx,
        &mut streamed_text,
    )
    .unwrap();
    assert!(
        rx.try_recv().is_err(),
        "an identical event is emitted only once"
    );
}

#[test]
fn reasoning_item_lifecycle_and_all_deltas_map_to_agent_events() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let mut streamed_text = false;
    let messages = [
        turn_item_message(
            "item/started",
            json!({
                "type": "reasoning",
                "id": "reasoning_1",
                "summary": ["Plan"],
                "content": []
            }),
        ),
        json!({
            "method": "item/reasoning/summaryPartAdded",
            "params": {
                "threadId": "thr_1",
                "turnId": "turn_1",
                "itemId": "reasoning_1",
                "summaryIndex": 1
            }
        }),
        json!({
            "method": "item/reasoning/summaryTextDelta",
            "params": {
                "threadId": "thr_1",
                "turnId": "turn_1",
                "itemId": "reasoning_1",
                "summaryIndex": 1,
                "delta": "Inspect repo"
            }
        }),
        json!({
            "method": "item/reasoning/textDelta",
            "params": {
                "threadId": "thr_1",
                "turnId": "turn_1",
                "itemId": "reasoning_1",
                "contentIndex": 0,
                "delta": "raw reasoning"
            }
        }),
        turn_item_message(
            "item/completed",
            json!({
                "type": "reasoning",
                "id": "reasoning_1",
                "summary": ["Plan", "Inspect repo"],
                "content": ["raw reasoning"]
            }),
        ),
    ];

    for message in messages {
        assert_eq!(
            super::process_turn_message(
                &session,
                &message,
                "thr_1",
                "turn_1",
                &tx,
                &mut streamed_text,
            )
            .unwrap(),
            None
        );
    }
    drop(tx);

    let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert_eq!(
        events,
        vec![
            AgentEvent::ReasoningStarted {
                reasoning: AgentReasoning {
                    id: "reasoning_1".into(),
                    summary: vec!["Plan".into()],
                    content: vec![],
                },
                started_at_ms: 1_000,
            },
            AgentEvent::ReasoningSummaryPartAdded {
                item_id: "reasoning_1".into(),
                summary_index: 1,
            },
            AgentEvent::ReasoningSummaryTextDelta {
                item_id: "reasoning_1".into(),
                summary_index: 1,
                delta: "Inspect repo".into(),
            },
            AgentEvent::ReasoningTextDelta {
                item_id: "reasoning_1".into(),
                content_index: 0,
                delta: "raw reasoning".into(),
            },
            AgentEvent::ReasoningCompleted {
                reasoning: AgentReasoning {
                    id: "reasoning_1".into(),
                    summary: vec!["Plan".into(), "Inspect repo".into()],
                    content: vec!["raw reasoning".into()],
                },
                completed_at_ms: 2_250,
            },
        ]
    );
    assert!(!streamed_text);
}

#[test]
fn terminal_interaction_maps_poll_and_redacts_stdin_content() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let mut streamed_text = false;
    for (process_id, stdin) in [("95225", ""), ("95225", "super-secret\n")] {
        let message = json!({
            "method": "item/commandExecution/terminalInteraction",
            "params": {
                "threadId": "thr_1",
                "turnId": "turn_1",
                "itemId": "exec_1",
                "processId": process_id,
                "stdin": stdin
            }
        });
        assert_eq!(
            super::process_turn_message(
                &session,
                &message,
                "thr_1",
                "turn_1",
                &tx,
                &mut streamed_text,
            )
            .unwrap(),
            None
        );
    }

    assert_eq!(
        rx.try_recv().unwrap(),
        AgentEvent::CommandTerminalInteraction {
            item_id: "exec_1".into(),
            process_id: "95225".into(),
            wrote_stdin: false,
        }
    );
    assert_eq!(
        rx.try_recv().unwrap(),
        AgentEvent::CommandTerminalInteraction {
            item_id: "exec_1".into(),
            process_id: "95225".into(),
            wrote_stdin: true,
        }
    );
    assert!(rx.try_recv().is_err());
    assert!(!streamed_text);

    for (field, invalid) in [
        ("itemId", json!(1)),
        ("processId", json!(null)),
        ("stdin", json!([])),
    ] {
        let mut message = json!({
            "method": "item/commandExecution/terminalInteraction",
            "params": {
                "threadId": "thr_1",
                "turnId": "turn_1",
                "itemId": "exec_1",
                "processId": "95225",
                "stdin": ""
            }
        });
        message["params"][field] = invalid;
        assert_turn_message_fails(&message, &[field, "必须是字符串"]);
    }
}

#[test]
fn reasoning_schema_errors_fail_fast() {
    for (item, expected) in [
        (json!({"type":"reasoning"}), "item.id"),
        (
            json!({"type":"reasoning","id":"reasoning_1","summary":{}}),
            "item.summary",
        ),
        (
            json!({"type":"reasoning","id":"reasoning_1","summary":[1]}),
            "item.summary[0]",
        ),
        (
            json!({"type":"reasoning","id":"reasoning_1","content":[1]}),
            "item.content[0]",
        ),
    ] {
        assert_turn_message_fails(
            &turn_item_message("item/started", item),
            &["item/started", "reasoning", expected],
        );
    }

    let mut missing_timestamp = turn_item_message(
        "item/started",
        json!({"type":"reasoning","id":"reasoning_1"}),
    );
    missing_timestamp["params"]
        .as_object_mut()
        .unwrap()
        .remove("startedAtMs");
    assert_turn_message_fails(&missing_timestamp, &["startedAtMs"]);

    for (message, expected) in [
        (
            json!({"method":"item/reasoning/summaryPartAdded","params":{"threadId":"thr_1","turnId":"turn_1","itemId":"reasoning_1","summaryIndex":-1}}),
            "非负索引",
        ),
        (
            json!({"method":"item/reasoning/summaryTextDelta","params":{"threadId":"thr_1","turnId":"turn_1","itemId":"reasoning_1","summaryIndex":0}}),
            "params.delta",
        ),
        (
            json!({"method":"item/reasoning/textDelta","params":{"threadId":"thr_1","turnId":"turn_1","itemId":"reasoning_1","contentIndex":"0","delta":"text"}}),
            "params.contentIndex",
        ),
    ] {
        assert_turn_message_fails(&message, &[expected]);
    }
}

#[test]
fn image_view_started_and_completed_map_to_the_same_agent_item() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let mut streamed_text = false;
    for method in ["item/started", "item/completed"] {
        let message = turn_item_message(
            method,
            json!({
                "type": "imageView",
                "id": "image_1",
                "path": "/tmp/reference.png"
            }),
        );
        assert_eq!(
            super::process_turn_message(
                &session,
                &message,
                "thr_1",
                "turn_1",
                &tx,
                &mut streamed_text,
            )
            .unwrap(),
            None
        );
    }
    drop(tx);
    assert_eq!(
        rx.try_recv().unwrap(),
        AgentEvent::ImageViewed(AgentImageView {
            id: "image_1".into(),
            path: PathBuf::from("/tmp/reference.png"),
        })
    );
    assert_eq!(
        rx.try_recv().unwrap(),
        AgentEvent::ImageViewed(AgentImageView {
            id: "image_1".into(),
            path: PathBuf::from("/tmp/reference.png"),
        })
    );
    assert!(rx.try_recv().is_err());

    for (item, expected) in [
        (json!({"type":"imageView","path":"/tmp/a.png"}), "item.id"),
        (json!({"type":"imageView","id":"image_1"}), "item.path"),
        (
            json!({"type":"imageView","id":"image_1","path":1}),
            "item.path",
        ),
    ] {
        assert_turn_message_fails(
            &turn_item_message("item/started", item),
            &["imageView", expected],
        );
    }
}

#[test]
fn image_generation_started_completed_and_failed_map_to_canonical_items() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let mut streamed_text = false;
    let started = turn_item_message(
        "item/started",
        json!({
            "type": "imageGeneration",
            "id": "image_generation_live",
            "status": "in_progress",
            "revisedPrompt": null,
            "result": "",
            "transparentBackground": null,
            "failure": null
        }),
    );
    super::process_turn_message(
        &session,
        &started,
        "thr_1",
        "turn_1",
        &tx,
        &mut streamed_text,
    )
    .unwrap();

    let mut png_header = b"\x89PNG\r\n\x1a\n".to_vec();
    png_header.extend_from_slice(&[0, 0, 0, 13, b'I', b'H', b'D', b'R']);
    png_header.extend_from_slice(&1024u32.to_be_bytes());
    png_header.extend_from_slice(&768u32.to_be_bytes());
    let completed = turn_item_message(
        "item/completed",
        json!({
            "type": "imageGeneration",
            "id": "image_generation_live",
            "status": "completed",
            "revisedPrompt": "a red paper airplane",
            "result": base64::engine::general_purpose::STANDARD.encode(&png_header),
            "transparentBackground": false,
            "failure": null,
            "savedPath": null
        }),
    );
    super::process_turn_message(
        &session,
        &completed,
        "thr_1",
        "turn_1",
        &tx,
        &mut streamed_text,
    )
    .unwrap();

    let failed = turn_item_message(
        "item/completed",
        json!({
            "type": "imageGeneration",
            "id": "image_generation_failed",
            "status": "failed",
            "revisedPrompt": null,
            "result": "",
            "transparentBackground": null,
            "failure": {
                "type": "usageLimitExceeded",
                "limitId": "image_generation",
                "resetsAt": 1788566400
            }
        }),
    );
    super::process_turn_message(
        &session,
        &failed,
        "thr_1",
        "turn_1",
        &tx,
        &mut streamed_text,
    )
    .unwrap();
    drop(tx);

    let AgentEvent::ImageGenerationUpdated(started) = rx.try_recv().unwrap() else {
        panic!("expected started image generation");
    };
    assert_eq!(started.status, AgentImageGenerationStatus::InProgress);
    assert!(started.path.is_none());

    let AgentEvent::ImageGenerationUpdated(completed) = rx.try_recv().unwrap() else {
        panic!("expected completed image generation");
    };
    assert_eq!(completed.status, AgentImageGenerationStatus::Completed);
    assert_eq!(completed.dimensions, Some((1024, 768)));
    assert_eq!(
        completed.revised_prompt.as_deref(),
        Some("a red paper airplane")
    );
    let materialized = completed
        .path
        .expect("base64 result should be materialized");
    assert!(materialized.is_file());
    std::fs::remove_file(materialized).unwrap();

    let AgentEvent::ImageGenerationUpdated(failed) = rx.try_recv().unwrap() else {
        panic!("expected failed image generation");
    };
    assert_eq!(failed.status, AgentImageGenerationStatus::Failed);
    assert!(matches!(
        failed.failure,
        Some(AgentImageGenerationFailure::UsageLimitExceeded {
            ref limit_id,
            resets_at: Some(1788566400)
        }) if limit_id == "image_generation"
    ));
    assert!(rx.try_recv().is_err());
}

#[test]
fn image_generation_current_schema_fails_fast_but_history_aliases_are_compatible() {
    for (item, expected) in [
        (
            json!({"type":"imageGeneration","id":"image_1","status":"completed"}),
            "item.result",
        ),
        (
            json!({"type":"imageGeneration","id":"image_1","status":"done","result":""}),
            "item.status",
        ),
        (
            json!({"type":"imageGeneration","id":"image_1","status":"failed","result":"","failure":{"type":"usageLimitExceeded","limitId":1}}),
            "failure.limitId",
        ),
    ] {
        assert_turn_message_fails(
            &turn_item_message("item/completed", item),
            &["imageGeneration", expected],
        );
    }

    let legacy = json!({
        "type": "image_generation",
        "id": "legacy_image",
        "status": "inProgress",
        "revised_prompt": "legacy prompt",
        "transparent_background": true,
        "saved_path": null,
        "failure": null
    });
    let parsed = super::parse_image_generation(legacy.as_object().unwrap(), true).unwrap();
    assert_eq!(parsed.status, AgentImageGenerationStatus::InProgress);
    assert_eq!(parsed.revised_prompt.as_deref(), Some("legacy prompt"));
    assert_eq!(parsed.transparent_background, Some(true));
}

#[test]
fn context_compaction_started_and_completed_map_to_the_same_agent_item() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let mut streamed_text = false;
    for method in ["item/started", "item/completed"] {
        super::process_turn_message(
            &session,
            &turn_item_message(
                method,
                json!({"type": "contextCompaction", "id": "compact_1"}),
            ),
            "thr_1",
            "turn_1",
            &tx,
            &mut streamed_text,
        )
        .unwrap();
    }
    drop(tx);
    assert_eq!(
        rx.try_recv().unwrap(),
        AgentEvent::ContextCompactionUpdated(crate::agent::AgentContextCompaction {
            id: "compact_1".into(),
            completed: false,
        })
    );
    assert_eq!(
        rx.try_recv().unwrap(),
        AgentEvent::ContextCompactionUpdated(crate::agent::AgentContextCompaction {
            id: "compact_1".into(),
            completed: true,
        })
    );
    assert!(rx.try_recv().is_err());

    for item in [
        json!({"type": "contextCompaction"}),
        json!({"type": "contextCompaction", "id": 1}),
    ] {
        assert_turn_message_fails(
            &turn_item_message("item/started", item),
            &["contextCompaction", "item.id"],
        );
    }
}

#[test]
fn collaboration_lifecycle_preserves_parallel_state_and_terminal_failure() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let mut streamed_text = false;
    let started = json!({
        "type": "collabAgentToolCall",
        "id": "collab_1",
        "tool": "spawnAgent",
        "status": "inProgress",
        "senderThreadId": "thr_1",
        "receiverThreadIds": ["agent_a", "agent_b"],
        "agentsStates": {
            "agent_a": {"status": "running", "message": null},
            "agent_b": {"status": "pendingInit"}
        },
        "prompt": "Inspect in parallel",
        "model": "gpt-test",
        "reasoningEffort": "high"
    });
    let completed = json!({
        "type": "collabAgentToolCall",
        "id": "collab_1",
        "tool": "spawnAgent",
        "status": "failed",
        "senderThreadId": "thr_1",
        "receiverThreadIds": ["agent_a", "agent_b"],
        "agentsStates": {
            "agent_a": {"status": "completed", "message": "done"},
            "agent_b": {"status": "errored", "message": "fixture failure"}
        },
        "prompt": "Inspect in parallel",
        "model": "gpt-test",
        "reasoningEffort": "high"
    });

    for (method, item) in [("item/started", started), ("item/completed", completed)] {
        assert_eq!(
            super::process_turn_message(
                &session,
                &turn_item_message(method, item),
                "thr_1",
                "turn_1",
                &tx,
                &mut streamed_text,
            )
            .unwrap(),
            None
        );
    }

    let AgentEvent::CollaborationUpdated(started) = rx.try_recv().unwrap() else {
        panic!("expected started collaboration update");
    };
    assert_eq!(
        started.status,
        crate::agent::AgentCollaborationStatus::InProgress
    );
    assert_eq!(started.receiver_thread_ids, ["agent_a", "agent_b"]);
    assert_eq!(
        started.agents_states["agent_b"].status,
        crate::agent::AgentCollaboratorStatus::PendingInit
    );
    assert_eq!(started.prompt.as_deref(), Some("Inspect in parallel"));

    let AgentEvent::CollaborationUpdated(completed) = rx.try_recv().unwrap() else {
        panic!("expected completed collaboration update");
    };
    assert_eq!(
        completed.status,
        crate::agent::AgentCollaborationStatus::Failed
    );
    assert_eq!(
        completed.agents_states["agent_a"].status,
        crate::agent::AgentCollaboratorStatus::Completed
    );
    assert_eq!(
        completed.agents_states["agent_b"].message.as_deref(),
        Some("fixture failure")
    );
    assert!(rx.try_recv().is_err());
    assert!(!streamed_text);
}

#[test]
fn public_collab_tool_call_maps_single_target_metadata_and_lifecycle() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let mut streamed_text = false;
    for (method, item) in [
        (
            "item/started",
            json!({
                "type": "collabToolCall",
                "id": "public_collab_1",
                "tool": "spawnAgent",
                "status": "inProgress",
                "senderThreadId": "thr_1",
                "newThreadId": "agent_a",
                "newAgentNickname": "Protocol auditor",
                "agentStatus": "running",
                "prompt": "Inspect protocol evidence"
            }),
        ),
        (
            "item/completed",
            json!({
                "type": "collabToolCall",
                "id": "public_collab_1",
                "tool": "spawnAgent",
                "status": "completed",
                "senderThreadId": "thr_1",
                "receiverThreadId": "agent_a",
                "agentName": "Protocol auditor",
                "agentStatus": {"status": "completed", "message": "done"},
                "prompt": "Inspect protocol evidence"
            }),
        ),
    ] {
        assert_eq!(
            super::process_turn_message(
                &session,
                &turn_item_message(method, item),
                "thr_1",
                "turn_1",
                &tx,
                &mut streamed_text,
            )
            .unwrap(),
            None
        );
    }

    let AgentEvent::CollaborationUpdated(started) = rx.try_recv().unwrap() else {
        panic!("expected public collaboration started update");
    };
    assert_eq!(started.receiver_thread_ids, ["agent_a"]);
    assert_eq!(
        started.agents_states["agent_a"].name.as_deref(),
        Some("Protocol auditor")
    );
    assert_eq!(
        started.status,
        crate::agent::AgentCollaborationStatus::InProgress
    );

    let AgentEvent::CollaborationUpdated(completed) = rx.try_recv().unwrap() else {
        panic!("expected public collaboration completed update");
    };
    assert_eq!(completed.id, "public_collab_1");
    assert_eq!(
        completed.status,
        crate::agent::AgentCollaborationStatus::Completed
    );
    assert_eq!(
        completed.agents_states["agent_a"].status,
        crate::agent::AgentCollaboratorStatus::Completed
    );
    assert_eq!(
        completed.agents_states["agent_a"].message.as_deref(),
        Some("done")
    );
    assert!(rx.try_recv().is_err());
    assert!(!streamed_text);
}

#[test]
fn every_collaboration_tool_and_legacy_kind_is_typed() {
    for tool in [
        "spawnAgent",
        "sendInput",
        "resumeAgent",
        "wait",
        "closeAgent",
        "sendMessage",
        "followupTask",
        "interruptAgent",
        "listAgents",
    ] {
        let item = json!({
            "type": "collabAgentToolCall",
            "id": format!("{tool}_1"),
            "tool": tool,
            "status": "completed",
            "senderThreadId": "thr_1",
            "receiverThreadIds": [],
            "agentsStates": {},
            "prompt": null,
            "model": null,
            "reasoningEffort": null
        });
        let parsed = super::parse_collaboration(item.as_object().unwrap()).unwrap();
        assert_eq!(
            parsed.status,
            crate::agent::AgentCollaborationStatus::Completed
        );
        assert_eq!(parsed.id, format!("{tool}_1"));
    }

    for (kind, expected_status) in [
        (
            "started",
            crate::agent::AgentCollaborationStatus::InProgress,
        ),
        (
            "interacted",
            crate::agent::AgentCollaborationStatus::InProgress,
        ),
        (
            "interrupted",
            crate::agent::AgentCollaborationStatus::Interrupted,
        ),
        (
            "completed",
            crate::agent::AgentCollaborationStatus::Completed,
        ),
    ] {
        let item = json!({
            "type": "subAgentActivity",
            "id": format!("legacy_{kind}"),
            "kind": kind,
            "agentThreadId": "agent_a",
            "agentPath": "/root/agent_a"
        });
        let parsed = super::parse_collaboration(item.as_object().unwrap()).unwrap();
        assert_eq!(parsed.status, expected_status);
        assert_eq!(parsed.receiver_thread_ids, ["agent_a"]);
        assert_eq!(parsed.legacy_agent_path.as_deref(), Some("/root/agent_a"));
    }
}

#[test]
fn malformed_collaboration_items_fail_with_turn_correlation() {
    let cases = [
        (
            json!({
                "type":"collabAgentToolCall","id":"bad_1","tool":"futureTool",
                "status":"completed","senderThreadId":"thr_1","receiverThreadIds":[],
                "agentsStates":{}
            }),
            "item.tool 包含未知值",
        ),
        (
            json!({
                "type":"collabAgentToolCall","id":"bad_1","tool":"wait",
                "status":"future","senderThreadId":"thr_1","receiverThreadIds":[],
                "agentsStates":{}
            }),
            "item.status 包含未知值",
        ),
        (
            json!({
                "type":"collabAgentToolCall","id":"bad_1","tool":"wait",
                "status":"completed","senderThreadId":"thr_1","receiverThreadIds":[1],
                "agentsStates":{}
            }),
            "receiverThreadIds[0]",
        ),
        (
            json!({
                "type":"collabAgentToolCall","id":"bad_1","tool":"wait",
                "status":"completed","senderThreadId":"thr_1","receiverThreadIds":[],
                "agentsStates":{"agent_a":{"status":"future"}}
            }),
            "agent.status 包含未知值",
        ),
        (
            json!({
                "type":"subAgentActivity","id":"bad_1","kind":"future",
                "agentThreadId":"agent_a","agentPath":"/root/a"
            }),
            "item.kind 包含未知值",
        ),
        (
            json!({
                "type":"collabToolCall","id":"bad_1","tool":"wait",
                "status":"completed","senderThreadId":"thr_1","agentStatus":"running"
            }),
            "必须提供 receiverThreadId 或 newThreadId",
        ),
    ];
    for (item, expected) in cases {
        assert_turn_message_fails(
            &turn_item_message("item/completed", item),
            &["item/completed", "bad_1", "thr_1", "turn_1", expected],
        );
    }
}

#[test]
fn mcp_tool_call_lifecycle_preserves_schema_payload_and_progress() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let mut streamed_text = false;
    let started = turn_item_message(
        "item/started",
        json!({
            "type": "mcpToolCall",
            "id": "exec_mcp_1",
            "server": "codex_app",
            "tool": "get_usage_limits",
            "status": "inProgress",
            "arguments": {"scope": "account"},
            "appContext": null,
            "pluginId": null,
            "readOnlyHint": true,
            "result": null,
            "error": null,
            "durationMs": null
        }),
    );
    super::process_turn_message(
        &session,
        &started,
        "thr_1",
        "turn_1",
        &tx,
        &mut streamed_text,
    )
    .unwrap();
    super::process_turn_message(
        &session,
        &json!({
            "method": "item/mcpToolCall/progress",
            "params": {
                "threadId": "thr_1",
                "turnId": "turn_1",
                "itemId": "exec_mcp_1",
                "message": "Reading limits"
            }
        }),
        "thr_1",
        "turn_1",
        &tx,
        &mut streamed_text,
    )
    .unwrap();
    let completed = turn_item_message(
        "item/completed",
        json!({
            "type": "mcpToolCall",
            "id": "exec_mcp_1",
            "server": "codex_app",
            "tool": "get_usage_limits",
            "status": "completed",
            "arguments": {"scope": "account"},
            "appContext": {
                "connectorId": "connector_1",
                "appName": "Codex App Tools",
                "actionName": "Get usage limits"
            },
            "pluginId": "plugin_1",
            "mcpAppResourceUri": "ui://legacy/usage.html",
            "readOnlyHint": true,
            "result": {
                "content": [{"type": "text", "text": "ok"}],
                "structuredContent": {"remaining": 29},
                "_meta": {"source": "fixture"}
            },
            "error": null,
            "durationMs": 1535
        }),
    );
    super::process_turn_message(
        &session,
        &completed,
        "thr_1",
        "turn_1",
        &tx,
        &mut streamed_text,
    )
    .unwrap();
    drop(tx);

    assert_eq!(
        rx.try_recv().unwrap(),
        AgentEvent::McpToolCallUpdated(AgentMcpToolCall {
            id: "exec_mcp_1".into(),
            server: "codex_app".into(),
            tool: "get_usage_limits".into(),
            status: AgentMcpToolCallStatus::InProgress,
            arguments: json!({"scope": "account"}),
            app_context: None,
            plugin_id: None,
            result: None,
            error: None,
            legacy_resource_uri: None,
            read_only_hint: Some(true),
            duration_ms: None,
            progress: Vec::new(),
        })
    );
    assert_eq!(
        rx.try_recv().unwrap(),
        AgentEvent::McpToolCallProgress {
            item_id: "exec_mcp_1".into(),
            message: "Reading limits".into(),
        }
    );
    let AgentEvent::McpToolCallUpdated(completed) = rx.try_recv().unwrap() else {
        panic!("expected completed MCP tool call");
    };
    assert_eq!(completed.status, AgentMcpToolCallStatus::Completed);
    assert_eq!(completed.plugin_id.as_deref(), Some("plugin_1"));
    assert_eq!(
        completed.legacy_resource_uri.as_deref(),
        Some("ui://legacy/usage.html")
    );
    assert_eq!(completed.duration_ms, Some(1535));
    assert_eq!(
        completed.result.as_ref().unwrap()["structuredContent"]["remaining"],
        29
    );
    assert!(rx.try_recv().is_err());
}

#[test]
fn mcp_tool_call_failure_and_legacy_missing_metadata_are_valid() {
    let failed = json!({
        "type": "mcpToolCall",
        "id": "exec_mcp_failed",
        "server": "connector",
        "tool": "delete_record",
        "status": "failed",
        "arguments": null,
        "error": {"message": "Declined by user"}
    });
    let parsed = super::parse_mcp_tool_call(failed.as_object().unwrap()).unwrap();
    assert_eq!(parsed.status, AgentMcpToolCallStatus::Failed);
    assert_eq!(parsed.error.as_deref(), Some("Declined by user"));
    assert_eq!(parsed.arguments, Value::Null);
    assert!(parsed.app_context.is_none());
    assert!(parsed.plugin_id.is_none());
    assert!(parsed.result.is_none());

    for (field, invalid, expected) in [
        ("status", json!("declined"), "status"),
        ("appContext", json!({}), "connectorId"),
        ("result", json!({}), "result.content"),
        ("error", json!({}), "error.message"),
        ("pluginId", json!(7), "pluginId"),
    ] {
        let mut item = failed.clone();
        item[field] = invalid;
        assert_turn_message_fails(
            &turn_item_message("item/completed", item),
            &["mcpToolCall", expected],
        );
    }
}

#[test]
fn every_unsupported_thread_item_type_fails_for_started_and_completed() {
    for item_type in [
        "hookPrompt",
        "functionCallOutput",
        "dynamicToolCall",
        "enteredReviewMode",
        "exitedReviewMode",
    ] {
        for method in ["item/started", "item/completed"] {
            let item_id = format!("{item_type}_1");
            let message = turn_item_message(
                method,
                json!({"type": item_type, "id": item_id, "probe": true}),
            );
            assert_turn_message_fails(&message, &[method, item_type, &item_id, "thr_1", "turn_1"]);
        }
    }
}

#[test]
fn required_item_and_delta_fields_never_fall_through() {
    let cases = vec![
        (
            "started missing item",
            json!({"method":"item/started","params":{"threadId":"thr_1","turnId":"turn_1"}}),
            "params.item",
        ),
        (
            "completed missing item",
            json!({"method":"item/completed","params":{"threadId":"thr_1","turnId":"turn_1"}}),
            "params.item",
        ),
        (
            "item is not object",
            turn_item_message("item/started", json!("agentMessage")),
            "params.item",
        ),
        (
            "missing type",
            turn_item_message("item/started", json!({"id":"msg_1","text":"hello"})),
            "item.type",
        ),
        (
            "completed missing type",
            turn_item_message("item/completed", json!({"id":"msg_1","text":"hello"})),
            "item.type",
        ),
        (
            "type is not string",
            turn_item_message(
                "item/completed",
                json!({"type":1,"id":"msg_1","text":"hello"}),
            ),
            "item.type",
        ),
        (
            "missing id",
            turn_item_message(
                "item/started",
                json!({"type":"agentMessage","text":"hello"}),
            ),
            "item.id",
        ),
        (
            "completed missing id",
            turn_item_message(
                "item/completed",
                json!({"type":"agentMessage","text":"hello"}),
            ),
            "item.id",
        ),
        (
            "id is not string",
            turn_item_message(
                "item/completed",
                json!({"type":"agentMessage","id":1,"text":"hello"}),
            ),
            "item.id",
        ),
        (
            "missing text",
            turn_item_message("item/started", json!({"type":"agentMessage","id":"msg_1"})),
            "item.text",
        ),
        (
            "completed missing text",
            turn_item_message(
                "item/completed",
                json!({"type":"agentMessage","id":"msg_1"}),
            ),
            "item.text",
        ),
        (
            "text is not string",
            turn_item_message(
                "item/completed",
                json!({"type":"agentMessage","id":"msg_1","text":1}),
            ),
            "item.text",
        ),
        (
            "agent delta missing itemId",
            json!({"method":"item/agentMessage/delta","params":{"threadId":"thr_1","turnId":"turn_1","delta":"hello"}}),
            "params.itemId",
        ),
        (
            "agent delta missing delta",
            json!({"method":"item/agentMessage/delta","params":{"threadId":"thr_1","turnId":"turn_1","itemId":"msg_1"}}),
            "params.delta",
        ),
        (
            "agent delta itemId is not string",
            json!({"method":"item/agentMessage/delta","params":{"threadId":"thr_1","turnId":"turn_1","itemId":1,"delta":"hello"}}),
            "params.itemId",
        ),
        (
            "agent delta is not string",
            json!({"method":"item/agentMessage/delta","params":{"threadId":"thr_1","turnId":"turn_1","itemId":"msg_1","delta":1}}),
            "params.delta",
        ),
        (
            "command delta missing itemId",
            json!({"method":"item/commandExecution/outputDelta","params":{"threadId":"thr_1","turnId":"turn_1","delta":"output"}}),
            "params.itemId",
        ),
        (
            "command delta missing delta",
            json!({"method":"item/commandExecution/outputDelta","params":{"threadId":"thr_1","turnId":"turn_1","itemId":"exec_1"}}),
            "params.delta",
        ),
        (
            "command delta itemId is not string",
            json!({"method":"item/commandExecution/outputDelta","params":{"threadId":"thr_1","turnId":"turn_1","itemId":1,"delta":"output"}}),
            "params.itemId",
        ),
        (
            "command delta is not string",
            json!({"method":"item/commandExecution/outputDelta","params":{"threadId":"thr_1","turnId":"turn_1","itemId":"exec_1","delta":1}}),
            "params.delta",
        ),
    ];
    for (name, message, expected) in cases {
        let error = assert_turn_message_fails(&message, &[expected]);
        assert!(
            error.contains("缺少字符串") || error.contains("必须是"),
            "{name}: {error}"
        );
    }
}

#[test]
fn command_execution_required_fields_and_nullable_fields_are_strict() {
    for field in ["id", "command", "commandActions", "cwd", "status"] {
        let mut item = command_execution_item("inProgress");
        item.as_object_mut().unwrap().remove(field);
        let message = turn_item_message("item/started", item);
        assert_turn_message_fails(&message, &["commandExecution", field]);
    }

    for (field, invalid) in [
        ("id", json!(1)),
        ("command", json!(1)),
        ("commandActions", json!({})),
        ("cwd", json!(1)),
        ("status", json!(1)),
    ] {
        let mut item = command_execution_item("inProgress");
        item[field] = invalid;
        let message = turn_item_message("item/started", item);
        assert_turn_message_fails(&message, &["commandExecution", field]);
    }

    for (field, invalid) in [("aggregatedOutput", json!(7)), ("exitCode", json!("zero"))] {
        let mut item = command_execution_item("completed");
        item[field] = invalid;
        let message = turn_item_message("item/completed", item);
        assert_turn_message_fails(&message, &["commandExecution", field]);
    }
}

#[test]
fn command_execution_preserves_structured_actions_for_activity_rendering() {
    let mut item = command_execution_item("completed");
    item["commandActions"] = json!([
        {
            "type": "read",
            "command": "sed -n '1,20p' src/main.rs",
            "name": "main.rs",
            "path": "src/main.rs"
        },
        {
            "type": "listFiles",
            "command": "find src -maxdepth 1 -type f",
            "path": "src"
        },
        {
            "type": "search",
            "command": "rg -n app_server src",
            "path": "src",
            "query": "app_server"
        },
        {
            "type": "unknown",
            "command": "cargo check"
        }
    ]);
    item["exitCode"] = json!(0);
    let command = super::parse_command_execution(item.as_object().unwrap()).unwrap();

    assert_eq!(command.command, "sed -n '1,20p' src/main.rs");
    assert_eq!(
        command.actions,
        vec![
            super::CommandExecutionAction::Read {
                command: "sed -n '1,20p' src/main.rs".into(),
                name: "main.rs".into(),
                path: "src/main.rs".into(),
            },
            super::CommandExecutionAction::ListFiles {
                command: "find src -maxdepth 1 -type f".into(),
                path: Some("src".into()),
            },
            super::CommandExecutionAction::Search {
                command: "rg -n app_server src".into(),
                path: Some("src".into()),
                query: Some("app_server".into()),
            },
            super::CommandExecutionAction::Unknown {
                command: "cargo check".into(),
            },
        ]
    );
}

#[test]
fn command_execution_unknown_status_fails_fast() {
    let message = turn_item_message(
        "item/completed",
        command_execution_item("pausedByFutureServer"),
    );
    assert_turn_message_fails(
        &message,
        &[
            "item/completed",
            "commandExecution",
            "exec_1",
            "pausedByFutureServer",
            "thr_1",
            "turn_1",
        ],
    );
}

#[test]
fn agent_message_completion_is_explicit_with_and_without_streaming() {
    for streamed in [false, true] {
        let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
        let (tx, rx) = async_channel::unbounded();
        let mut streamed_text = false;
        let started = turn_item_message(
            "item/started",
            json!({"type":"agentMessage","id":"msg_1","text":""}),
        );
        super::process_turn_message(
            &session,
            &started,
            "thr_1",
            "turn_1",
            &tx,
            &mut streamed_text,
        )
        .unwrap();
        if streamed {
            let delta = json!({
                "method":"item/agentMessage/delta",
                "params":{"threadId":"thr_1","turnId":"turn_1","itemId":"msg_1","delta":"hello"}
            });
            super::process_turn_message(
                &session,
                &delta,
                "thr_1",
                "turn_1",
                &tx,
                &mut streamed_text,
            )
            .unwrap();
        }
        let completed = turn_item_message(
            "item/completed",
            json!({"type":"agentMessage","id":"msg_1","text":"hello"}),
        );
        super::process_turn_message(
            &session,
            &completed,
            "thr_1",
            "turn_1",
            &tx,
            &mut streamed_text,
        )
        .unwrap();
        drop(tx);

        let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert_eq!(
            events,
            vec![
                AgentEvent::AssistantMessageStarted {
                    item_id: "msg_1".into()
                },
                AgentEvent::TextDelta("hello".into()),
            ],
            "streamed={streamed}"
        );
    }
}

#[test]
fn active_turn_event_delivery_failure_is_fatal() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    drop(rx);
    let mut streamed_text = false;
    let message = turn_item_message(
        "item/started",
        json!({"type":"agentMessage","id":"msg_1","text":""}),
    );
    let error = super::process_turn_message(
        &session,
        &message,
        "thr_1",
        "turn_1",
        &tx,
        &mut streamed_text,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("事件通道已经关闭"));
    assert!(error.contains("item/started agentMessage"));
    for fragment in ["agentMessage", "msg_1", "thr_1", "turn_1", "item="] {
        assert!(error.contains(fragment), "missing `{fragment}`: {error}");
    }
}

#[test]
fn forwarded_notification_delivery_failure_is_fatal() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    drop(rx);
    let mut streamed_text = false;
    let message = json!({
        "method": "turn/started",
        "params": {
            "threadId": "thr_1",
            "turn": {"id": "turn_1", "items": [], "status": "inProgress"}
        }
    });
    let error = super::process_turn_message(
        &session,
        &message,
        "thr_1",
        "turn_1",
        &tx,
        &mut streamed_text,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("事件通道已经关闭"));
    assert!(error.contains("turn/started"));
}

#[test]
fn thread_created_delivery_failure_stops_the_session() {
    let input = concat!(
        "{\"id\":1,\"result\":{}}\n",
        "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_1\"}}}\n"
    );
    let mut reader = Cursor::new(input.as_bytes());
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    drop(rx);
    let error = drive_session(
        &mut reader,
        &session,
        &AgentRequest {
            client_message_id: None,
            prompt: "probe".into(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            thread_id: None,
            model: "gpt-test".into(),
            effort: "medium".into(),
            service_tier: None,
            permission_mode: AgentPermissionMode::Full,
            context: Default::default(),
        },
        &tx,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("thread created 事件通道已经关闭"));
}

#[test]
fn active_turn_stops_at_the_first_unknown_method() {
    let input = concat!(
        "{\"id\":1,\"result\":{}}\n",
        "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_1\"}}}\n",
        "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_1\"}}}\n",
        "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_1\",\"turn\":{\"id\":\"turn_1\",\"items\":[],\"status\":\"inProgress\"}}}\n",
        "{\"method\":\"item/brandNew/delta\",\"params\":{\"delta\":\"diagnostic payload\"}}\n",
        "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thr_1\",\"turn\":{\"id\":\"turn_1\",\"status\":\"completed\"}}}\n"
    );
    let mut reader = Cursor::new(input.as_bytes());
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();

    let error = drive_session(
        &mut reader,
        &session,
        &AgentRequest {
            client_message_id: None,
            prompt: "probe".into(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            thread_id: None,
            model: "gpt-test".into(),
            effort: "medium".into(),
            service_tier: None,
            permission_mode: AgentPermissionMode::Full,
            context: Default::default(),
        },
        &tx,
    )
    .unwrap_err()
    .to_string();
    drop(tx);

    assert!(error.contains("item/brandNew/delta"));
    assert!(error.contains("diagnostic payload"));
    assert_eq!(
        rx.try_recv().unwrap(),
        AgentEvent::ThreadCreated {
            thread_id: "thr_1".into()
        }
    );
    assert_eq!(rx.try_recv().unwrap(), AgentEvent::Started);
    assert!(rx.try_recv().is_err());
}

#[test]
fn model_notifications_are_normalized_into_agent_events() {
    let input = concat!(
        "{\"id\":1,\"result\":{}}\n",
        "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_1\"}}}\n",
        "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_1\"}}}\n",
        "{\"method\":\"turn/started\",\"params\":{\"threadId\":\"thr_1\",\"turn\":{\"id\":\"turn_1\",\"items\":[],\"status\":\"inProgress\"}}}\n",
        "{\"method\":\"model/rerouted\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"fromModel\":\"model-a\",\"toModel\":\"model-b\",\"reason\":\"highRiskCyberActivity\"}}\n",
        "{\"method\":\"model/safetyBuffering/updated\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"model\":\"model-b\",\"useCases\":[\"cyber\"],\"reasons\":[\"review\"],\"showBufferingUi\":true,\"fasterModel\":\"model-c\"}}\n",
        "{\"method\":\"model/safetyBuffering/updated\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"model\":\"model-b\",\"useCases\":[],\"reasons\":[],\"showBufferingUi\":false,\"fasterModel\":null}}\n",
        "{\"method\":\"model/verification\",\"params\":{\"threadId\":\"thr_1\",\"turnId\":\"turn_1\",\"verifications\":[\"trustedAccessForCyber\"]}}\n",
        "{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thr_1\",\"turn\":{\"id\":\"turn_1\",\"status\":\"completed\"}}}\n"
    );
    let mut reader = Cursor::new(input.as_bytes());
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let outcome = drive_session(
        &mut reader,
        &session,
        &AgentRequest {
            client_message_id: None,
            prompt: "probe".into(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            thread_id: None,
            model: "model-a".into(),
            effort: "high".into(),
            service_tier: None,
            permission_mode: AgentPermissionMode::Full,
            context: Default::default(),
        },
        &tx,
    )
    .unwrap();
    assert_eq!(outcome, TurnOutcome::Completed);
    tx.send_blocking(outcome.into_event()).unwrap();
    drop(tx);

    let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert_eq!(
        events,
        vec![
            AgentEvent::ThreadCreated {
                thread_id: "thr_1".into()
            },
            AgentEvent::Started,
            AgentEvent::ModelRerouted {
                from_model: "model-a".into(),
                to_model: "model-b".into(),
                reason: "highRiskCyberActivity".into(),
            },
            AgentEvent::ModelSafetyBufferingUpdated {
                model: "model-b".into(),
                use_cases: vec!["cyber".into()],
                reasons: vec!["review".into()],
                show_buffering_ui: true,
                faster_model: Some("model-c".into()),
            },
            AgentEvent::ModelSafetyBufferingUpdated {
                model: "model-b".into(),
                use_cases: Vec::new(),
                reasons: Vec::new(),
                show_buffering_ui: false,
                faster_model: None,
            },
            AgentEvent::ModelVerificationRequired {
                verifications: vec!["trustedAccessForCyber".into()],
            },
            AgentEvent::Completed,
        ]
    );
}

#[test]
fn continued_turn_accepts_a_later_assistant_item_without_deltas_once() {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    let (tx, rx) = async_channel::unbounded();
    let mut streamed = false;
    let items = [
        turn_item_message(
            "item/started",
            json!({"type":"agentMessage","id":"a","text":""}),
        ),
        json!({"method":"item/agentMessage/delta","params":{"threadId":"thr_1","turnId":"turn_1","itemId":"a","delta":"before"}}),
        turn_item_message(
            "item/completed",
            json!({"type":"agentMessage","id":"a","text":"before"}),
        ),
        turn_item_message(
            "item/started",
            json!({"type":"agentMessage","id":"b","text":""}),
        ),
        turn_item_message(
            "item/completed",
            json!({"type":"agentMessage","id":"b","text":"after"}),
        ),
        turn_item_message(
            "item/completed",
            json!({"type":"agentMessage","id":"b","text":"after"}),
        ),
    ];
    for message in items {
        super::process_turn_message(&session, &message, "thr_1", "turn_1", &tx, &mut streamed)
            .unwrap();
    }
    let received = std::iter::from_fn(|| rx.try_recv().ok())
        .filter_map(|e| {
            if let AgentEvent::TextDelta(t) = e {
                Some(t)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(received, vec!["before", "after"]);
}
