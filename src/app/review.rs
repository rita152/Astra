//! Review behavior and presentation for the application shell.

use gpui::Context;

use super::{ChatApp, image_preview::finder_reveal_command};
use crate::components::file_change::{
    DiffFileVisualState, DiffReviewEvent, DiffReviewPresentation,
};

impl ChatApp {
    pub(super) fn open_diff_review(
        &mut self,
        review: DiffReviewPresentation,
        cx: &mut Context<Self>,
    ) {
        self.right_panel.diff_review = Some(review);
        self.right_panel.subagent = None;
        self.right_panel.subagent_menu_open = false;
        self.right_panel.open = true;
        self.right_panel.mode = None;
        // Natural long-diff capture 29: the 2560px viewport split begins at
        // x=1202.359375, leaving a 1357.640625px Review panel. Its 250px file
        // tree leaves the measured 1107.640625px scroll viewport.
        self.right_panel.width = Some(1_357.640_6);
        self.right_panel.keyboard_focus = false;
        self.right_panel.focus_pending = false;
        cx.notify();
    }
    pub(super) fn handle_diff_review_event(
        &mut self,
        event: DiffReviewEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            DiffReviewEvent::Close => {
                self.right_panel.diff_review = None;
                self.right_panel.open = false;
            }
            DiffReviewEvent::ToggleFile(index) => {
                let Some(file) = self
                    .right_panel
                    .diff_review
                    .as_mut()
                    .and_then(|review| review.files.get_mut(index))
                else {
                    return;
                };
                file.visual_state = if file.visual_state.is_expanded() {
                    DiffFileVisualState::Collapsed
                } else {
                    DiffFileVisualState::Expanded
                };
            }
            DiffReviewEvent::HeaderHoverChanged { index, hovered } => {
                let Some(file) = self
                    .right_panel
                    .diff_review
                    .as_mut()
                    .and_then(|review| review.files.get_mut(index))
                else {
                    return;
                };
                if file.visual_state.is_expanded() {
                    file.visual_state = if hovered {
                        DiffFileVisualState::HeaderHovered
                    } else {
                        DiffFileVisualState::Expanded
                    };
                }
            }
            DiffReviewEvent::CopyPath(path) => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(path));
            }
            DiffReviewEvent::OpenLocation(path) => {
                // Keep this as a native OS action. `Command::arg` passes the
                // path as data (rather than through a shell), so spaces and
                // other filename characters cannot be interpreted as code.
                let _ = finder_reveal_command(&path).spawn();
            }
        }
        cx.notify();
    }
}
