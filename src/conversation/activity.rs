//! Conversation presentation data and canonical activity updates.

use crate::{
    agent::{
        AgentCollaboration, AgentConfigWarning, AgentContextCompaction, AgentFileChange,
        AgentFileSystemAccess, AgentFileSystemPath, AgentFileSystemSpecialPath,
        AgentImageGeneration, AgentImageGenerationStatus, AgentImageView, AgentMcpToolCall,
        AgentOptionalField, AgentPermissionRequestProfile, AgentReasoning, CommandExecution,
        CommandExecutionStatus,
    },
    components::{
        approval::ApprovalCardViewModel,
        file_change::{FileApprovalPresentation, FileChangeActivityPresentation},
        permissions_approval::{
            PermissionApprovalPresentation, PermissionPathAccess, PermissionPathRequest,
        },
        user_input_request::UserInputRequestPresentation,
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReasoningActivityPresentation {
    pub item_id: String,
    pub summary: Vec<String>,
    pub content: Vec<String>,
    pub started_at_ms: i64,
    pub completed_at_ms: Option<i64>,
}

impl ReasoningActivityPresentation {
    pub fn is_active(&self) -> bool {
        self.completed_at_ms.is_none()
    }

    pub fn elapsed_ms(&self) -> Option<u64> {
        let elapsed = self.completed_at_ms?.checked_sub(self.started_at_ms)?;
        u64::try_from(elapsed).ok().filter(|elapsed| *elapsed > 0)
    }

    pub fn display_text(&self) -> String {
        let parts = if self.summary.iter().any(|part| !part.is_empty()) {
            &self.summary
        } else {
            &self.content
        };
        reasoning_parts_text(parts)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ConversationActivity {
    AutoApprovalReview(Box<super::auto_approval::AutoApprovalReviewPresentation>),
    StrictReview(super::auto_approval::StrictReviewPresentation),
    GuardianWarning(crate::agent::AgentGuardianWarning),
    AssistantMessage {
        item_id: String,
        text: String,
    },
    Reasoning(ReasoningActivityPresentation),
    Command(CommandExecution),
    Approval(ApprovalCardViewModel),
    FileApproval(FileApprovalPresentation),
    PermissionsApproval(PermissionApprovalPresentation),
    FileChange(FileChangeActivityPresentation),
    ImageView(AgentImageView),
    ImageViews(Vec<AgentImageView>),
    ImageGeneration(AgentImageGeneration),
    ContextCompaction(AgentContextCompaction),
    Collaboration(AgentCollaboration),
    McpToolCall(Box<AgentMcpToolCall>),
    TurnPlan(crate::agent::AgentTurnPlan),
    Plan(crate::agent::AgentPlan),
    Sleep(crate::agent::AgentSleep),
    WebSearch(crate::agent::AgentWebSearch),
    QuestionReply {
        item_id: String,
        question: String,
        answer: String,
    },
    UserInput(UserInputRequestPresentation),
    ProtocolError {
        message: String,
        details: Option<String>,
        will_retry: bool,
    },
    Warning {
        message: String,
    },
    ConfigWarning(AgentConfigWarning),
    Error {
        message: String,
    },
}

impl ConversationActivity {
    pub(crate) fn shows_request(&self) -> bool {
        match self {
            Self::Approval(model) => model.should_render(),
            Self::FileApproval(model) => model.should_render(),
            Self::PermissionsApproval(model) => model.should_render(),
            Self::UserInput(model) => model.should_render(),
            _ => false,
        }
    }
}

pub(crate) fn reasoning_parts_text(parts: &[String]) -> String {
    let Some((first, rest)) = parts.split_first() else {
        return String::new();
    };
    if rest.is_empty() || first.is_empty() || first.starts_with("**") {
        parts.join("\n\n")
    } else {
        format!("**{first}**\n\n{}", rest.join("\n\n"))
    }
}

pub(crate) fn permission_presentation_data(
    permissions: &AgentPermissionRequestProfile,
) -> (bool, Vec<PermissionPathRequest>) {
    let network_enabled = matches!(
        &permissions.network,
        AgentOptionalField::Value(network)
            if matches!(network.enabled, AgentOptionalField::Value(true))
    );
    let mut paths = Vec::new();
    let AgentOptionalField::Value(file_system) = &permissions.file_system else {
        return (network_enabled, paths);
    };
    if let AgentOptionalField::Value(read) = &file_system.read {
        paths.extend(
            read.iter()
                .cloned()
                .map(|path| PermissionPathRequest::new(path, PermissionPathAccess::Read)),
        );
    }
    if let AgentOptionalField::Value(write) = &file_system.write {
        paths.extend(
            write
                .iter()
                .cloned()
                .map(|path| PermissionPathRequest::new(path, PermissionPathAccess::Write)),
        );
    }
    if let AgentOptionalField::Value(entries) = &file_system.entries {
        paths.extend(entries.iter().map(|entry| {
            let access = match entry.access {
                AgentFileSystemAccess::Read => PermissionPathAccess::Read,
                AgentFileSystemAccess::Write => PermissionPathAccess::Write,
                AgentFileSystemAccess::Deny => PermissionPathAccess::Deny,
            };
            PermissionPathRequest::new(permission_path_display(&entry.path), access)
        }));
    }
    (network_enabled, paths)
}

pub(crate) fn permission_path_display(path: &AgentFileSystemPath) -> String {
    match path {
        AgentFileSystemPath::Path(path) => path.clone(),
        AgentFileSystemPath::GlobPattern(pattern) => format!("glob:{pattern}"),
        AgentFileSystemPath::Special(special) => match special {
            AgentFileSystemSpecialPath::Root => "/".to_owned(),
            AgentFileSystemSpecialPath::Minimal => "<minimal>".to_owned(),
            AgentFileSystemSpecialPath::ProjectRoots { subpath } => match subpath {
                AgentOptionalField::Value(subpath) => format!("<project_roots>/{subpath}"),
                AgentOptionalField::Unspecified | AgentOptionalField::Null => {
                    "<project_roots>".to_owned()
                }
            },
            AgentFileSystemSpecialPath::Tmpdir => "<tmpdir>".to_owned(),
            AgentFileSystemSpecialPath::SlashTmp => "/tmp".to_owned(),
            AgentFileSystemSpecialPath::Unknown { path, subpath } => match subpath {
                AgentOptionalField::Value(subpath) => format!("{path}/{subpath}"),
                AgentOptionalField::Unspecified | AgentOptionalField::Null => path.clone(),
            },
        },
    }
}

pub(crate) fn find_command_activity_mut<'a>(
    activities: &'a mut [ConversationActivity],
    item_id: &str,
) -> Option<&'a mut CommandExecution> {
    activities.iter_mut().find_map(|activity| match activity {
        ConversationActivity::Command(command) if command.id == item_id => Some(command),
        _ => None,
    })
}

pub(crate) fn find_mcp_tool_call_activity_mut<'a>(
    activities: &'a mut [ConversationActivity],
    item_id: &str,
) -> Option<&'a mut AgentMcpToolCall> {
    activities.iter_mut().find_map(|activity| match activity {
        ConversationActivity::McpToolCall(tool_call) if tool_call.id == item_id => {
            Some(tool_call.as_mut())
        }
        _ => None,
    })
}

