//! Automatic review observations. These never carry a user approval responder.

use super::requests::AgentPermissionRequestProfile;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AgentAutoApprovalReviewKey {
    pub thread_id: String,
    pub turn_id: String,
    pub review_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentAutoApprovalReviewStatus {
    InProgress,
    Approved,
    Denied,
    TimedOut,
    Aborted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentAutoApprovalReviewAction {
    Command {
        command: String,
        cwd: String,
        source: String,
    },
    Execve {
        program: String,
        argv: Vec<String>,
        cwd: String,
        source: String,
    },
    WriteStdin {
        approval_id: String,
        process_id: String,
        stdin: String,
        cwd: String,
    },
    ApplyPatch {
        cwd: String,
        files: Vec<String>,
    },
    NetworkAccess {
        host: String,
        port: u16,
        protocol: String,
        target: String,
    },
    McpToolCall {
        server: String,
        tool_name: String,
        tool_title: Option<String>,
        connector_id: Option<String>,
        connector_name: Option<String>,
    },
    RequestPermissions {
        permissions: Box<AgentPermissionRequestProfile>,
        reason: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentAutoApprovalReview {
    pub key: AgentAutoApprovalReviewKey,
    pub target_item_id: Option<String>,
    pub action: AgentAutoApprovalReviewAction,
    pub status: AgentAutoApprovalReviewStatus,
    pub rationale: Option<String>,
    pub risk_level: Option<String>,
    pub user_authorization: Option<String>,
    pub started_at_ms: i64,
    pub completed_at_ms: Option<i64>,
    pub decision_source: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentStrictReviewRequirement {
    pub thread_id: String,
    pub turn_id: String,
    pub started_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentGuardianWarning {
    pub thread_id: String,
    pub message: String,
}
