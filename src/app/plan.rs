//! Proposed plans use read-only file tabs and explicit Markdown export.
use super::ChatApp;
use crate::agent::AgentPlan;
use gpui::Context;
impl ChatApp {
    pub(super) fn open_plan(&mut self, plan: AgentPlan, cx: &mut Context<Self>) {
        self.right_panel.open = true;
        self.select_right_panel_item(3, cx);
        self.file_panels[&self.active_conversation]
            .update(cx, |panel, cx| panel.open_plan(plan, cx));
        self.plan_export_error = None;
        cx.notify();
    }
    pub(super) fn download_plan(&mut self, plan: AgentPlan, cx: &mut Context<Self>) {
        let directory = self
            .conversation_hosts
            .get(&self.active_conversation)
            .map(|h| h.cwd.clone())
            .unwrap_or_default();
        let destination = cx.prompt_for_new_path(&directory, Some("plan.md"));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(destination))) = destination.await else {
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move { std::fs::write(destination, plan.text) })
                .await;
            if let Err(error) = result {
                let _ = this.update(cx, |this, cx| {
                    this.plan_export_error = Some(format!("无法保存计划：{error}"));
                    cx.notify();
                });
            }
        })
        .detach();
    }
}
