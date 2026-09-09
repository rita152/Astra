//! Pending server requests, typed payloads, and completion tracking.

use std::collections::{HashMap, HashSet};

use anyhow::{Context as _, Result, bail};
use serde_json::{Value, json};

use super::{approvals::command_choice_value, permissions::permission_profile_value};
use crate::agent::{
    AgentCommandApprovalChoice, AgentFileApprovalChoice, AgentPermissionRequestProfile,
    AgentPermissionsApprovalChoice, AgentServerRequestId, AgentServerRequestMetadata,
    AgentUserInputResponse,
};

#[derive(Clone, Debug)]
pub(super) struct PendingCommandApproval {
    #[allow(dead_code)] // Retained verbatim for protocol auditing; exercised by registry tests.
    pub(super) params: Value,
    pub(super) available_decisions: Vec<Value>,
}

#[derive(Clone, Debug)]
pub(super) struct PendingUserInputRequest {
    pub(super) question_ids: Vec<String>,
}

#[derive(Clone, Debug)]
pub(super) struct PendingPermissionsApprovalRequest {
    pub(super) permissions: AgentPermissionRequestProfile,
}

#[derive(Clone, Debug)]
pub(super) enum PendingServerRequestPayload {
    CommandApproval(PendingCommandApproval),
    FileApproval,
    UserInput(PendingUserInputRequest),
    PermissionsApproval(PendingPermissionsApprovalRequest),
}

#[derive(Clone, Debug)]
struct PendingServerRequest {
    pub(super) metadata: AgentServerRequestMetadata,
    pub(super) payload: PendingServerRequestPayload,
    pub(super) responded: bool,
}

#[derive(Default)]
pub(super) struct ServerRequestRegistry {
    pending: HashMap<AgentServerRequestId, PendingServerRequest>,
    completed: HashMap<AgentServerRequestId, AgentServerRequestMetadata>,
    closed: bool,
}

pub(super) enum ServerRequestResolution {
    Resolved(AgentServerRequestMetadata),
    AlreadyResolved,
}

impl ServerRequestRegistry {
    pub(super) fn close(&mut self) {
        self.closed = true;
    }

    fn ensure_open(&self) -> Result<()> {
        if self.closed {
            bail!("Codex turn 已结束，server request 不再可回复或注册");
        }
        Ok(())
    }
    pub(super) fn register_server_request(
        &mut self,
        metadata: AgentServerRequestMetadata,
        payload: PendingServerRequestPayload,
    ) -> Result<()> {
        self.ensure_open()?;
        if self.pending.contains_key(&metadata.request_id)
            || self.completed.contains_key(&metadata.request_id)
        {
            bail!(
                "收到重复的 Codex server request id {:?}",
                metadata.request_id
            );
        }
        self.pending.insert(
            metadata.request_id.clone(),
            PendingServerRequest {
                metadata,
                payload,
                responded: false,
            },
        );
        Ok(())
    }
    pub(super) fn resolve_server_request(
        &mut self,
        request_id: &AgentServerRequestId,
        thread_id: &str,
    ) -> Result<ServerRequestResolution> {
        if let Some(request) = self.pending.get(request_id) {
            if request.metadata.thread_id != thread_id {
                bail!(
                    "serverRequest/resolved threadId `{thread_id}` 与 pending request {:?} 的 threadId `{}` 不一致",
                    request_id,
                    request.metadata.thread_id
                );
            }
            let metadata = request.metadata.clone();
            self.pending.remove(request_id);
            self.completed.insert(request_id.clone(), metadata.clone());
            return Ok(ServerRequestResolution::Resolved(metadata));
        }
        if let Some(metadata) = self.completed.get(request_id) {
            if metadata.thread_id != thread_id {
                bail!(
                    "重复 serverRequest/resolved 的 threadId `{thread_id}` 与 request {:?} 的 threadId `{}` 不一致",
                    request_id,
                    metadata.thread_id
                );
            }
            return Ok(ServerRequestResolution::AlreadyResolved);
        }
        bail!(
            "serverRequest/resolved 引用了未知 request {:?}（threadId=`{thread_id}`）",
            request_id
        )
    }
    pub(super) fn drain_pending_server_requests(
        &mut self,
    ) -> Result<Vec<AgentServerRequestMetadata>> {
        let pending = std::mem::take(&mut self.pending);
        let mut metadata = Vec::with_capacity(pending.len());
        for (request_id, request) in pending {
            self.completed.insert(request_id, request.metadata.clone());
            metadata.push(request.metadata);
        }
        Ok(metadata)
    }
    pub(super) fn command_decision(
        &mut self,
        request_id: &AgentServerRequestId,
        choice: AgentCommandApprovalChoice,
    ) -> Result<Value> {
        self.ensure_open()?;
        let request = self
            .pending
            .get_mut(request_id)
            .with_context(|| format!("command approval {request_id:?} 已经 resolved 或不存在"))?;
        if request.responded {
            bail!("command approval {request_id:?} 已经回复，拒绝重复 decision");
        }
        let PendingServerRequestPayload::CommandApproval(command) = &request.payload else {
            bail!("request {request_id:?} 不是 command approval，拒绝错误类型的 responder")
        };
        let requested = command_choice_value(&choice);
        let decision = command
            .available_decisions
            .iter()
            .find(|decision| **decision == requested)
            .cloned()
            .with_context(|| {
                format!("command approval {request_id:?} 未提供所选 decision，拒绝越权回复")
            })?;
        request.responded = true;
        Ok(decision)
    }

