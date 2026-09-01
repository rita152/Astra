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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentMcpServerStartupState {
    Starting,
    Ready,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentMcpServerStartupFailureReason {
    ReauthenticationRequired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpServerStartupStatus {
    pub thread_id: Option<String>,
    pub name: String,
    pub state: AgentMcpServerStartupState,
    pub error: Option<String>,
    pub failure_reason: Option<AgentMcpServerStartupFailureReason>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentThreadActiveFlag {
    WaitingOnApproval,
    WaitingOnUserInput,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentThreadStatusState {
    NotLoaded,
    Idle,
    SystemError,
    Active {
        active_flags: Vec<AgentThreadActiveFlag>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadStatus {
    pub thread_id: String,
    pub state: AgentThreadStatusState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTokenUsageBreakdown {
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub cache_write_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadTokenUsage {
    pub thread_id: String,
    pub turn_id: String,
    pub total: AgentTokenUsageBreakdown,
    pub last: AgentTokenUsageBreakdown,
    pub model_context_window: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRateLimitWindow {
    pub used_percent: i32,
    pub window_duration_mins: Option<i64>,
    pub resets_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentCreditsSnapshot {
    pub has_credits: bool,
    pub unlimited: bool,
    pub balance: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSpendControlLimit {
    pub limit: String,
    pub used: String,
    pub remaining_percent: i32,
    pub resets_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentAccountRateLimits {
    pub limit_id: Option<String>,
    pub limit_name: Option<String>,
    pub primary: Option<AgentRateLimitWindow>,
    pub secondary: Option<AgentRateLimitWindow>,
    pub credits: Option<AgentCreditsSnapshot>,
    pub individual_limit: Option<AgentSpendControlLimit>,
    pub spend_control_reached: Option<bool>,
    pub plan_type: Option<String>,
    pub rate_limit_reached_type: Option<String>,
}

/// JSON-RPC request ids are deliberately not normalized: a numeric `7` and a
/// string `"7"` identify different server requests and must be echoed with
/// their original type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AgentServerRequestId {
    Number(i64),
    String(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AgentServerRequestKind {
    CommandApproval,
    UserInput,
    PermissionsApproval,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentServerRequestMetadata {
    pub request_id: AgentServerRequestId,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub kind: AgentServerRequestKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum AgentOptionalField<T> {
    #[default]
    Unspecified,
    Null,
    Value(T),
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
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
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
    pub can_accept_with_execpolicy_amendment: bool,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentUserInputOption {
    pub label: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentUserInputQuestion {
    pub id: String,
    pub header: String,
    pub question: String,
    pub options: Vec<AgentUserInputOption>,
    pub allows_other: bool,
    pub is_secret: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentUserInputRequest {
    pub request_id: AgentServerRequestId,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub questions: Vec<AgentUserInputQuestion>,
    pub is_blocking: bool,
    pub auto_resolution_ms: Option<u64>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct AgentUserInputAnswer {
    pub question_id: String,
    pub answers: Vec<String>,
}

impl fmt::Debug for AgentUserInputAnswer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentUserInputAnswer")
            .field("question_id", &self.question_id)
            .field("answer_count", &self.answers.len())
            .field("answers", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct AgentUserInputResponse {
    pub answers: Vec<AgentUserInputAnswer>,
}

impl fmt::Debug for AgentUserInputResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentUserInputResponse")
            .field("answers", &self.answers)
            .finish()
    }
}

pub(crate) trait AgentUserInputControl: Send + Sync {
    fn respond(
        &self,
        request_id: &AgentServerRequestId,
        response: AgentUserInputResponse,
    ) -> Result<(), String>;
}

#[derive(Clone)]
pub struct AgentUserInputHandle {
    request_id: AgentServerRequestId,
    control: Arc<dyn AgentUserInputControl>,
}

impl AgentUserInputHandle {
    pub(crate) fn new(
        request_id: AgentServerRequestId,
        control: Arc<dyn AgentUserInputControl>,
    ) -> Self {
        Self {
            request_id,
            control,
        }
    }

    pub fn respond(&self, response: AgentUserInputResponse) -> Result<(), String> {
        self.control.respond(&self.request_id, response)
    }
}

impl fmt::Debug for AgentUserInputHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentUserInputHandle")
            .field("request_id", &self.request_id)
            .finish_non_exhaustive()
    }
}

impl PartialEq for AgentUserInputHandle {
    fn eq(&self, other: &Self) -> bool {
        self.request_id == other.request_id && Arc::ptr_eq(&self.control, &other.control)
    }
}

impl Eq for AgentUserInputHandle {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentFileSystemAccess {
    Read,
    Write,
    Deny,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentFileSystemSpecialPath {
    Root,
    Minimal,
    ProjectRoots {
        subpath: AgentOptionalField<String>,
    },
    Tmpdir,
    SlashTmp,
    Unknown {
        path: String,
        subpath: AgentOptionalField<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentFileSystemPath {
    Path(String),
    GlobPattern(String),
    Special(AgentFileSystemSpecialPath),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentFileSystemPermissionEntry {
    pub path: AgentFileSystemPath,
    pub access: AgentFileSystemAccess,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentAdditionalFileSystemPermissions {
    pub read: AgentOptionalField<Vec<String>>,
    pub write: AgentOptionalField<Vec<String>>,
    pub glob_scan_max_depth: AgentOptionalField<u64>,
    pub entries: AgentOptionalField<Vec<AgentFileSystemPermissionEntry>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentAdditionalNetworkPermissions {
    pub enabled: AgentOptionalField<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentPermissionRequestProfile {
    pub file_system: AgentOptionalField<AgentAdditionalFileSystemPermissions>,
    pub network: AgentOptionalField<AgentAdditionalNetworkPermissions>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPermissionsApprovalRequest {
    pub request_id: AgentServerRequestId,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub environment_id: Option<String>,
    pub started_at_ms: i64,
    pub cwd: String,
    pub reason: Option<String>,
    pub permissions: AgentPermissionRequestProfile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentPermissionsApprovalChoice {
    AllowOnce,
    AllowForSession,
    Decline,
}

pub(crate) trait AgentPermissionsApprovalControl: Send + Sync {
    fn respond(
        &self,
        request_id: &AgentServerRequestId,
        choice: AgentPermissionsApprovalChoice,
    ) -> Result<(), String>;
}

#[derive(Clone)]
pub struct AgentPermissionsApprovalHandle {
    request_id: AgentServerRequestId,
    control: Arc<dyn AgentPermissionsApprovalControl>,
}

impl AgentPermissionsApprovalHandle {
    pub(crate) fn new(
        request_id: AgentServerRequestId,
        control: Arc<dyn AgentPermissionsApprovalControl>,
    ) -> Self {
        Self {
            request_id,
            control,
        }
    }

    pub fn respond(&self, choice: AgentPermissionsApprovalChoice) -> Result<(), String> {
        self.control.respond(&self.request_id, choice)
    }
}

impl fmt::Debug for AgentPermissionsApprovalHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentPermissionsApprovalHandle")
            .field("request_id", &self.request_id)
            .finish_non_exhaustive()
    }
}

impl PartialEq for AgentPermissionsApprovalHandle {
    fn eq(&self, other: &Self) -> bool {
        self.request_id == other.request_id && Arc::ptr_eq(&self.control, &other.control)
    }
}

impl Eq for AgentPermissionsApprovalHandle {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentServerRequestFailureKind {
    Cancelled,
    Failed,
}

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
    McpServerStartupStatusUpdated(AgentMcpServerStartupStatus),
    ThreadStatusChanged(AgentThreadStatus),
    ThreadTokenUsageUpdated(AgentThreadTokenUsage),
    AccountRateLimitsUpdated(AgentAccountRateLimits),
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
    UserInputRequested {
        request: AgentUserInputRequest,
        responder: AgentUserInputHandle,
    },
    PermissionsApprovalRequested {
        request: AgentPermissionsApprovalRequest,
        responder: AgentPermissionsApprovalHandle,
    },
    ServerRequestResolved {
        request: AgentServerRequestMetadata,
    },
    ServerRequestFailed {
        request: AgentServerRequestMetadata,
        kind: AgentServerRequestFailureKind,
        message: String,
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
