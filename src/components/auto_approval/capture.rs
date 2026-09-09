//! Deterministic native review specimen using the production reducer and view.

use super::*;
use crate::agent::{
    AgentAutoApprovalReview, AgentAutoApprovalReviewKey, AgentEvent, AgentStrictReviewRequirement,
};
use crate::conversation::{ConversationActivity, ConversationState};
use gpui::{AppContext, Bounds, Entity, WindowBounds, WindowOptions, size};

fn review(status: Status, rationale: String) -> AgentAutoApprovalReview {
    AgentAutoApprovalReview {
        key: AgentAutoApprovalReviewKey {
            thread_id: "auto-approval-capture".into(),
            turn_id: "turn-a".into(),
            review_id: "review-a".into(),
        },
        target_item_id: None,
        action: Action::NetworkAccess {
            host: "example.com".into(),
            port: 443,
            protocol: "https".into(),
            target: "example.com:443".into(),
        },
        status,
        rationale: Some(rationale),
        risk_level: Some("low".into()),
        user_authorization: Some("high".into()),
        started_at_ms: 1788854400000,
        completed_at_ms: (status != Status::InProgress).then_some(1788854401500),
        decision_source: (status != Status::InProgress).then(|| "agent".into()),
    }
}

struct Capture {
    state: ConversationState,
    view: Entity<AutoApprovalReviewView>,
    mode: ThemeMode,
    selected: String,
    rationale: String,
    focus: FocusHandle,
    output_dir: Option<std::path::PathBuf>,
    capture_count: usize,
}

impl Capture {
    fn select(&mut self, selected: &str, cx: &mut Context<Self>) {
        self.selected = selected.to_owned();
        self.state = ConversationState::default();
        self.state
            .begin_prompt("Check the public example.com endpoint.");
        self.state.apply_agent_event_batch(vec![
            AgentEvent::ThreadCreated {
                thread_id: "auto-approval-capture".into(),
            },
            AgentEvent::TurnReady(crate::agent::AgentTurnIdentity {
                generation: 0,
                thread_id: "auto-approval-capture".into(),
                turn_id: "turn-a".into(),
            }),
            AgentEvent::Started,
        ]);
        let status = match selected {
            "approved" => Status::Approved,
            "denied" => Status::Denied,
            "timedOut" => Status::TimedOut,
            "aborted" => Status::Aborted,
            _ => Status::InProgress,
        };
        let event = match selected {
            "strict" => AgentEvent::StrictReviewRequired(AgentStrictReviewRequirement {
                thread_id: "auto-approval-capture".into(),
                turn_id: "turn-a".into(),
                started_at_ms: 1788854400000,
            }),
            "warning" => AgentEvent::GuardianWarning(AgentGuardianWarning {
                thread_id: "auto-approval-capture".into(),
                message:
                    "Automatic approval review rejected too many approval requests for this turn"
                        .into(),
            }),
            _ => AgentEvent::AutoApprovalReviewUpdated(Box::new(review(
                status,
                self.rationale.clone(),
            ))),
        };
        self.state.apply_agent_event_batch(vec![event]);
        if let Some(ConversationActivity::AutoApprovalReview(model)) = self.state.activities.first()
        {
            self.view.update(cx, |view, cx| {
                view.sync(*model.clone(), self.mode, cx);
                view.expanded = false;
                view.details_expanded = false;
            });
        }
        cx.notify();
    }
}

impl Render for Capture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_mode(self.mode);
        let mut content = div().w_full().flex().flex_col();
        match self.state.activities.first() {
            Some(ConversationActivity::StrictReview(model)) => {
                content = content.child(strict_review(model, theme))
            }
            Some(ConversationActivity::GuardianWarning(warning)) => {
                content = content.child(guardian_warning(warning, theme))
            }
            Some(ConversationActivity::AutoApprovalReview(model))
                if model.status() != Status::Approved =>
            {
                content = content.child(self.view.clone())
            }
            _ => {}
        }
        div()
            .id("auto-approval-capture")
            .size_full()
            .bg(theme.surface)
            .font(ui_font())
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|s, e: &gpui::KeyDownEvent, window, cx| {
                if navigate_tab(e, window, cx) {
                    return;
                }
                if e.keystroke.key == "f12" {
                    if let Some(dir) = &s.output_dir {
                        std::fs::create_dir_all(dir).expect("create capture directory");
                        let path =
                            dir.join(format!("native-{:02}-{}.png", s.capture_count, s.selected));
                        s.capture_count += 1;
                        let path = path.to_string_lossy().into_owned();
                        let view = s.view.read(cx);
                        let metadata = serde_json::json!({
                            "state": s.selected, "phase": format!("{:?}",s.state.phase),
                            "threadId": s.state.thread_id, "turnId": s.state.turn_id,
                            "expanded": view.expanded, "detailsExpanded": view.details_expanded,
                            "selection": [view.selection.start, view.selection.end],
                            "viewport": [f32::from(window.viewport_size().width), f32::from(window.viewport_size().height)],
                            "dpr": window.scale_factor(),
                        });
                        std::fs::write(format!("{path}.render.json"),serde_json::to_vec_pretty(&metadata).expect("encode capture state")).expect("write capture state");
                        window.on_next_frame(move |window, _| {
                            crate::save_screenshot(window, &path).expect("save review capture");
                            println!("{path}");
                        });
                        window.refresh();
                    }
                    cx.stop_propagation();
                    return;
                }
                let state = match e.keystroke.key.as_str() {
                    "1" => "inProgress",
                    "2" => "approved",
                    "3" => "denied",
                    "4" => "timedOut",
                    "5" => "aborted",
                    "6" => "strict",
                    "7" => "warning",
                    _ => return,
                };
                s.focus.focus(window,cx);
                s.select(state, cx);
                cx.stop_propagation();
            }))
            .child(
                div()
                    .id("auto-approval-capture-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .pt(px(120.))
                    .px(px(336.))
                    .pb(px(40.))
                    .child(content),
            )
    }
}

