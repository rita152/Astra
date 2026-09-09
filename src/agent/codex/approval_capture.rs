//! Deterministic, offline JSON-RPC replay for the screenshot/Computer Use build.
//! Reuses production parsing, request registration, responders and resolution.

use super::{
    CodexAppServerBackend, CodexTurnSession, TurnOutcome, cleanup_pending_server_requests,
    dispatch::process_turn_message, requests::handle_server_request_resolved,
};
use crate::agent::{AgentEvent, AgentInterruptHandle, AgentRun};
use anyhow::{Context as _, Result};
use serde_json::{Value, json};
use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

struct ReplayWriter {
    output: File,
    buffer: Vec<u8>,
    fail_writes: bool,
    replied: Box<dyn Fn(Value) + Send + Sync>,
}

impl Write for ReplayWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if self.fail_writes {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "审批验收注入的 transport 写入失败",
            ));
        }
        self.buffer.extend_from_slice(buffer);
        Ok(buffer.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.output.write_all(&self.buffer)?;
        self.output.flush()?;
        let value = serde_json::from_slice(&self.buffer).map_err(std::io::Error::other)?;
        self.buffer.clear();
        (self.replied)(value);
        Ok(())
    }
}

pub struct ApprovalCapture {
    pub run: AgentRun,
    pub user_message: String,
    pub assistant_message: String,
    pub cwd: PathBuf,
}

impl CodexAppServerBackend {
    pub fn replay_approvals(path: &Path) -> Result<ApprovalCapture> {
        let fixture: Value = serde_json::from_slice(&std::fs::read(path)?)?;
        let messages = fixture
            .get("events")
            .and_then(Value::as_array)
            .context("审批回放缺少 events 数组")?;
        let started = messages
            .iter()
            .find(|message| message["method"] == "turn/started")
            .context("审批回放缺少 turn/started")?;
        let thread_id = started
            .pointer("/params/threadId")
            .and_then(Value::as_str)
            .context("缺少 threadId")?
            .to_owned();
        let turn_id = started
            .pointer("/params/turn/id")
            .and_then(Value::as_str)
            .context("缺少 turn.id")?
            .to_owned();
        let output = File::create(path.with_extension("responses.jsonl"))?;
        let remaining = Arc::new(AtomicUsize::new(
            messages
                .iter()
                .filter(|message| message.get("id").is_some())
                .count(),
        ));
        let (tx, rx) = async_channel::unbounded();
        let session: Arc<CodexTurnSession<ReplayWriter>> = Arc::new_cyclic(
            |weak: &std::sync::Weak<CodexTurnSession<ReplayWriter>>| {
                let weak = weak.clone();
                let tx = tx.clone();
                let thread_id = thread_id.clone();
                CodexTurnSession::new(
                    ReplayWriter {
                        output,
                        buffer: Vec::new(),
                        fail_writes: fixture["failWrites"].as_bool().unwrap_or(false),
                        replied: Box::new(move |response| {
                            let weak = weak.clone();
                            let tx = tx.clone();
                            let thread_id = thread_id.clone();
                            let remaining = remaining.clone();
                            std::thread::spawn(move || {
                                std::thread::sleep(Duration::from_millis(150));
                                let Some(session) = weak.upgrade() else {
                                    return;
                                };
                                if response.get("result").is_some() {
                                    let resolved = json!({"method":"serverRequest/resolved","params":{"threadId":thread_id,"requestId":response["id"]}});
                                    if let Err(error) =
                                        handle_server_request_resolved(&session, &resolved, &tx)
                                    {
                                        let _ = tx.send_blocking(AgentEvent::Failed(format!(
                                            "{error:#}"
                                        )));
                                        return;
                                    }
                                    if remaining.fetch_sub(1, Ordering::AcqRel) > 1 {
                                        return;
                                    }
                                } else if response["method"] != "turn/interrupt" {
                                    return;
                                }
                                let outcome = if response["method"] == "turn/interrupt"
                                    || response.pointer("/result/decision")
                                        == Some(&json!("cancel"))
                                {
                                    TurnOutcome::Interrupted
                                } else {
                                    TurnOutcome::Completed
                                };
                                session.mark_terminal();
                                let result = Ok(outcome.clone());
                                let _ = cleanup_pending_server_requests(&session, &result, &tx);
                                let _ = tx.send_blocking(outcome.into_event());
                            });
                        }),
                    },
                    None,
                )
            },
        );
        session.activate_turn(thread_id.clone(), turn_id.clone())?;
        tx.send_blocking(AgentEvent::ThreadCreated {
            thread_id: thread_id.clone(),
        })?;
        let mut streamed = false;
        for message in messages {
            process_turn_message(&session, message, &thread_id, &turn_id, &tx, &mut streamed)?;
        }
        Ok(ApprovalCapture {
            run: AgentRun::new(rx, Some(AgentInterruptHandle::new(session))),
            user_message: fixture["userMessage"].as_str().unwrap_or("").to_owned(),
            assistant_message: fixture["assistantMessage"]
                .as_str()
                .unwrap_or("")
                .to_owned(),
            cwd: fixture["cwd"]
                .as_str()
                .map(PathBuf::from)
                .unwrap_or_else(|| path.parent().unwrap_or(Path::new("/")).to_path_buf()),
        })
    }
}
