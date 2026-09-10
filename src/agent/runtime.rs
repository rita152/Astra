//! Runtime observations are independent of requests, approvals, and turn success.

mod state;
pub use state::AgentRuntimeState;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentDeprecationNotice {
    pub summary: String,
    pub details: Option<String>,
}

/// Why the client stopped waiting. This never rewrites a server result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentLocalClosure {
    TurnCompleted,
    Interrupted,
    Failed,
    Disconnected,
    ThreadClosed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentAuthRecovery {
    pub thread_id: String,
    pub turn_id: String,
    pub provider: String,
    pub started_message: Option<String>,
    pub completed_message: Option<String>,
    pub closed_locally: Option<AgentLocalClosure>,
}

impl AgentAuthRecovery {
    pub fn is_waiting(&self) -> bool {
        self.completed_message.is_none() && self.closed_locally.is_none()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentHookStatus {
    Running,
    Completed,
    Failed,
    Blocked,
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentHookOutput {
    pub kind: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentHookRun {
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub id: String,
    pub display_order: i64,
    pub event_name: String,
    pub execution_mode: String,
    pub handler_type: String,
    pub scope: String,
    pub source: String,
    pub source_path: String,
    pub status: AgentHookStatus,
    pub status_message: Option<String>,
    pub entries: Vec<AgentHookOutput>,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub received_completed: bool,
    pub closed_locally: Option<AgentLocalClosure>,
}

impl AgentHookRun {
    pub fn is_waiting(&self) -> bool {
        self.status == AgentHookStatus::Running
            && !self.received_completed
            && self.closed_locally.is_none()
    }

    /// Server completion is monotonic even when its optional timestamp is absent.
    pub fn would_regress(&self, update: &Self) -> bool {
        (self.received_completed && !update.received_completed)
            || (self.status != AgentHookStatus::Running
                && update.status == AgentHookStatus::Running)
            || (!update.received_completed
                && update.status == AgentHookStatus::Running
                && update.started_at < self.started_at)
            || self
                .completed_at
                .zip(update.completed_at)
                .is_some_and(|(old, new)| new < old)
    }
}

/// Hook-injected model input; never a user message or an approval request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentHookPrompt {
    pub id: String,
    pub fragments: Vec<AgentHookPromptFragment>,
    /// None in history: the item payload has no completion status or timestamp.
    pub completed: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentHookPromptFragment {
    pub hook_run_id: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentScopedHookPrompt {
    pub thread_id: String,
    pub turn_id: String,
    pub prompt: AgentHookPrompt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRuntimeEvent {
    pub generation: u64,
    pub observation: AgentRuntimeObservation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentRuntimeObservation {
    GenerationStarted,
    Hook(Box<AgentHookRun>),
    HookPrompt(AgentScopedHookPrompt),
    AuthRecovery(AgentAuthRecovery),
    TurnClosed {
        thread_id: String,
        turn_id: String,
        reason: AgentLocalClosure,
    },
    ThreadClosed {
        thread_id: String,
    },
    Disconnected,
}
