mod codex;

use std::{path::PathBuf, sync::mpsc::Receiver};

pub use codex::CodexAppServerBackend;

/// Agent-neutral input consumed by every coding-agent adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRequest {
    pub prompt: String,
    pub cwd: PathBuf,
}

/// Agent-neutral output consumed by the UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentEvent {
    Started,
    TextDelta(String),
    Completed,
    Failed(String),
}

/// Boundary between the application and a concrete coding-agent protocol.
pub trait AgentBackend: Send + Sync {
    fn run_prompt(&self, request: AgentRequest) -> Receiver<AgentEvent>;
}
