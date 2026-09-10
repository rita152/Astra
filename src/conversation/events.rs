//! Agent event reduction and connection event scoping.

use super::{
    activity::{
        ConversationActivity, ensure_reasoning_part, find_command_activity_mut,
        find_mcp_tool_call_activity_mut, find_reasoning_activity_mut, permission_presentation_data,
        remove_unfinished_image_generations, upsert_collaboration_activity,
        upsert_command_activity, upsert_context_compaction_activity, upsert_mcp_tool_call_activity,
        upsert_reasoning_completed, upsert_reasoning_started,
    },
    state::ConversationState,
    transcript::{ConversationPhase, current_local_time_label},
};
use crate::{
    agent::{
        AgentAccountRateLimits, AgentConnectionEvent, AgentCreditsSnapshot, AgentEvent,
        AgentFileChange, AgentFileChangeStatus, AgentMcpServerStartupFailureReason,
        AgentMcpServerStartupState, AgentPermissionsApprovalChoice, AgentRateLimitWindow,
        AgentServerRequestFailureKind, AgentServerRequestKind, AgentServerRequestMetadata,
        AgentUserInputResponse, CommandExecution, CommandExecutionStatus,
    },
    components::{
        approval::ApprovalCardStatus,
        file_change::DiffReviewPresentation,
        permissions_approval::{PermissionApprovalPresentation, PermissionApprovalStatus},
        user_input_request::{
            UserInputOptionPresentation, UserInputQuestionPresentation,
            UserInputRequestPresentation, UserInputRequestStatus,
        },
    },
};

pub(crate) fn merge_available<T>(current: &mut Option<T>, update: Option<T>) {
    if let Some(update) = update {
        *current = Some(update);
    }
}

pub(crate) fn merge_rate_limit_window(
    current: &mut Option<AgentRateLimitWindow>,
    update: Option<AgentRateLimitWindow>,
) {
    let Some(update) = update else {
        return;
    };
    if let Some(current) = current {
        current.used_percent = update.used_percent;
        merge_available(
            &mut current.window_duration_mins,
            update.window_duration_mins,
        );
        merge_available(&mut current.resets_at, update.resets_at);
    } else {
        *current = Some(update);
    }
}

pub(crate) fn merge_credits_snapshot(
    current: &mut Option<AgentCreditsSnapshot>,
    update: Option<AgentCreditsSnapshot>,
) {
    let Some(update) = update else {
        return;
    };
    if let Some(current) = current {
        current.has_credits = update.has_credits;
        current.unlimited = update.unlimited;
        merge_available(&mut current.balance, update.balance);
    } else {
        *current = Some(update);
    }
}

pub(crate) fn merge_account_rate_limits(
    current: &mut Option<AgentAccountRateLimits>,
    update: AgentAccountRateLimits,
) {
    let current = current.get_or_insert_with(AgentAccountRateLimits::default);
    merge_available(&mut current.limit_id, update.limit_id);
    merge_available(&mut current.limit_name, update.limit_name);
    merge_rate_limit_window(&mut current.primary, update.primary);
    merge_rate_limit_window(&mut current.secondary, update.secondary);
    merge_credits_snapshot(&mut current.credits, update.credits);
    merge_available(&mut current.individual_limit, update.individual_limit);
    merge_available(
        &mut current.spend_control_reached,
        update.spend_control_reached,
    );
    merge_available(&mut current.plan_type, update.plan_type);
    merge_available(
        &mut current.rate_limit_reached_type,
        update.rate_limit_reached_type,
    );
}