pub(crate) fn find_reasoning_activity_mut<'a>(
    activities: &'a mut [ConversationActivity],
    item_id: &str,
) -> Option<&'a mut ReasoningActivityPresentation> {
    activities.iter_mut().find_map(|activity| match activity {
        ConversationActivity::Reasoning(reasoning) if reasoning.item_id == item_id => {
            Some(reasoning)
        }
        _ => None,
    })
}

pub(crate) fn ensure_reasoning_part(parts: &mut Vec<String>, index: usize) -> &mut String {
    if parts.len() <= index {
        parts.resize(index + 1, String::new());
    }
    &mut parts[index]
}

pub(crate) fn upsert_reasoning_started(
    activities: &mut Vec<ConversationActivity>,
    reasoning: AgentReasoning,
    started_at_ms: i64,
) {
    let presentation = ReasoningActivityPresentation {
        item_id: reasoning.id,
        summary: reasoning.summary,
        content: reasoning.content,
        started_at_ms,
        completed_at_ms: None,
    };
    if let Some(existing) = find_reasoning_activity_mut(activities, &presentation.item_id) {
        *existing = presentation;
    } else {
        activities.push(ConversationActivity::Reasoning(presentation));
    }
}

pub(crate) fn upsert_reasoning_completed(
    activities: &mut Vec<ConversationActivity>,
    reasoning: AgentReasoning,
    completed_at_ms: i64,
) {
    if let Some(existing) = find_reasoning_activity_mut(activities, &reasoning.id) {
        existing.summary = reasoning.summary;
        existing.content = reasoning.content;
        existing.completed_at_ms = Some(completed_at_ms);
    } else {
        activities.push(ConversationActivity::Reasoning(
            ReasoningActivityPresentation {
                item_id: reasoning.id,
                summary: reasoning.summary,
                content: reasoning.content,
                started_at_ms: completed_at_ms,
                completed_at_ms: Some(completed_at_ms),
            },
        ));
    }
}

