//! Plan cards, turn progress and wait rows using the desktop activity typography.
use super::HomeView;
use crate::{
    agent::{AgentActivityStatus, AgentPlan, AgentPlanStepStatus, AgentSleep, AgentTurnPlan},
    components::{icons::icon, markdown::render_selectable_plan},
    theme::Theme,
};
use gpui::{
    App, ClipboardItem, Context, Div, Entity, IntoElement, Render, Role, SharedString, Window, div,
    prelude::*, px,
};

pub(super) fn sleep_label(sleep: &AgentSleep) -> String {
    let duration = if sleep.duration_ms.is_multiple_of(1000) {
        format!("{} 秒", sleep.duration_ms / 1000)
    } else {
        format!("{} 毫秒", sleep.duration_ms)
    };
    let verb = match sleep.status {
        AgentActivityStatus::InProgress => "正在等待",
        AgentActivityStatus::Completed => "已等待",
        AgentActivityStatus::Interrupted => "等待已中断",
        AgentActivityStatus::Failed => "等待失败",
    };
    if matches!(
        sleep.status,
        AgentActivityStatus::Interrupted | AgentActivityStatus::Failed
    ) {
        format!("{verb} · 原定 {duration}")
    } else {
        format!("{verb} · {duration}")
    }
}

pub(super) fn sleep_activity(sleep: AgentSleep, theme: Theme) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("sleep-{}", sleep.id)))
        .role(Role::Status)
        .aria_label(sleep_label(&sleep))
        .min_h(px(21.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(px(14.0))
        .line_height(px(21.0))
        .text_color(theme.markdown_text.alpha(0.60))
        .child(icon("scheduled", theme.markdown_text.alpha(0.60).into()).size(px(14.0)))
        .child(sleep_label(&sleep))
}

fn toggle_plan(home: &Entity<HomeView>, id: &str, cx: &mut App) {
    home.update(cx, |home, cx| {
        if !home.expanded_commands.remove(id) {
            home.expanded_commands.insert(id.to_owned());
        }
        cx.notify();
    });
}

fn plan_button(
    id: String,
    label: &'static str,
    glyph: &'static str,
    theme: Theme,
    action: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(SharedString::from(id))
        .size(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .focusable()
        .tab_stop(true)
        .role(Role::Button)
        .aria_label(label)
        .cursor_pointer()
        .hover(|s| s.bg(theme.sidebar_hover))
        .focus_visible(|s| s.border_1().border_color(theme.markdown_text.alpha(0.65)))
        .on_click(move |_, _, cx| {
            action(cx);
            cx.stop_propagation();
        })
        .on_key_down(move |e, window, cx| {
            if e.keystroke.key == "tab" {
                if e.keystroke.modifiers.shift {
                    window.focus_prev(cx);
                } else {
                    window.focus_next(cx);
                }
                cx.stop_propagation();
            }
        })
        .child(icon(glyph, theme.markdown_text.alpha(0.4).into()).size(px(14.0)))
}

pub(super) fn plan_activity(
    home: Entity<HomeView>,
    plan: AgentPlan,
    feedback_open: bool,
    in_panel: bool,
    shimmer_progress: f32,
    theme: Theme,
) -> impl IntoElement {
    let active = plan.status == AgentActivityStatus::InProgress;
    let id = plan.id.clone();
    let open_home = home.clone();
    let open_plan = plan.clone();
    let open = move |cx: &mut App| {
        open_home.update(cx, |_, cx| cx.emit(super::OpenPlan(open_plan.clone())))
    };
    let download_home = home.clone();
    let download_plan = plan.clone();
    let copy = plan.text.clone();
    let feedback_id = format!("plan-feedback-{}", plan.id);
    let feedback_home = home.clone();
    let feedback_key = feedback_id.clone();
    let rating_home = home.clone();
    let card_open = open.clone();
    div()
        .id(SharedString::from(format!("plan-card-{id}")))
        .relative()
        .w_full()
        .min_w(px(0.0))
        .rounded(px(12.5))
        .border_1()
        .border_color(theme.plan_border)
        .bg(theme.plan_surface)
        .overflow_hidden()
        .when(!active, |card| {
            card.focusable()
                .tab_stop(true)
                .role(Role::Button)
                .aria_label("打开计划")
                .cursor_pointer()
                .on_click(move |event, _, cx| {
                    if let gpui::ClickEvent::Mouse(mouse) = event
                        && ((mouse.up.position.x - mouse.down.position.x).abs() > px(3.0)
                            || (mouse.up.position.y - mouse.down.position.y).abs() > px(3.0))
                    {
                        return;
                    }
                    card_open(cx);
                })
                .on_key_down(move |e, window, cx| {
                    if e.keystroke.key == "tab" {
                        if e.keystroke.modifiers.shift {
                            window.focus_prev(cx);
                        } else {
                            window.focus_next(cx);
                        }
                        cx.stop_propagation();
                    }
                })
        })
        .child(
            div()
                .h(px(40.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .justify_between()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(theme.markdown_text.alpha(0.4))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(icon("plan", theme.markdown_text.alpha(0.4).into()).size(px(16.0)))
                        .when(active, |header| {
                            header.child(super::animation::shimmer_label(
                                "编写计划",
                                56.0,
                                theme,
                                shimmer_progress,
                            ))
                        })
                        .when(!active, |header| header.child("套餐")),
                )
                .when(!active, |header| {
                    header.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .when(!in_panel, |actions| {
                                actions
                                    .child(plan_button(
                                        format!("plan-download-{id}"),
                                        "下载计划",
                                        "plan-download",
                                        theme,
                                        move |cx| {
                                            download_home.update(cx, |_, cx| {
                                                cx.emit(super::DownloadPlan(download_plan.clone()))
                                            })
                                        },
                                    ))
                                    .child(plan_button(
                                        format!("plan-copy-{id}"),
                                        "复制计划",
                                        "plan-copy",
                                        theme,
                                        move |cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                copy.clone(),
                                            ))
                                        },
                                    ))
                                    .child(plan_button(
                                        format!("plan-feedback-{id}"),
                                        "评价回复",
                                        "plan-feedback",
                                        theme,
                                        move |cx| toggle_plan(&feedback_home, &feedback_key, cx),
                                    ))
                            })
                            .child(plan_button(
                                format!("plan-open-{id}"),
                                "在侧边面板中打开计划",
                                "plan-open",
                                theme,
                                open,
                            )),
                    )
                }),
        )
        .when(!in_panel, |card| {
            card.child(
                div()
                    .relative()
                    .max_h(px(160.0))
                    .overflow_hidden()
                    .px(px(16.0))
                    .py(px(12.0))
                    .child(render_selectable_plan(
                        &plan.text,
                        theme,
                        &format!("plan-text-{id}"),
                    ))
                    .child(
                        div()
                            .absolute()
                            .bottom_0()
                            .left_0()
                            .w_full()
                            .h(px(64.0))
                            .rounded_b(px(12.5))
                            .bg(gpui::linear_gradient(
                                180.0,
                                gpui::linear_color_stop(theme.plan_surface.alpha(0.0), 0.0),
                                gpui::linear_color_stop(theme.plan_surface, 1.0),
                            )),
                    ),
            )
        })
        .when(feedback_open && !active && !in_panel, |card| {
            card.child(
                div()
                    .absolute()
                    .top(px(36.0))
                    .right(px(12.0))
                    .p(px(4.0))
                    .rounded(px(10.0))
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.border)
                    .flex()
                    .gap(px(4.0))
                    .children(
                        [
                            (1, "赞", "message-thumb-up"),
                            (-1, "踩", "message-thumb-down"),
                        ]
                        .into_iter()
                        .map(|(value, label, glyph)| {
                            let home = rating_home.clone();
                            let key = feedback_id.clone();
                            plan_button(
                                format!("plan-rating-{id}-{value}"),
                                label,
                                glyph,
                                theme,
                                move |cx| {
                                    home.update(cx, |home, cx| {
                                        home.response_feedback = if home.response_feedback == value
                                        {
                                            0
                                        } else {
                                            value
                                        };
                                        home.expanded_commands.remove(&key);
                                        cx.notify();
                                    })
                                },
                            )
                        }),
                    ),
            )
        })
}

