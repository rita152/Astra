//! Capture behavior and presentation for the application shell.

use std::{path::PathBuf, time::Duration};

#[cfg(feature = "screenshot")]
use super::ConversationKey;
#[cfg(feature = "screenshot")]
use crate::components::file_panel::FilePanel;
#[cfg(feature = "screenshot")]
use gpui::AppContext;
use gpui::Context;

use super::{
    ChatApp,
    state::{ProjectCreationKind, ProjectCreationStep},
};
use crate::{
    agent::{ProjectId, ThreadId},
    components::file_change::captured_diff_review_fixture,
};

impl ChatApp {
    /// Opens a persisted thread without requiring the sidebar to finish loading first.
    ///
    /// This is used by the deterministic Markdown capture path. It deliberately
    /// follows the same history-loading path as a real sidebar selection.
    pub fn resume_thread_for_capture(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.select_thread_for_capture(thread_id.clone(), cx)
        });
        self.select_conversation(thread_id, cx);
    }
    /// Returns whether the requested thread has finished hydrating, or its load error.
    #[cfg(feature = "screenshot")]
    pub fn resumed_thread_ready(&self, thread_id: &str, cx: &gpui::App) -> Result<bool, String> {
        let sidebar_ready = self
            .sidebar
            .read(cx)
            .resumed_thread_ready_for_capture(thread_id)?;
        if !(self.startup_model_catalog_resolved
            && self.startup_sidebar_resolved
            && self.startup_minimum_duration_elapsed
            && sidebar_ready)
        {
            return Ok(false);
        }
        let key = ConversationKey::Thread(thread_id.to_owned());
        let Some(host) = self.conversation_hosts.get(&key) else {
            return Ok(false);
        };
        let composer = host.composer.read(cx);
        if let Some(error) = composer.history_error() {
            return Err(error.to_owned());
        }
        Ok(!composer.history_loading()
            && composer.thread_id() == Some(thread_id)
            && composer.model_catalog_ready_for_capture()?)
    }
    #[cfg(feature = "screenshot")]
    pub fn resumed_render_audit(&self, thread_id: &str, cx: &gpui::App) -> serde_json::Value {
        let composer = self.conversation_hosts[&ConversationKey::Thread(thread_id.to_owned())]
            .composer
            .read(cx);
        let mut turns = composer
            .transcript_render_snapshot()
            .into_iter()
            .map(|turn| {
                serde_json::json!({
                    "id":turn.resumed.map(|r|r.id), "user_message":turn.user_message,
                    "units":crate::components::home::resumed_activity_audit(&turn.activities)
                })
            })
            .collect::<Vec<_>>();
        let (_, user, _, _, _, activities) = composer.conversation_render_snapshot();
        turns.push(
            serde_json::json!({"id":composer.resumed_turn().map(|r|r.id), "user_message":user,
            "units":crate::components::home::resumed_activity_audit(&activities)}),
        );
        serde_json::json!({"thread_id":thread_id,"turns":turns})
    }
    #[cfg(feature = "screenshot")]
    pub fn set_conversation_scroll_from_bottom_for_capture(
        &mut self,
        distance: f32,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.set_conversation_scroll_from_bottom_for_capture(distance, cx)
        });
    }
    pub fn complete_startup_for_capture(&mut self, cx: &mut Context<Self>) {
        self.startup_model_catalog_resolved = true;
        self.startup_sidebar_resolved = true;
        self.startup_minimum_duration_elapsed = true;
        cx.notify();
    }
    pub fn open_profile_menu(&mut self, cx: &mut Context<Self>) {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_profile_menu_open(true, cx));
    }
    pub fn open_activity(&mut self, cx: &mut Context<Self>) {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_activity_open(true, cx));
    }
    pub fn open_projects_section_menu(&mut self, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.open_projects_section_menu_for_capture(cx)
        });
    }
    pub fn open_project_menu_for_capture(&mut self, project_id: ProjectId, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.open_project_menu_for_capture(project_id, cx)
        });
    }
    pub fn set_activity_scroll_for_capture(&mut self, offset: f32, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_activity_scroll_for_capture(offset, cx)
        });
    }
    pub fn set_activity_hovered_thread_for_capture(
        &mut self,
        thread_id: ThreadId,
        cx: &mut Context<Self>,
    ) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_activity_hovered_thread_for_capture(thread_id, cx)
        });
    }
    pub fn open_model_picker(&mut self, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| home.open_model_picker(cx));
    }
    pub fn open_model_picker_submenu(&mut self, name: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.open_model_picker_submenu(name, cx));
    }
    pub fn open_model_picker_slider_at(
        &mut self,
        index: usize,
        fast: bool,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.open_model_picker_slider_at(index, fast, cx)
        });
    }
    pub fn set_dictation_state_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.set_dictation_state_for_capture(state, cx)
        });
    }
    pub fn submit_prompt_for_capture(&mut self, prompt: &str, cx: &mut Context<Self>) {
        if self.startup_model_catalog_resolved {
            self.home
                .update(cx, |home, cx| home.submit_prompt_for_capture(prompt, cx));
            return;
        }

        let prompt = prompt.to_owned();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(25))
                    .await;
                match this.update(cx, |this, cx| {
                    if !this.startup_model_catalog_resolved {
                        return false;
                    }
                    this.home
                        .update(cx, |home, cx| home.submit_prompt_for_capture(&prompt, cx));
                    true
                }) {
                    Ok(true) | Err(_) => return,
                    Ok(false) => {}
                }
            }
        })
        .detach();
    }
    pub fn show_user_message_actions_for_capture(&mut self, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.show_user_message_actions_for_capture(cx)
        });
    }
    pub fn set_command_tool_for_capture(
        &mut self,
        running: bool,
        expanded: bool,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.set_command_tool_for_capture(running, expanded, cx)
        });
    }
    pub fn set_context_compaction_for_capture(&mut self, running: bool, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.set_context_compaction_for_capture(running, cx)
        });
    }
    pub fn set_collaboration_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.set_collaboration_for_capture(state, cx));
    }
    pub fn set_mcp_tool_call_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.set_mcp_tool_call_for_capture(state, cx));
    }
    pub fn set_tool_group_for_capture(
        &mut self,
        running: bool,
        expanded: bool,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.set_tool_group_for_capture(running, expanded, cx)
        });
    }
    pub fn set_reasoning_for_capture(
        &mut self,
        state: &str,
        expanded: bool,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.set_reasoning_for_capture(state, expanded, cx)
        });
    }
    pub fn set_approval_for_capture(&mut self, kind: &str, state: &str, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.set_approval_for_capture(kind, state, cx)
        });
    }
    pub fn set_user_input_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.set_user_input_for_capture(state, cx));
    }
    pub fn set_file_approval_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.set_file_approval_for_capture(state, cx));
    }
    pub fn set_permissions_approval_for_capture(
        &mut self,
        kind: &str,
        state: &str,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.set_permissions_approval_for_capture(kind, state, cx)
        });
    }
    pub fn set_file_change_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.set_file_change_for_capture(state, cx));
    }
    pub fn set_turn_diff_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.set_file_change_for_capture("completed", cx)
        });
        self.open_diff_review(captured_diff_review_fixture(state), cx);
    }
    pub fn set_image_generation_for_capture(
        &mut self,
        state: &str,
        path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.home.update(cx, |home, cx| {
            home.set_image_generation_for_capture(state, path, cx)
        });
        cx.notify();
    }
    pub fn enable_permission_ui_for_capture(&mut self, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.enable_permission_ui_for_capture(cx));
    }
    pub fn set_permission_mode_for_capture(&mut self, mode: &str, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.set_permission_mode_for_capture(mode, cx)
        });
    }
    pub fn open_permission_menu_for_capture(&mut self, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.open_permission_menu_for_capture(cx));
    }
    pub fn set_permission_menu_capture_state(&mut self, state: &str, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| {
            home.set_permission_menu_capture_state(state, cx)
        });
    }
    pub fn open_permission_confirmation_for_capture(&mut self, cx: &mut Context<Self>) {
        self.enable_permission_ui_for_capture(cx);
        self.permission_confirmation_open = true;
        cx.notify();
    }
    pub fn open_project_creation_remote_for_capture(&mut self, cx: &mut Context<Self>) {
        self.open_project_creation(cx);
        self.project_creation.kind = ProjectCreationKind::Remote;
        self.project_creation.step = ProjectCreationStep::Remote;
        cx.notify();
    }
    pub fn append_bottom_panel_item_for_capture(&mut self, name: &str, cx: &mut Context<Self>) {
        self.open_bottom_panel(cx);
        let index = match name {
            "review" => 0,
            "terminal" => 1,
            "browser" => 2,
            "files" => 3,
            "side-chat" => 4,
            _ => return,
        };
        self.select_bottom_panel_item(index, cx);
    }
    #[cfg(feature = "screenshot")]
    pub fn capture_files(&mut self, root: PathBuf, path: Option<PathBuf>, cx: &mut Context<Self>) {
        let panel = cx.new(|cx| FilePanel::new(root, self.mode, cx));
        if let Some(path) = path {
            panel.update(cx, |p, cx| p.open_path(path, None, cx));
        }
        self.file_panels
            .insert(self.active_conversation.clone(), panel);
        self.right_panel.open = true;
        self.select_right_panel_item(3, cx);
    }
}
