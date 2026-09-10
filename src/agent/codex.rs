//! Codex app-server adapter. Protocol details remain inside this module.

#[cfg(feature = "screenshot")]
mod approval_capture;
mod approvals;
mod auto_approval;
mod backend;
mod catalog;
mod dispatch;
mod input;
mod items;
mod manager;
mod methods;
mod notifications;
mod permissions;
mod progress;
mod registry;
mod requests;
mod runtime;
mod session;
mod transport;
mod workspace_protocol;

#[cfg(test)]
mod approvals_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

pub use backend::CodexAppServerBackend;
use catalog::{MODEL_LIST_PAGE_SIZE, ModelListResponse, PermissionProfileListResponse};
use dispatch::process_turn_message;
use items::{
    materialize_image_generation_result, parse_collaboration, parse_command_execution,
    parse_image_generation, parse_mcp_tool_call,
};
pub use manager::CodexAppServerManager;
use methods::{
    TURN_SCOPED_SERVER_METHODS, ensure_server_method_is_defined,
    is_integrated_server_request_method,
};
use notifications::{
    parse_agent_notification, parse_mcp_server_startup_status_updated, parse_thread_status_changed,
    thread_started_id, validate_remote_control_status_changed, validate_resume_goal_cleared,
};
use permissions::{permission_fields, thread_settings_update_request};
use requests::{handle_server_request_resolved, request_id_from_value};
use session::{
    CodexTurnSession, TurnOutcome, cleanup_pending_server_requests, ensure_session_message_matches,
};
#[cfg(test)]
use test_support::{
    INITIALIZE_ID, drive_model_catalog, drive_permission_profiles, drive_session,
    drive_thread_settings_update, finish_prompt_session, run_model_catalog_process,
    wait_for_response,
};
use transport::{AppServerProcess, send};

#[cfg(test)]
use crate::agent::{
    AgentAccountRateLimits, AgentBackend, AgentCommandApprovalChoice, AgentConfigWarning,
    AgentCreditsSnapshot, AgentEvent, AgentFileChangeStatus, AgentImageGenerationFailure,
    AgentImageGenerationStatus, AgentImageView, AgentInterruptControl, AgentInterruptHandle,
    AgentInterruptOutcome, AgentMcpServerStartupFailureReason, AgentMcpServerStartupState,
    AgentMcpServerStartupStatus, AgentMcpToolCall, AgentMcpToolCallStatus, AgentOptionalField,
    AgentPermissionMode, AgentPermissionProfile, AgentPermissionsApprovalChoice,
    AgentRateLimitWindow, AgentReasoning, AgentRequest, AgentServerRequestFailureKind,
    AgentServerRequestId, AgentServerRequestKind, AgentServerRequestMetadata,
    AgentThreadActiveFlag, AgentThreadSettings, AgentThreadStatus, AgentThreadStatusState,
    AgentThreadTokenUsage, AgentTokenUsageBreakdown, AgentUserInputResponse, CommandExecution,
    CommandExecutionAction, CommandExecutionStatus,
};

#[cfg(test)]
use methods::UNDEFINED_METHOD_PARAMS_LIMIT;
#[cfg(test)]
use requests::respond_to_server_request_on_session;
