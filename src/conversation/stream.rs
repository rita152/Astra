//! Bounded event collection and adjacent delta coalescing.

use std::time::Duration;

use crate::agent::AgentEvent;

// Collect for half a 60 Hz frame after the first event. This catches protocol
// micro-bursts while leaving enough time for GPUI's next 60/120 Hz paint.
pub(crate) const STREAM_UPDATE_INTERVAL: Duration = Duration::from_millis(8);

// Keep an unexpectedly large command-output burst from monopolizing the UI
// executor. Adjacent deltas are merged before they touch view state.
pub(crate) const STREAM_EVENTS_PER_UPDATE: usize = 512;

pub(crate) const STREAM_DISCONNECTED_MESSAGE: &str = "Codex 事件流意外断开";

pub(crate) fn push_coalesced_agent_event(batch: &mut Vec<AgentEvent>, event: AgentEvent) {
    match event {
        AgentEvent::TextDelta(delta) => {
            if let Some(AgentEvent::TextDelta(buffered)) = batch.last_mut() {
                buffered.push_str(&delta);
            } else {
                batch.push(AgentEvent::TextDelta(delta));
            }
        }
        AgentEvent::CommandOutputDelta { item_id, delta } => {
            if let Some(AgentEvent::CommandOutputDelta {
                item_id: buffered_item_id,
                delta: buffered,
            }) = batch.last_mut()
                && buffered_item_id == &item_id
            {
                buffered.push_str(&delta);
            } else {
                batch.push(AgentEvent::CommandOutputDelta { item_id, delta });
            }
        }
        AgentEvent::CommandTerminalInteraction { .. } => batch.push(event),
        AgentEvent::ReasoningSummaryTextDelta {
            item_id,
            summary_index,
            delta,
        } => {
            if let Some(AgentEvent::ReasoningSummaryTextDelta {
                item_id: buffered_item_id,
                summary_index: buffered_summary_index,
                delta: buffered,
            }) = batch.last_mut()
                && buffered_item_id == &item_id
                && *buffered_summary_index == summary_index
            {
                buffered.push_str(&delta);
            } else {
                batch.push(AgentEvent::ReasoningSummaryTextDelta {
                    item_id,
                    summary_index,
                    delta,
                });
            }
        }
        AgentEvent::ReasoningTextDelta {
            item_id,
            content_index,
            delta,
        } => {
            if let Some(AgentEvent::ReasoningTextDelta {
                item_id: buffered_item_id,
                content_index: buffered_content_index,
                delta: buffered,
            }) = batch.last_mut()
                && buffered_item_id == &item_id
                && *buffered_content_index == content_index
            {
                buffered.push_str(&delta);
            } else {
                batch.push(AgentEvent::ReasoningTextDelta {
                    item_id,
                    content_index,
                    delta,
                });
            }
        }
        event => batch.push(event),
    }
}

pub(crate) fn collect_ready_agent_events(
    receiver: &async_channel::Receiver<AgentEvent>,
    first_event: AgentEvent,
) -> (Vec<AgentEvent>, bool) {
    let mut batch = Vec::with_capacity(16);
    push_coalesced_agent_event(&mut batch, first_event);
    let mut channel_closed = false;
    for _ in 1..STREAM_EVENTS_PER_UPDATE {
        match receiver.try_recv() {
            Ok(event) => push_coalesced_agent_event(&mut batch, event),
            Err(async_channel::TryRecvError::Empty) => break,
            Err(async_channel::TryRecvError::Closed) => {
                channel_closed = true;
                break;
            }
        }
    }
    (batch, channel_closed)
}

pub(crate) fn ensure_closed_batch_is_terminal(batch: &mut Vec<AgentEvent>) {
    if !batch.iter().any(|event| {
        matches!(
            event,
            AgentEvent::Completed | AgentEvent::Interrupted | AgentEvent::Failed(_)
        )
    }) {
        batch.push(AgentEvent::Failed(STREAM_DISCONNECTED_MESSAGE.to_owned()));
    }
}