pub(crate) fn upsert_command_activity(
    activities: &mut Vec<ConversationActivity>,
    mut incoming: CommandExecution,
) {
    if let Some(existing) = find_command_activity_mut(activities, &incoming.id) {
        if incoming.output.is_empty() {
            incoming.output = std::mem::take(&mut existing.output);
        }
        if incoming.actions.is_empty() {
            incoming.actions = std::mem::take(&mut existing.actions);
        }
        if incoming.status == CommandExecutionStatus::InProgress
            && incoming.terminal_process_id.is_none()
        {
            incoming.terminal_process_id = existing.terminal_process_id.take();
        }
        *existing = incoming;
    } else {
        activities.push(ConversationActivity::Command(incoming));
    }
}

pub(crate) fn upsert_mcp_tool_call_activity(
    activities: &mut Vec<ConversationActivity>,
    mut incoming: AgentMcpToolCall,
) {
    if let Some(existing) = find_mcp_tool_call_activity_mut(activities, &incoming.id) {
        if incoming.progress.is_empty() {
            incoming.progress = std::mem::take(&mut existing.progress);
        }
        *existing = incoming;
    } else {
        activities.push(ConversationActivity::McpToolCall(Box::from(incoming)));
    }
}

pub(crate) fn upsert_context_compaction_activity(
    activities: &mut Vec<ConversationActivity>,
    incoming: AgentContextCompaction,
) {
    if let Some(existing) = activities.iter_mut().find_map(|activity| match activity {
        ConversationActivity::ContextCompaction(existing) if existing.id == incoming.id => {
            Some(existing)
        }
        _ => None,
    }) {
        *existing = incoming;
    } else {
        activities.push(ConversationActivity::ContextCompaction(incoming));
    }
}

pub(crate) fn collaborations_share_identity(
    existing: &AgentCollaboration,
    incoming: &AgentCollaboration,
) -> bool {
    match (existing.legacy_kind, incoming.legacy_kind) {
        (Some(_), Some(_)) => existing
            .receiver_thread_ids
            .first()
            .is_some_and(|thread_id| incoming.receiver_thread_ids.first() == Some(thread_id)),
        (None, None) => existing.id == incoming.id,
        _ => false,
    }
}

pub(crate) fn upsert_collaboration_activity(
    activities: &mut Vec<ConversationActivity>,
    incoming: AgentCollaboration,
) {
    if let Some(existing) = activities.iter_mut().find_map(|activity| match activity {
        ConversationActivity::Collaboration(existing)
            if collaborations_share_identity(existing, &incoming) =>
        {
            Some(existing)
        }
        _ => None,
    }) {
        *existing = incoming;
    } else {
        activities.push(ConversationActivity::Collaboration(incoming));
    }
}

