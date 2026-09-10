//! Connection event subscribers and latest-state replay.

use std::collections::HashMap;

use async_channel::{Receiver, Sender};

use crate::agent::{AgentAutoApprovalReviewStatus, AgentConnectionEvent};

#[derive(Default)]
pub(super) struct ConnectionEventHub {
    pub(super) runtime: crate::agent::AgentRuntimeState,
    pub(super) subscribers: Vec<Sender<AgentConnectionEvent>>,
    pub(super) snapshots: HashMap<String, AgentConnectionEvent>,
}

impl ConnectionEventHub {
    pub(super) fn subscribe(&mut self) -> Receiver<AgentConnectionEvent> {
        let (sender, receiver) = async_channel::unbounded();
        for event in self.runtime.snapshot() {
            let _ = sender.send_blocking(AgentConnectionEvent::Runtime(event));
        }
        for event in self.snapshots.values().cloned() {
            let _ = sender.send_blocking(event);
        }
        self.subscribers.push(sender);
        receiver
    }

    pub(super) fn publish(&mut self, mut event: AgentConnectionEvent) {
        let key = connection_event_key(&event);
        if let AgentConnectionEvent::AutoApprovalReviewUpdated(update) = &mut event
            && let Some(AgentConnectionEvent::AutoApprovalReviewUpdated(existing)) =
                self.snapshots.get(&key)
        {
            if (existing.completed_at_ms.is_some() && update.completed_at_ms.is_none())
                || (existing.status != AgentAutoApprovalReviewStatus::InProgress
                    && update.status == AgentAutoApprovalReviewStatus::InProgress)
                || existing
                    .completed_at_ms
                    .zip(update.completed_at_ms)
                    .is_some_and(|(old, new)| new < old)
            {
                return;
            }
            if update.rationale.is_none() {
                update.rationale.clone_from(&existing.rationale);
            }
            if update.risk_level.is_none() {
                update.risk_level.clone_from(&existing.risk_level);
            }
            if update.user_authorization.is_none() {
                update
                    .user_authorization
                    .clone_from(&existing.user_authorization);
            }
            if update.as_ref() == existing.as_ref() {
                return;
            }
        }
        self.snapshots.insert(key, event.clone());
        self.subscribers
            .retain(|subscriber| subscriber.send_blocking(event.clone()).is_ok());
    }

    pub(super) fn publish_runtime(
        &mut self,
        event: crate::agent::AgentRuntimeEvent,
    ) -> anyhow::Result<()> {
        if let Some(event) = self.runtime.apply(event).map_err(anyhow::Error::msg)? {
            self.subscribers.retain(|subscriber| {
                subscriber
                    .send_blocking(AgentConnectionEvent::Runtime(event.clone()))
                    .is_ok()
            });
        }
        Ok(())
    }
}

pub(super) fn connection_event_key(event: &AgentConnectionEvent) -> String {
    match event {
        AgentConnectionEvent::Runtime(_) => {
            unreachable!("runtime observations use their generation-aware reducer")
        }
        AgentConnectionEvent::DeprecationNotice(notice) => format!("deprecation:{notice:?}"),
        AgentConnectionEvent::AutoApprovalReviewUpdated(review) => {
            format!("auto-review:{:?}", review.key)
        }
        AgentConnectionEvent::StrictReviewRequired(requirement) => {
            format!("strict-review:{:?}", requirement)
        }
        AgentConnectionEvent::GuardianWarning(warning) => format!("guardian-warning:{warning:?}"),
        AgentConnectionEvent::Warning { thread_id, message } => {
            format!("warning:{thread_id:?}:{message}")
        }
        AgentConnectionEvent::ConfigWarning(warning) => format!(
            "config:{:?}:{:?}:{:?}:{}",
            warning.path, warning.line, warning.column, warning.summary
        ),
        AgentConnectionEvent::McpServerStartupStatusUpdated(status) => {
            format!("mcp:{:?}:{}", status.thread_id, status.name)
        }
        AgentConnectionEvent::ThreadStatusChanged(status) => {
            format!("thread-status:{}", status.thread_id)
        }
        AgentConnectionEvent::ThreadSettingsUpdated { thread_id, .. } => {
            format!("thread-settings:{thread_id}")
        }
        AgentConnectionEvent::ProjectChanged { project_id, .. } => {
            format!("project:{project_id}")
        }
        AgentConnectionEvent::ThreadArchived { thread_id }
        | AgentConnectionEvent::ThreadUnarchived { thread_id }
        | AgentConnectionEvent::ThreadDeleted { thread_id } => {
            format!("thread-membership:{thread_id}")
        }
        AgentConnectionEvent::ThreadNameUpdated { thread_id, .. } => {
            format!("thread-name:{thread_id}")
        }
        AgentConnectionEvent::ThreadClosed { thread_id } => {
            format!("thread-closed:{thread_id}")
        }
        AgentConnectionEvent::ThreadProjectUpdated { thread_id, .. } => {
            format!("thread-project:{thread_id}")
        }
        AgentConnectionEvent::AccountRateLimitsUpdated(_) => "rate-limits".to_owned(),
    }
}
