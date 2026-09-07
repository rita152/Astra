//! State and focus ownership for independent application panels.

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use gpui::{Context, Entity, FocusHandle};

use super::ChatApp;
use crate::{
    agent::ThreadId,
    components::{file_change::DiffReviewPresentation, home::HomeView},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BottomPanelMode {
    Review,
    Terminal,
    Browser,
    Files,
    SideChat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RightPanelMode {
    Files,
    SideChat,
    Browser,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProjectCreationStep {
    Kind,
    Remote,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProjectCreationKind {
    Local,
    Remote,
}

#[derive(Clone)]
pub(super) struct SubagentPanel {
    pub(super) thread_id: ThreadId,
    pub(super) name: String,
    pub(super) home: Entity<HomeView>,
}

pub(super) struct SidebarLayoutState {
    pub(super) collapsed: bool,
    pub(super) resize_hovered: bool,
    pub(super) resize_dragging: bool,
    pub(super) resize_pointer_offset: f32,
    pub(super) reveal: f32,
    pub(super) animation_from: f32,
    pub(super) animation_to: f32,
    pub(super) animation_started_at: Option<Instant>,
    pub(super) animation_duration: Duration,
    pub(super) animation_running: bool,
}

impl Default for SidebarLayoutState {
    fn default() -> Self {
        Self {
            collapsed: false,
            resize_hovered: false,
            resize_dragging: false,
            resize_pointer_offset: 0.0,
            reveal: 1.0,
            animation_from: 1.0,
            animation_to: 1.0,
            animation_started_at: None,
            animation_duration: Duration::ZERO,
            animation_running: false,
        }
    }
}

pub(super) struct BottomPanelState {
    pub(super) open: bool,
    pub(super) tabs: Vec<BottomPanelMode>,
    pub(super) active_tab: Option<usize>,
    pub(super) hovered_tab: Option<usize>,
    pub(super) add_menu_open: bool,
    pub(super) focused_item: usize,
    pub(super) keyboard_focus: bool,
    pub(super) focus: FocusHandle,
    pub(super) focus_pending: bool,
}

impl BottomPanelState {
    pub(super) fn new(cx: &mut Context<ChatApp>) -> Self {
        Self {
            open: false,
            tabs: Vec::new(),
            active_tab: None,
            hovered_tab: None,
            add_menu_open: false,
            focused_item: 0,
            keyboard_focus: false,
            focus: cx.focus_handle().tab_stop(true),
            focus_pending: false,
        }
    }
}

pub(super) struct RightPanelState {
    pub(super) open: bool,
    pub(super) mode: Option<RightPanelMode>,
    pub(super) focused_item: usize,
    pub(super) keyboard_focus: bool,
    pub(super) focus: FocusHandle,
    pub(super) focus_pending: bool,
    pub(super) width: Option<f32>,
    pub(super) resize_hovered: bool,
    pub(super) resize_dragging: bool,
    pub(super) resize_pointer_offset: f32,
    pub(super) subagent: Option<SubagentPanel>,
    pub(super) subagent_menu_open: bool,
    pub(super) diff_review: Option<DiffReviewPresentation>,
}

impl RightPanelState {
    pub(super) fn new(cx: &mut Context<ChatApp>) -> Self {
        Self {
            open: false,
            mode: None,
            focused_item: 0,
            keyboard_focus: false,
            focus: cx.focus_handle().tab_stop(true),
            focus_pending: false,
            width: None,
            resize_hovered: false,
            resize_dragging: false,
            resize_pointer_offset: 0.0,
            subagent: None,
            subagent_menu_open: false,
            diff_review: None,
        }
    }
}

pub(super) struct ImagePreviewState {
    pub(super) path: Option<PathBuf>,
    pub(super) focus: FocusHandle,
    pub(super) focus_active: bool,
    pub(super) previous_focus: Option<FocusHandle>,
    pub(super) dimensions: Option<(u32, u32)>,
    pub(super) zoom: f32,
}

impl ImagePreviewState {
    pub(super) fn new(cx: &mut Context<ChatApp>) -> Self {
        Self {
            path: None,
            focus: cx.focus_handle(),
            focus_active: false,
            previous_focus: None,
            dimensions: None,
            zoom: 1.0,
        }
    }
}

pub(super) struct ProjectCreationState {
    pub(super) open: bool,
    pub(super) kind: ProjectCreationKind,
    pub(super) step: ProjectCreationStep,
    pub(super) focused_item: usize,
    pub(super) keyboard_focus: bool,
    pub(super) focus: FocusHandle,
    pub(super) focus_pending: bool,
}

impl ProjectCreationState {
    pub(super) fn new(cx: &mut Context<ChatApp>) -> Self {
        Self {
            open: false,
            kind: ProjectCreationKind::Local,
            step: ProjectCreationStep::Kind,
            focused_item: 0,
            keyboard_focus: false,
            focus: cx.focus_handle().tab_stop(true),
            focus_pending: false,
        }
    }
}