pub(crate) fn capture_auto_approval(args: &[String]) -> bool {
    let Some(selected) = args
        .iter()
        .find_map(|a| a.strip_prefix("--auto-approval-ui-state="))
        .map(str::to_owned)
    else {
        return false;
    };
    let arg = |prefix: &str| args.iter().find_map(|a| a.strip_prefix(prefix));
    let mode = ThemeMode::from_name(arg("--theme=").unwrap_or("dark"));
    let width = arg("--window-width=")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1470.);
    let height = arg("--window-height=")
        .and_then(|v| v.parse().ok())
        .unwrap_or(923.);
    let output = arg("--screenshot=").map(str::to_owned);
    let output_dir = arg("--auto-approval-capture-dir=").map(std::path::PathBuf::from);
    let motion_output = arg("--auto-approval-motion-output=").map(std::path::PathBuf::from);
    let reduce_motion = args.iter().any(|arg| arg == "--reduce-motion");
    let expanded = args.iter().any(|arg| arg == "--auto-approval-expanded");
    let details_expanded = args
        .iter()
        .any(|arg| arg == "--auto-approval-details-expanded");
    let rationale = arg("--auto-approval-rationale-file=")
        .map(|p| std::fs::read_to_string(p).expect("read review rationale"))
        .unwrap_or_else(|| "This action only reads public information from example.com.".into());
    crate::typography::configure();
    gpui_platform::application()
        .with_assets(crate::Assets {
            base: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"),
        })
        .run(move |cx| {
            crate::typography::initialize_fonts(cx);
            init(cx);
            cx.set_reduce_motion(reduce_motion);
            cx.set_window_appearance(Some(match mode {
                ThemeMode::Light => gpui::WindowAppearance::Light,
                ThemeMode::Dark => gpui::WindowAppearance::Dark,
            }));
            let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("GPUI Capture".into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    if let Some(output) = output {
                        crate::schedule_screenshot(window, output, 4);
                    }
                    cx.new(|cx| {
                        let model = AutoApprovalReviewPresentation {
                            review: review(Status::InProgress, rationale.clone()),
                            closed_locally: false,
                            attached_to_item: false,
                        };
                        let view = cx.new(|cx| AutoApprovalReviewView::new(model, mode, cx));
                        let focus = cx.focus_handle();
                        focus.focus(window, cx);
                        let mut capture = Capture {
                            state: ConversationState::default(),
                            view,
                            mode,
                            selected: String::new(),
                            rationale,
                            focus,
                            output_dir,
                            capture_count: 0,
                        };
                        capture.select(&selected, cx);
                        capture.view.update(cx, |view, cx| {
                            view.expanded = expanded;
                            view.details_expanded = details_expanded;
                            cx.notify();
                        });
                        if let Some(path) = motion_output {
                            if let Some(parent) = path.parent() { std::fs::create_dir_all(parent).expect("create motion capture directory"); }
                            let weak = capture.view.downgrade();
                            let records = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
                            let start = std::time::Instant::now();
                            capture.view.update(cx, |view,_| view.on_change(UiCallback::new(move |(),window,_| {
                                let weak=weak.clone();let records=records.clone();let path=path.clone();
                                window.on_next_frame(move |window,cx| {
                                    if let Some(view)=weak.upgrade() {
                                        let view=view.read(cx);
                                        records.borrow_mut().push(serde_json::json!({
                                            "elapsedMs":start.elapsed().as_secs_f64()*1000.,
                                            "actionTarget":view.expanded,"detailsTarget":view.details_expanded,
                                            "actionProgress":view.action_transition.value,"detailsProgress":view.details_transition.value,
                                            "renderedHeight":view.capture_height.get(),"width":view.content_width.get(),"dpr":window.scale_factor(),
                                        }));
                                        std::fs::write(&path,serde_json::to_vec_pretty(&*records.borrow()).expect("encode motion samples")).expect("write motion samples");
                                    }
                                });
                            })));
                        }
                        capture
                    })
                },
            )
            .expect("open automatic-review capture");
            cx.activate(true);
        });
    true
}
