//! Immutable data shared by the conversation rendering paths.

use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

use gpui::{Entity, ListState, ScrollHandle};

use super::{ConfigWarningFile, ConversationListRow, HomeView};
use crate::{
    agent::UserMessageImage,
    conversation::{ConversationActivity, ConversationPhase, ResumedTurnPresentation},
    theme::Theme,
};

pub(super) struct DisclosureRenderState {
    pub(super) expanded_reasoning: HashSet<String>,
    pub(super) reasoning_disclosure_progress: HashMap<String, f32>,
    pub(super) reasoning_scroll_handles: HashMap<String, ScrollHandle>,
    pub(super) expanded_tool_groups: HashSet<String>,
    pub(super) collapsed_active_tool_groups: HashSet<String>,
    pub(super) tool_group_disclosure_progress: HashMap<String, (f32, f32)>,
    pub(super) tool_group_scroll_handles: HashMap<String, ScrollHandle>,
    pub(super) expanded_commands: HashSet<String>,
    pub(super) command_scroll_handles: HashMap<String, ScrollHandle>,
    pub(super) expanded_collaborations: HashSet<String>,
    pub(super) auto_review_views: HashMap<
        crate::agent::AgentAutoApprovalReviewKey,
        Entity<crate::components::auto_approval::AutoApprovalReviewView>,
    >,
}

#[derive(Clone)]
pub(super) struct ConversationRenderContext {
    pub(super) home_entity: Entity<HomeView>,
    pub(super) theme: Theme,
    pub(super) thinking_shimmer_progress: f32,
    pub(super) response_feedback: i8,
    pub(super) user_message_actions_visible_for_capture: bool,
    pub(super) disclosures: Rc<DisclosureRenderState>,
}

pub(super) struct MainConversationSnapshot {
    pub(super) side_chat: bool,
    pub(super) composer_height: f32,
    pub(super) rows: Rc<Vec<ConversationListRow>>,
    pub(super) phase: ConversationPhase,
    pub(super) activities: Rc<Vec<ConversationActivity>>,
    pub(super) list: ListState,
}

pub(super) struct CurrentTurnRows<'a> {
    pub(super) phase: ConversationPhase,
    pub(super) user_message: String,
    pub(super) user_images: Vec<UserMessageImage>,
    pub(super) user_message_time: String,
    pub(super) assistant_message: String,
    pub(super) assistant_message_time: Option<String>,
    pub(super) conversation_activity: &'a [ConversationActivity],
    pub(super) resumed_turn: Option<ResumedTurnPresentation>,
}

pub(super) struct ToolGroupDisclosure {
    pub(super) review_views: HashMap<
        crate::agent::AgentAutoApprovalReviewKey,
        Entity<crate::components::auto_approval::AutoApprovalReviewView>,
    >,
    pub(super) expanded: bool,
    pub(super) disclosure_progress: f32,
    pub(super) chevron_progress: f32,
    pub(super) scroll_handle: ScrollHandle,
}

pub(super) struct NoticePresentation {
    pub(super) summary: String,
    pub(super) details: Option<String>,
    pub(super) file: Option<ConfigWarningFile>,
    pub(super) accessible_kind: &'static str,
    pub(super) outer_gap: f32,
    pub(super) content_gap: f32,
}
