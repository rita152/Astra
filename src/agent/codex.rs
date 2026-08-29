use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

use anyhow::{Context as _, Result, anyhow, bail};
use async_channel::{Receiver, Sender};
use serde_json::{Value, json};

use super::{AgentBackend, AgentEvent, AgentRequest, CommandExecution, CommandExecutionStatus};

const INITIALIZE_ID: u64 = 1;
const THREAD_START_ID: u64 = 2;
const TURN_START_ID: u64 = 3;
const UNDEFINED_METHOD_PARAMS_LIMIT: usize = 2_000;

// These protocol methods are intentionally recognized even though this view
// does not render them yet. Keep the list exact: a prefix match or wildcard
// would hide new app-server surface area instead of reporting it.
const PASSIVE_SERVER_METHODS: &[&str] = &[
    "remoteControl/status/changed",
    "thread/started",
    "mcpServer/startupStatus/updated",
    "thread/status/changed",
    "turn/started",
    "turn/plan/updated",
    "thread/tokenUsage/updated",
    "account/rateLimits/updated",
];

/// Codex CLI adapter. JSON-RPC details intentionally stay inside this module.
#[derive(Default)]
pub struct CodexAppServerBackend;

impl CodexAppServerBackend {
    pub fn new() -> Self {
        Self
    }
}

impl AgentBackend for CodexAppServerBackend {
    fn run_prompt(&self, request: AgentRequest) -> Receiver<AgentEvent> {
        let (events_tx, events_rx) = async_channel::unbounded();
        std::thread::spawn(move || {
            if let Err(error) = run_prompt_process(&request, &events_tx) {
                let _ = events_tx.send_blocking(AgentEvent::Failed(format!("{error:#}")));
            }
        });
        events_rx
    }
}

