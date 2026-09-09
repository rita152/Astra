//! Prompt preparation and interruption independently of UI focus and menus.

use super::{
    state::ConversationState,
    transcript::{ConversationPhase, current_local_time_label},
};
use crate::agent::{
    AgentEvent, AgentInterruptHandle, AgentInterruptOutcome, normalize_user_message_for_display,
};

impl ConversationState {
    pub(crate) fn begin_prompt(&mut self, prompt: &str) -> u64 {
        self.commit_current_turn();
        self.turn_id = None;
        self.history_loading = false;
        self.history_error = None;
        self.user_message = Some(normalize_user_message_for_display(prompt));
        self.user_message_time = Some(current_local_time_label());
        self.assistant_message.clear();
        self.activities.clear();
        self.approval_responders.clear();
        self.command_approval_requests.clear();
        self.file_approval_responders.clear();
        self.file_changes.clear();
        self.user_input_responders.clear();
        self.permissions_approval_responders.clear();
        self.server_request_contexts.clear();
        self.assistant_message_time = None;
        self.phase = ConversationPhase::Starting;
        self.cycle = self.cycle.wrapping_add(1);
        self.cycle
    }

    pub(crate) fn stop_generation(&mut self) -> bool {
        if matches!(
            self.phase,
            ConversationPhase::Starting
                | ConversationPhase::Thinking
                | ConversationPhase::Streaming
                | ConversationPhase::Stopping
        ) {
            let result = self
                .active_turn
                .as_ref()
                .map(AgentInterruptHandle::interrupt)
                .unwrap_or_else(|| Err("当前 Codex turn 没有可用的中断连接".to_owned()));
            match result {
                Ok(AgentInterruptOutcome::Requested | AgentInterruptOutcome::AlreadyRequested) => {
                    self.phase = ConversationPhase::Stopping;
                }
                Ok(AgentInterruptOutcome::AlreadyFinished) => {}
                Err(error) => {
                    self.apply_agent_event_batch(vec![AgentEvent::Failed(format!(
                        "无法中断 Codex turn：{error}"
                    ))]);
                }
            }
            true
        } else {
            false
        }
    }
}
