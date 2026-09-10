//! Per-parent temporary chats. Hiding the panel keeps its tabs and live turns.

mod render;
#[cfg(test)]
mod tests;
gpui::actions!(
    side_chat,
    [
        RestoreSideChat,
        CloseDialogNext,
        CloseDialogPrevious,
        CloseDialogActivate,
        CloseDialogCancel
    ]
);
pub(crate) use render::restore_tab;

pub fn init(cx: &mut gpui::App) {
    cx.bind_keys([
        gpui::KeyBinding::new("tab", CloseDialogNext, Some("SideChatCloseDialog")),
        gpui::KeyBinding::new(
            "shift-tab",
            CloseDialogPrevious,
            Some("SideChatCloseDialog"),
        ),
        gpui::KeyBinding::new("space", CloseDialogActivate, Some("SideChatCloseDialog")),
        gpui::KeyBinding::new("enter", CloseDialogActivate, Some("SideChatCloseDialog")),
        gpui::KeyBinding::new("escape", CloseDialogCancel, Some("SideChatCloseDialog")),
    ]);
}

use crate::{
    agent::{AgentBackend, AgentConnectionEvent, ThreadId},
    components::{
        composer::{ComposerView, ConversationChanged, RequestFullAccessConfirmation},
        file_change::DiffReviewPresentation,
        home::{HomeView, OpenDiffReview, OpenImagePreview, RetryImageGeneration},
    },
    theme::ThemeMode,
};
use gpui::{AppContext, Context, Entity, FocusHandle, KeyDownEvent, Window};
use std::{collections::HashSet, path::PathBuf, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideChatDestination {
    Review,
    Terminal,
    Browser,
    Files,
}

pub enum SideChatEvent {
    Empty,
    Fullscreen,
    OpenPanel(SideChatDestination),
    OpenDiff(DiffReviewPresentation),
    OpenImage(PathBuf),
    OpenHookSettings,
    FullAccess(Entity<ComposerView>),
    SkipCloseConfirmation(bool),
}

#[derive(Clone)]
struct SideChatTabDrag {
    owner: gpui::EntityId,
    id: u64,
    title: String,
    mode: ThemeMode,
}

struct SideChatTab {
    id: u64,
    title: String,
    home: Entity<HomeView>,
    composer: Entity<ComposerView>,
    thread_id: Option<ThreadId>,
    loading: bool,
    error: Option<String>,
    cycle: u64,
    running: bool,
    unread: bool,
}

pub struct SideChatPanel {
    backend: Arc<dyn AgentBackend>,
    parent: Entity<ComposerView>,
    mode: ThemeMode,
    tabs: Vec<SideChatTab>,
    active: Option<u64>,
    next_id: u64,
    visible: bool,
    fullscreen: bool,
    focus_pending: bool,
    focus: FocusHandle,
    menu_open: bool,
    menu_index: usize,
    menu_focus: FocusHandle,
    close_confirmation: Option<u64>,
    confirm_focus: FocusHandle,
    remember_focus: FocusHandle,
    cancel_focus: FocusHandle,
    confirm_focus_pending: bool,
    skip_confirmation: bool,
    remember_close: bool,
    tab_scroll: gpui::ScrollHandle,
    closed_threads: HashSet<ThreadId>,
}

impl gpui::EventEmitter<SideChatEvent> for SideChatPanel {}

impl Drop for SideChatPanel {
    fn drop(&mut self) {
        for tab in &self.tabs {
            if let Some(id) = &tab.thread_id {
                self.backend.close_side_conversation(id.clone());
            }
        }
    }
}

impl SideChatPanel {
    pub fn new(
        parent: Entity<ComposerView>,
        backend: Arc<dyn AgentBackend>,
        mode: ThemeMode,
        skip_confirmation: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let receiver = backend.subscribe_connection_events();
        cx.spawn(async move |this, cx| {
            while let Ok(event) = receiver.recv().await {
                if let AgentConnectionEvent::ThreadClosed { thread_id } = event {
                    let _ = this.update(cx, |this, cx| {
                        this.closed_threads.insert(thread_id.clone());
                        if let Some(tab) = this
                            .tabs
                            .iter_mut()
                            .find(|tab| tab.thread_id.as_ref() == Some(&thread_id))
                        {
                            tab.error = Some(
                                "侧边聊天的连接已结束。你仍可查看和复制消息，或新建侧边聊天继续。"
                                    .into(),
                            );
                            tab.composer.update(cx, |composer, cx| {
                                composer.side_conversation_disconnected(cx)
                            });
                            cx.notify();
                        }
                    });
                }
            }
        })
        .detach();
        Self {
            backend,
            parent,
            mode,
            tabs: Vec::new(),
            active: None,
            next_id: 1,
            visible: true,
            fullscreen: false,
            focus_pending: true,
            focus: cx.focus_handle(),
            menu_open: false,
            menu_index: 4,
            menu_focus: cx.focus_handle(),
            close_confirmation: None,
            confirm_focus: cx.focus_handle(),
            remember_focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            confirm_focus_pending: false,
            skip_confirmation,
            remember_close: false,
            tab_scroll: gpui::ScrollHandle::new(),
            closed_threads: HashSet::new(),
        }
    }

    pub fn new_chat(&mut self, cx: &mut Context<Self>) {
        let Some(config) = self.parent.read(cx).side_chat_configuration() else {
            return;
        };
        let id = self.next_id;
        self.next_id += 1;
        let title = if self.tabs.is_empty() {
            "侧边聊天".into()
        } else {
            format!("侧边聊天 {}", self.tabs.len() + 1)
        };
        let backend = self.backend.clone();
        let composer =
            cx.new(|cx| ComposerView::new_side_chat(self.mode, backend, config.clone(), cx));
        let home = cx.new(|cx| HomeView::new_side_chat(self.mode, composer.clone(), cx));
        cx.subscribe(
            &composer,
            move |this, composer, _: &ConversationChanged, cx| {
                if let Some(tab) = this.tabs.iter_mut().find(|tab| tab.id == id) {
                    let composer = composer.read(cx);
                    if let Some(title) = composer.side_chat_title() {
                        tab.title = title;
                    }
                    let running = composer.is_running();
                    if tab.running && !running && (!this.visible || this.active != Some(id)) {
                        tab.unread = true;
                    }
                    tab.running = running;
                    cx.notify();
                }
            },
        )
        .detach();
        cx.subscribe(&home, |_, _, event: &OpenDiffReview, cx| {
            cx.emit(SideChatEvent::OpenDiff(event.0.clone()))
        })
        .detach();
        cx.subscribe(&home, |_, _, event: &OpenImagePreview, cx| {
            cx.emit(SideChatEvent::OpenImage(event.0.clone()))
        })
        .detach();
        cx.subscribe(
            &home,
            |_, _, _: &crate::components::home::OpenHookSettings, cx| {
                cx.emit(SideChatEvent::OpenHookSettings);
            },
        )
        .detach();
        let permission_composer = composer.clone();
        cx.subscribe(&home, move |_, _, _: &RequestFullAccessConfirmation, cx| {
            cx.emit(SideChatEvent::FullAccess(permission_composer.clone()));
        })
        .detach();
        let retry_composer = composer.clone();
        cx.subscribe(&home, move |_, _, _: &RetryImageGeneration, cx| {
            retry_composer.update(cx, |c, cx| c.retry_image_generation(cx))
        })
        .detach();
        self.tabs.push(SideChatTab {
            id,
            title,
            home,
            composer,
            thread_id: None,
            loading: true,
            error: None,
            cycle: 0,
            running: false,
            unread: false,
        });
        self.active = Some(id);
        self.tab_scroll
            .scroll_to_item(self.tabs.len().saturating_sub(1));
        self.focus_pending = true;
        self.menu_open = false;
        self.open_tab(id, cx);
        cx.notify();
    }

    fn open_tab(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(config) = self.parent.read(cx).side_chat_configuration() else {
            return;
        };
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) else {
            return;
        };
        tab.cycle += 1;
        tab.loading = true;
        tab.error = None;
        let cycle = tab.cycle;
        let receiver = self.backend.open_side_conversation(config.request);
        let backend = self.backend.clone();
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await.unwrap_or_else(|_| {
                Err(crate::agent::WorkspaceError::backend(
                    "创建侧边聊天的响应通道已关闭",
                ))
            });
            let created = result.as_ref().ok().cloned();
            let cleanup = this
                .update(cx, |this, cx| {
                    let Some(tab) = this
                        .tabs
                        .iter_mut()
                        .find(|tab| tab.id == id && tab.cycle == cycle)
                    else {
                        return true;
                    };
                    tab.loading = false;
                    match result {
                        Ok(thread_id) => {
                            tab.thread_id = Some(thread_id.clone());
                            let closed = this.closed_threads.contains(&thread_id);
                            tab.composer.update(cx, |composer, cx| {
                                composer.set_side_thread(thread_id, cx);
                                if closed {
                                    composer.side_conversation_disconnected(cx);
                                }
                            });
                            if closed {
                                tab.error =
                                    Some("侧边聊天的连接已结束，请新建侧边聊天继续。".into());
                            }
                        }
                        Err(error) => tab.error = Some(error.user_message("打开侧边聊天")),
                    }
                    cx.notify();
                    false
                })
                .unwrap_or(true);
            if cleanup && let Some(thread_id) = created {
                backend.close_side_conversation(thread_id);
            }
        })
        .detach();
    }

    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        for tab in &self.tabs {
            tab.home.update(cx, |home, cx| home.set_mode(mode, cx));
        }
        cx.notify();
    }
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.visible = visible;
        if !visible {
            self.menu_open = false;
            self.close_confirmation = None;
            self.focus_pending = false;
            for tab in &self.tabs {
                tab.composer.update(cx, |c, cx| c.close_side_menus(cx));
            }
        } else if let Some(tab) = self.tabs.iter_mut().find(|tab| Some(tab.id) == self.active) {
            tab.unread = false;
        }
        cx.notify();
    }
    pub fn focus(&mut self, cx: &mut Context<Self>) {
        self.visible = true;
        self.focus_pending = true;
        cx.notify();
    }
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }
    pub fn set_fullscreen(&mut self, fullscreen: bool, cx: &mut Context<Self>) {
        if self.fullscreen != fullscreen {
            self.fullscreen = fullscreen;
            cx.notify();
        }
    }
    pub fn set_skip_confirmation(&mut self, skip: bool) {
        self.skip_confirmation = skip;
    }

    fn activate(&mut self, id: u64, cx: &mut Context<Self>) {
        for tab in &self.tabs {
            tab.composer.update(cx, |c, cx| c.close_side_menus(cx));
        }
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.unread = false;
            self.active = Some(id);
            if let Some(index) = self.tabs.iter().position(|tab| tab.id == id) {
                self.tab_scroll.scroll_to_item(index);
            }
            self.focus_pending = true;
            self.menu_open = false;
            cx.notify();
        }
    }
    fn reorder_tab(&mut self, id: u64, before: u64, cx: &mut Context<Self>) {
        let Some(from) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        let Some(to) = self.tabs.iter().position(|tab| tab.id == before) else {
            return;
        };
        if from != to {
            let tab = self.tabs.remove(from);
            self.tabs.insert(to, tab);
        }
        self.activate(id, cx);
    }
    fn request_close(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == id) else {
            return;
        };
        if tab.composer.read(cx).has_messages() && !self.skip_confirmation {
            self.close_confirmation = Some(id);
            self.remember_close = false;
            self.confirm_focus_pending = true;
        } else {
            self.close_tab(id, cx);
        }
        cx.notify();
    }
    fn close_tab(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        let tab = self.tabs.remove(index);
        tab.composer
            .update(cx, |composer, cx| composer.close_side_chat(cx));
        if let Some(thread_id) = tab.thread_id {
            self.backend.close_side_conversation(thread_id);
        }
        self.close_confirmation = None;
        if self.active == Some(id) {
            self.active = self
                .tabs
                .get(index.min(self.tabs.len().saturating_sub(1)))
                .map(|tab| tab.id);
            self.focus_pending = true;
            if let Some(index) = self.tabs.iter().position(|tab| Some(tab.id) == self.active) {
                self.tab_scroll.scroll_to_item(index);
            }
        }
        if self.tabs.is_empty() {
            cx.emit(SideChatEvent::Empty);
        }
        cx.notify();
    }
    fn confirm_close(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.close_confirmation {
            if self.remember_close {
                self.skip_confirmation = true;
                cx.emit(SideChatEvent::SkipCloseConfirmation(true));
            }
            self.close_tab(id, cx);
        }
    }
    fn move_dialog_focus(&self, backwards: bool, window: &mut Window, cx: &mut Context<Self>) {
        let handles = [
            &self.remember_focus,
            &self.cancel_focus,
            &self.confirm_focus,
        ];
        let index = handles
            .iter()
            .position(|handle| handle.is_focused(window))
            .unwrap_or(2);
        handles[(index + if backwards { 2 } else { 1 }) % 3].focus(window, cx);
    }
    fn activate_dialog_control(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.remember_focus.is_focused(window) {
            self.remember_close = !self.remember_close;
            cx.notify();
        } else if self.cancel_focus.is_focused(window) {
            self.dismiss_transient(cx);
        } else {
            self.confirm_close(cx);
        }
    }
    pub fn dismiss_transient(&mut self, cx: &mut Context<Self>) -> bool {
        if self.close_confirmation.take().is_some() {
            self.focus_pending = true;
            cx.notify();
            return true;
        }
        if self.menu_open {
            self.menu_open = false;
            self.focus_pending = true;
            cx.notify();
            return true;
        }
        for tab in &self.tabs {
            tab.composer.update(cx, |c, cx| c.close_side_menus(cx));
        }
        if let Some(tab) = self.tabs.iter().find(|tab| Some(tab.id) == self.active)
            && tab
                .home
                .update(cx, |home, cx| home.dismiss_hook_tooltips(cx))
        {
            return true;
        }
        false
    }
    fn select_menu(&mut self, index: usize, cx: &mut Context<Self>) {
        self.menu_open = false;
        match index {
            0 => cx.emit(SideChatEvent::OpenPanel(SideChatDestination::Review)),
            1 => cx.emit(SideChatEvent::OpenPanel(SideChatDestination::Terminal)),
            2 => cx.emit(SideChatEvent::OpenPanel(SideChatDestination::Browser)),
            3 => cx.emit(SideChatEvent::OpenPanel(SideChatDestination::Files)),
            _ => self.new_chat(cx),
        }
        cx.notify();
    }
    fn handle_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        if key.modifiers.platform && key.key == "w" {
            if let Some(id) = self.active {
                self.request_close(id, cx);
            }
            cx.stop_propagation();
        } else if key.modifiers.control && key.key == "tab" {
            if let Some(index) = self.tabs.iter().position(|tab| Some(tab.id) == self.active) {
                let offset = if key.modifiers.shift {
                    self.tabs.len() - 1
                } else {
                    1
                };
                self.activate(self.tabs[(index + offset) % self.tabs.len()].id, cx);
            }
            cx.stop_propagation();
        } else if key.key == "escape" && self.dismiss_transient(cx) {
            cx.stop_propagation();
        }
    }
}
