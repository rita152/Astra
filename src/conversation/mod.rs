//! Conversation state and event processing, independent of GPUI entities.

mod activity;
mod auto_approval;
mod events;
mod lifecycle;
mod model;
mod state;
mod stream;
mod transcript;

pub(crate) use activity::{ConversationActivity, ReasoningActivityPresentation};
pub(crate) use auto_approval::{AutoApprovalReviewPresentation, StrictReviewPresentation};
pub(crate) use state::ConversationState;
pub(crate) use stream::{
    STREAM_DISCONNECTED_MESSAGE, STREAM_UPDATE_INTERVAL, collect_ready_agent_events,
    ensure_closed_batch_is_terminal,
};
pub(crate) use transcript::{
    ConversationPhase, ConversationTranscriptTurn, ResumedTurnPresentation,
    current_local_time_label,
};

#[cfg(test)]
pub(crate) use activity::{
    find_command_activity_mut, reasoning_parts_text, upsert_command_activity,
};
#[cfg(test)]
pub(crate) use stream::{STREAM_EVENTS_PER_UPDATE, push_coalesced_agent_event};
