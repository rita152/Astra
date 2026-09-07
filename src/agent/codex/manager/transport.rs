//! Managed process spawning, serialized writes, and the connection reader.

use std::{
    io::{BufRead, BufReader, Error as IoError, ErrorKind, Write},
    process::{Command, Stdio},
    sync::{Arc, Mutex, Weak, atomic::Ordering},
};

use anyhow::{Context as _, Result, bail};
use serde_json::Value;

use super::{super::AppServerProcess, ManagerInner, connection::Connection};

pub(super) trait ManagedProcess: Send + Sync {
    fn terminate_and_wait(&self) -> Result<()>;
}

impl ManagedProcess for AppServerProcess {
    fn terminate_and_wait(&self) -> Result<()> {
        AppServerProcess::terminate_and_wait(self)
    }
}

pub(super) struct SpawnedAppServer {
    pub(super) reader: Box<dyn BufRead + Send>,
    pub(super) writer: Box<dyn Write + Send>,
    pub(super) process: Arc<dyn ManagedProcess>,
}

pub(super) trait AppServerSpawner: Send + Sync {
    fn spawn(&self) -> Result<SpawnedAppServer>;
}

pub(super) struct RealAppServerSpawner;

impl AppServerSpawner for RealAppServerSpawner {
    fn spawn(&self) -> Result<SpawnedAppServer> {
        let mut child = Command::new("codex")
            .args(["app-server", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("无法启动 `codex app-server --stdio`；请确认 Codex CLI 已安装并完成登录")?;
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            bail!("无法读取 Codex app-server stdout");
        };
        let Some(stdin) = child.stdin.take() else {
            let _ = child.kill();
            let _ = child.wait();
            bail!("无法写入 Codex app-server stdin");
        };
        Ok(SpawnedAppServer {
            reader: Box::new(BufReader::new(stdout)),
            writer: Box::new(stdin),
            process: Arc::new(AppServerProcess::new(child)),
        })
    }
}

pub(super) struct SharedWriterState {
    pub(super) writer: Mutex<Option<Box<dyn Write + Send>>>,
    pub(super) manager: Weak<ManagerInner>,
    pub(super) generation: u64,
}

pub(super) struct SharedJsonWriter {
    pub(super) state: Arc<SharedWriterState>,
    pub(super) buffer: Vec<u8>,
}

impl Clone for SharedJsonWriter {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            buffer: Vec::new(),
        }
    }
}

impl SharedJsonWriter {
    pub(super) fn new(
        writer: Box<dyn Write + Send>,
        manager: Weak<ManagerInner>,
        generation: u64,
    ) -> Self {
        Self {
            state: Arc::new(SharedWriterState {
                writer: Mutex::new(Some(writer)),
                manager,
                generation,
            }),
            buffer: Vec::new(),
        }
    }

    pub(super) fn close(&self) {
        if let Ok(mut writer) = self.state.writer.lock() {
            writer.take();
        }
    }

    pub(super) fn transport_error(&self, error: &IoError) {
        if let Some(manager) = self.state.manager.upgrade() {
            manager.fail_generation(
                self.state.generation,
                format!("写入 Codex app-server transport 失败：{error}"),
            );
        }
    }
}

impl Write for SharedJsonWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let result = match self.state.writer.lock() {
            Ok(mut writer) => match writer.as_mut() {
                Some(writer) => writer.write_all(&self.buffer).and_then(|()| writer.flush()),
                None => Err(IoError::new(
                    ErrorKind::BrokenPipe,
                    "Codex app-server 连接已经关闭",
                )),
            },
            Err(_) => Err(IoError::other("Codex app-server stdin 锁已损坏")),
        };
        if let Err(error) = &result {
            self.transport_error(error);
        } else {
            self.buffer.clear();
        }
        result
    }
}

impl ManagerInner {
    pub(super) fn reader_loop(
        manager: Weak<Self>,
        connection: Arc<Connection>,
        mut reader: Box<dyn BufRead + Send>,
    ) {
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    if !connection.failed.load(Ordering::Acquire)
                        && let Some(manager) = manager.upgrade()
                    {
                        manager.fail_generation(
                            connection.generation,
                            "Codex app-server stdout EOF；connection generation 已失败".to_owned(),
                        );
                    }
                    return;
                }
                Ok(_) => {}
                Err(error) => {
                    if let Some(manager) = manager.upgrade() {
                        manager.fail_generation(
                            connection.generation,
                            format!("读取 Codex app-server stdout 失败：{error}"),
                        );
                    }
                    return;
                }
            }
            let message: Value = match serde_json::from_str(&line) {
                Ok(message) => message,
                Err(error) => {
                    if let Some(manager) = manager.upgrade() {
                        manager.fail_generation(
                            connection.generation,
                            format!("无法解析 Codex JSON-RPC 消息：{error}；payload={line}"),
                        );
                    }
                    return;
                }
            };
            let Some(manager) = manager.upgrade() else {
                let _ = connection.process.terminate_and_wait();
                return;
            };
            if let Err(error) = manager.handle_message(&connection, &message) {
                manager.fail_generation(connection.generation, format!("{error:#}"));
                return;
            }
        }
    }
}
