use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
};

use anyhow::{Context as _, Result, anyhow, bail};
use serde_json::{Value, json};

use super::{AgentBackend, AgentEvent, AgentRequest};

const INITIALIZE_ID: u64 = 1;
const THREAD_START_ID: u64 = 2;
const TURN_START_ID: u64 = 3;

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
        let (events_tx, events_rx) = mpsc::channel();
        std::thread::spawn(move || {
            if let Err(error) = run_prompt_process(&request, &events_tx) {
                let _ = events_tx.send(AgentEvent::Failed(format!("{error:#}")));
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
    let _ = events.send(AgentEvent::Started);

    let mut streamed_text = false;
    loop {
        let message = read_message(reader)?;
        respond_to_server_request(writer, &message)?;

        match message.get("method").and_then(Value::as_str) {
            Some("item/agentMessage/delta") => {
                if let Some(delta) = message.pointer("/params/delta").and_then(Value::as_str) {
                    streamed_text = true;
                    let _ = events.send(AgentEvent::TextDelta(delta.to_owned()));
                }
            }
            Some("item/completed") if !streamed_text => {
                let item = message.pointer("/params/item");
                if item
                    .and_then(|item| item.get("type"))
                    .and_then(Value::as_str)
                    == Some("agentMessage")
                {
                    if let Some(text) = item
                        .and_then(|item| item.get("text"))
                        .and_then(Value::as_str)
                    {
                        let _ = events.send(AgentEvent::TextDelta(text.to_owned()));
                    }
                }
            }
            Some("turn/completed") => {
                let status = message
                    .pointer("/params/turn/status")
                    .and_then(Value::as_str)
                    .unwrap_or("completed");
                if status == "completed" {
                    let _ = events.send(AgentEvent::Completed);
                    return Ok(());
                }
                bail!("Codex turn 结束，状态为 {status}");
            }
            _ => {}
        }
    }
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
    use std::{io::Cursor, path::PathBuf, sync::mpsc};

    use super::{AgentEvent, AgentRequest, drive_session};

    #[test]
    fn drives_one_complete_prompt_and_normalizes_stream_events() {
        let input = concat!(
            "{\"id\":1,\"result\":{}}\n",
            "{\"id\":2,\"result\":{\"thread\":{\"id\":\"thr_1\"}}}\n",
            "{\"id\":3,\"result\":{\"turn\":{\"id\":\"turn_1\"}}}\n",
            "{\"method\":\"item/agentMessage/delta\",\"params\":{\"delta\":\"你好\"}}\n",
            "{\"method\":\"item/agentMessage/delta\",\"params\":{\"delta\":\"！\"}}\n",
            "{\"method\":\"turn/completed\",\"params\":{\"turn\":{\"status\":\"completed\"}}}\n"
        );
        let mut reader = Cursor::new(input.as_bytes());
        let mut output = Vec::new();
        let (tx, rx) = mpsc::channel();
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

        assert_eq!(
            rx.into_iter().collect::<Vec<_>>(),
            vec![
                AgentEvent::Started,
                AgentEvent::TextDelta("你好".into()),
                AgentEvent::TextDelta("！".into()),
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
}