impl ConversationState {
    pub(crate) fn apply_connection_event(&mut self, event: AgentConnectionEvent) -> bool {
        if let AgentConnectionEvent::Runtime(event) = event {
            return self.apply_runtime_event(event);
        }
        if let AgentConnectionEvent::DeprecationNotice(notice) = event {
            if self.deprecation_notices.contains(&notice) {
                return false;
            }
            self.deprecation_notices.push(notice);
            return true;
        }
        let scoped_thread_id = match &event {
            AgentConnectionEvent::Runtime(_) | AgentConnectionEvent::DeprecationNotice(_) => {
                unreachable!()
            }
            AgentConnectionEvent::AutoApprovalReviewUpdated(review) => {
                Some(review.key.thread_id.as_str())
            }
            AgentConnectionEvent::StrictReviewRequired(requirement) => {
                Some(requirement.thread_id.as_str())
            }
            AgentConnectionEvent::GuardianWarning(warning) => Some(warning.thread_id.as_str()),
            AgentConnectionEvent::Warning { thread_id, .. } => thread_id.as_deref(),
            AgentConnectionEvent::McpServerStartupStatusUpdated(status) => {
                status.thread_id.as_deref()
            }
            AgentConnectionEvent::ThreadStatusChanged(status) => Some(status.thread_id.as_str()),
            AgentConnectionEvent::ThreadSettingsUpdated { thread_id, .. } => {
                Some(thread_id.as_str())
            }
            AgentConnectionEvent::ConfigWarning(_)
            | AgentConnectionEvent::AccountRateLimitsUpdated(_) => None,
            AgentConnectionEvent::ProjectChanged { .. }
            | AgentConnectionEvent::ThreadArchived { .. }
            | AgentConnectionEvent::ThreadUnarchived { .. }
            | AgentConnectionEvent::ThreadDeleted { .. }
            | AgentConnectionEvent::ThreadNameUpdated { .. }
            | AgentConnectionEvent::ThreadClosed { .. }
            | AgentConnectionEvent::ThreadProjectUpdated { .. } => return false,
        };
        if let Some(thread_id) = scoped_thread_id
            && self.thread_id.as_deref() != Some(thread_id)
        {
            if self.thread_id.is_none() {
                self.pending_connection_events
                    .entry(thread_id.to_owned())
                    .or_default()
                    .push(event);
            }
            return false;
        }
        let event = match event {
            AgentConnectionEvent::Runtime(_) | AgentConnectionEvent::DeprecationNotice(_) => {
                unreachable!()
            }
            AgentConnectionEvent::AutoApprovalReviewUpdated(review) => {
                AgentEvent::AutoApprovalReviewUpdated(review)
            }
            AgentConnectionEvent::StrictReviewRequired(requirement) => {
                AgentEvent::StrictReviewRequired(requirement)
            }
            AgentConnectionEvent::GuardianWarning(warning) => AgentEvent::GuardianWarning(warning),
            AgentConnectionEvent::Warning { message, .. } => AgentEvent::Warning { message },
            AgentConnectionEvent::ConfigWarning(warning) => AgentEvent::ConfigWarning(warning),
            AgentConnectionEvent::McpServerStartupStatusUpdated(status) => {
                AgentEvent::McpServerStartupStatusUpdated(status)
            }
            AgentConnectionEvent::ThreadStatusChanged(status) => {
                AgentEvent::ThreadStatusChanged(status)
            }
            AgentConnectionEvent::ThreadSettingsUpdated { settings, .. } => {
                AgentEvent::ThreadSettingsUpdated(settings)
            }
            AgentConnectionEvent::AccountRateLimitsUpdated(rate_limits) => {
                AgentEvent::AccountRateLimitsUpdated(rate_limits)
            }
            AgentConnectionEvent::ProjectChanged { .. }
            | AgentConnectionEvent::ThreadArchived { .. }
            | AgentConnectionEvent::ThreadUnarchived { .. }
            | AgentConnectionEvent::ThreadDeleted { .. }
            | AgentConnectionEvent::ThreadNameUpdated { .. }
            | AgentConnectionEvent::ThreadClosed { .. }
            | AgentConnectionEvent::ThreadProjectUpdated { .. } => return false,
        };
        self.apply_agent_event_batch(vec![event]);
        true
    }
    pub(crate) fn apply_agent_event_batch(&mut self, events: Vec<AgentEvent>) -> bool {
        let mut finished = false;
        for event in events {
            if finished
                && !matches!(
                    &event,
                    AgentEvent::AutoApprovalReviewUpdated(_)
                        | AgentEvent::StrictReviewRequired(_)
                        | AgentEvent::GuardianWarning(_)
                )
            {
                continue;
            }
            if matches!(
                self.phase,
                ConversationPhase::Complete
                    | ConversationPhase::Stopped
                    | ConversationPhase::Failed
            ) && matches!(
                &event,
                AgentEvent::PlanUpdated(_)
                    | AgentEvent::PlanDelta { .. }
                    | AgentEvent::TurnPlanUpdated(_)
                    | AgentEvent::WebSearchUpdated(_)
                    | AgentEvent::SleepUpdated(_)
            ) {
                continue;
            }
            if matches!(
                &event,
                AgentEvent::PlanUpdated(_)
                    | AgentEvent::PlanDelta { .. }
                    | AgentEvent::TurnPlanUpdated(_)
                    | AgentEvent::WebSearchUpdated(_)
                    | AgentEvent::SleepUpdated(_)
            ) && self.phase != ConversationPhase::Stopping
            {
                self.phase = ConversationPhase::Streaming;
            }
            match event {
                AgentEvent::HookPromptUpdated(prompt) => self.apply_hook_prompt(prompt),
                AgentEvent::AutoApprovalReviewUpdated(review) => {
                    self.apply_auto_approval_review(*review)
                }
                AgentEvent::StrictReviewRequired(requirement) => {
                    self.apply_strict_review(requirement)
                }
                AgentEvent::GuardianWarning(warning) => self.apply_guardian_warning(warning),
                AgentEvent::ThreadCreated { thread_id } => {
                    self.thread_id = Some(thread_id.clone());
                    if let Some(pending) = self.pending_connection_events.remove(&thread_id) {
                        for event in pending {
                            self.apply_connection_event(event);
                        }
                    }
                }
                AgentEvent::TurnReady(identity) => {
                    if self.thread_id.as_deref() == Some(identity.thread_id.as_str()) {
                        self.turn_id = Some(identity.turn_id.clone());
                        self.turn_identity = Some(identity.clone());
                        self.sync_runtime_prompts();
                        self.replay_pending_reviews();
                        if self.phase == ConversationPhase::Starting {
                            self.phase = ConversationPhase::Thinking;
                        }
                        for submission in self
                            .submissions
                            .iter_mut()
                            .filter(|s| s.cycle == self.cycle && s.initial)
                        {
                            submission.target = Some(identity.clone());
                            submission.status = super::SubmissionStatus::Accepted;
                        }
                    }
                }
                AgentEvent::UserMessage {
                    item_id,
                    client_message_id,
                    text,
                    images,
                } => {
                    self.receive_user_message(item_id, client_message_id, text, images);
                }
                AgentEvent::Started => {
                    if matches!(
                        self.phase,
                        ConversationPhase::Empty | ConversationPhase::Starting
                    ) {
                        self.phase = ConversationPhase::Thinking;
                    }
                }
                AgentEvent::Error {
                    message,
                    details,
                    will_retry,
                } => {
                    self.activities.push(ConversationActivity::ProtocolError {
                        message,
                        details,
                        will_retry,
                    });
                }
                AgentEvent::ThreadSettingsUpdated(settings) => {
                    if let Some(permissions) = &settings.permissions {
                        self.effective_permissions = Some(permissions.clone());
                        self.permission_error = None;
                    }
                    self.selected_model = settings.model.clone();
                    if let Some(effort) = &settings.effort {
                        self.selected_effort = effort.clone();
                    }
                    self.selected_service_tier = settings.service_tier.clone();
                    self.slider_index = self
                        .selected_model_entry()
                        .and_then(|model| {
                            model
                                .supported_reasoning_efforts
                                .iter()
                                .position(|effort| effort.id == self.selected_effort)
                        })
                        .unwrap_or(0);
                    self.actual_model = Some(settings.model);
                    self.model_status = None;
                    self.safety_buffering = false;
                }
                AgentEvent::Warning { message } => {
                    self.activities
                        .push(ConversationActivity::Warning { message });
                }
                AgentEvent::ConfigWarning(warning) => {
                    self.activities
                        .push(ConversationActivity::ConfigWarning(warning));
                }
                AgentEvent::McpServerStartupStatusUpdated(status) => {
                    let key = (status.thread_id.clone(), status.name.clone());
                    let changed = self.mcp_server_startup_statuses.get(&key) != Some(&status);
                    self.mcp_server_startup_statuses.insert(key, status.clone());
                    if changed && status.state == AgentMcpServerStartupState::Failed {
                        let mut message = format!("MCP 服务 `{}` 启动失败", status.name);
                        if let Some(error) = status.error.filter(|error| !error.trim().is_empty()) {
                            message.push_str(&format!("：{error}"));
                        }
                        if status.failure_reason
                            == Some(AgentMcpServerStartupFailureReason::ReauthenticationRequired)
                        {
                            message.push_str("；认证已失效，请重新连接该服务");
                        }
                        self.activities
                            .push(ConversationActivity::Warning { message });
                    }
                }
                AgentEvent::ThreadStatusChanged(status) => {
                    self.thread_statuses
                        .insert(status.thread_id.clone(), status);
                }
                AgentEvent::ThreadTokenUsageUpdated(usage) => {
                    self.thread_token_usages
                        .insert(usage.thread_id.clone(), usage);
                }
                AgentEvent::AccountRateLimitsUpdated(rate_limits) => {
                    merge_account_rate_limits(&mut self.account_rate_limits, rate_limits);
                }
                AgentEvent::AssistantMessageStarted { item_id } => {
                    if !self.activities.iter().any(|activity| {
                        matches!(
                            activity,
                            ConversationActivity::AssistantMessage {
                                item_id: existing,
                                ..
                            } if existing == &item_id
                        )
                    }) {
                        self.activities
                            .push(ConversationActivity::AssistantMessage {
                                item_id,
                                text: String::new(),
                            });
                    }
                }
                AgentEvent::TextDelta(delta) => {
                    self.assistant_message.push_str(&delta);
                    if let Some(ConversationActivity::AssistantMessage { text, .. }) =
                        self.activities.iter_mut().rev().find(|activity| {
                            matches!(activity, ConversationActivity::AssistantMessage { .. })
                        })
                    {
                        text.push_str(&delta);
                    }
                    if self.phase != ConversationPhase::Stopping {
                        self.phase = ConversationPhase::Streaming;
                    }
                }
                AgentEvent::ReasoningStarted {
                    reasoning,
                    started_at_ms,
                } => {
                    upsert_reasoning_started(&mut self.activities, reasoning, started_at_ms);
                    if self.phase != ConversationPhase::Stopping {
                        self.phase = ConversationPhase::Thinking;
                    }
                }
                AgentEvent::ReasoningSummaryPartAdded {
                    item_id,
                    summary_index,
                } => {
                    if let Some(reasoning) =
                        find_reasoning_activity_mut(&mut self.activities, &item_id)
                    {
                        ensure_reasoning_part(&mut reasoning.summary, summary_index);
                    }
                }
                AgentEvent::ReasoningSummaryTextDelta {
                    item_id,
                    summary_index,
                    delta,
                } => {
                    if let Some(reasoning) =
                        find_reasoning_activity_mut(&mut self.activities, &item_id)
                    {
                        ensure_reasoning_part(&mut reasoning.summary, summary_index)
                            .push_str(&delta);
                    }
                }
                AgentEvent::ReasoningTextDelta {
                    item_id,
                    content_index,
                    delta,
                } => {
                    if let Some(reasoning) =
                        find_reasoning_activity_mut(&mut self.activities, &item_id)
                    {
                        ensure_reasoning_part(&mut reasoning.content, content_index)
                            .push_str(&delta);
                    }
                }
                AgentEvent::ReasoningCompleted {
                    reasoning,
                    completed_at_ms,
                } => {
                    upsert_reasoning_completed(&mut self.activities, reasoning, completed_at_ms);
                    if self.phase != ConversationPhase::Stopping {
                        self.phase = ConversationPhase::Thinking;
                    }
                }
                AgentEvent::CommandStarted(command) => {
                    upsert_command_activity(&mut self.activities, command);
                    if self.phase != ConversationPhase::Stopping {
                        self.phase = ConversationPhase::Streaming;
                    }
                }
                AgentEvent::CommandOutputDelta { item_id, delta } => {
                    if let Some(command) = find_command_activity_mut(&mut self.activities, &item_id)
                    {
                        command.output.push_str(&delta);
                    } else {
                        self.activities
                            .push(ConversationActivity::Command(CommandExecution {
                                id: item_id,
                                command: String::new(),
                                actions: Vec::new(),
                                cwd: String::new(),
                                output: delta,
                                terminal_process_id: None,
                                status: CommandExecutionStatus::InProgress,
                                exit_code: None,
                            }));
                    }
                    if self.phase != ConversationPhase::Stopping {
                        self.phase = ConversationPhase::Streaming;
                    }
                }
                AgentEvent::CommandTerminalInteraction {
                    item_id,
                    process_id,
                    wrote_stdin: _,
                } => {
                    if let Some(command) = find_command_activity_mut(&mut self.activities, &item_id)
                    {
                        command.terminal_process_id = Some(process_id);
                    } else {
                        self.activities
                            .push(ConversationActivity::Command(CommandExecution {
                                id: item_id,
                                command: String::new(),
                                actions: Vec::new(),
                                cwd: String::new(),
                                output: String::new(),
                                terminal_process_id: Some(process_id),
                                status: CommandExecutionStatus::InProgress,
                                exit_code: None,
                            }));
                    }
                    if self.phase != ConversationPhase::Stopping {
                        self.phase = ConversationPhase::Streaming;
                    }
                }
                AgentEvent::CommandCompleted(command) => {
                    upsert_command_activity(&mut self.activities, command);
                    if self.phase != ConversationPhase::Stopping {
                        self.phase = ConversationPhase::Streaming;
                    }
                }
                AgentEvent::FileChangeUpdated(change) => {
                    self.file_change_updated(change);
                    if self.phase != ConversationPhase::Stopping {
                        self.phase = ConversationPhase::Streaming;
                    }
                }
                AgentEvent::ImageViewed(image) => {
                    if let Some(ConversationActivity::ImageView(existing)) = self
                        .activities
                        .iter_mut()
                        .find(|activity| matches!(activity, ConversationActivity::ImageView(existing) if existing.id == image.id))
                    {
                        *existing = image;
                    } else {
                        self.activities
                            .push(ConversationActivity::ImageView(image));
                    }
                    if self.phase != ConversationPhase::Stopping {
                        self.phase = ConversationPhase::Streaming;
                    }
                }
                AgentEvent::ImageGenerationUpdated(image) => {
                    if let Some(ConversationActivity::ImageGeneration(existing)) = self
                        .activities
                        .iter_mut()
                        .find(|activity| matches!(activity, ConversationActivity::ImageGeneration(existing) if existing.id == image.id))
                    {
                        *existing = image;
                    } else {
                        self.activities
                            .push(ConversationActivity::ImageGeneration(image));
                    }
                    if self.phase != ConversationPhase::Stopping {
                        self.phase = ConversationPhase::Streaming;
                    }
                }
                AgentEvent::PlanUpdated(v) => super::activity::upsert_progress_activity(
                    &mut self.activities,
                    ConversationActivity::Plan(v),
                ),
                AgentEvent::WebSearchUpdated(v) => super::activity::upsert_progress_activity(
                    &mut self.activities,
                    ConversationActivity::WebSearch(v),
                ),
                AgentEvent::SleepUpdated(v) => super::activity::upsert_progress_activity(
                    &mut self.activities,
                    ConversationActivity::Sleep(v),
                ),
                AgentEvent::TurnPlanUpdated(v) => super::activity::upsert_progress_activity(
                    &mut self.activities,
                    ConversationActivity::TurnPlan(v),
                ),
                AgentEvent::PlanDelta { item_id, delta } => {
                    super::activity::append_plan_delta(&mut self.activities, item_id, delta)
                }
                AgentEvent::ContextCompactionUpdated(compaction) => {
                    upsert_context_compaction_activity(&mut self.activities, compaction);
                    if self.phase != ConversationPhase::Stopping {
                        self.phase = ConversationPhase::Streaming;
                    }
                }
                AgentEvent::CollaborationUpdated(collaboration) => {
                    upsert_collaboration_activity(&mut self.activities, collaboration);
                    if self.phase != ConversationPhase::Stopping {
                        self.phase = ConversationPhase::Streaming;
                    }
                }
                AgentEvent::McpToolCallUpdated(tool_call) => {
                    upsert_mcp_tool_call_activity(&mut self.activities, tool_call);
                    if self.phase != ConversationPhase::Stopping {
                        self.phase = ConversationPhase::Streaming;
                    }
                }
                AgentEvent::McpToolCallProgress { item_id, message } => {
                    if let Some(tool_call) =
                        find_mcp_tool_call_activity_mut(&mut self.activities, &item_id)
                        && tool_call.progress.last() != Some(&message)
                    {
                        tool_call.progress.push(message);
                    }
                    if self.phase != ConversationPhase::Stopping {
                        self.phase = ConversationPhase::Streaming;
                    }
                }
                AgentEvent::FileChangePatchUpdated { item_id, changes } => {
                    self.file_change_updated(AgentFileChange {
                        id: item_id,
                        changes,
                        status: AgentFileChangeStatus::InProgress,
                    });
                }
                AgentEvent::TurnDiffUpdated { diff } => {
                    if let Some(ConversationActivity::FileChange(activity)) =
                        self.activities.iter_mut().rev().find(|activity| {
                            matches!(activity, ConversationActivity::FileChange(_))
                        })
                    {
                        let review = DiffReviewPresentation::from_unified_diff(
                            format!("turn-diff-{}", activity.item_id),
                            "上一轮",
                            &diff,
                            Some(&self.cwd),
                        );
                        if !review.files.is_empty() {
                            *activity = activity.clone().with_review(review);
                        }
                    }
                }
                AgentEvent::CommandApprovalRequested { request, responder } => {
                    self.command_approval_requested(request, responder);
                }
                AgentEvent::FileApprovalRequested { request, responder } => {
                    self.file_approval_requested(request, responder);
                }
                AgentEvent::UserInputRequested { request, responder } => {
                    let context = AgentServerRequestMetadata {
                        request_id: request.request_id.clone(),
                        thread_id: request.thread_id.clone(),
                        turn_id: request.turn_id.clone(),
                        item_id: request.item_id.clone(),
                        kind: AgentServerRequestKind::UserInput,
                    };
                    let request_id = request.request_id.ui_key();
                    let questions = request
                        .questions
                        .into_iter()
                        .map(|question| UserInputQuestionPresentation {
                            id: question.id,
                            header: Some(question.header),
                            question: question.question,
                            options: question
                                .options
                                .into_iter()
                                .map(|option| {
                                    UserInputOptionPresentation::new(
                                        option.label,
                                        Some(option.description),
                                    )
                                })
                                .collect(),
                            allows_other: question.allows_other,
                            other_placeholder: "其他".to_owned(),
                            is_secret: question.is_secret,
                        })
                        .collect();
                    let mut model = UserInputRequestPresentation::pending(&request_id, questions);
                    model.is_blocking = request.is_blocking;
                    model.auto_resolution_ms = request.auto_resolution_ms;
                    let has_questions = !model.questions.is_empty();
                    self.server_request_contexts
                        .insert(request_id.clone(), context);
                    self.user_input_responders
                        .insert(request_id.clone(), responder);
                    self.activities.push(ConversationActivity::UserInput(model));
                    if !has_questions {
                        let response = self
                            .user_input_responders
                            .get(&request_id)
                            .map(|responder| responder.respond(AgentUserInputResponse::default()));
                        if let Some(ConversationActivity::UserInput(model)) =
                            self.activities.last_mut()
                        {
                            match response {
                                Some(Ok(())) => {
                                    model.status = UserInputRequestStatus::Submitting;
                                }
                                Some(Err(error)) => {
                                    model.status = UserInputRequestStatus::Failed;
                                    model.failure_message = Some(error);
                                }
                                None => {
                                    model.status = UserInputRequestStatus::Failed;
                                    model.failure_message =
                                        Some("用户输入 responder 不存在".to_owned());
                                }
                            }
                        }
                    }
                    if self.phase != ConversationPhase::Stopping {
                        self.phase = ConversationPhase::Streaming;
                    }
                }
                AgentEvent::PermissionsApprovalRequested { request, responder } => {
                    let context = AgentServerRequestMetadata {
                        request_id: request.request_id.clone(),
                        thread_id: request.thread_id.clone(),
                        turn_id: request.turn_id.clone(),
                        item_id: request.item_id.clone(),
                        kind: AgentServerRequestKind::PermissionsApproval,
                    };
                    let request_id = request.request_id.ui_key();
                    let (network_enabled, file_system) =
                        permission_presentation_data(&request.permissions);
                    let model = PermissionApprovalPresentation::pending(
                        &request_id,
                        network_enabled,
                        file_system,
                        request.reason,
                    )
                    .with_cwd(request.cwd);
                    let has_actions = !model.actions().is_empty();
                    self.server_request_contexts
                        .insert(request_id.clone(), context);
                    self.permissions_approval_responders
                        .insert(request_id.clone(), responder);
                    self.activities
                        .push(ConversationActivity::PermissionsApproval(model));
                    if !has_actions {
                        let response = self.permissions_approval_responders.get(&request_id).map(
                            |responder| {
                                responder.respond(AgentPermissionsApprovalChoice::AllowOnce)
                            },
                        );
                        if let Some(ConversationActivity::PermissionsApproval(model)) =
                            self.activities.last_mut()
                        {
                            match response {
                                Some(Ok(())) => {
                                    model.status = PermissionApprovalStatus::Approved;
                                }
                                Some(Err(error)) => {
                                    model.status = PermissionApprovalStatus::Failed;
                                    model.failure_message = Some(error);
                                }
                                None => {
                                    model.status = PermissionApprovalStatus::Failed;
                                    model.failure_message =
                                        Some("权限审批 responder 不存在".to_owned());
                                }
                            }
                        }
                    }
                    if self.phase != ConversationPhase::Stopping {
                        self.phase = ConversationPhase::Streaming;
                    }
                }
                AgentEvent::ServerRequestResolved { request } => {
                    let request_id = request.request_id.ui_key();
                    match self.server_request_contexts.get(&request_id) {
                        Some(expected) if expected == &request => {}
                        Some(expected) => {
                            self.activities.push(ConversationActivity::ProtocolError {
                                message:
                                    "serverRequest/resolved 标识与 Composer pending request 不一致"
                                        .to_owned(),
                                details: Some(format!("expected={expected:?}; actual={request:?}")),
                                will_retry: false,
                            });
                            continue;
                        }
                        None => {
                            self.activities.push(ConversationActivity::ProtocolError {
                                message:
                                    "serverRequest/resolved 在 Composer 中没有对应 pending request"
                                        .to_owned(),
                                details: Some(format!("actual={request:?}")),
                                will_retry: false,
                            });
                            continue;
                        }
                    }
                    self.server_request_contexts.remove(&request_id);
                    match request.kind {
                        AgentServerRequestKind::CommandApproval => {
                            self.approval_responders.remove(&request_id);
                            self.command_approval_requests.remove(&request_id);
                            if let Some(ConversationActivity::Approval(model)) =
                                self.activities.iter_mut().find(|activity| {
                                    matches!(activity, ConversationActivity::Approval(model) if model.request_id == request_id)
                                })
                            {
                                model.status = ApprovalCardStatus::Resolved;
                            }
                        }
                        AgentServerRequestKind::FileApproval => {
                            self.file_approval_responders.remove(&request_id);
                            if let Some(ConversationActivity::FileApproval(model)) = self.activities.iter_mut().find(|activity| {
                                matches!(activity, ConversationActivity::FileApproval(model) if model.request_id == request_id)
                            }) { model.status = crate::components::file_change::FileApprovalStatus::Resolved; }
                        }
                        AgentServerRequestKind::UserInput => {
                            self.user_input_responders.remove(&request_id);
                            if let Some(ConversationActivity::UserInput(model)) =
                                self.activities.iter_mut().find(|activity| {
                                    matches!(activity, ConversationActivity::UserInput(model) if model.request_id == request_id)
                                })
                            {
                                model.status = UserInputRequestStatus::Resolved;
                            }
                        }
                        AgentServerRequestKind::PermissionsApproval => {
                            self.permissions_approval_responders.remove(&request_id);
                            if let Some(ConversationActivity::PermissionsApproval(model)) =
                                self.activities.iter_mut().find(|activity| {
                                    matches!(activity, ConversationActivity::PermissionsApproval(model) if model.request_id == request_id)
                                })
                            {
                                model.status = PermissionApprovalStatus::Resolved;
                            }
                        }
                    }
                }
                AgentEvent::ServerRequestFailed {
                    request,
                    kind,
                    message,
                } => {
                    let request_id = request.request_id.ui_key();
                    match self.server_request_contexts.get(&request_id) {
                        Some(expected) if expected == &request => {}
                        Some(expected) => {
                            self.activities.push(ConversationActivity::ProtocolError {
                                message:
                                    "server request 清理标识与 Composer pending request 不一致"
                                        .to_owned(),
                                details: Some(format!("expected={expected:?}; actual={request:?}")),
                                will_retry: false,
                            });
                            continue;
                        }
                        None => {
                            self.activities.push(ConversationActivity::ProtocolError {
                                message:
                                    "server request 清理在 Composer 中没有对应 pending request"
                                        .to_owned(),
                                details: Some(format!("actual={request:?}")),
                                will_retry: false,
                            });
                            continue;
                        }
                    }
                    self.server_request_contexts.remove(&request_id);
                    match request.kind {
                        AgentServerRequestKind::CommandApproval => {
                            self.approval_responders.remove(&request_id);
                            self.command_approval_requests.remove(&request_id);
                            if let Some(ConversationActivity::Approval(model)) =
                                self.activities.iter_mut().find(|activity| {
                                    matches!(activity, ConversationActivity::Approval(model) if model.request_id == request_id)
                                })
                            {
                                model.status = ApprovalCardStatus::Resolved;
                            }
                        }
                        AgentServerRequestKind::FileApproval => {
                            self.file_approval_responders.remove(&request_id);
                            if let Some(ConversationActivity::FileApproval(model)) = self.activities.iter_mut().find(|activity| {
                                matches!(activity, ConversationActivity::FileApproval(model) if model.request_id == request_id)
                            }) { model.status = crate::components::file_change::FileApprovalStatus::Resolved; }
                        }
                        AgentServerRequestKind::UserInput => {
                            self.user_input_responders.remove(&request_id);
                            if let Some(ConversationActivity::UserInput(model)) =
                                self.activities.iter_mut().find(|activity| {
                                    matches!(activity, ConversationActivity::UserInput(model) if model.request_id == request_id)
                                })
                            {
                                model.status = match kind {
                                    AgentServerRequestFailureKind::Cancelled => {
                                        UserInputRequestStatus::Cancelled
                                    }
                                    AgentServerRequestFailureKind::Failed => {
                                        UserInputRequestStatus::Failed
                                    }
                                };
                                model.failure_message = Some(message.clone());
                            }
                        }
                        AgentServerRequestKind::PermissionsApproval => {
                            self.permissions_approval_responders.remove(&request_id);
                            if let Some(ConversationActivity::PermissionsApproval(model)) =
                                self.activities.iter_mut().find(|activity| {
                                    matches!(activity, ConversationActivity::PermissionsApproval(model) if model.request_id == request_id)
                                })
                            {
                                model.status = match kind {
                                    AgentServerRequestFailureKind::Cancelled => {
                                        PermissionApprovalStatus::Cancelled
                                    }
                                    AgentServerRequestFailureKind::Failed => {
                                        PermissionApprovalStatus::Failed
                                    }
                                };
                                model.failure_message = Some(message.clone());
                            }
                        }
                    }
                    self.activities.push(ConversationActivity::ProtocolError {
                        message,
                        details: Some(format!("request={request:?}")),
                        will_retry: false,
                    });
                }
                AgentEvent::ModelRerouted {
                    from_model,
                    to_model,
                    reason,
                } => {
                    self.actual_model = Some(to_model.clone());
                    self.model_status = Some(format!(
                        "已从 {} 自动切换到 {}（{}）",
                        self.model_display_name(&from_model),
                        self.model_display_name(&to_model),
                        reason
                    ));
                    self.safety_buffering = false;
                }
                AgentEvent::ModelVerificationRequired { verifications } => {
                    let requirements = if verifications.is_empty() {
                        "未知验证".to_owned()
                    } else {
                        verifications.join("、")
                    };
                    let error = format!("所选模型需要额外账户验证：{requirements}");
                    self.assistant_message = error.clone();
                    self.activities
                        .push(ConversationActivity::Error { message: error });
                    self.assistant_message_time = Some(current_local_time_label());
                    self.phase = ConversationPhase::Failed;
                    self.model_status = Some("需要账户验证".to_owned());
                    self.safety_buffering = false;
                    finished = true;
                    continue;
                }
                AgentEvent::ModelSafetyBufferingUpdated {
                    model,
                    use_cases,
                    reasons,
                    show_buffering_ui,
                    faster_model,
                } => {
                    self.actual_model = Some(model);
                    self.safety_buffering = show_buffering_ui;
                    self.model_status = show_buffering_ui.then(|| {
                        let mut message = "安全检查中".to_owned();
                        if !use_cases.is_empty() || !reasons.is_empty() {
                            let detail = use_cases
                                .into_iter()
                                .chain(reasons)
                                .collect::<Vec<_>>()
                                .join("、");
                            message.push_str(&format!("：{detail}"));
                        }
                        if let Some(faster_model) = faster_model {
                            message.push_str(&format!(
                                "；可改用 {}",
                                self.model_display_name(&faster_model)
                            ));
                        }
                        message
                    });
                }
                AgentEvent::Completed => {
                    self.close_runtime_turn(crate::agent::AgentLocalClosure::TurnCompleted);
                    super::activity::finish_progress_activities(
                        &mut self.activities,
                        crate::agent::AgentActivityStatus::Completed,
                    );
                    if self
                        .submissions
                        .iter()
                        .any(|s| s.cycle == self.cycle && !s.initial && s.item_id.is_some())
                        && let Some(text) = self.activities.iter().rev().find_map(|a| match a {
                            ConversationActivity::AssistantMessage { text, .. } => {
                                Some(text.clone())
                            }
                            _ => None,
                        })
                    {
                        self.assistant_message = text;
                    }
                    remove_unfinished_image_generations(&mut self.activities);
                    if self.safety_buffering {
                        self.model_status = None;
                        self.safety_buffering = false;
                    }
                    self.assistant_message_time = Some(current_local_time_label());
                    self.phase = ConversationPhase::Complete;
                    finished = true;
                    continue;
                }
                AgentEvent::Interrupted => {
                    self.close_runtime_turn(crate::agent::AgentLocalClosure::Interrupted);
                    super::activity::finish_progress_activities(
                        &mut self.activities,
                        crate::agent::AgentActivityStatus::Interrupted,
                    );
                    remove_unfinished_image_generations(&mut self.activities);
                    self.assistant_message_time = Some(current_local_time_label());
                    self.phase = ConversationPhase::Stopped;
                    self.safety_buffering = false;
                    finished = true;
                    continue;
                }
                AgentEvent::Failed(error) => {
                    self.close_runtime_turn(crate::agent::AgentLocalClosure::Failed);
                    super::activity::finish_progress_activities(
                        &mut self.activities,
                        crate::agent::AgentActivityStatus::Failed,
                    );
                    remove_unfinished_image_generations(&mut self.activities);
                    self.assistant_message = error.clone();
                    let already_visible = self.activities.iter().rev().any(|activity| {
                        matches!(
                            activity,
                            ConversationActivity::ProtocolError {
                                message,
                                will_retry: false,
                                ..
                            } if error.starts_with(message)
                        )
                    });
                    if !already_visible {
                        self.activities
                            .push(ConversationActivity::Error { message: error });
                    }
                    self.assistant_message_time = Some(current_local_time_label());
                    self.phase = ConversationPhase::Failed;
                    self.safety_buffering = false;
                    finished = true;
                    continue;
                }
            }
        }
        if finished {
            self.clear_terminal_approvals();
            self.close_auto_approval_reviews();
            for submission in self.submissions.iter_mut().filter(|s| {
                s.cycle == self.cycle && s.initial && s.status == super::SubmissionStatus::Sending
            }) {
                submission.status =
                    super::SubmissionStatus::Failed("提交未被接受。输入快照已保留。".into());
            }
            self.active_turn.take();
        }
        finished
    }
}
