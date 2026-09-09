//! Append to an existing generation and turn; never create or resume a connection.
use super::{
    CodexAppServerManager,
    connection::{Connection, TurnKey},
    turn::ManagedTurn,
};
use crate::agent::AgentSteerRequest;
use anyhow::{Context as _, Result, anyhow, bail};
use async_channel::{Receiver, Sender};
use serde_json::{Value, json};
use std::sync::{Arc, atomic::Ordering};

pub(super) fn build_steer_params(request: &AgentSteerRequest) -> Result<Value> {
    Ok(json!({
        "threadId": request.target.thread_id,
        "expectedTurnId": request.target.turn_id,
        "clientUserMessageId": request.client_message_id,
        "input": super::super::input::encode_input(&request.prompt, &request.context)?,
    }))
}

impl CodexAppServerManager {
    fn steer_owner(
        &self,
        request: &AgentSteerRequest,
    ) -> Result<(Arc<Connection>, Arc<ManagedTurn>)> {
        let connection = self
            .inner
            .state
            .lock()
            .map_err(|_| anyhow!("连接状态不可用"))?
            .current
            .clone()
            .context("连接已断开，未追加输入。请保留草稿并手动重试。")?;
        if connection.generation != request.target.generation
            || connection.failed.load(Ordering::Acquire)
        {
            bail!("原轮次的连接已失效，未追加输入。请保留草稿并手动重试。");
        }
        self.validate_temporary_thread(&connection, &request.target.thread_id)?;
        let turn = connection
            .state
            .lock()
            .map_err(|_| anyhow!("轮次状态不可用"))?
            .turns
            .get(&TurnKey {
                thread_id: request.target.thread_id.clone(),
                turn_id: request.target.turn_id.clone(),
            })
            .cloned()
            .context("当前轮次已结束或尚未就绪，未追加输入。请保留草稿并手动重试。")?;
        Ok((connection, turn))
    }

    pub(in crate::agent::codex) fn steer_turn(
        &self,
        request: AgentSteerRequest,
    ) -> Receiver<Result<(), String>> {
        let (sender, receiver) = async_channel::bounded(1);
        // Synchronously enqueue on this turn, then write in FIFO order off the UI
        // thread. Acknowledgements are awaited independently and may be reversed.
        let result = (|| -> Result<()> {
            let (_, turn) = self.steer_owner(&request)?;
            let mut queue = turn
                .steer_queue
                .lock()
                .map_err(|_| anyhow!("追加输入发送队列不可用"))?;
            let queue = queue.get_or_insert_with(|| {
                let (sender, jobs) = async_channel::unbounded::<QueuedSteer>();
                let owner = Arc::downgrade(&turn);
                std::thread::spawn(move || {
                    while let Ok(job) = jobs.recv_blocking() {
                        let response = owner
                            .upgrade()
                            .context("原轮次已结束，未追加输入。")
                            .and_then(|turn| begin_steer(&turn, &job.request));
                        match response {
                            Ok(response) => {
                                std::thread::spawn(move || {
                                    let result =
                                        validate_ack(response, &job.request.target.turn_id)
                                            .map_err(|error| format!("{error:#}"));
                                    let _ = job.result.send_blocking(result);
                                });
                            }
                            Err(error) => {
                                let _ = job.result.send_blocking(Err(format!("{error:#}")));
                            }
                        }
                    }
                });
                sender
            });
            queue
                .try_send(QueuedSteer {
                    request,
                    result: sender.clone(),
                })
                .map_err(|_| anyhow!("原轮次的输入连接已结束，未追加输入。"))?;
            Ok(())
        })();
        if let Err(error) = result {
            let _ = sender.send_blocking(Err(format!("{error:#}")));
        }
        receiver
    }
}

pub(super) struct QueuedSteer {
    request: AgentSteerRequest,
    result: Sender<Result<(), String>>,
}

fn begin_steer(
    turn: &Arc<ManagedTurn>,
    request: &AgentSteerRequest,
) -> Result<Receiver<Result<Value, String>>> {
    let connection = turn
        .connection
        .upgrade()
        .context("原轮次的连接已结束，未追加输入。")?;
    let params = build_steer_params(request)?;
    let _input = turn
        .input_lock
        .lock()
        .map_err(|_| anyhow!("轮次输入锁不可用"))?;
    let control = turn
        .control
        .state
        .lock()
        .map_err(|_| anyhow!("轮次状态不可用"))?;
    if control.terminal || turn.terminal.load(Ordering::Acquire) {
        bail!("当前轮次已结束，未追加输入。请手动重试。");
    }
    if control.interrupt_requested || turn.interrupt_requested.load(Ordering::Acquire) {
        bail!("正在停止当前轮次，未追加输入。请等待停止完成后手动发送。");
    }
    drop(control);
    connection.begin_request("turn/steer", params)
}

fn validate_ack(response: Receiver<Result<Value, String>>, expected_turn: &str) -> Result<()> {
    let response = response
        .recv_blocking()
        .context("追加输入响应连接已关闭，接受状态未知，请勿自动重发")?
        .map_err(anyhow::Error::msg)?;
    let actual = response
        .pointer("/result/turnId")
        .and_then(Value::as_str)
        .context("追加输入响应缺少 turnId，接受状态无法确认")?;
    if actual != expected_turn {
        bail!("追加输入响应 turnId 不匹配，接受状态无法确认。请检查当前聊天后手动重试。");
    }
    Ok(())
}