pub(super) fn turn_plan_label(plan: &AgentTurnPlan) -> String {
    let completed = plan
        .steps
        .iter()
        .filter(|s| s.status == AgentPlanStepStatus::Completed)
        .count();
    if completed == plan.steps.len() {
        format!("已完成 {} 个步骤", completed)
    } else {
        format!("第 {} / {} 步", completed + 1, plan.steps.len())
    }
}
struct PlanTooltip {
    plan: AgentTurnPlan,
    theme: Theme,
}
impl Render for PlanTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        step_details(&self.plan, self.theme)
    }
}
fn step_details(plan: &AgentTurnPlan, theme: Theme) -> gpui::Stateful<Div> {
    div()
        .id(SharedString::from(format!("plan-steps-{}", plan.turn_id)))
        .role(Role::List)
        .aria_label("计划步骤")
        .max_w(px(336.0))
        .p(px(8.0))
        .rounded(px(12.0))
        .bg(theme.surface)
        .border_1()
        .border_color(theme.border)
        .flex()
        .flex_col()
        .gap(px(8.0))
        .children(plan.steps.iter().enumerate().map(|(index, step)| {
            div()
                .id(("plan-step", index))
                .role(Role::ListItem)
                .aria_label(format!(
                    "{}：{}",
                    match step.status {
                        AgentPlanStepStatus::Pending => "待开始",
                        AgentPlanStepStatus::InProgress => "进行中",
                        AgentPlanStepStatus::Completed => "已完成",
                    },
                    step.step
                ))
                .flex()
                .items_start()
                .gap(px(8.0))
                .text_size(px(14.0))
                .line_height(px(16.0))
                .text_color(theme.markdown_text.alpha(
                    if step.status == AgentPlanStepStatus::Completed {
                        0.4
                    } else {
                        0.6
                    },
                ))
                .child(
                    icon(
                        if step.status == AgentPlanStepStatus::Completed {
                            "plan-step-completed"
                        } else if step.status == AgentPlanStepStatus::InProgress {
                            "plan-step-running"
                        } else {
                            "plan-step-pending"
                        },
                        theme.markdown_text.alpha(0.65).into(),
                    )
                    .size(px(16.0)),
                )
                .child(div().flex_1().min_w(px(0.0)).child(step.step.clone()))
        }))
}
pub(super) fn turn_plan_control(
    home: Entity<HomeView>,
    plan: AgentTurnPlan,
    expanded: bool,
    theme: Theme,
) -> impl IntoElement {
    let id = format!("turn-plan-{}", plan.turn_id);
    let click_id = id.clone();
    let click_home = home.clone();
    let tooltip_plan = plan.clone();
    div()
        .relative()
        .child(
            div()
                .id(SharedString::from(id.clone()))
                .focusable()
                .tab_stop(true)
                .role(Role::Button)
                .aria_label(turn_plan_label(&plan))
                .aria_expanded(expanded)
                .h(px(38.0))
                .px(px(12.0))
                .rounded(px(25.0))
                .border_1()
                .border_color(theme.markdown_text.alpha(0.066))
                .bg(theme.plan_progress_surface)
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_size(px(14.0))
                .line_height(px(21.0))
                .text_color(theme.markdown_text.alpha(0.65))
                .cursor_pointer()
                .hover(|s| s.text_color(theme.text))
                .tooltip(move |_, cx| {
                    cx.new(|_| PlanTooltip {
                        plan: tooltip_plan.clone(),
                        theme,
                    })
                    .into()
                })
                .on_click(move |_, _, cx| toggle_plan(&click_home, &click_id, cx))
                .focus_visible(|s| s.border_color(theme.accent))
                .on_key_down(move |e, window, cx| match e.keystroke.key.as_str() {
                    "escape" if expanded => {
                        home.update(cx, |home, cx| {
                            home.expanded_commands.remove(&id);
                            cx.notify();
                        });
                        cx.stop_propagation();
                    }
                    "tab" => {
                        if e.keystroke.modifiers.shift {
                            window.focus_prev(cx);
                        } else {
                            window.focus_next(cx);
                        }
                        cx.stop_propagation();
                    }
                    _ => {}
                })
                .child(progress_ring(&plan))
                .child(turn_plan_label(&plan)),
        )
        .when(expanded, |s| {
            s.child(
                div()
                    .absolute()
                    .bottom(px(36.0))
                    .left_0()
                    .child(step_details(&plan, theme)),
            )
        })
}

