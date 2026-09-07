//! Runtime behavior and presentation for the prompt composer.

#[cfg(not(test))]
use super::ModelCatalogLoadFinished;

use gpui::Context;

use super::{ComposerView, ConversationChanged, ConversationThreadCreated};
use crate::{
    agent::{AgentConnectionEvent, AgentEvent, AgentRequest},
    conversation::{
        ConversationActivity, ConversationPhase, STREAM_DISCONNECTED_MESSAGE,
        STREAM_UPDATE_INTERVAL, collect_ready_agent_events, current_local_time_label,
        ensure_closed_batch_is_terminal,
    },
};

impl ComposerView {
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
    pub(super) fn submit_prompt(&mut self, prompt: String, cx: &mut Context<Self>) {
        if prompt.trim().is_empty()
            || matches!(
                self.conversation.phase,
                ConversationPhase::Starting
                    | ConversationPhase::Thinking
                    | ConversationPhase::Streaming
                    | ConversationPhase::Stopping
            )
        {
            return;
        }

        let selection = if self.conversation.selected_model.is_empty()
            || self.conversation.selected_effort.is_empty()
        {
            Err(self
                .conversation
                .model_catalog_error
                .clone()
                .unwrap_or_else(|| "没有可用的 Codex 模型".to_owned()))
        } else {
            Ok((
                self.conversation.selected_model.clone(),
                self.conversation.selected_effort.clone(),
                self.conversation.selected_service_tier.clone(),
            ))
        };

        let cycle = self.conversation.begin_prompt(&prompt);
        self.menu_open = false;
        self.permission_menu_open = false;
        self.permission_menu_keyboard_focus = false;
        self.submenu = None;
        self.prompt_input.update(cx, |input, cx| input.clear(cx));

        let (model, effort, service_tier) = match selection {
            Ok(selection) => selection,
            Err(error) => {
                self.conversation.assistant_message = error.clone();
                self.conversation
                    .activities
                    .push(ConversationActivity::Error { message: error });
                self.conversation.assistant_message_time = Some(current_local_time_label());
                self.conversation.phase = ConversationPhase::Failed;
                cx.emit(ConversationChanged);
                cx.notify();
                return;
            }
        };
        self.conversation.actual_model = Some(model.clone());
        self.conversation.model_status = None;
        self.conversation.safety_buffering = false;
        cx.emit(ConversationChanged);
        cx.notify();

        let run = self.backend.run_prompt(AgentRequest {
            prompt,
            cwd: self.conversation.cwd.clone(),
            project_id: self.conversation.project_id.clone(),
            thread_id: self.conversation.thread_id.clone(),
            model,
            effort,
            service_tier,
            permission_mode: self.permission_mode.agent_mode(),
        });
        let (receiver, interrupt) = run.into_parts();
        self.conversation.active_turn = interrupt;
        self.consume_agent_events(receiver, cycle, cx);
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
        cx.spawn(async move |this, cx| {
            while let Ok(event) = receiver.recv().await {
                let _ = this.update(cx, |this, cx| {
                    if this.apply_connection_event(event) {
                        cx.emit(ConversationChanged);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
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