fn run_prompt_process(request: &AgentRequest, events: &Sender<AgentEvent>) -> Result<()> {
    let mut child = Command::new("codex")
        .args(["app-server", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("无法启动 `codex app-server --stdio`；请确认 Codex CLI 已安装并完成登录")?;

    let stdout = child
        .stdout
        .take()
        .context("无法读取 Codex app-server stdout")?;
    let mut stdin = child
        .stdin
        .take()
        .context("无法写入 Codex app-server stdin")?;
    let mut reader = BufReader::new(stdout);
    let result = drive_session(&mut reader, &mut stdin, request, events);

    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn drive_session<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    request: &AgentRequest,
    events: &Sender<AgentEvent>,
) -> Result<()> {
    send(
        writer,
        json!({
            "method": "initialize",
            "id": INITIALIZE_ID,
            "params": {
                "clientInfo": {
                    "name": "gpui_chat_clone",
                    "title": "GPUI Chat Clone",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        }),
    )?;
    wait_for_response(reader, writer, INITIALIZE_ID)?;

    send(writer, json!({ "method": "initialized", "params": {} }))?;
    send(
        writer,
        json!({
            "method": "thread/start",
            "id": THREAD_START_ID,
            "params": {
                "cwd": request.cwd,
                "approvalPolicy": "never",
                "sandbox": "read-only",
                "ephemeral": true,
                "serviceName": "gpui-chat-clone"
            }
        }),
    )?;
    let thread_response = wait_for_response(reader, writer, THREAD_START_ID)?;
    let thread_id = thread_response
        .pointer("/result/thread/id")
        .and_then(Value::as_str)
        .context("thread/start 响应缺少 result.thread.id")?;

    send(
        writer,
        json!({
            "method": "turn/start",
            "id": TURN_START_ID,
            "params": {
                "threadId": thread_id,
                "input": [{ "type": "text", "text": request.prompt }]
            }
        }),
    )?;
    wait_for_response(reader, writer, TURN_START_ID)?;
    let _ = events.send_blocking(AgentEvent::Started);

    let mut streamed_text = false;
    loop {
        let message = read_message(reader)?;
        respond_to_server_request(writer, &message)?;
        ensure_server_method_is_defined(&message)?;

        match message.get("method").and_then(Value::as_str) {
            Some("item/started") => {
                let item = message.pointer("/params/item");
                match item
                    .and_then(|item| item.get("type"))
                    .and_then(Value::as_str)
                {
                    Some("agentMessage") => {
                        if let Some(item_id) =
                            item.and_then(|item| item.get("id")).and_then(Value::as_str)
                        {
                            let _ = events.send_blocking(AgentEvent::AssistantMessageStarted {
                                item_id: item_id.to_owned(),
                            });
                        }
                    }
                    Some("commandExecution") => {
                        if let Some(command) = item.and_then(parse_command_execution) {
                            let _ = events.send_blocking(AgentEvent::CommandStarted(command));
                        }
                    }
                    _ => {}
                }
            }
            Some("item/agentMessage/delta") => {
                if let Some(delta) = message.pointer("/params/delta").and_then(Value::as_str) {
                    streamed_text = true;
                    let _ = events.send_blocking(AgentEvent::TextDelta(delta.to_owned()));
                }
            }
            Some("item/commandExecution/outputDelta") => {
                let item_id = message.pointer("/params/itemId").and_then(Value::as_str);
                let delta = message.pointer("/params/delta").and_then(Value::as_str);
                if let (Some(item_id), Some(delta)) = (item_id, delta) {
                    let _ = events.send_blocking(AgentEvent::CommandOutputDelta {
                        item_id: item_id.to_owned(),
                        delta: delta.to_owned(),
                    });
                }
            }
            Some("item/completed") => {
                let item = message.pointer("/params/item");
                if let Some(command) = item.and_then(parse_command_execution) {
                    let _ = events.send_blocking(AgentEvent::CommandCompleted(command));
                } else if !streamed_text
                    && item
                        .and_then(|item| item.get("type"))
                        .and_then(Value::as_str)
                        == Some("agentMessage")
                {
                    if let Some(text) = item
                        .and_then(|item| item.get("text"))
                        .and_then(Value::as_str)
                    {
                        let _ = events.send_blocking(AgentEvent::TextDelta(text.to_owned()));
                    }
                }
            }
            Some("turn/completed") => {
                let status = message
                    .pointer("/params/turn/status")
                    .and_then(Value::as_str)
                    .unwrap_or("completed");
                if status == "completed" {
                    let _ = events.send_blocking(AgentEvent::Completed);
                    return Ok(());
                }
                bail!("Codex turn 结束，状态为 {status}");
            }
            Some(method) if PASSIVE_SERVER_METHODS.contains(&method) => {}
            Some(method) => return Err(undefined_server_method_error(method, &message)),
            None => {}
        }
    }
}

fn is_defined_server_method(method: &str) -> bool {
    matches!(
        method,
        "item/started"
            | "item/agentMessage/delta"
            | "item/commandExecution/outputDelta"
            | "item/completed"
            | "turn/completed"
    ) || PASSIVE_SERVER_METHODS.contains(&method)
}

fn ensure_server_method_is_defined(message: &Value) -> Result<()> {
    let Some(method) = message.get("method") else {
        return Ok(());
    };
    let Some(method) = method.as_str() else {
        bail!(
            "Codex JSON-RPC 消息的 `method` 必须是字符串：{}",
            summarize_json(method)
        );
    };
    if is_defined_server_method(method) {
        Ok(())
    } else {
        Err(undefined_server_method_error(method, message))
    }
}

fn undefined_server_method_error(method: &str, message: &Value) -> anyhow::Error {
    let kind = if message.get("id").is_some() {
        "请求"
    } else {
        "通知"
    };
    let params = message
        .get("params")
        .map(summarize_json)
        .unwrap_or_else(|| "null".to_owned());
    anyhow!("遇到未定义的 Codex JSON-RPC {kind}方法 `{method}`；params={params}")
}

fn summarize_json(value: &Value) -> String {
    let rendered = value.to_string();
    let mut characters = rendered.chars();
    let mut summary: String = characters
        .by_ref()
        .take(UNDEFINED_METHOD_PARAMS_LIMIT)
        .collect();
    if characters.next().is_some() {
        summary.push('…');
    }
    summary
}

fn parse_command_execution(item: &Value) -> Option<CommandExecution> {
    if item.get("type").and_then(Value::as_str) != Some("commandExecution") {
        return None;
    }
    let status = match item.get("status").and_then(Value::as_str) {
        Some("completed") if item.get("exitCode").and_then(Value::as_i64).unwrap_or(0) == 0 => {
            CommandExecutionStatus::Completed
        }
        Some("failed" | "declined") => CommandExecutionStatus::Failed,
        Some("completed") => CommandExecutionStatus::Failed,
        _ => CommandExecutionStatus::InProgress,
    };
    let command = item
        .pointer("/commandActions/0/command")
        .or_else(|| item.get("command"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    Some(CommandExecution {
        id: item.get("id").and_then(Value::as_str)?.to_owned(),
        command,
        cwd: item
            .get("cwd")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        output: item
            .get("aggregatedOutput")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        status,
        exit_code: item.get("exitCode").and_then(Value::as_i64),
    })
}

fn send(writer: &mut impl Write, message: Value) -> Result<()> {
    serde_json::to_writer(&mut *writer, &message).context("序列化 Codex JSON-RPC 消息失败")?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn read_message(reader: &mut impl BufRead) -> Result<Value> {
    let mut line = String::new();
    let bytes = reader.read_line(&mut line)?;
    if bytes == 0 {
        bail!("Codex app-server 在 turn 完成前关闭了输出流");
    }
    serde_json::from_str(&line).with_context(|| format!("无法解析 Codex JSON-RPC 消息：{line}"))
}

fn wait_for_response(
    reader: &mut impl BufRead,
    writer: &mut impl Write,
    expected_id: u64,
) -> Result<Value> {
    loop {
        let message = read_message(reader)?;
        respond_to_server_request(writer, &message)?;
        ensure_server_method_is_defined(&message)?;
        if message.get("id").and_then(Value::as_u64) != Some(expected_id) {
            continue;
        }
        if let Some(error) = message.get("error") {
            return Err(anyhow!("Codex JSON-RPC 请求 {expected_id} 失败：{error}"));
        }
        return Ok(message);
    }
}

fn respond_to_server_request(writer: &mut impl Write, message: &Value) -> Result<()> {
    let Some(id) = message.get("id") else {
        return Ok(());
    };
    if message.get("method").is_none() {
        return Ok(());
    }
    send(
        writer,
        json!({
            "id": id,
            "error": {
                "code": -32601,
                "message": "This minimal client does not implement server-initiated requests"
            }
        }),
    )
}

#[cfg(test)]
mod tests {
    use std::{io::Cursor, path::PathBuf};

    use serde_json::json;

    use super::{
        AgentEvent, AgentRequest, INITIALIZE_ID, PASSIVE_SERVER_METHODS,
        UNDEFINED_METHOD_PARAMS_LIMIT, drive_session, ensure_server_method_is_defined,
        wait_for_response,
    };

    #[test]
    fn drives_one_complete_prompt_and_normalizes_stream_events() {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_1\"}}}\n",
            "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_1\"}}}\n",
            "{\"method\":\"thread/status/changed\",\"params\":{\"threadId\":\"thr_1\"}}\n",
            "{\"method\":\"item/started\",\"params\":{\"item\":{\"type\":\"agentMessage\",\"id\":\"msg_1\",\"text\":\"\"}}}\n",
            "{\"method\":\"item/agentMessage/delta\",\"params\":{\"delta\":\"你好\"}}\n",
            "{\"method\":\"item/agentMessage/delta\",\"params\":{\"delta\":\"！\"}}\n",
            "{\"method\":\"item/started\",\"params\":{\"item\":{\"type\":\"commandExecution\",\"id\":\"exec_1\",\"command\":\"/bin/zsh -lc pwd\",\"cwd\":\"/tmp/project\",\"status\":\"inProgress\",\"commandActions\":[{\"type\":\"unknown\",\"command\":\"pwd\"}],\"aggregatedOutput\":null,\"exitCode\":null}}}\n",
            "{\"method\":\"item/commandExecution/outputDelta\",\"params\":{\"itemId\":\"exec_1\",\"delta\":\"/tmp/project\\n\"}}\n",
            "{\"method\":\"item/completed\",\"params\":{\"item\":{\"type\":\"commandExecution\",\"id\":\"exec_1\",\"command\":\"/bin/zsh -lc pwd\",\"cwd\":\"/tmp/project\",\"status\":\"completed\",\"commandActions\":[{\"type\":\"unknown\",\"command\":\"pwd\"}],\"aggregatedOutput\":\"/tmp/project\\n\",\"exitCode\":0}}}\n",
            "{\"method\":\"turn/completed\",\"params\":{\"turn\":{\"status\":\"completed\"}}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let mut output = Vec::new();
        let (tx, rx) = async_channel::unbounded();
        drive_session(
            &mut reader,
            &mut output,
            &AgentRequest {
                prompt: "打个招呼".into(),
                cwd: PathBuf::from("/tmp/project"),
            },
            &tx,
        )
        .unwrap();
        drop(tx);

        let mut received = Vec::new();
        while let Ok(event) = rx.try_recv() {
            received.push(event);
        }
        assert_eq!(
            received,
            vec![
                AgentEvent::Started,
                AgentEvent::AssistantMessageStarted {
                    item_id: "msg_1".into(),
                },
                AgentEvent::TextDelta("你好".into()),
                AgentEvent::TextDelta("！".into()),
                AgentEvent::CommandStarted(super::CommandExecution {
                    id: "exec_1".into(),
                    command: "pwd".into(),
                    cwd: "/tmp/project".into(),
                    output: String::new(),
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
                    cwd: "/tmp/project".into(),
                    output: "/tmp/project\n".into(),
                    status: super::CommandExecutionStatus::Completed,
                    exit_code: Some(0),
                }),
                AgentEvent::Completed,
            ]
        );

        let sent = String::from_utf8(output).unwrap();
        assert!(sent.contains("\"method\":\"initialize\""));
        assert!(sent.contains("\"method\":\"initialized\""));
        assert!(sent.contains("\"method\":\"thread/start\""));
        assert!(sent.contains("\"method\":\"turn/start\""));
        assert!(sent.contains("\"threadId\":\"thr_1\""));
    }

    #[test]
    fn every_captured_passive_method_is_explicitly_defined() {
        for method in PASSIVE_SERVER_METHODS {
            ensure_server_method_is_defined(&json!({
                "method": method,
                "params": { "probe": true }
            }))
            .unwrap();
        }
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

        let error = wait_for_response(&mut reader, &mut output, INITIALIZE_ID)
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

        let error = wait_for_response(&mut reader, &mut output, INITIALIZE_ID)
            .unwrap_err()
            .to_string();
        let response = String::from_utf8(output).unwrap();

        assert!(error.contains("请求"));
        assert!(error.contains("item/futureApproval/request"));
        assert!(response.contains("\"id\":99"));
        assert!(response.contains("\"code\":-32601"));
    }

    #[test]
    fn active_turn_stops_at_the_first_unknown_method() {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_1\"}}}\n",
            "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_1\"}}}\n",
            "{\"method\":\"item/brandNew/delta\",\"params\":{\"delta\":\"diagnostic payload\"}}\n",
            "{\"method\":\"turn/completed\",\"params\":{\"turn\":{\"status\":\"completed\"}}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let mut output = Vec::new();
        let (tx, rx) = async_channel::unbounded();

        let error = drive_session(
            &mut reader,
            &mut output,
            &AgentRequest {
                prompt: "probe".into(),
                cwd: PathBuf::from("/tmp/project"),
            },
            &tx,
        )
        .unwrap_err()
        .to_string();
        drop(tx);

        assert!(error.contains("item/brandNew/delta"));
        assert!(error.contains("diagnostic payload"));
        assert_eq!(rx.try_recv().unwrap(), AgentEvent::Started);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn captured_method_set_replays_without_false_unknowns() {
        let input = concat!(
            "{\"method\":\"remoteControl/status/changed\",\"params\":{}}\n",
            "{\"method\":\"mcpServer/startupStatus/updated\",\"params\":{}}\n",
            "{\"id\":1,\"result\":{}}\n",
            "{\"method\":\"thread/started\",\"params\":{}}\n",
            "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_1\"}}}\n",
            "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_1\"}}}\n",
            "{\"method\":\"thread/status/changed\",\"params\":{}}\n",
            "{\"method\":\"turn/started\",\"params\":{}}\n",
            "{\"method\":\"turn/plan/updated\",\"params\":{}}\n",
            "{\"method\":\"thread/tokenUsage/updated\",\"params\":{}}\n",
            "{\"method\":\"account/rateLimits/updated\",\"params\":{}}\n",
            "{\"method\":\"item/started\",\"params\":{\"item\":{\"type\":\"commandExecution\",\"id\":\"exec_1\",\"command\":\"pwd\",\"cwd\":\"/tmp/project\",\"status\":\"inProgress\"}}}\n",
            "{\"method\":\"item/commandExecution/outputDelta\",\"params\":{\"itemId\":\"exec_1\",\"delta\":\"/tmp/project\\n\"}}\n",
            "{\"method\":\"item/completed\",\"params\":{\"item\":{\"type\":\"commandExecution\",\"id\":\"exec_1\",\"command\":\"pwd\",\"cwd\":\"/tmp/project\",\"status\":\"completed\",\"aggregatedOutput\":\"/tmp/project\\n\",\"exitCode\":0}}}\n",
            "{\"method\":\"turn/completed\",\"params\":{\"turn\":{\"status\":\"completed\"}}}\n"
        );

        let mut reader = Cursor::new(input.as_bytes());
        let mut output = Vec::new();
        let (tx, rx) = async_channel::unbounded();
        drive_session(
            &mut reader,
            &mut output,
            &AgentRequest {
                prompt: "深入分析当前项目".into(),
                cwd: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            },
            &tx,
        )
        .unwrap();
        drop(tx);

        let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert_eq!(events.first(), Some(&AgentEvent::Started));
        assert_eq!(events.last(), Some(&AgentEvent::Completed));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AgentEvent::CommandOutputDelta { .. }))
        );
    }
}
