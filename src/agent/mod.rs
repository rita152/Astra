mod codex;

use std::{fmt, path::PathBuf, sync::Arc};

use async_channel::Receiver;
use serde_json::Value;

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
    pub thread_id: Option<String>,
    pub model: String,
    pub effort: String,
    pub service_tier: Option<String>,
    pub permission_mode: AgentPermissionMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentPermissionMode {
    Request,
    Assist,
    Full,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentActivePermissionProfile {
    pub id: String,
    pub extends: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentEffectivePermissions {
    pub approval_policy: String,
    pub approvals_reviewer: String,
    pub sandbox_policy: Option<Value>,
    pub active_permission_profile: Option<AgentActivePermissionProfile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub struct AgentPermissionProfile {
    pub id: String,
    pub allowed: bool,
    pub extends: Option<String>,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadSettings {
    pub model: String,
    pub effort: Option<String>,
    pub service_tier: Option<String>,
    pub cwd: String,
    pub permissions: Option<AgentEffectivePermissions>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentConfigWarning {
    pub summary: String,
    pub details: Option<String>,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub column: Option<u64>,
}

/// JSON-RPC request ids are deliberately not normalized: a numeric `7` and a
/// string `"7"` identify different server requests and must be echoed with
/// their original type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AgentServerRequestId {
    Number(i64),
    String(String),
}

impl AgentServerRequestId {
    pub fn ui_key(&self) -> String {
        match self {
            Self::Number(id) => format!("number:{id}"),
            Self::String(id) => format!("string:{id}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentCommandApprovalRequest {
    pub request_id: AgentServerRequestId,
    pub command: String,
    pub reason: Option<String>,
    pub network_host: Option<String>,
    pub allow_once: bool,
    /// Whether the server listed the legacy `decline` decision.
    pub decline: bool,
    /// Whether the server listed `cancel`. This enables the ChatGPT-compatible
    /// Reject affordance, but that affordance still sends `decline` so the turn
    /// can continue.
    pub cancel: bool,
    /// The exact decision object supplied by `availableDecisions`.
    pub accept_with_execpolicy_amendment: Option<Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentCommandApprovalChoice {
    Accept,
    /// Reject this command item while allowing the active turn to continue.
    Decline,
    AcceptWithExecpolicyAmendment,
}

pub(crate) trait AgentApprovalControl: Send + Sync {
    fn respond(
        &self,
        request_id: &AgentServerRequestId,
        choice: AgentCommandApprovalChoice,
    ) -> Result<(), String>;
}

#[derive(Clone)]
pub struct AgentApprovalHandle {
    request_id: AgentServerRequestId,
    control: Arc<dyn AgentApprovalControl>,
}

impl AgentApprovalHandle {
    pub(crate) fn new(
        request_id: AgentServerRequestId,
        control: Arc<dyn AgentApprovalControl>,
    ) -> Self {
        Self {
            request_id,
            control,
        }
    }

    pub fn respond(&self, choice: AgentCommandApprovalChoice) -> Result<(), String> {
        self.control.respond(&self.request_id, choice)
    }
}

impl fmt::Debug for AgentApprovalHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentApprovalHandle")
            .field("request_id", &self.request_id)
            .finish_non_exhaustive()
    }
}

impl PartialEq for AgentApprovalHandle {
    fn eq(&self, other: &Self) -> bool {
        self.request_id == other.request_id && Arc::ptr_eq(&self.control, &other.control)
    }
}

impl Eq for AgentApprovalHandle {}

/// Agent-neutral output consumed by the UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentEvent {
    ThreadCreated {
        thread_id: String,
    },
    Started,
    Error {
        message: String,
        details: Option<String>,
        will_retry: bool,
    },
    ThreadSettingsUpdated(AgentThreadSettings),
    Warning {
        message: String,
    },
    ConfigWarning(AgentConfigWarning),
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
    CommandApprovalRequested {
        request: AgentCommandApprovalRequest,
        responder: AgentApprovalHandle,
    },
    CommandApprovalResolved {
        request_id: AgentServerRequestId,
    },
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
    #[allow(dead_code)]
    fn load_permission_profiles(
        &self,
        cwd: PathBuf,
    ) -> Receiver<Result<Vec<AgentPermissionProfile>, String>>;
    fn update_thread_permissions(
        &self,
        thread_id: String,
        cwd: PathBuf,
        mode: AgentPermissionMode,
    ) -> Receiver<Result<AgentThreadSettings, String>>;
    fn run_prompt(&self, request: AgentRequest) -> AgentRun;
}
