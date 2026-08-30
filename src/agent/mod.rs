mod codex;

use std::{path::PathBuf, sync::Arc};

use async_channel::Receiver;

pub use codex::CodexAppServerBackend;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentReasoningEffort {
    pub id: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentServiceTier {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentModel {
    pub id: String,
    pub model: String,
    pub display_name: String,
    pub description: String,
    pub supported_reasoning_efforts: Vec<AgentReasoningEffort>,
    pub default_reasoning_effort: String,
    pub service_tiers: Vec<AgentServiceTier>,
    pub default_service_tier: Option<String>,
    pub is_default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentModelCatalog {
    pub models: Vec<AgentModel>,
}

/// Agent-neutral input consumed by every coding-agent adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRequest {
    pub prompt: String,
    pub cwd: PathBuf,
    pub model: String,
    pub effort: String,
    pub service_tier: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentInterruptOutcome {
    Requested,
    AlreadyRequested,
    AlreadyFinished,
}

pub(crate) trait AgentInterruptControl: Send + Sync {
    fn request_interrupt(&self) -> Result<AgentInterruptOutcome, String>;
    fn abandon(&self);
}

pub struct AgentInterruptHandle {
    control: Arc<dyn AgentInterruptControl>,
}

impl AgentInterruptHandle {
    pub(crate) fn new(control: Arc<dyn AgentInterruptControl>) -> Self {
        Self { control }
    }

    pub fn interrupt(&self) -> Result<AgentInterruptOutcome, String> {
        self.control.request_interrupt()
    }
}

impl Drop for AgentInterruptHandle {
    fn drop(&mut self) {
        self.control.abandon();
    }
}

pub struct AgentRun {
    events: Receiver<AgentEvent>,
    interrupt: Option<AgentInterruptHandle>,
}

impl AgentRun {
    pub(crate) fn new(
        events: Receiver<AgentEvent>,
        interrupt: Option<AgentInterruptHandle>,
    ) -> Self {
        Self { events, interrupt }
    }

    pub fn into_parts(self) -> (Receiver<AgentEvent>, Option<AgentInterruptHandle>) {
        (self.events, self.interrupt)
    }
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
    AssistantMessageStarted {
        item_id: String,
    },
    TextDelta(String),
    CommandStarted(CommandExecution),
    CommandOutputDelta {
        item_id: String,
        delta: String,
    },
    CommandCompleted(CommandExecution),
    ModelRerouted {
        from_model: String,
        to_model: String,
        reason: String,
    },
    ModelVerificationRequired {
        verifications: Vec<String>,
    },
    ModelSafetyBufferingUpdated {
        model: String,
        use_cases: Vec<String>,
        reasons: Vec<String>,
        show_buffering_ui: bool,
        faster_model: Option<String>,
    },
    Completed,
    Interrupted,
    Failed(String),
}

/// Boundary between the application and a concrete coding-agent protocol.
pub trait AgentBackend: Send + Sync {
    #[cfg_attr(test, allow(dead_code))]
    fn load_model_catalog(&self) -> Receiver<Result<AgentModelCatalog, String>>;
    fn run_prompt(&self, request: AgentRequest) -> AgentRun;
}
