//! Approval presentation and item correlation within one conversation cycle.

use super::{
    ConversationActivity, ConversationPhase, ConversationState,
    activity::{permission_presentation_data, upsert_file_change_activity},
};
use crate::{
    agent::{
        AgentApprovalHandle, AgentCommandApprovalChoice, AgentCommandApprovalKind,
        AgentCommandApprovalRequest, AgentFileApprovalHandle, AgentFileApprovalRequest,
        AgentFileChange, AgentNetworkPolicyAction, AgentOptionalField, AgentServerRequestKind,
        AgentServerRequestMetadata,
    },
    components::{
        approval::{
            ApprovalCardStatus, ApprovalCardViewModel, ApprovalChoiceKind,
            ApprovalChoicePresentation, ApprovalDecision, ApprovalRequestPresentation,
            ApprovalScope,
        },
        file_change::{
            DiffReviewPresentation, FileApprovalPathPresentation, FileApprovalPresentation,
            FileApprovalStatus,
        },
        permissions_approval::PermissionPathAccess,
    },
};

impl ConversationState {
    fn accepts_approval(&self, context: &AgentServerRequestMetadata) -> bool {
        !self
            .thread_id
            .as_ref()
            .is_some_and(|thread| thread != &context.thread_id)
            && !self
                .server_request_contexts
                .contains_key(&context.request_id.ui_key())
            && !matches!(
                self.phase,
                ConversationPhase::Complete
                    | ConversationPhase::Stopped
                    | ConversationPhase::Failed
            )
    }

    pub(super) fn command_approval_requested(
        &mut self,
        request: AgentCommandApprovalRequest,
        responder: AgentApprovalHandle,
    ) {
        let context = AgentServerRequestMetadata {
            request_id: request.request_id.clone(),
            thread_id: request.thread_id.clone(),
            turn_id: request.turn_id.clone(),
            item_id: request.item_id.clone(),
            kind: AgentServerRequestKind::CommandApproval,
        };
        if !self.accepts_approval(&context) {
            return;
        }
        let id = request.request_id.ui_key();
        let presentation = if let Some(network) = &request.network {
            ApprovalRequestPresentation::network(
                format!(
                    "{}://{}",
                    match network.protocol {
                        crate::agent::AgentNetworkApprovalProtocol::Http => "http",
                        crate::agent::AgentNetworkApprovalProtocol::Https => "https",
                        crate::agent::AgentNetworkApprovalProtocol::Socks5Tcp => "socks5",
                        crate::agent::AgentNetworkApprovalProtocol::Socks5Udp => "socks5",
                    },
                    network.host
                ),
                None,
                request.reason.clone(),
            )
        } else if request.kind == AgentCommandApprovalKind::WriteStdin {
            ApprovalRequestPresentation::WriteStdin {
                input: request.command.clone(),
                reason: request.reason.clone(),
            }
        } else {
            let command = if request.command.is_empty() {
                self.activities
                    .iter()
                    .find_map(|activity| match activity {
                        ConversationActivity::Command(command) if command.id == request.item_id => {
                            Some(command.command.clone())
                        }
                        _ => None,
                    })
                    .unwrap_or_default()
            } else {
                request.command.clone()
            };
            ApprovalRequestPresentation::command(command, request.reason.clone())
        };
        let mut model = ApprovalCardViewModel::pending(&id, presentation);
        let decisions = &request.available_decisions;
        model.set_available_decisions(
            decisions.contains(&AgentCommandApprovalChoice::Accept),
            decisions.contains(&AgentCommandApprovalChoice::Decline),
            decisions.contains(&AgentCommandApprovalChoice::Cancel),
            decisions
                .iter()
                .any(|choice| {
                    matches!(
                        choice,
                        AgentCommandApprovalChoice::AcceptWithExecpolicyAmendment(_)
                    )
                })
                .then_some(ApprovalScope::SimilarCommands)
                .or_else(|| {
                    decisions
                        .contains(&AgentCommandApprovalChoice::AcceptForSession)
                        .then_some(ApprovalScope::Session)
                }),
        );
        model.server_choices = decisions
            .iter()
            .enumerate()
            .map(|(index, choice)| {
                let (label, description, is_rejection) = match choice {
                    AgentCommandApprovalChoice::Accept => ("允许一次".to_owned(), None, false),
                    AgentCommandApprovalChoice::AcceptForSession => {
                        ("允许此对话".to_owned(), None, false)
                    }
                    AgentCommandApprovalChoice::Decline => (
                        "拒绝".to_owned(),
                        Some("拒绝此操作，继续当前轮次".to_owned()),
                        true,
                    ),
                    AgentCommandApprovalChoice::Cancel => (
                        "拒绝并停止".to_owned(),
                        Some("拒绝此操作并停止当前轮次".to_owned()),
                        true,
                    ),
                    AgentCommandApprovalChoice::AcceptWithExecpolicyAmendment(prefix) => {
                        ("允许类似命令".to_owned(), Some(prefix.join(" ")), false)
                    }
                    AgentCommandApprovalChoice::ApplyNetworkPolicyAmendment(rule) => (
                        if rule.action == AgentNetworkPolicyAction::Allow {
                            "始终允许此网站"
                        } else {
                            "始终拒绝此网站"
                        }
                        .to_owned(),
                        Some(rule.host.clone()),
                        rule.action == AgentNetworkPolicyAction::Deny,
                    ),
                };
                let kind = match choice {
                    AgentCommandApprovalChoice::Accept => ApprovalChoiceKind::Once,
                    AgentCommandApprovalChoice::AcceptForSession => ApprovalChoiceKind::Session,
                    AgentCommandApprovalChoice::Decline => ApprovalChoiceKind::Decline,
                    AgentCommandApprovalChoice::Cancel => ApprovalChoiceKind::Cancel,
                    AgentCommandApprovalChoice::AcceptWithExecpolicyAmendment(_) => {
                        ApprovalChoiceKind::ExecPolicy
                    }
                    AgentCommandApprovalChoice::ApplyNetworkPolicyAmendment(rule) => {
                        if rule.action == AgentNetworkPolicyAction::Allow {
                            ApprovalChoiceKind::NetworkAllow
                        } else {
                            ApprovalChoiceKind::NetworkDeny
                        }
                    }
                };
                ApprovalChoicePresentation {
                    kind,
                    decision: ApprovalDecision::ServerChoice(index),
                    label,
                    description,
                    is_rejection,
                }
            })
            .collect();
        if let AgentOptionalField::Value(permissions) = &request.additional_permissions {
            let (network, paths) = permission_presentation_data(permissions);
            if network {
                model.permission_details.push("互联网访问".to_owned());
            }
            model
                .permission_details
                .extend(paths.into_iter().map(|path| {
                    format!(
                        "{}：{}",
                        match path.access {
                            PermissionPathAccess::Read => "读取",
                            PermissionPathAccess::Write => "写入",
                            PermissionPathAccess::Deny => "禁止访问",
                        },
                        path.path
                    )
                }));
        }
        if model.server_choices.is_empty() {
            model.status = ApprovalCardStatus::Failed;
            model.failure_message = Some("服务器没有提供可用的审批选项".to_owned());
        }
        self.command_approval_requests.insert(id.clone(), request);
        self.approval_responders.insert(id.clone(), responder);
        self.server_request_contexts.insert(id, context);
        self.activities.push(ConversationActivity::Approval(model));
        if self.phase != ConversationPhase::Stopping {
            self.phase = ConversationPhase::Streaming;
        }
    }