fn progress_ring(plan: &AgentTurnPlan) -> impl IntoElement {
    let fraction = plan
        .steps
        .iter()
        .filter(|s| s.status == AgentPlanStepStatus::Completed)
        .count() as f32
        / plan.steps.len().max(1) as f32;
    gpui::canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            for (portion, alpha) in [(1.0, 0.16), (fraction, 1.0)] {
                if portion <= 0.0 {
                    continue;
                }
                let mut path = gpui::PathBuilder::stroke(px(2.0));
                for index in 0..=64 {
                    let angle = -std::f32::consts::FRAC_PI_2
                        + std::f32::consts::TAU * portion * index as f32 / 64.0;
                    let position = gpui::point(
                        bounds.origin.x + px(6.0 + 5.0 * angle.cos()),
                        bounds.origin.y + px(6.0 + 5.0 * angle.sin()),
                    );
                    if index == 0 {
                        path.move_to(position);
                    } else {
                        path.line_to(position);
                    }
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, gpui::rgba(0x3b82f6ff).alpha(alpha));
                }
            }
        },
    )
    .size(px(12.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wait_labels_preserve_zero_subsecond_and_interrupted_requested_duration() {
        for (duration_ms, status, label) in [
            (0, AgentActivityStatus::Completed, "已等待 · 0 秒"),
            (
                15_000,
                AgentActivityStatus::Interrupted,
                "等待已中断 · 原定 15 秒",
            ),
            (125, AgentActivityStatus::InProgress, "正在等待 · 125 毫秒"),
        ] {
            assert_eq!(
                sleep_label(&AgentSleep {
                    id: "s".into(),
                    duration_ms,
                    status
                }),
                label
            );
        }
    }
    #[test]
    fn single_search_keeps_query_visible_and_plan_card_follows_activity() {
        use super::super::timeline::{ActivityStreamUnit, activity_stream_units};
        use crate::{agent::AgentWebSearch, conversation::ConversationActivity};
        let search = AgentWebSearch {
            id: "s".into(),
            query: "Rust".into(),
            action: serde_json::Value::Null,
            results: serde_json::Value::Null,
            extra: Default::default(),
            status: AgentActivityStatus::Completed,
        };
        let units = activity_stream_units(&[
            ConversationActivity::Plan(AgentPlan {
                id: "p".into(),
                text: "plan".into(),
                status: AgentActivityStatus::Completed,
            }),
            ConversationActivity::WebSearch(search),
        ]);
        assert!(
            matches!(&units[0],ActivityStreamUnit::Standalone(ConversationActivity::WebSearch(s)) if s.query=="Rust")
        );
        assert!(matches!(
            &units[1],
            ActivityStreamUnit::Standalone(ConversationActivity::Plan(_))
        ));
    }
    #[test]
    fn completed_plan_steps_do_not_render_a_nonexistent_next_step() {
        let plan = AgentTurnPlan {
            turn_id: "t".into(),
            explanation: None,
            steps: vec![crate::agent::AgentPlanStep {
                step: "done".into(),
                status: AgentPlanStepStatus::Completed,
            }],
        };
        assert_eq!(turn_plan_label(&plan), "已完成 1 个步骤");
    }
    #[test]
    fn progress_pill_mouse_enter_space_and_escape_paths() {
        use gpui::{
            AppContext, Bounds, MouseButton, TestApp, WindowBounds, WindowOptions, point, size,
        };
        struct Probe {
            home: Entity<HomeView>,
            plan: AgentTurnPlan,
        }
        impl Render for Probe {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                let expanded = self
                    .home
                    .read(cx)
                    .expanded_commands
                    .contains("turn-plan-test");
                div().size_full().p(px(10.0)).child(turn_plan_control(
                    self.home.clone(),
                    self.plan.clone(),
                    expanded,
                    Theme::for_mode(crate::theme::ThemeMode::Dark),
                ))
            }
        }
        let mut app = TestApp::new();
        let home = app.update(|cx| cx.new(|cx| HomeView::new(crate::theme::ThemeMode::Dark, cx)));
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.0), px(0.0)),
                    size(px(400.0), px(300.0)),
                ))),
                ..Default::default()
            },
            |_, _| Probe {
                home: home.clone(),
                plan: AgentTurnPlan {
                    turn_id: "test".into(),
                    explanation: None,
                    steps: vec![crate::agent::AgentPlanStep {
                        step: "check".into(),
                        status: AgentPlanStepStatus::InProgress,
                    }],
                },
            },
        );
        window.draw();
        window.simulate_click(point(px(30.0), px(30.0)), MouseButton::Left);
        assert!(app.read_entity(&home, |h, _| h.expanded_commands.contains("turn-plan-test")));
        window.draw();
        window.simulate_keystroke("enter");
        window.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("enter").unwrap(),
        });
        assert!(!app.read_entity(&home, |h, _| h.expanded_commands.contains("turn-plan-test")));
        window.draw();
        window.simulate_keystroke("space");
        window.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("space").unwrap(),
        });
        assert!(app.read_entity(&home, |h, _| h.expanded_commands.contains("turn-plan-test")));
        window.draw();
        window.simulate_keystroke("escape");
        assert!(!app.read_entity(&home, |h, _| h.expanded_commands.contains("turn-plan-test")));
    }
}

