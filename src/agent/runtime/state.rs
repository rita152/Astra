//! Shared monotonic reduction for live delivery and late-subscriber snapshots.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    AgentAuthRecovery, AgentHookRun, AgentLocalClosure, AgentRuntimeEvent,
    AgentRuntimeObservation as Observation,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentRuntimeState {
    pub generation: u64,
    pub hooks: Vec<AgentHookRun>,
    pub auth_recoveries: Vec<AgentAuthRecovery>,
    pub hook_prompts: Vec<super::AgentScopedHookPrompt>,
    closed_turns: BTreeMap<(String, String), AgentLocalClosure>,
    closed_threads: BTreeSet<String>,
    disconnected: bool,
}

impl AgentRuntimeState {
    pub fn apply(
        &mut self,
        mut event: AgentRuntimeEvent,
    ) -> Result<Option<AgentRuntimeEvent>, String> {
        if event.generation < self.generation {
            return Ok(None);
        }
        if matches!(event.observation, Observation::GenerationStarted) {
            if event.generation == self.generation {
                return Ok(None);
            }
            *self = Self {
                generation: event.generation,
                ..Self::default()
            };
            return Ok(Some(event));
        }
        if event.generation != self.generation {
            return Err("Runtime observation arrived before generation start".into());
        }
        match &mut event.observation {
            Observation::HookPrompt(update) => {
                if let Some(existing) = self.hook_prompts.iter_mut().find(|v| {
                    v.thread_id == update.thread_id
                        && v.turn_id == update.turn_id
                        && v.prompt.id == update.prompt.id
                }) {
                    if existing.prompt.completed == Some(true)
                        && update.prompt.completed != Some(true)
                    {
                        return Ok(None);
                    }
                    if existing == update {
                        return Ok(None);
                    }
                    *existing = update.clone();
                } else {
                    self.hook_prompts.push(update.clone());
                }
            }
            Observation::Hook(update) => {
                let closure = self.closure(&update.thread_id, update.turn_id.as_deref());
                if let Some(existing) = self.hooks.iter_mut().find(|v| {
                    v.thread_id == update.thread_id
                        && v.turn_id == update.turn_id
                        && v.id == update.id
                }) {
                    if existing.would_regress(update) {
                        return Ok(None);
                    }
                    update.closed_locally = existing
                        .closed_locally
                        .or(update.closed_locally)
                        .or(closure);
                    if existing == update.as_ref() {
                        return Ok(None);
                    }
                    *existing = *update.clone();
                } else {
                    update.closed_locally = update.closed_locally.or(closure);
                    self.hooks.push(*update.clone());
                }
            }
            Observation::AuthRecovery(update) => {
                let closure = self.closure(&update.thread_id, Some(&update.turn_id));
                if let Some(existing) = self.auth_recoveries.iter_mut().find(|v| {
                    v.thread_id == update.thread_id
                        && v.turn_id == update.turn_id
                        && v.provider == update.provider
                }) {
                    if existing.completed_message.is_some() && update.completed_message.is_none() {
                        return Ok(None);
                    }
                    if update.started_message.is_none() {
                        update.started_message.clone_from(&existing.started_message);
                    }
                    if update.completed_message.is_none() {
                        update
                            .completed_message
                            .clone_from(&existing.completed_message);
                    }
                    update.closed_locally = existing
                        .closed_locally
                        .or(update.closed_locally)
                        .or(closure);
                    if existing == update {
                        return Ok(None);
                    }
                    *existing = update.clone();
                } else {
                    update.closed_locally = update.closed_locally.or(closure);
                    self.auth_recoveries.push(update.clone());
                }
            }
            Observation::TurnClosed {
                thread_id,
                turn_id,
                reason,
            } => {
                self.closed_turns
                    .entry((thread_id.clone(), turn_id.clone()))
                    .or_insert(*reason);
                for hook in &mut self.hooks {
                    if hook.thread_id == *thread_id
                        && hook.turn_id.as_ref() == Some(turn_id)
                        && hook.is_waiting()
                    {
                        hook.closed_locally = Some(*reason);
                    }
                }
                for auth in &mut self.auth_recoveries {
                    if auth.thread_id == *thread_id && auth.turn_id == *turn_id && auth.is_waiting()
                    {
                        auth.closed_locally = Some(*reason);
                    }
                }
            }
            Observation::ThreadClosed { thread_id } => {
                self.closed_threads.insert(thread_id.clone());
                for hook in &mut self.hooks {
                    if hook.thread_id == *thread_id && hook.is_waiting() {
                        hook.closed_locally = Some(AgentLocalClosure::ThreadClosed);
                    }
                }
                for auth in &mut self.auth_recoveries {
                    if auth.thread_id == *thread_id && auth.is_waiting() {
                        auth.closed_locally = Some(AgentLocalClosure::ThreadClosed);
                    }
                }
            }
            Observation::Disconnected => {
                self.disconnected = true;
                for hook in &mut self.hooks {
                    if hook.is_waiting() {
                        hook.closed_locally = Some(AgentLocalClosure::Disconnected);
                    }
                }
                for auth in &mut self.auth_recoveries {
                    if auth.is_waiting() {
                        auth.closed_locally = Some(AgentLocalClosure::Disconnected);
                    }
                }
            }
            Observation::GenerationStarted => unreachable!(),
        }
        Ok(Some(event))
    }

    fn closure(&self, thread: &str, turn: Option<&str>) -> Option<AgentLocalClosure> {
        turn.and_then(|turn| {
            self.closed_turns
                .get(&(thread.to_owned(), turn.to_owned()))
                .copied()
        })
        .or_else(|| {
            self.closed_threads
                .contains(thread)
                .then_some(AgentLocalClosure::ThreadClosed)
        })
        .or_else(|| self.disconnected.then_some(AgentLocalClosure::Disconnected))
    }

    pub fn snapshot(&self) -> Vec<AgentRuntimeEvent> {
        if self.generation == 0 {
            return Vec::new();
        }
        let mut observations = vec![Observation::GenerationStarted];
        observations.extend(
            self.hooks
                .iter()
                .cloned()
                .map(|v| Observation::Hook(Box::new(v))),
        );
        observations.extend(
            self.auth_recoveries
                .iter()
                .cloned()
                .map(Observation::AuthRecovery),
        );
        observations.extend(
            self.hook_prompts
                .iter()
                .cloned()
                .map(Observation::HookPrompt),
        );
        observations.extend(self.closed_turns.iter().map(|((thread, turn), reason)| {
            Observation::TurnClosed {
                thread_id: thread.clone(),
                turn_id: turn.clone(),
                reason: *reason,
            }
        }));
        observations.extend(
            self.closed_threads
                .iter()
                .cloned()
                .map(|thread_id| Observation::ThreadClosed { thread_id }),
        );
        if self.disconnected {
            observations.push(Observation::Disconnected);
        }
        observations
            .into_iter()
            .map(|observation| AgentRuntimeEvent {
                generation: self.generation,
                observation,
            })
            .collect()
    }
}