    pub(super) fn file_approval_requested(
        &mut self,
        request: AgentFileApprovalRequest,
        responder: AgentFileApprovalHandle,
    ) {
        let context = AgentServerRequestMetadata {
            request_id: request.request_id.clone(),
            thread_id: request.thread_id.clone(),
            turn_id: request.turn_id.clone(),
            item_id: request.item_id.clone(),
            kind: AgentServerRequestKind::FileApproval,
        };
        if !self.accepts_approval(&context) {
            return;
        }
        let id = request.request_id.ui_key();
        let mut model = FileApprovalPresentation::pending(&id, Vec::new(), request.reason);
        model.grant_root = request.grant_root;
        model.changes_ready = false;
        self.server_request_contexts.insert(id.clone(), context);
        self.file_approval_responders.insert(id, responder);
        self.activities
            .push(ConversationActivity::FileApproval(model));
        self.refresh_file_approvals(&request.item_id);
        if self.phase != ConversationPhase::Stopping {
            self.phase = ConversationPhase::Streaming;
        }
    }

    pub(super) fn file_change_updated(&mut self, change: AgentFileChange) {
        let item_id = change.id.clone();
        self.file_changes.insert(item_id.clone(), change.clone());
        upsert_file_change_activity(&mut self.activities, change, &self.cwd);
        self.refresh_file_approvals(&item_id);
    }

    fn refresh_file_approvals(&mut self, item_id: &str) {
        let Some(change) = self.file_changes.get(item_id) else {
            return;
        };
        let review = DiffReviewPresentation::from_file_change_entries(
            format!("file-approval-{item_id}"),
            "待审批",
            &change.changes,
            Some(&self.cwd),
        );
        for activity in &mut self.activities {
            let ConversationActivity::FileApproval(model) = activity else {
                continue;
            };
            if !model.is_interactive()
                || !self
                    .server_request_contexts
                    .get(&model.request_id)
                    .is_some_and(|context| {
                        context.item_id == item_id
                            && context.kind == AgentServerRequestKind::FileApproval
                    })
            {
                continue;
            }
            model.files = review
                .files
                .iter()
                .enumerate()
                .map(|(index, file)| {
                    let path = change
                        .changes
                        .get(index)
                        .map(|entry| entry.path.as_str())
                        .unwrap_or(&file.path);
                    FileApprovalPathPresentation::new(path, file.additions, file.deletions)
                })
                .collect();
            model.changes_ready = !change.changes.is_empty();
            model.review = Some(review.clone());
        }
    }

    pub(super) fn clear_terminal_approvals(&mut self) {
        for activity in &mut self.activities {
            match activity {
                ConversationActivity::Approval(model) => {
                    model.status = ApprovalCardStatus::Resolved;
                }
                ConversationActivity::FileApproval(model) => {
                    model.status = FileApprovalStatus::Resolved;
                }
                ConversationActivity::UserInput(model) if model.is_interactive() => {
                    model.status =
                        crate::components::user_input_request::UserInputRequestStatus::Cancelled;
                }
                ConversationActivity::PermissionsApproval(model) if model.is_interactive() => {
                    model.status=crate::components::permissions_approval::PermissionApprovalStatus::Cancelled;
                }
                _ => {}
            }
        }
        self.approval_responders.clear();
        self.command_approval_requests.clear();
        self.file_approval_responders.clear();
        self.file_changes.clear();
        self.user_input_responders.clear();
        self.permissions_approval_responders.clear();
        self.server_request_contexts.clear();
    }
}
