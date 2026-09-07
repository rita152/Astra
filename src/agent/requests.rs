//! Interactive server requests and typed response handles.

use std::{fmt, sync::Arc};

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
