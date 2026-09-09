//! Connection-scoped and turn-scoped events delivered by agent backends.

use super::{
    activity::{
        AgentCollaboration, AgentContextCompaction, AgentFileChange, AgentFileChangeEntry,
        AgentImageGeneration, AgentImageView, AgentMcpToolCall, AgentReasoning, CommandExecution,
    },
    catalog::AgentThreadSettings,
    requests::{
        AgentApprovalHandle, AgentCommandApprovalRequest, AgentPermissionsApprovalHandle,
        AgentPermissionsApprovalRequest, AgentServerRequestFailureKind, AgentServerRequestMetadata,
        AgentUserInputHandle, AgentUserInputRequest,
    },
    status::{
        AgentAccountRateLimits, AgentConfigWarning, AgentMcpServerStartupStatus, AgentThreadStatus,
        AgentThreadTokenUsage,
    },
    thread::{ProjectChange, ProjectId, ThreadId},
};

/// Agent-neutral events whose lifetime belongs to a backend connection or a
/// loaded thread rather than to one particular turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentConnectionEvent {
    Warning {
        thread_id: Option<String>,
        message: String,
    },
    ConfigWarning(AgentConfigWarning),
    McpServerStartupStatusUpdated(AgentMcpServerStartupStatus),
    ThreadStatusChanged(AgentThreadStatus),
    ThreadSettingsUpdated {
        thread_id: String,
        settings: AgentThreadSettings,
    },
    ProjectChanged {
        project_id: ProjectId,
        change: ProjectChange,
    },
    ThreadArchived {
        thread_id: ThreadId,
    },
    ThreadUnarchived {
        thread_id: ThreadId,
    },
    ThreadDeleted {
        thread_id: ThreadId,
    },
    ThreadNameUpdated {
        thread_id: ThreadId,
        name: Option<String>,
    },
    ThreadClosed {
        thread_id: ThreadId,
    },
    ThreadProjectUpdated {
        thread_id: ThreadId,
        project_id: Option<ProjectId>,
    },
    AccountRateLimitsUpdated(AgentAccountRateLimits),
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
    ReasoningStarted {
        reasoning: AgentReasoning,
        started_at_ms: i64,
    },
    ReasoningSummaryPartAdded {
        item_id: String,
        summary_index: usize,
    },
    ReasoningSummaryTextDelta {
        item_id: String,
        summary_index: usize,
        delta: String,
    },
    ReasoningTextDelta {
        item_id: String,
        content_index: usize,
        delta: String,
    },
    ReasoningCompleted {
        reasoning: AgentReasoning,
        completed_at_ms: i64,
    },
    CommandStarted(CommandExecution),
    CommandOutputDelta {
        item_id: String,
        delta: String,
    },
    CommandTerminalInteraction {
        item_id: String,
        process_id: String,
        /// Preserve the interaction semantic without retaining possibly
        /// sensitive terminal input in the UI model.
        wrote_stdin: bool,
    },
    CommandCompleted(CommandExecution),
    FileChangeUpdated(AgentFileChange),
    ImageViewed(AgentImageView),
    ImageGenerationUpdated(AgentImageGeneration),
    PlanUpdated(super::activity::AgentPlan),
    PlanDelta {
        item_id: String,
        delta: String,
    },
    TurnPlanUpdated(super::activity::AgentTurnPlan),
    WebSearchUpdated(super::activity::AgentWebSearch),
    SleepUpdated(super::activity::AgentSleep),
    ContextCompactionUpdated(AgentContextCompaction),
    CollaborationUpdated(AgentCollaboration),
    McpToolCallUpdated(AgentMcpToolCall),
    McpToolCallProgress {
        item_id: String,
        message: String,
    },
    FileChangePatchUpdated {
        item_id: String,
        changes: Vec<AgentFileChangeEntry>,
    },
    TurnDiffUpdated {
        diff: String,
    },
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
