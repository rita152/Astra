//! Conversations behavior and presentation for the application shell.

use std::path::PathBuf;

use gpui::{Context, Entity, prelude::*};

use super::{ChatApp, ConversationHost, ConversationKey, DraftId, state::RightPanelMode};
use crate::{
    agent::{ProjectId, ThreadId},
    components::composer::ComposerView,
    workspace::project_id_for_thread,
};

impl ChatApp {
    pub(super) fn switch_home_to(&mut self, key: ConversationKey, cx: &mut Context<Self>) {
        let Some(host) = self.conversation_hosts.get(&key) else {
            return;
        };
        let composer = host.composer.clone();
        let cwd = host.cwd.clone();
        let project_id = host.project_id.clone();
        let thread_id = match &key {
            ConversationKey::Draft(_) => None,
            ConversationKey::Thread(thread_id) => Some(thread_id.clone()),
        };
        composer.update(cx, |composer, cx| {
            composer.set_workspace_context(cwd, project_id, thread_id, cx);
        });
        self.deactivate_review(cx);
        self.active_conversation = key;
        if self.right_panel.open && self.right_panel.mode == Some(RightPanelMode::Files) {
            self.ensure_files(cx);
        }
        if self.right_panel.open && self.right_panel.mode == Some(RightPanelMode::Terminal) {
            self.ensure_terminal(cx);
        }
        if self.right_panel.open && self.right_panel.mode == Some(RightPanelMode::Review) {
            self.ensure_review(cx);
        }
        self.home
            .update(cx, |home, cx| home.set_composer(composer, cx));
        cx.notify();
    }
    pub(super) fn start_draft(
        &mut self,
        project_id: Option<ProjectId>,
        cwd: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let draft_id = DraftId(self.next_draft_id);
        self.next_draft_id = self.next_draft_id.wrapping_add(1).max(1);
        let backend = self.agent_backend.clone();
        let composer = cx.new(|cx| ComposerView::new_with_backend(self.mode, backend, cx));
        composer.update(cx, |composer, cx| {
            composer.set_workspace_context(cwd.clone(), project_id.clone(), None, cx);
        });
        let key = ConversationKey::Draft(draft_id);
        self.conversation_hosts.insert(
            key.clone(),
            ConversationHost {
                composer,
                cwd,
                project_id,
            },
        );
        self.switch_home_to(key, cx);
    }
    pub(super) fn ensure_thread_conversation(
        &mut self,
        thread_id: ThreadId,
        cx: &mut Context<Self>,
    ) -> Entity<ComposerView> {
        let key = ConversationKey::Thread(thread_id.clone());
        if let Some(host) = self.conversation_hosts.get(&key) {
            let composer = host.composer.clone();
            let retry_history = composer.read(cx).history_needs_retry();
            if retry_history {
                composer.update(cx, |composer, cx| composer.set_history_loading(true, cx));
                self.load_conversation_history(
                    ConversationKey::Thread(thread_id.clone()),
                    thread_id,
                    composer.clone(),
                    cx,
                );
            }
            return composer;
        }

        let snapshot = self.workspace_store.snapshot();
        let summary = snapshot.thread(&thread_id).cloned().or_else(|| {
            snapshot
                .search_results
                .iter()
                .find(|result| result.thread.thread_id == thread_id)
                .map(|result| result.thread.clone())
        });
        let cwd = summary
            .as_ref()
            .map(|thread| thread.cwd.clone())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        let project_id = summary
            .as_ref()
            .and_then(|thread| project_id_for_thread(thread, &snapshot.projects));
        let backend = self.agent_backend.clone();
        let composer = cx.new(|cx| ComposerView::new_with_backend(self.mode, backend, cx));
        composer.update(cx, |composer, cx| {
            composer.set_workspace_context(
                cwd.clone(),
                project_id.clone(),
                Some(thread_id.clone()),
                cx,
            );
            composer.set_history_loading(true, cx);
        });
        self.conversation_hosts.insert(
            key.clone(),
            ConversationHost {
                composer: composer.clone(),
                cwd,
                project_id,
            },
        );

        self.load_conversation_history(
            ConversationKey::Thread(thread_id.clone()),
            thread_id,
            composer.clone(),
            cx,
        );
        composer
    }
    pub(super) fn select_conversation(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        let key = ConversationKey::Thread(thread_id.clone());
        self.ensure_thread_conversation(thread_id, cx);
        self.switch_home_to(key, cx);
    }
    pub(super) fn load_conversation_history(
        &mut self,
        key: ConversationKey,
        thread_id: ThreadId,
        composer: Entity<ComposerView>,
        cx: &mut Context<Self>,
    ) {
        let receiver = self.workspace_store.load_history(thread_id);
        cx.spawn(async move |this, cx| {
            let result = receiver.recv().await.unwrap_or_else(|_| {
                Err(crate::agent::WorkspaceError::backend(
                    "读取聊天历史的响应通道提前关闭",
                ))
            });
            let _ = this.update(cx, |this, cx| match result {
                Ok(history) => {
                    if let Some(host) = this.conversation_hosts.get_mut(&key)
                        && host.composer == composer
                    {
                        host.cwd = history.thread.cwd.clone();
                        host.project_id = history.thread.project_id.clone();
                    }
                    composer.update(cx, |composer, cx| composer.hydrate_history(history, cx));
                    if this.active_conversation == key
                        && this.right_panel.mode == Some(RightPanelMode::Review)
                    {
                        this.ensure_review(cx);
                    }
                }
                Err(error) => composer.update(cx, |composer, cx| {
                    composer.set_history_error(error.user_message("读取聊天历史"), cx)
                }),
            });
        })
        .detach();
    }
    pub(super) fn rekey_created_thread(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        let draft_key = self.conversation_hosts.iter().find_map(|(key, host)| {
            matches!(key, ConversationKey::Draft(_))
                .then(|| {
                    (host.composer.read(cx).thread_id() == Some(thread_id.as_str()))
                        .then(|| key.clone())
                })
                .flatten()
        });
        let Some(draft_key) = draft_key else {
            return;
        };
        let Some(host) = self.conversation_hosts.remove(&draft_key) else {
            return;
        };
        let real_key = ConversationKey::Thread(thread_id);
        if self.active_conversation == draft_key {
            self.active_conversation = real_key.clone();
        }
        if let Some(panel) = self.file_panels.remove(&draft_key) {
            self.file_panels.insert(real_key.clone(), panel);
        }
        if let Some(panel) = self.terminal_panels.remove(&draft_key) {
            self.terminal_panels.insert(real_key.clone(), panel);
        }
        if let Some(panel) = self.review_panels.remove(&draft_key) {
            self.review_panels.insert(real_key.clone(), panel);
        }
        self.conversation_hosts.insert(real_key, host);
        #[cfg(not(test))]
        self.workspace_store.refresh_all();
        cx.notify();
    }
}
