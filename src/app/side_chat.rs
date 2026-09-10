//! Attach temporary chat panels to the parent conversation's UI lifetime.

use super::ChatApp;
use crate::{
    components::side_chat::{SideChatDestination, SideChatEvent, SideChatPanel},
    media::read_image_dimensions,
};
use gpui::{Context, prelude::*};

impl ChatApp {
    pub(super) fn deactivate_side_chat(&mut self, cx: &mut Context<Self>) {
        if let Some(panel) = self.side_chat_panels.get(&self.active_conversation) {
            panel.update(cx, |panel, cx| panel.set_visible(false, cx));
        }
    }

    pub(super) fn ensure_side_chat(&mut self, new: bool, cx: &mut Context<Self>) {
        let key = self.active_conversation.clone();
        let Some(parent) = self
            .conversation_hosts
            .get(&key)
            .map(|host| host.composer.clone())
        else {
            return;
        };
        if parent.read(cx).side_chat_configuration().is_none() {
            self.right_panel.mode = None;
            cx.notify();
            return;
        }
        self.deactivate_review(cx);
        if !self.side_chat_panels.contains_key(&key) {
            let backend = self.agent_backend.clone();
            let skip = self
                .workspace_store
                .snapshot()
                .preferences
                .skip_side_chat_close_confirmation;
            let panel = cx.new(|cx| SideChatPanel::new(parent, backend, self.mode, skip, cx));
            cx.observe(&panel, |_, _, cx| cx.notify()).detach();
            cx.subscribe(&panel, |this, _, event: &SideChatEvent, cx| {
                match event {
                    SideChatEvent::Empty => {
                        this.right_panel.mode = None;
                        this.right_panel.fullscreen = false;
                        this.right_panel.focus_pending = true;
                    }
                    SideChatEvent::Fullscreen => {
                        this.right_panel.fullscreen = !this.right_panel.fullscreen;
                        if let Some(panel) = this.side_chat_panels.get(&this.active_conversation) {
                            panel.update(cx, |panel, cx| {
                                panel.set_fullscreen(this.right_panel.fullscreen, cx)
                            });
                        }
                    }
                    SideChatEvent::OpenPanel(destination) => {
                        let index = match destination {
                            SideChatDestination::Review => 4,
                            SideChatDestination::Terminal => 2,
                            SideChatDestination::Browser => 1,
                            SideChatDestination::Files => 3,
                        };
                        this.select_right_panel_item(index, cx);
                    }
                    SideChatEvent::OpenDiff(review) => {
                        this.deactivate_side_chat(cx);
                        this.open_diff_review(review.clone(), cx);
                    }
                    SideChatEvent::OpenImage(path) => {
                        this.image_preview.path = Some(path.clone());
                        this.image_preview.dimensions = read_image_dimensions(path).ok().flatten();
                        this.image_preview.zoom = 1.0;
                    }
                    SideChatEvent::OpenHookSettings => {
                        this.settings
                            .update(cx, |settings, cx| settings.select("hooks-settings", cx));
                        this.showing_settings = true;
                    }
                    SideChatEvent::FullAccess(composer) => {
                        this.open_permission_confirmation(composer.clone(), cx);
                    }
                    SideChatEvent::SkipCloseConfirmation(skip) => {
                        this.workspace_store
                            .set_skip_side_chat_close_confirmation(*skip);
                        for panel in this.side_chat_panels.values() {
                            panel.update(cx, |panel, _| panel.set_skip_confirmation(*skip));
                        }
                    }
                }
                cx.notify();
            })
            .detach();
            self.side_chat_panels.insert(key.clone(), panel);
        }
        self.side_chat_panels[&key].update(cx, |panel, cx| {
            panel.set_fullscreen(self.right_panel.fullscreen, cx);
            if new || panel.is_empty() {
                panel.new_chat(cx);
            }
            panel.set_visible(true, cx);
            panel.focus(cx);
        });
        self.right_panel.focus_pending = false;
        self.terminal_return_focus_pending = false;
        cx.notify();
    }
}
