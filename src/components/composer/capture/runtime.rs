//! Deterministic observations; no hook execution, credential mutation, or model RPC.

use super::{ComposerView, ConversationChanged};
use crate::agent::*;
use gpui::Context;

impl ComposerView {
    pub fn set_runtime_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        // These identities are synthetic. Retire startup permission reads so
        // their callbacks cannot try to resume a fixture thread on the server.
        self.permission_catalog_cycle = self.permission_catalog_cycle.wrapping_add(1);
        self.permission_read_cycle = self.permission_read_cycle.wrapping_add(1);
        self.permission_catalog_loading = false;
        self.permission_effective_loading = false;
        self.permission_catalog_error = None;
        let generation = 1_000_000;
        let thread = "capture-runtime";
        let turn = "capture-runtime-turn";
        self.conversation.thread_id = Some(thread.into());
        self.conversation
            .begin_prompt("Runtime compatibility check.");
        self.conversation.turn_id = Some(turn.into());
        self.conversation.user_message_time = None;
        self.conversation.apply_runtime_event(AgentRuntimeEvent {
            generation,
            observation: AgentRuntimeObservation::GenerationStarted,
        });
        if state == "deprecation" {
            self.conversation.thread_id = None;
            self.conversation.turn_id = None;
            self.conversation.phase = crate::conversation::ConversationPhase::Empty;
            self.conversation.user_message = None;
            self.conversation
                .apply_connection_event(AgentConnectionEvent::DeprecationNotice(
                    AgentDeprecationNotice {
                        summary: "Runtime compatibility reference".into(),
                        details: Some(
                            "This feature is deprecated. Use the supported replacement.".into(),
                        ),
                    },
                ));
        } else if state == "history" {
            self.conversation.hydrate_history(ThreadHistory {
                thread: ThreadSummary {
                    thread_id: thread.into(),
                    title: "Runtime compatibility check".into(),
                    preview: String::new(),
                    cwd: std::env::current_dir().unwrap_or_default(),
                    project_id: None,
                    section: None,
                    created_at: 0,
                    updated_at: 0,
                    recency_at: None,
                    activity: ThreadActivity::Idle,
                },
                turns: vec![ThreadTurn {
                    turn_id: turn.into(),
                    status: HistoryTurnStatus::Completed,
                    items_view: HistoryItemDetail::Full,
                    items: vec![
                        ThreadHistoryItem::UserMessage {
                            item_id: "user".into(),
                            client_message_id: None,
                            text: "Runtime compatibility check.".into(),
                            images: vec![],
                        },
                        ThreadHistoryItem::HookPrompt(capture_hook_prompt(None, false)),
                        ThreadHistoryItem::AssistantMessage {
                            item_id: "answer".into(),
                            text: "Runtime check complete.".into(),
                            phase: Some("final_answer".into()),
                        },
                    ],
                    started_at: None,
                    completed_at: None,
                    duration_ms: None,
                    error: None,
                }],
                next_turn_cursor: None,
                backwards_turn_cursor: None,
            });
        } else {
            let mut hook = AgentHookRun {
                thread_id: thread.into(),
                turn_id: if state == "turnless" {
                    None
                } else {
                    Some(turn.into())
                },
                id: "reference-hook".into(),
                display_order: 0,
                event_name: "stop".into(),
                execution_mode: "sync".into(),
                handler_type: "command".into(),
                scope: "turn".into(),
                source: "project".into(),
                source_path: "/tmp/hooks.json".into(),
                status: AgentHookStatus::Running,
                status_message: None,
                entries: vec![],
                started_at: 1000,
                completed_at: None,
                duration_ms: None,
                received_completed: false,
                closed_locally: None,
            };
            self.conversation.apply_runtime_event(AgentRuntimeEvent {
                generation,
                observation: AgentRuntimeObservation::Hook(Box::new(hook.clone())),
            });
            self.conversation.apply_runtime_event(AgentRuntimeEvent {
                generation,
                observation: AgentRuntimeObservation::AuthRecovery(AgentAuthRecovery {
                    thread_id: thread.into(),
                    turn_id: turn.into(),
                    provider: "openai".into(),
                    started_message: Some("Refreshing credentials".into()),
                    completed_message: None,
                    closed_locally: None,
                }),
            });
            self.conversation
                .apply_agent_event_batch(vec![AgentEvent::Started]);
            if !matches!(
                state,
                "auth-started" | "auth-completed" | "running" | "turnless"
            ) {
                self.conversation
                    .apply_agent_event_batch(vec![AgentEvent::HookPromptUpdated(
                        capture_hook_prompt(Some(true), state == "long"),
                    )]);
            }
            if state == "auth-completed" {
                self.conversation.apply_runtime_event(AgentRuntimeEvent {
                    generation,
                    observation: AgentRuntimeObservation::AuthRecovery(AgentAuthRecovery {
                        thread_id: thread.into(),
                        turn_id: turn.into(),
                        provider: "openai".into(),
                        started_message: None,
                        completed_message: Some("Credentials refreshed; retry pending".into()),
                        closed_locally: None,
                    }),
                });
            } else if state == "interrupted" {
                self.conversation
                    .apply_agent_event_batch(vec![AgentEvent::Interrupted]);
            } else if state == "disconnected" {
                self.conversation.apply_runtime_event(AgentRuntimeEvent {
                    generation,
                    observation: AgentRuntimeObservation::Disconnected,
                });
                self.conversation
                    .apply_agent_event_batch(vec![AgentEvent::Failed("Connection closed".into())]);
            } else if !matches!(state, "running" | "auth-started" | "turnless") {
                hook.status = AgentHookStatus::Completed;
                hook.received_completed = true;
                hook.completed_at = Some(2000);
                hook.duration_ms = Some(1000);
                hook.status_message = Some("Hook finished".into());
                hook.entries = vec![
                    AgentHookOutput {
                        kind: "warning".into(),
                        text: "Runtime compatibility reference warning".into(),
                    },
                    AgentHookOutput {
                        kind: "context".into(),
                        text: "Hidden internal context".into(),
                    },
                ];
                self.conversation.apply_runtime_event(AgentRuntimeEvent {
                    generation,
                    observation: AgentRuntimeObservation::Hook(Box::new(hook)),
                });
                self.conversation.apply_agent_event_batch(vec![
                    AgentEvent::AssistantMessageStarted {
                        item_id: "answer".into(),
                    },
                    AgentEvent::TextDelta("Runtime check complete.".into()),
                    AgentEvent::Completed,
                ]);
            }
        }
        self.conversation.assistant_message_time = None;
        if matches!(
            state,
            "running" | "auth-started" | "auth-completed" | "turnless"
        ) {
            let (sender, receiver) = async_channel::unbounded();
            self.conversation.active_turn = Some(AgentInterruptHandle::new(std::sync::Arc::new(
                RuntimeCaptureInterrupt(sender),
            )));
            let cycle = self.conversation.cycle;
            cx.spawn(async move |this, cx| {
                if let Ok(event) = receiver.recv().await {
                    let _ = this.update(cx, |this, cx| {
                        if this.conversation.cycle != cycle {
                            return;
                        }
                        this.conversation.apply_agent_event_batch(vec![event]);
                        this.conversation.assistant_message_time = None;
                        cx.emit(ConversationChanged);
                        cx.notify();
                    });
                }
            })
            .detach();
        }
        if let Some(path) = std::env::var_os("GPUI_RUNTIME_AUDIT_OUTPUT") {
            let audit = serde_json::json!({"source":"deterministic capture; no server execution","state":state,"phase":format!("{:?}",self.conversation.phase),"runtime":format!("{:#?}",self.conversation.runtime),"deprecationNotices":format!("{:#?}",self.conversation.deprecation_notices),"activities":format!("{:#?}",self.conversation.activities),"transcript":format!("{:#?}",self.conversation.transcript)});
            if let Err(error) = std::fs::write(path, serde_json::to_vec_pretty(&audit).unwrap()) {
                eprintln!("runtime audit: {error}");
            }
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }
}

struct RuntimeCaptureInterrupt(async_channel::Sender<AgentEvent>);
impl AgentInterruptControl for RuntimeCaptureInterrupt {
    fn request_interrupt(&self) -> Result<AgentInterruptOutcome, String> {
        self.0
            .send_blocking(AgentEvent::Interrupted)
            .map_err(|error| error.to_string())?;
        Ok(AgentInterruptOutcome::Requested)
    }
    fn abandon(&self) {
        self.0.close();
    }
}

fn capture_hook_prompt(completed: Option<bool>, long: bool) -> AgentHookPrompt {
    AgentHookPrompt {
        id: "reference-prompt".into(),
        completed,
        fragments: if long {
            vec![AgentHookPromptFragment {
                hook_run_id: "reference-hook".into(),
                text: (1..=20)
                    .map(|i| format!("Hook context line {i}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            }]
        } else {
            vec![
                AgentHookPromptFragment {
                    hook_run_id: "reference-hook".into(),
                    text: "Hook supplied context.".into(),
                },
                AgentHookPromptFragment {
                    hook_run_id: "reference-hook-2".into(),
                    text: "Keep this fragment second.".into(),
                },
            ]
        },
    }
}