pub(crate) fn remove_unfinished_image_generations(activities: &mut Vec<ConversationActivity>) {
    activities.retain(|activity| {
        !matches!(
            activity,
            ConversationActivity::ImageGeneration(image)
                if image.status == AgentImageGenerationStatus::InProgress
        )
    });
}

pub(crate) fn upsert_file_change_activity(
    activities: &mut Vec<ConversationActivity>,
    change: AgentFileChange,
    cwd: &std::path::Path,
) {
    let presentation =
        FileChangeActivityPresentation::from_agent_change(&change, "上一轮", Some(cwd));
    if let Some(existing) = activities.iter_mut().find_map(|activity| match activity {
        ConversationActivity::FileChange(existing) if existing.item_id == change.id => {
            Some(existing)
        }
        _ => None,
    }) {
        *existing = presentation;
    } else {
        activities.push(ConversationActivity::FileChange(presentation));
    }
}

/// Ignore a late start after a terminal item and preserve early plan deltas.
pub(crate) fn upsert_progress_activity(
    activities: &mut Vec<ConversationActivity>,
    incoming: ConversationActivity,
) {
    use ConversationActivity::*;
    let existing = activities
        .iter_mut()
        .find(|current| match (&**current, &incoming) {
            (Plan(a), Plan(b)) => a.id == b.id,
            (WebSearch(a), WebSearch(b)) => a.id == b.id,
            (Sleep(a), Sleep(b)) => a.id == b.id,
            (TurnPlan(a), TurnPlan(b)) => a.turn_id == b.turn_id,
            _ => false,
        });
    if let Some(existing) = existing {
        use crate::agent::AgentActivityStatus::InProgress;
        match (&*existing, &incoming) {
            (Plan(a), Plan(b))
                if b.status == InProgress && (a.status != InProgress || !a.text.is_empty()) =>
            {
                return;
            }
            (WebSearch(a), WebSearch(b)) if b.status == InProgress && a.status != InProgress => {
                return;
            }
            (Sleep(a), Sleep(b)) if b.status == InProgress && a.status != InProgress => return,
            _ => {}
        }
        *existing = incoming;
    } else {
        activities.push(incoming);
    }
}

pub(crate) fn append_plan_delta(
    activities: &mut Vec<ConversationActivity>,
    item_id: String,
    delta: String,
) {
    use crate::agent::{AgentActivityStatus, AgentPlan};
    if let Some(plan) = activities.iter_mut().find_map(|a| match a {
        ConversationActivity::Plan(p) if p.id == item_id => Some(p),
        _ => None,
    }) {
        if plan.status == AgentActivityStatus::InProgress {
            plan.text.push_str(&delta);
        }
    } else {
        activities.push(ConversationActivity::Plan(AgentPlan {
            id: item_id,
            text: delta,
            status: AgentActivityStatus::InProgress,
        }));
    }
}

pub(crate) fn finish_progress_activities(
    activities: &mut [ConversationActivity],
    outcome: crate::agent::AgentActivityStatus,
) {
    for activity in activities {
        let status = match activity {
            ConversationActivity::TurnPlan(v) => {
                for step in &mut v.steps {
                    if step.status == crate::agent::AgentPlanStepStatus::InProgress {
                        step.status = crate::agent::AgentPlanStepStatus::Pending;
                    }
                }
                continue;
            }
            ConversationActivity::Plan(v) => &mut v.status,
            ConversationActivity::WebSearch(v) => &mut v.status,
            ConversationActivity::Sleep(v) => &mut v.status,
            _ => continue,
        };
        if *status == crate::agent::AgentActivityStatus::InProgress {
            *status = outcome;
        }
    }
}

#[cfg(test)]
mod progress_tests;
