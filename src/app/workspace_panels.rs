//! Workspace panels behavior and presentation for the application shell.

use std::time::Duration;

use gpui::{Context, Window, prelude::*};

use super::ChatApp;
use crate::components::{file_panel::FilePanel, terminal::TerminalPanel};

impl ChatApp {
    pub(super) fn ensure_terminal(&mut self, cx: &mut Context<Self>) {
        self.deactivate_review(cx);
        self.terminal_return_focus_pending = false;
        let key = self.active_conversation.clone();
        if !self.terminal_panels.contains_key(&key) {
            let cwd = self
                .conversation_hosts
                .get(&key)
                .map(|host| host.cwd.clone())
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            let panel = cx.new(|cx| TerminalPanel::new(cwd, self.mode, cx));
            self.terminal_panels.insert(key.clone(), panel);
        }
        self.terminal_panels[&key].update(cx, |panel, cx| panel.focus(cx));
        self.right_panel.focus_pending = false;
    }
    pub fn request_window_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self
            .file_panels
            .values()
            .any(|p| p.read(cx).has_unsaved(cx))
        {
            return true;
        }
        if self.file_close_prompt_open {
            return false;
        }
        self.file_close_prompt_open = true;
        for panel in self.file_panels.values() {
            panel.update(cx, |p, cx| p.save_all(cx));
        }
        let mut window_cx = window.to_async(cx);
        cx.spawn(async move |this, cx| {
            // Finish queued automatic saves before deciding whether closing needs a prompt.
            for _ in 0..40 {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                let pending = this
                    .read_with(cx, |s, cx| {
                        s.file_panels.values().any(|p| p.read(cx).has_unsaved(cx))
                    })
                    .unwrap_or(false);
                if !pending {
                    let _ = window_cx.update(|w, _| w.remove_window());
                    return;
                }
            }
            let answer = window_cx.update(|w, cx| {
                w.prompt(
                    gpui::PromptLevel::Warning,
                    "文件仍有未保存的编辑",
                    Some("自动保存未完成或遇到冲突。返回编辑以保留当前内容。"),
                    &["返回编辑", "放弃未保存的编辑并关闭"],
                    cx,
                )
            });
            if let Ok(answer) = answer
                && answer.await.ok() == Some(1)
            {
                let _ = window_cx.update(|w, _| w.remove_window());
            } else {
                let _ = this.update(cx, |s, _| s.file_close_prompt_open = false);
            }
        })
        .detach();
        false
    }
    pub(super) fn ensure_files(&mut self, cx: &mut Context<Self>) {
        self.deactivate_review(cx);
        let key = self.active_conversation.clone();
        if !self.file_panels.contains_key(&key) {
            let cwd = self
                .conversation_hosts
                .get(&key)
                .map(|h| h.cwd.clone())
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            let panel = cx.new(|cx| FilePanel::new(cwd, self.mode, cx));
            self.file_panels.insert(key.clone(), panel);
        }
        self.file_panels[&key].update(cx, |p, cx| p.focus(cx));
        self.file_panels[&key].update(cx, |p, cx| {
            p.set_review_available(self.review_panels.contains_key(&key), cx)
        });
        self.right_panel.focus_pending = false;
    }
    pub(super) fn open_files(&mut self, cx: &mut Context<Self>) {
        self.right_panel.open = true;
        self.select_right_panel_item(3, cx);
        self.file_panels[&self.active_conversation].update(cx, |p, cx| p.show_picker(cx));
    }
}
