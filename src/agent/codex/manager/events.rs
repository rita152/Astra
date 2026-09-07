//! Connection event subscribers and latest-state replay.

use std::collections::HashMap;

use async_channel::{Receiver, Sender};

use crate::agent::AgentConnectionEvent;

#[derive(Default)]
pub(super) struct ConnectionEventHub {
    pub(super) subscribers: Vec<Sender<AgentConnectionEvent>>,
    pub(super) snapshots: HashMap<String, AgentConnectionEvent>,
}

impl ConnectionEventHub {
    pub(super) fn subscribe(&mut self) -> Receiver<AgentConnectionEvent> {
        let (sender, receiver) = async_channel::unbounded();
        for event in self.snapshots.values().cloned() {
            let _ = sender.send_blocking(event);
        }
        self.subscribers.push(sender);
        receiver
    }

    pub(super) fn publish(&mut self, event: AgentConnectionEvent) {
        self.snapshots
            .insert(connection_event_key(&event), event.clone());
        self.subscribers
            .retain(|subscriber| subscriber.send_blocking(event.clone()).is_ok());
    }
}

pub(super) fn connection_event_key(event: &AgentConnectionEvent) -> String {
    match event {
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
