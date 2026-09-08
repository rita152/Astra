//! Bounded command execution shared by Git and GitHub CLI operations.
//!
//! Adapted from the side-chat review implementation: drain both output pipes,
//! enforce a deadline, and reclaim helpers with their owning process group.

use anyhow::{Context, Result, bail};
use std::{
    io::{Read, Write},
    process::{Child, Command, Output, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

const MAX_OUTPUT: usize = 16 * 1024 * 1024;
static PROCESSES: Mutex<Vec<u32>> = Mutex::new(Vec::new());

struct RunningCommand {
    child: Child,
    finished: bool,
}

impl Drop for RunningCommand {
    fn drop(&mut self) {
        if !self.finished {
            terminate_group(self.child.id());
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Ok(mut pids) = PROCESSES.lock() {
            pids.retain(|pid| *pid != self.child.id());
        }
    }
}

fn terminate_group(pid: u32) {
    #[cfg(unix)]
    // Only commands started below with process_group(0) enter this registry.
    unsafe {
        libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
    }
    #[cfg(not(unix))]
    let _ = pid;
}

#[cfg(not(test))]
pub(super) fn shutdown() {
    if let Ok(pids) = PROCESSES.lock() {
        for &pid in pids.iter() {
            terminate_group(pid);
        }
    }
}

fn read_limited(mut input: impl Read, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            return Ok(output);
        }
        let keep = count.min((limit + 1).saturating_sub(output.len()));
        output.extend_from_slice(&buffer[..keep]);
    }
}

pub(super) fn run(
    command: &mut Command,
    input: Option<&[u8]>,
    timeout: Duration,
) -> Result<Output> {
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let child = command.spawn().context("无法启动审查命令")?;
    if let Ok(mut pids) = PROCESSES.lock() {
        pids.push(child.id());
    }
    let mut running = RunningCommand {
        child,
        finished: false,
    };
    let stdout = running.child.stdout.take().context("命令输出不可用")?;
    let stderr = running.child.stderr.take().context("命令错误输出不可用")?;
    let reader = std::thread::spawn(move || read_limited(stdout, MAX_OUTPUT));
    let errors = std::thread::spawn(move || read_limited(stderr, 64 * 1024));
    // Writing stdin must also respect the deadline when a hook/helper stops reading.
    let writer = if let Some(input) = input {
        let mut stdin = running.child.stdin.take().context("命令输入不可用")?;
        let input = input.to_vec();
        Some(std::thread::spawn(move || stdin.write_all(&input)))
    } else {
        None
    };
    let start = Instant::now();
    let status = loop {
        let status = running.child.try_wait().context("无法读取命令状态")?;
        if let Some(status) = status
            && reader.is_finished()
            && errors.is_finished()
            && writer.as_ref().is_none_or(|writer| writer.is_finished())
        {
            break status;
        }
        if start.elapsed() >= timeout {
            bail!("审查命令超时，已停止相关进程，请刷新后重试");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    running.finished = true;
    let stdout = reader
        .join()
        .map_err(|_| anyhow::anyhow!("读取命令输出失败"))??;
    let stderr = errors
        .join()
        .map_err(|_| anyhow::anyhow!("读取命令错误失败"))??;
    if let Some(writer) = writer {
        writer
            .join()
            .map_err(|_| anyhow::anyhow!("写入命令输入失败"))??;
    }
    if stdout.len() > MAX_OUTPUT {
        bail!("差异过大，请选择较小的审查范围");
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_is_bounded_but_the_pipe_is_fully_drained() {
        let input = vec![b'x'; 100_000];
        assert_eq!(read_limited(input.as_slice(), 512).unwrap().len(), 513);
    }

    #[test]
    fn large_stdin_and_both_output_streams_do_not_deadlock() {
        let input = vec![b'x'; 256 * 1024];
        let output = run(
            Command::new("sh").args(["-c", "cat; printf diagnostic >&2"]),
            Some(&input),
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, input);
        assert_eq!(output.stderr, b"diagnostic");
    }

    #[test]
    fn deadline_includes_blocked_stdin_and_inherited_output_pipes() {
        for script in ["sleep 30", "sleep 30 & exit 0"] {
            let start = Instant::now();
            let result = run(
                Command::new("sh").args(["-c", script]),
                Some(&vec![b'x'; 1024 * 1024]),
                Duration::from_millis(100),
            );
            assert!(result.unwrap_err().to_string().contains("超时"));
            assert!(start.elapsed() < Duration::from_secs(2));
        }
    }
}
