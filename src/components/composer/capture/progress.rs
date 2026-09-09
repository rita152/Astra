//! Timed, interruptible activity fixtures driven through the production reducer.
use super::{ComposerView, ConversationChanged};
use gpui::Context;
impl ComposerView {
    pub fn set_progress_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        use crate::agent::{
            AgentActivityStatus as S, AgentEvent as E, AgentPlan, AgentPlanStep,
            AgentPlanStepStatus as P, AgentSleep, AgentTurnPlan, AgentWebSearch,
        };
        self.conversation.begin_prompt("核对计划、搜索与等待活动。");
        self.conversation.user_message_time = None;
        self.conversation.apply_agent_event_batch(vec![
            E::Started,
            E::TurnPlanUpdated(AgentTurnPlan { turn_id: "capture-progress-turn".into(), explanation: Some("核对资料并观察等待状态".into()), steps: vec![AgentPlanStep {step:"核对资料".into(),status:P::Completed},AgentPlanStep{step:"等待观察".into(),status:P::InProgress},AgentPlanStep{step:"汇总结果".into(),status:P::Pending}] }),
            E::PlanDelta { item_id:"capture-plan".into(), delta:"# 验收计划\n\n核对资料，等待观察，然后汇总结果。".into() },
            E::WebSearchUpdated(AgentWebSearch { id:"capture-search".into(), query:"Rust programming language official website".into(), action:serde_json::json!({"type":"search","queries":["Rust programming language official website"]}), results:serde_json::json!([{"url":"https://rust-lang.org/","title":"Rust Programming Language"}]), extra:Default::default(), status:S::Completed }),
            E::SleepUpdated(AgentSleep { id:"capture-sleep".into(), duration_ms:15000, status:S::InProgress }),
        ]);
        if !matches!(state, "running" | "streaming") {
            self.conversation
                .apply_agent_event_batch(vec![E::PlanUpdated(AgentPlan {
                    id: "capture-plan".into(),
                    text: "# 验收计划\n\n核对资料，等待观察，然后汇总结果。".into(),
                    status: S::Completed,
                })]);
            if state == "interrupted" {
                self.conversation
                    .apply_agent_event_batch(vec![E::Interrupted]);
            } else {
                self.conversation.apply_agent_event_batch(vec![
                    E::SleepUpdated(AgentSleep {
                        id: "capture-sleep".into(),
                        duration_ms: 15000,
                        status: S::Completed,
                    }),
                    E::Completed,
                ]);
            }
        }
        if matches!(state, "running" | "streaming") {
            let (sender, receiver) = async_channel::unbounded();
            self.conversation.active_turn = Some(crate::agent::AgentInterruptHandle::new(
                std::sync::Arc::new(CaptureInterrupt(sender.clone())),
            ));
            let cycle = self.conversation.cycle;
            cx.spawn(async move |this, cx| {
                while let Ok(event) = receiver.recv().await {
                    let terminal = matches!(&event, E::Completed | E::Interrupted | E::Failed(_));
                    let _ = this.update(cx, |this, cx| {
                        if this.conversation.cycle == cycle {
                            this.conversation.apply_agent_event_batch(vec![event]);
                            this.conversation.assistant_message_time = None;
                            cx.emit(ConversationChanged);
                            cx.notify();
                        }
                    });
                    if terminal {
                        break;
                    }
                }
            })
            .detach();
            if state == "streaming" {
                cx.spawn(async move |_, cx| {
                    for text in ["\n\n正在核对", "增量文本。"] {
                        cx.background_executor()
                            .timer(std::time::Duration::from_secs(2))
                            .await;
                        eprintln!("progress capture: plan delta");
                        if sender
                            .send(E::PlanDelta {
                                item_id: "capture-plan".into(),
                                delta: text.into(),
                            })
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(2))
                        .await;
                    eprintln!("progress capture: authoritative plan item");
                    let _ = sender
                        .send(E::PlanUpdated(AgentPlan {
                            id: "capture-plan".into(),
                            text: "# 验收计划\n\n核对资料，等待观察，然后汇总结果。".into(),
                            status: S::Completed,
                        }))
                        .await;
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(9))
                        .await;
                    eprintln!("progress capture: sleep completed, then turn completed");
                    let _ = sender
                        .send(E::SleepUpdated(AgentSleep {
                            id: "capture-sleep".into(),
                            duration_ms: 15000,
                            status: S::Completed,
                        }))
                        .await;
                    let _ = sender.send(E::Completed).await;
                })
                .detach();
            }
        }
        self.conversation.assistant_message_time = None;
        cx.emit(ConversationChanged);
        cx.notify();
    }
}

struct CaptureInterrupt(async_channel::Sender<crate::agent::AgentEvent>);
impl crate::agent::AgentInterruptControl for CaptureInterrupt {
    fn request_interrupt(&self) -> Result<crate::agent::AgentInterruptOutcome, String> {
        eprintln!("progress capture: interrupt requested");
        self.0
            .send_blocking(crate::agent::AgentEvent::Interrupted)
            .map_err(|e| e.to_string())?;
        Ok(crate::agent::AgentInterruptOutcome::Requested)
    }
    fn abandon(&self) {
        self.0.close();
    }
}
