//! Full-access confirmation owns focus and the original composer until resolution.
use super::ChatApp;
use crate::components::composer::ComposerView;
use gpui::{Context, Entity, KeyDownEvent, Window};

impl ChatApp {
    pub(super) fn open_permission_confirmation(
        &mut self,
        target: Entity<ComposerView>,
        cx: &mut Context<Self>,
    ) {
        self.permission_confirmation_target = Some(target);
        self.permission_confirmation_open = true;
        self.permission_confirmation_focus_pending = true;
        self.permission_confirmation_choice = 0;
        self.permission_confirmation_keyboard = false;
        cx.notify();
    }
    pub(super) fn cancel_permission_confirmation(&mut self, cx: &mut Context<Self>) {
        if self.permission_confirmation_open {
            if let Some(target) = self.permission_confirmation_target.take() {
                target.update(cx, |composer, _| composer.cancel_full_access_confirmation());
            }
            self.permission_confirmation_open = false;
            self.permission_confirmation_focus_pending = true;
            cx.notify();
        }
    }
    pub(super) fn resolve_permission_confirmation(
        &mut self,
        confirm: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.permission_confirmation_open {
            return;
        }
        self.permission_confirmation_open = false;
        self.permission_confirmation_focus_pending = false;
        if let Some(target) = self.permission_confirmation_target.take() {
            target.update(cx, |composer, cx| {
                if confirm {
                    composer.confirm_full_access(cx);
                } else {
                    composer.cancel_full_access_confirmation();
                }
                composer.focus_permission_control(window, cx);
            });
        } else {
            self.root_focus.focus(window, cx);
        }
        cx.notify();
    }
    pub(super) fn permission_confirmation_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "escape" => self.resolve_permission_confirmation(false, window, cx),
            "tab" | "left" | "right" => {
                self.permission_confirmation_choice = 1 - self.permission_confirmation_choice;
                self.permission_confirmation_keyboard = true;
                cx.notify();
            }
            "enter" | "space" => self.resolve_permission_confirmation(
                self.permission_confirmation_choice == 1,
                window,
                cx,
            ),
            "f12" if event.keystroke.modifiers.platform && event.keystroke.modifiers.shift => {
                cx.propagate();
                return;
            }
            _ => {}
        }
        cx.stop_propagation();
    }
}