#[cfg(test)]
mod steer_integration_tests {
    use super::super::timeline::{ActivityStreamUnit, activity_stream_units};
    use crate::{
        agent::{AgentActivityStatus, AgentPlan},
        conversation::ConversationActivity,
    };

    #[test]
    fn steering_keeps_each_plan_on_its_side_of_the_user_message() {
        let plan = |id: &str| {
            ConversationActivity::Plan(AgentPlan {
                id: id.into(),
                text: id.into(),
                status: AgentActivityStatus::Completed,
            })
        };
        let units = activity_stream_units(&[
            plan("before"),
            ConversationActivity::UserMessage {
                item_id: "steer".into(),
                text: "revise".into(),
                images: vec![],
            },
            plan("after"),
        ]);
        assert_eq!(units.len(), 3, "steering must not replace the earlier plan");
        assert!(
            matches!(&units[0], ActivityStreamUnit::Standalone(ConversationActivity::Plan(p)) if p.id == "before")
        );
        assert!(
            matches!(&units[1], ActivityStreamUnit::Standalone(ConversationActivity::UserMessage { item_id, .. }) if item_id == "steer")
        );
        assert!(
            matches!(&units[2], ActivityStreamUnit::Standalone(ConversationActivity::Plan(p)) if p.id == "after")
        );
    }
}
