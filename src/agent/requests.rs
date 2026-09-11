//! Interactive server requests and typed response handles.

use std::{fmt, sync::Arc};

// Keep the four response types distinct while sharing their handle mechanics.
// Equality deliberately includes control identity: reused request IDs from a
// different connection must never compare equal. Debug must not expose controls.
macro_rules! response_handle {
    ($handle:ident, $control:ident, $response:ty, $argument:ident) => {
        pub(crate) trait $control: Send + Sync {
            fn respond(
                &self,
                request_id: &AgentServerRequestId,
                $argument: $response,
            ) -> Result<(), String>;
        }

        #[derive(Clone)]
        pub struct $handle {
            request_id: AgentServerRequestId,
            control: Arc<dyn $control>,
        }

        impl $handle {
            pub(crate) fn new(
                request_id: AgentServerRequestId,
                control: Arc<dyn $control>,
            ) -> Self {
                Self {
                    request_id,
                    control,
                }
            }

            pub fn respond(&self, $argument: $response) -> Result<(), String> {
                self.control.respond(&self.request_id, $argument)
            }
        }

        impl fmt::Debug for $handle {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_struct(stringify!($handle))
                    .field("request_id", &self.request_id)
                    .finish_non_exhaustive()
            }
        }

        impl PartialEq for $handle {
            fn eq(&self, other: &Self) -> bool {
                self.request_id == other.request_id && Arc::ptr_eq(&self.control, &other.control)
            }
        }

        impl Eq for $handle {}
    };
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
    FileApproval,
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
    pub approval_id: Option<String>,
    pub kind: AgentCommandApprovalKind,
    pub environment_id: Option<String>,
    pub started_at_ms: i64,
    pub cwd: Option<String>,
    pub command: String,
    pub reason: Option<String>,
    pub network: Option<AgentNetworkApprovalContext>,
    pub additional_permissions: AgentOptionalField<AgentPermissionRequestProfile>,
    /// Ordered, validated choices. A missing/null wire list uses the protocol's
    /// legacy choices, with amendments only when the server proposes them.
    pub available_decisions: Vec<AgentCommandApprovalChoice>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentCommandApprovalKind {
    Command,
    WriteStdin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentNetworkApprovalContext {
    pub host: String,
    pub protocol: AgentNetworkApprovalProtocol,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentNetworkApprovalProtocol {
    Http,
    Https,
    Socks5Tcp,
    Socks5Udp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentNetworkPolicyAction {
    Allow,
    Deny,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentNetworkPolicyAmendment {
    pub host: String,
    pub action: AgentNetworkPolicyAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentCommandApprovalChoice {
    Accept,
    AcceptForSession,
    /// Reject this command item while allowing the active turn to continue.
    Decline,
    /// Reject the command and interrupt its turn. Never substitute `decline`.
    Cancel,
    AcceptWithExecpolicyAmendment(Vec<String>),
    ApplyNetworkPolicyAmendment(AgentNetworkPolicyAmendment),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentFileApprovalRequest {
    pub request_id: AgentServerRequestId,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub started_at_ms: i64,
    pub reason: Option<String>,
    pub grant_root: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentFileApprovalChoice {
    Accept,
    AcceptForSession,
    Decline,
    Cancel,
}

response_handle!(
    AgentFileApprovalHandle,
    AgentFileApprovalControl,
    AgentFileApprovalChoice,
    choice
);

response_handle!(
    AgentApprovalHandle,
    AgentApprovalControl,
    AgentCommandApprovalChoice,
    choice
);

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

response_handle!(
    AgentUserInputHandle,
    AgentUserInputControl,
    AgentUserInputResponse,
    response
);

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

response_handle!(
    AgentPermissionsApprovalHandle,
    AgentPermissionsApprovalControl,
    AgentPermissionsApprovalChoice,
    choice
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentServerRequestFailureKind {
    Cancelled,
    Failed,
}

#[cfg(test)]
mod tests;