    pub(super) fn file_decision(
        &mut self,
        request_id: &AgentServerRequestId,
        choice: AgentFileApprovalChoice,
    ) -> Result<Value> {
        self.ensure_open()?;
        let request = self
            .pending
            .get_mut(request_id)
            .with_context(|| format!("file approval {request_id:?} 已经 resolved 或不存在"))?;
        if request.responded {
            bail!("file approval {request_id:?} 已经回复，拒绝重复 decision");
        }
        if !matches!(request.payload, PendingServerRequestPayload::FileApproval) {
            bail!("request {request_id:?} 不是 file approval，拒绝错误类型的 responder");
        }
        let decision = match choice {
            AgentFileApprovalChoice::Accept => "accept",
            AgentFileApprovalChoice::AcceptForSession => "acceptForSession",
            AgentFileApprovalChoice::Decline => "decline",
            AgentFileApprovalChoice::Cancel => "cancel",
        };
        request.responded = true;
        Ok(json!(decision))
    }
    pub(super) fn user_input_answers(
        &mut self,
        request_id: &AgentServerRequestId,
        response: AgentUserInputResponse,
    ) -> Result<Value> {
        self.ensure_open()?;
        let request = self
            .pending
            .get_mut(request_id)
            .with_context(|| format!("user input request {request_id:?} 已经 resolved 或不存在"))?;
        if request.responded {
            bail!("user input request {request_id:?} 已经回复，拒绝重复 answers");
        }
        let PendingServerRequestPayload::UserInput(pending) = &request.payload else {
            bail!("request {request_id:?} 不是 user input，拒绝错误类型的 responder")
        };
        let question_ids = pending.question_ids.iter().collect::<HashSet<_>>();
        let mut seen = HashSet::new();
        let mut answers = serde_json::Map::new();
        for answer in response.answers {
            if !seen.insert(answer.question_id.clone()) {
                bail!(
                    "user input request {request_id:?} 对 question id `{}` 提供了重复答案",
                    answer.question_id
                );
            }
            if !question_ids.contains(&answer.question_id) {
                bail!(
                    "user input request {request_id:?} 不包含 question id `{}`",
                    answer.question_id
                );
            }
            answers.insert(answer.question_id, json!({ "answers": answer.answers }));
        }
        request.responded = true;
        Ok(Value::Object(answers))
    }
    pub(super) fn permissions_decision(
        &mut self,
        request_id: &AgentServerRequestId,
        choice: AgentPermissionsApprovalChoice,
    ) -> Result<(Value, &'static str)> {
        self.ensure_open()?;
        let request = self.pending.get_mut(request_id).with_context(|| {
            format!("permissions approval {request_id:?} 已经 resolved 或不存在")
        })?;
        if request.responded {
            bail!("permissions approval {request_id:?} 已经回复，拒绝重复 decision");
        }
        let PendingServerRequestPayload::PermissionsApproval(pending) = &request.payload else {
            bail!("request {request_id:?} 不是 permissions approval，拒绝错误类型的 responder")
        };
        let response = match choice {
            AgentPermissionsApprovalChoice::AllowOnce => {
                (permission_profile_value(&pending.permissions), "turn")
            }
            AgentPermissionsApprovalChoice::AllowForSession => {
                (permission_profile_value(&pending.permissions), "session")
            }
            AgentPermissionsApprovalChoice::Decline => (Value::Object(Default::default()), "turn"),
        };
        request.responded = true;
        Ok(response)
    }
    #[cfg(test)]
    pub(super) fn pending_server_request_snapshot(
        &self,
    ) -> Vec<(AgentServerRequestMetadata, Option<Value>, bool)> {
        self.pending
            .values()
            .map(|request| {
                let params = match &request.payload {
                    PendingServerRequestPayload::CommandApproval(approval) => {
                        Some(approval.params.clone())
                    }
                    PendingServerRequestPayload::FileApproval
                    | PendingServerRequestPayload::UserInput(_)
                    | PendingServerRequestPayload::PermissionsApproval(_) => None,
                };
                (request.metadata.clone(), params, request.responded)
            })
            .collect()
    }
}
