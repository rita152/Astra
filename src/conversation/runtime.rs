//! Thread-isolated runtime state, projected without inventing historical runs.

#[cfg(test)]
mod tests;

use super::{ConversationActivity, ConversationPhase, ConversationState};
use crate::agent::{
    AgentConnectionEvent, AgentEvent, AgentHookPrompt, AgentRuntimeEvent,
    AgentRuntimeObservation as Observation,
};

impl ConversationState {
    pub(crate) fn apply_runtime_event(&mut self, event: AgentRuntimeEvent) -> bool {
        if event.generation < self.runtime.generation {
            return false;
        }
        if matches!(event.observation, Observation::GenerationStarted) {
            if event.generation > self.runtime.generation && self.runtime.generation != 0 {
                let mut retired = self.runtime.clone();
                let _ = retired.apply(AgentRuntimeEvent {
                    generation: retired.generation,
                    observation: Observation::Disconnected,
                });
                self.retired_runtime.push(retired);
            }
            self.pending_connection_events
                .values_mut()
                .for_each(|events| {
                    events.retain(|v|
                !matches!(v, AgentConnectionEvent::Runtime(v) if v.generation < event.generation))
                });
        }
        let thread = match &event.observation {
            Observation::HookPrompt(prompt) => Some(prompt.thread_id.as_str()),
            Observation::Hook(hook) => Some(hook.thread_id.as_str()),
            Observation::AuthRecovery(auth) => Some(auth.thread_id.as_str()),
            Observation::TurnClosed { thread_id, .. } | Observation::ThreadClosed { thread_id } => {
                Some(thread_id.as_str())
            }
            Observation::Disconnected | Observation::GenerationStarted => None,
        };
        if let Some(thread) = thread
            && self.thread_id.as_deref() != Some(thread)
        {
            if self.thread_id.is_none() {
                let events = self
                    .pending_connection_events
                    .entry(thread.to_owned())
                    .or_default();
                let event = AgentConnectionEvent::Runtime(event);
                if !events.contains(&event) {
                    events.push(event);
                }
            }
            return false;
        }
        match self.runtime.apply(event) {
            Ok(Some(_)) => {
                self.sync_runtime_prompts();
                true
            }
            Ok(None) => false,
            Err(message) => {
                self.apply_agent_event_batch(vec![AgentEvent::Error {
                    message,
                    details: None,
                    will_retry: false,
                }]);
                true
            }
        }
    }

    pub(crate) fn apply_hook_prompt(&mut self, prompt: AgentHookPrompt) {
        if let Some(thread_id) = self.thread_id.clone()
            && let Some(turn_id) = self.turn_id.clone()
        {
            upsert_prompt(
                &mut self.activities,
                crate::agent::AgentScopedHookPrompt {
                    thread_id,
                    turn_id,
                    prompt,
                },
            );
        }
    }

    pub(crate) fn sync_runtime_prompts(&mut self) {
        for prompt in self
            .retired_runtime
            .iter()
            .chain(std::iter::once(&self.runtime))
            .flat_map(|runtime| &runtime.hook_prompts)
        {
            if self.thread_id.as_deref() != Some(prompt.thread_id.as_str()) {
                continue;
            }
            if self.turn_id.as_deref() == Some(prompt.turn_id.as_str()) {
                upsert_prompt(&mut self.activities, prompt.clone());
            } else if let Some(turn) = self
                .transcript
                .iter_mut()
                .find(|turn| turn.turn_id.as_deref() == Some(prompt.turn_id.as_str()))
            {
                upsert_prompt(&mut turn.activities, prompt.clone());
            }
        }
    }

    pub(crate) fn change_runtime_scope(&mut self, thread_id: Option<&str>) {
        if self
            .thread_id
            .as_deref()
            .is_some_and(|id| Some(id) != thread_id)
        {
            let generation = self.runtime.generation;
            self.runtime = Default::default();
            self.retired_runtime.clear();
            let _ = self.runtime.apply(AgentRuntimeEvent {
                generation,
                observation: Observation::GenerationStarted,
            });
        }
    }

    pub(crate) fn close_runtime_turn(&mut self, reason: crate::agent::AgentLocalClosure) {
        if let Some(thread_id) = self.thread_id.clone()
            && let Some(turn_id) = self.turn_id.clone()
        {
            self.apply_runtime_event(AgentRuntimeEvent {
                generation: self.runtime.generation,
                observation: Observation::TurnClosed {
                    thread_id,
                    turn_id,
                    reason,
                },
            });
        }
    }

    pub(crate) fn project_runtime(
        &self,
        turn_id: Option<&str>,
        phase: ConversationPhase,
        activities: &mut Vec<ConversationActivity>,
    ) {
        for prompt in self
            .retired_runtime
            .iter()
            .chain(std::iter::once(&self.runtime))
            .flat_map(|runtime| &runtime.hook_prompts)
        {
            if self.thread_id.as_deref() == Some(prompt.thread_id.as_str())
                && turn_id == Some(prompt.turn_id.as_str())
            {
                upsert_prompt(activities, prompt.clone());
            }
        }
        // ChatGPT shows hook summaries after the turn, not in its work stream.
        if matches!(
            phase,
            ConversationPhase::Complete | ConversationPhase::Stopped | ConversationPhase::Failed
        ) {
            let mut hooks_by_id = std::collections::BTreeMap::new();
            for hook in self
                .retired_runtime
                .iter()
                .chain(std::iter::once(&self.runtime))
                .flat_map(|runtime| &runtime.hooks)
                .filter(|hook| {
                    self.thread_id.as_deref() == Some(&hook.thread_id)
                        && turn_id.is_some()
                        && hook.turn_id.as_deref() == turn_id
                })
            {
                hooks_by_id.insert(hook.id.clone(), hook.clone());
            }
            let mut hooks = hooks_by_id.into_values().collect::<Vec<_>>();
            hooks.sort_by_key(|hook| (hook.display_order, hook.started_at));
            if !hooks.is_empty() {
                activities.push(ConversationActivity::HookSummary(hooks));
            }
        }
    }
}

pub(super) fn upsert_prompt(
    activities: &mut Vec<ConversationActivity>,
    prompt: crate::agent::AgentScopedHookPrompt,
) {
    if let Some(ConversationActivity::HookPrompt(existing)) = activities
        .iter_mut()
        .find(|a| matches!(a, ConversationActivity::HookPrompt(v) if v.thread_id == prompt.thread_id && v.turn_id == prompt.turn_id && v.prompt.id == prompt.prompt.id))
    {
        if existing.prompt.completed != Some(true) || prompt.prompt.completed == Some(true) {
            *existing = prompt;
        }
    } else {
        activities.push(ConversationActivity::HookPrompt(prompt));
    }
}
