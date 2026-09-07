//! Child process ownership and newline-delimited JSON transport.

use std::{
    io::Write,
    process::Child,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Context as _, Result, anyhow};
use serde_json::Value;

pub(super) struct AppServerProcess {
    pub(super) child: Mutex<Option<Child>>,
    pub(super) reaped: AtomicBool,
}

impl AppServerProcess {
    pub(super) fn new(child: Child) -> Self {
        Self {
            child: Mutex::new(Some(child)),
            reaped: AtomicBool::new(false),
        }
    }

    pub(super) fn kill(&self) {
        let Ok(mut child) = self.child.lock() else {
            return;
        };
        if let Some(child) = child.as_mut()
            && child.try_wait().ok().flatten().is_none()
        {
            let _ = child.kill();
        }
    }

    pub(super) fn terminate_and_wait(&self) -> Result<()> {
        let mut child = self
            .child
            .lock()
            .map_err(|_| anyhow!("Codex app-server 子进程锁已损坏"))?
            .take();
        let Some(mut child) = child.take() else {
            return Ok(());
        };

        if child.try_wait()?.is_none()
            && let Err(kill_error) = child.kill()
            && child.try_wait()?.is_none()
        {
            self.child
                .lock()
                .map_err(|_| anyhow!("Codex app-server 子进程锁已损坏"))?
                .replace(child);
            return Err(kill_error).context("无法终止 Codex app-server 子进程");
        }
        child
            .wait()
            .context("等待 Codex app-server 子进程退出失败")?;
        self.reaped.store(true, Ordering::Release);
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn is_reaped(&self) -> bool {
        self.reaped.load(Ordering::Acquire)
    }
}

impl Drop for AppServerProcess {
    fn drop(&mut self) {
        let _ = self.terminate_and_wait();
    }
}

pub(super) fn send(writer: &mut impl Write, message: Value) -> Result<()> {
    let mut line = serde_json::to_vec(&message).context("序列化 Codex JSON-RPC 消息失败")?;
    line.push(b'\n');
    writer.write_all(&line)?;
    writer.flush()?;
    Ok(())
}
