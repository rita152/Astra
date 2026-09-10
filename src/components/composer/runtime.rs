//! Runtime behavior and presentation for the prompt composer.

#[cfg(not(test))]
use super::ModelCatalogLoadFinished;

use gpui::Context;

use super::{ComposerView, ConversationChanged, ConversationThreadCreated};
use crate::{
    agent::{AgentConnectionEvent, AgentEvent, AgentRequest},
    conversation::{
        STREAM_DISCONNECTED_MESSAGE, STREAM_UPDATE_INTERVAL, collect_ready_agent_events,
        ensure_closed_batch_is_terminal,
    },
};

impl ComposerView {
    pub(super) fn request_effort(&self) -> String {
        if self.prompt_context.plan_mode == Some(true) && !self.conversation.model_user_selected {
            self.conversation
                .plan_default_effort
                .clone()
                .unwrap_or_else(|| self.conversation.selected_effort.clone())
        } else {
            self.conversation.selected_effort.clone()
        }
    }

    #[cfg(not(test))]
    pub(super) fn load_model_catalog(&mut self, cx: &mut Context<Self>) {
        let receiver = self.backend.load_model_catalog();
        cx.spawn(async move |this, cx| {
            let result = receiver
                .recv()
                .await
                .unwrap_or_else(|_| Err("Codex 模型目录连接在返回结果前关闭".to_owned()));
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(catalog) => this.apply_model_catalog(catalog),
                    Err(error) => this.conversation.set_model_catalog_error(error),
                }
                cx.emit(ModelCatalogLoadFinished);
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn submit_prompt(&mut self, raw_prompt: String, cx: &mut Context<Self>) {
        if raw_prompt.trim().is_empty()
            && self.review_comments.is_empty()
            && self.prompt_context.files.is_empty()
        {
            return;
        }
        self.focus_prompt_pending = true;
        if !self.side_ready {
            self.submission_error =
                Some("聊天连接不可用，输入已保留。请重新连接或新建侧边聊天。".into());
            cx.notify();
            return;
        }
        if self.conversation.thread_id.is_none()
            && (self.permission_catalog_loading || self.permission_catalog_error.is_some())
        {
            self.submission_error = Some(
                self.permission_catalog_error
                    .clone()
                    .unwrap_or_else(|| "正在读取新会话配置，输入已保留。".into()),
            );
            cx.notify();
            return;
        }
        let running = self.is_running();
        if !running && self.conversation.permission_change.is_some() {
            self.submission_error =
                Some("权限变更尚未确认，输入已保留。请等待确认后发送新轮次。".into());
            cx.notify();
            return;
        }
        let target = if running {
            match self.conversation.steer_target() {
                Ok(target) => Some(target),
                Err(error) => {
                    self.submission_error = Some(error);
                    cx.notify();
                    return;
                }
            }
        } else {
            None
        };
        if !running
            && (self.conversation.selected_model.is_empty()
                || self.conversation.selected_effort.is_empty())
        {
            self.submission_error = Some("没有可用模型，请选择模型后重试。输入已保留。".into());
            cx.notify();
            return;
        }
        let draft = crate::conversation::SubmissionDraft {
            text: raw_prompt.clone(),
            context: self.prompt_context.clone(),
            comments: self.review_comments.clone(),
        };
        let prompt = if raw_prompt.trim().is_empty() && !self.prompt_context.files.is_empty() {
            "请查看附加文件。".to_owned()
        } else {
            raw_prompt
        };
        let prompt = if self.review_comments.is_empty() {
            prompt
        } else {
            format!(
                "{}\n\n请处理以下审查评论：\n\n{}",
                prompt.trim(),
                crate::git_review::comments_prompt(&self.review_comments)
            )
        };
        if !running {
            self.conversation.begin_prompt(&prompt);
            self.conversation.user_images = draft
                .context
                .files
                .iter()
                .map(|file| {
                    if file.image {
                        crate::agent::UserMessageAttachment::Local(file.path.clone())
                    } else {
                        crate::agent::UserMessageAttachment::File(file.path.clone())
                    }
                })
                .collect();
        }
        let id = self.conversation.record_submission(
            draft.clone(),
            crate::agent::normalize_user_message_for_display(&prompt),
            !running,
        );
        self.submission_error = None;
        self.draft_revision = self.draft_revision.wrapping_add(1);
        self.menu_open = false;
        self.permission_menu_open = false;
        self.permission_menu_keyboard_focus = false;
        self.submenu = None;
        self.clear_prompt(cx);
        self.prompt_context.files.clear();
        self.context_menu_open = false;
        if !self.review_comments.is_empty() {
            self.review_comments.clear();
            cx.emit(super::ReviewCommentsSubmitted);
        }
        let draft_revision = self.draft_revision;
        if let Some(target) = target {
            let receiver = self.backend.steer_turn(crate::agent::AgentSteerRequest {
                target,
                client_message_id: id.clone(),
                prompt,
                context: draft.context.clone(),
            });
            cx.spawn(async move |this, cx| {
                let result = receiver.recv().await.unwrap_or_else(|_| {
                    Err("追加输入响应连接已关闭，接受状态未知。输入快照已保留。".into())
                });
                let _ = this.update(cx, |this, cx| {
                    this.conversation.resolve_submission(&id, result);
                    let failure = this
                        .conversation
                        .submissions
                        .iter()
                        .find(|s| s.id == id)
                        .and_then(|s| {
                            if let crate::conversation::SubmissionStatus::Failed(error) = &s.status
                            {
                                Some((error.clone(), s.cycle))
                            } else {
                                None
                            }
                        });
                    if let Some((error, submission_cycle)) = failure
                        && submission_cycle == this.conversation.cycle
                    {
                        this.submission_error = Some(error);
                        if this.draft_revision == draft_revision && this.draft_is_empty(cx) {
                            this.restore_submission(&id, cx);
                        }
                    }
                    cx.emit(ConversationChanged);
                    cx.notify();
                });
            })
            .detach();
        } else {
            let cycle = self.conversation.cycle;
            let model = self.conversation.selected_model.clone();
            self.conversation.actual_model = Some(model.clone());
            self.conversation.model_status = None;
            self.conversation.safety_buffering = false;
            let run = self.backend.run_prompt(AgentRequest {
                client_message_id: Some(id),
                prompt,
                cwd: self.conversation.cwd.clone(),
                project_id: self.conversation.project_id.clone(),
                thread_id: self.conversation.thread_id.clone(),
                model,
                effort: self.request_effort(),
                service_tier: self.conversation.selected_service_tier.clone(),
                permission_mode: self.selected_agent_permission_mode(),
                context: draft.context,
            });
            let (receiver, interrupt) = run.into_parts();
            self.conversation.active_turn = interrupt;
            self.consume_agent_events(receiver, cycle, cx);
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }

    pub(super) fn draft_is_empty(&self, cx: &gpui::App) -> bool {
        self.prompt_text(cx).is_empty()
            && self.prompt_context.files.is_empty()
            && self.review_comments.is_empty()
    }

    pub(super) fn restore_submission(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.draft_is_empty(cx) {
            self.submission_error = Some("请先保存或清空当前草稿，再恢复失败的输入。".into());
            cx.notify();
            return;
        }
        if let Some(draft) = self
            .conversation
            .submissions
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.draft.clone())
        {
            self.prompt_editor
                .update(cx, |editor, cx| editor.set_text_silently(&draft.text, cx));
            self.prompt_context = draft.context;
            self.review_comments = draft.comments;
            if !self.review_comments.is_empty() {
                cx.emit(super::ReviewCommentsRestored(self.review_comments.clone()));
            }
            self.focus_prompt_pending = true;
            self.draft_revision = self.draft_revision.wrapping_add(1);
            cx.notify();
        }
    }
    pub(super) fn consume_agent_events(
        &mut self,
        receiver: async_channel::Receiver<AgentEvent>,
        cycle: u64,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            loop {
                let first_event = match receiver.recv().await {
                    Ok(event) => event,
                    Err(_) => {
                        let _ = this.update(cx, |this, cx| {
                            if this.conversation.cycle != cycle {
                                return;
                            }
                            this.apply_agent_event_batch(vec![AgentEvent::Failed(
                                STREAM_DISCONNECTED_MESSAGE.to_owned(),
                            )]);
                            cx.emit(ConversationChanged);
                            cx.notify();
                        });
                        return;
                    }
                };

                // Once the first event wakes us, leave a short collection
                // window for the rest of its protocol burst. No timer runs
                // while the channel is idle.
                cx.background_executor().timer(STREAM_UPDATE_INTERVAL).await;

                let (mut batch, channel_closed) =
                    collect_ready_agent_events(&receiver, first_event);
                if channel_closed {
                    ensure_closed_batch_is_terminal(&mut batch);
                }

                // Commit every frame's protocol burst atomically. Previously
                // each token emitted and notified independently, repeatedly
                // rebuilding the full conversation before the same paint.
                let result = this.update(cx, |this, cx| {
                    if this.conversation.cycle != cycle {
                        return true;
                    }
                    let created_thread = batch.iter().find_map(|event| match event {
                        AgentEvent::ThreadCreated { thread_id } => Some(thread_id.clone()),
                        _ => None,
                    });
                    let finished = this.apply_agent_event_batch(batch);
                    if let Some(thread_id) = created_thread {
                        cx.emit(ConversationThreadCreated { thread_id });
                    }
                    cx.emit(ConversationChanged);
                    cx.notify();
                    finished
                });
                if result.unwrap_or(true) || channel_closed {
                    return;
                }
            }
        })
        .detach();
    }
    pub(super) fn consume_connection_events(
        &mut self,
        receiver: async_channel::Receiver<AgentConnectionEvent>,
        cx: &mut Context<Self>,
    ) {
        self.connection_event_task = Some(cx.spawn(async move |this, cx| {
            while let Ok(event) = receiver.recv().await {
                let _ = this.update(cx, |this, cx| {
                    if this.apply_connection_event(event) {
                        cx.emit(ConversationChanged);
                        cx.notify();
                    }
                });
            }
        }));
    }
    pub(super) fn stop_generation(&mut self, cx: &mut Context<Self>) {
        if self.conversation.stop_generation() {
            cx.emit(ConversationChanged);
            cx.notify();
        }
    }
    pub fn retry_image_generation(&mut self, cx: &mut Context<Self>) {
        self.submit_prompt("请重新生成上一张图像，保持相同要求。".to_owned(), cx);
    }
}
