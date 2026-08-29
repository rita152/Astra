mod codex;

use std::path::PathBuf;

use async_channel::Receiver;

pub use codex::CodexAppServerBackend;

/// Agent-neutral input consumed by every coding-agent adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRequest {
    pub prompt: String,
    pub cwd: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandExecutionStatus {
    InProgress,
    Completed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandExecution {
    pub id: String,
    pub command: String,
    pub cwd: String,
    pub output: String,
    pub status: CommandExecutionStatus,
    pub exit_code: Option<i64>,
}

/// Agent-neutral output consumed by the UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentEvent {
    Started,
    AssistantMessageStarted { item_id: String },
    TextDelta(String),
    CommandStarted(CommandExecution),
    CommandOutputDelta { item_id: String, delta: String },
    CommandCompleted(CommandExecution),
    Completed,
    Failed(String),
}

/// Boundary between the application and a concrete coding-agent protocol.
pub trait AgentBackend: Send + Sync {
    fn run_prompt(&self, request: AgentRequest) -> Receiver<AgentEvent>;
}
