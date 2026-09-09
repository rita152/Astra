//! Stable application boundary for coding-agent backends.
//! Domain modules depend on each other explicitly; concrete protocols stay in adapters.

mod activity;
mod auto_approval;
mod backend;
mod catalog;
mod codex;
mod events;
mod message;
mod requests;
mod status;
mod thread;

pub use activity::{
    AgentCollaboration, AgentCollaborationStatus, AgentCollaborationTool, AgentCollaboratorState,
    AgentCollaboratorStatus, AgentContextCompaction, AgentFileChange, AgentFileChangeEntry,
    AgentFileChangeKind, AgentFileChangeStatus, AgentImageGeneration, AgentImageGenerationFailure,
    AgentImageGenerationStatus, AgentImageView, AgentMcpToolCall, AgentMcpToolCallStatus,
    AgentReasoning, CommandExecution, CommandExecutionAction, CommandExecutionStatus,
    LegacySubAgentActivityKind,
};
pub use auto_approval::{
    AgentAutoApprovalReview, AgentAutoApprovalReviewAction, AgentAutoApprovalReviewKey,
    AgentAutoApprovalReviewStatus, AgentGuardianWarning, AgentStrictReviewRequirement,
};
pub(crate) use backend::AgentInterruptControl;
pub use backend::{
    AgentBackend, AgentCapabilities, AgentCapability, AgentInputFile, AgentInterruptHandle,
    AgentInterruptOutcome, AgentPromptContext, AgentRequest, AgentRun, SideConversationRequest,
    WorkspaceError, WorkspaceResult,
};
pub use catalog::{
    AgentActivePermissionProfile, AgentEffectivePermissions, AgentModel, AgentModelCatalog,
    AgentPermissionMode, AgentPermissionProfile, AgentReasoningEffort, AgentServiceTier,
    AgentThreadSettings,
};
pub use codex::{CodexAppServerBackend, CodexAppServerManager};
pub use events::{AgentConnectionEvent, AgentEvent};
pub use message::normalize_user_message_for_display;
pub use requests::{
    AgentAdditionalFileSystemPermissions, AgentAdditionalNetworkPermissions, AgentApprovalHandle,
    AgentCommandApprovalChoice, AgentCommandApprovalRequest, AgentFileSystemAccess,
    AgentFileSystemPath, AgentFileSystemPermissionEntry, AgentFileSystemSpecialPath,
    AgentOptionalField, AgentPermissionRequestProfile, AgentPermissionsApprovalChoice,
    AgentPermissionsApprovalHandle, AgentPermissionsApprovalRequest, AgentServerRequestFailureKind,
    AgentServerRequestId, AgentServerRequestKind, AgentServerRequestMetadata, AgentUserInputAnswer,
    AgentUserInputHandle, AgentUserInputOption, AgentUserInputQuestion, AgentUserInputRequest,
    AgentUserInputResponse,
};
pub(crate) use requests::{
    AgentApprovalControl, AgentPermissionsApprovalControl, AgentUserInputControl,
};
pub use status::{
    AgentAccountRateLimits, AgentConfigWarning, AgentCreditsSnapshot,
    AgentMcpServerStartupFailureReason, AgentMcpServerStartupState, AgentMcpServerStartupStatus,
    AgentRateLimitWindow, AgentSpendControlLimit, AgentThreadActiveFlag, AgentThreadStatus,
    AgentThreadStatusState, AgentThreadTokenUsage, AgentTokenUsageBreakdown,
};
pub use thread::{
    CreateProject, FilterValue, HistoryItemDetail, HistoryTurnStatus, Page, PageRequest, Project,
    ProjectChange, ProjectId, SortDirection, ThreadActivity, ThreadHistory, ThreadHistoryItem,
    ThreadHistoryItemEntry, ThreadId, ThreadListRequest, ThreadMetadataUpdate, ThreadSearchResult,
    ThreadSection, ThreadSectionAppearance, ThreadSectionId, ThreadSortKey, ThreadSummary,
    ThreadTurn, UpdateProject, UserMessageImage,
};
