//! Native automatic-review disclosure, measured from the desktop ChatGPT component.

use gpui::{
    BoxShadow, ClipboardItem, Context, Div, FocusHandle, HighlightStyle, IntoElement, MouseButton,
    Render, Role, SharedString, Stateful, StyledText, Transformation, Window, div, prelude::*, px,
    radians, rgba,
};
use unicode_segmentation::UnicodeSegmentation;

use super::{callback::UiCallback, icons::icon};
mod disclosure;
mod shimmer;

gpui::actions!(auto_approval, [CopyReviewText, SelectAllReviewText]);

pub(crate) fn init(cx: &mut gpui::App) {
    cx.bind_keys([
        gpui::KeyBinding::new("cmd-c", CopyReviewText, Some("AutoApprovalReviewText")),
        gpui::KeyBinding::new("cmd-a", SelectAllReviewText, Some("AutoApprovalReviewText")),
    ]);
}
use crate::{
    agent::{
        AgentAutoApprovalReviewAction as Action, AgentAutoApprovalReviewStatus as Status,
        AgentGuardianWarning,
    },
    conversation::{AutoApprovalReviewPresentation, StrictReviewPresentation},
    theme::{Theme, ThemeMode, ui_font},
};

#[cfg(feature = "screenshot")]
mod capture;
#[cfg(feature = "screenshot")]
pub(crate) use capture::capture_auto_approval;
#[cfg(test)]
mod tests;

pub(crate) fn action_label(action: &Action) -> String {
    match action {
        Action::Command { command, .. } => command.clone(),
        Action::Execve { program, argv, .. } => std::iter::once(program.as_str())
            .chain(argv.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" "),
        Action::WriteStdin {
            process_id, stdin, ..
        } => format!("向进程 {process_id} 发送输入：{stdin}"),
        Action::ApplyPatch { files, .. } => {
            if files.len() == 1 {
                format!("正在编辑 {}", files[0])
            } else {
                format!("正在编辑 {} 个文件", files.len())
            }
        }
        Action::NetworkAccess { target, .. } => format!("通过网络访问 {target}"),
        Action::McpToolCall {
            tool_name,
            connector_name,
            server,
            ..
        } => format!(
            "{} 上的 MCP {}",
            connector_name.as_deref().unwrap_or(server),
            tool_name
        ),
        Action::RequestPermissions { reason, .. } => reason.as_ref().map_or_else(
            || "权限请求".to_owned(),
            |reason| format!("权限请求：{reason}"),
        ),
    }
}

pub(crate) fn status_label(status: Status) -> &'static str {
    match status {
        Status::InProgress => "自动审核中",
        Status::Approved => "自动审核已批准",
        Status::Denied => "需要明确授权",
        Status::TimedOut => "自动审核超时",
        Status::Aborted => "自动审核已停止",
    }
}

fn level_label(level: Option<&str>) -> &str {
    match level {
        Some("low") => "低",
        Some("medium") => "中",
        Some("high") => "高",
        Some("critical") => "严重",
        _ => "未知",
    }
}

fn time_label(time: Option<i64>) -> String {
    time.and_then(chrono::DateTime::from_timestamp_millis)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S%.3f")
                .to_string()
        })
        .unwrap_or_else(|| "未记录".to_owned())
}

fn fallback_explanation(status: Status) -> &'static str {
    match status {
        Status::InProgress => "经过精心提示的审查智能体正在审查此请求，随后 ChatGPT 才会运行它",
        Status::TimedOut => "经过精心提示的审查智能体在 ChatGPT 运行此请求前已超时。",
        Status::Aborted => "经过精心提示的审查智能体在 ChatGPT 运行此请求前已停止审查此请求",
        _ => "经优化提示的审查智能体已审查此请求。",
    }
}

pub(crate) fn navigate_tab(
    event: &gpui::KeyDownEvent,
    window: &mut Window,
    cx: &mut gpui::App,
) -> bool {
    let stroke = &event.keystroke;
    if stroke.key != "tab"
        || stroke.modifiers.control
        || stroke.modifiers.alt
        || stroke.modifiers.platform
        || stroke.modifiers.function
    {
        return false;
    }
    if stroke.modifiers.shift {
        window.focus_prev(cx);
    } else {
        window.focus_next(cx);
    }
    cx.stop_propagation();
    true
}

pub(crate) struct AutoApprovalReviewView {
    model: AutoApprovalReviewPresentation,
    mode: ThemeMode,
    pub(crate) expanded: bool,
    pub(crate) details_expanded: bool,
    pub(crate) attached: bool,
    selection: std::ops::Range<usize>,
    anchor: usize,
    selecting: bool,
    text_focus: FocusHandle,
    action_focus: FocusHandle,
    details_focus: FocusHandle,
    on_change: Option<UiCallback<()>>,
    action_transition: disclosure::DisclosureTransition,
    details_transition: disclosure::DisclosureTransition,
    content_width: std::rc::Rc<std::cell::Cell<f32>>,
    #[cfg(feature = "screenshot")]
    capture_height: std::rc::Rc<std::cell::Cell<f32>>,
}

impl AutoApprovalReviewView {
    pub(crate) fn new(
        model: AutoApprovalReviewPresentation,
        mode: ThemeMode,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            model,
            mode,
            expanded: false,
            details_expanded: false,
            attached: false,
            selection: 0..0,
            anchor: 0,
            selecting: false,
            text_focus: cx.focus_handle(),
            action_focus: cx.focus_handle().tab_stop(true),
            details_focus: cx.focus_handle().tab_stop(true),
            on_change: None,
            action_transition: Default::default(),
            details_transition: Default::default(),
            content_width: std::rc::Rc::new(std::cell::Cell::new(711.586)),
            #[cfg(feature = "screenshot")]
            capture_height: std::rc::Rc::new(std::cell::Cell::new(0.)),
        }
    }

    pub(crate) fn on_change(&mut self, callback: UiCallback<()>) {
        self.on_change = Some(callback);
    }

    pub(crate) fn sync(
        &mut self,
        model: AutoApprovalReviewPresentation,
        mode: ThemeMode,
        cx: &mut Context<Self>,
    ) {
        self.mode = mode;
        self.attached = model.attached_to_item;
        if self.model != model {
            if self.model.review.rationale != model.review.rationale
                || self.model.status() != model.status()
            {
                self.selection = 0..0;
            }
            self.model = model;
            cx.notify();
        }
    }

    fn toggle(&mut self, details: bool, window: &mut Window, cx: &mut Context<Self>) {
        if details {
            self.details_expanded = !self.details_expanded;
        } else {
            self.expanded = !self.expanded;
            if !self.expanded && cx.reduce_motion() {
                // The browser unmounts the nested disclosure when its parent closes.
                self.details_expanded = false;
                self.selection = 0..0;
            }
        }
        let now = std::time::Instant::now();
        self.action_transition.sample(
            self.expanded || self.attached,
            now,
            cx.reduce_motion() || self.attached,
        );
        self.details_transition
            .sample(self.details_expanded, now, cx.reduce_motion());
        self.selecting = false;
        if let Some(callback) = &self.on_change {
            callback.emit((), window, cx);
        }
        cx.notify();
    }

    fn header(
        &self,
        label: String,
        details: bool,
        theme: Theme,
        keyboard_focus: bool,
        progress: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let expanded = if details {
            self.details_expanded
        } else {
            self.expanded
        };
        let group: SharedString = if details {
            "auto-review-detail"
        } else {
            "auto-review-action"
        }
        .into();
        let status = self.model.status();
        let focus = if details {
            self.details_focus.clone()
        } else {
            self.action_focus.clone()
        }
        .tab_stop(!details || self.expanded || self.attached);
        let color = if details {
            theme.markdown_text.alpha(0.398_431)
        } else {
            theme.markdown_text.alpha(0.30)
        };
        div()
            .id(if details {
                "review-details-toggle"
            } else {
                "review-action-toggle"
            })
            .group(group.clone())
            .min_w(px(0.))
            .max_w_full()
            .h(px(21.))
            .flex()
            .items_center()
            .gap(px(if details { 6. } else { 4. }))
            .rounded(px(if details { 0. } else { 10. }))
            .track_focus(&focus)
            .tab_stop(true)
            .role(Role::Button)
            .aria_label(label.clone())
            .aria_expanded(expanded)
            .aria_description(format!(
                "{}；风险：{}；用户授权：{}；开始时间：{}；完成时间：{}",
                status_label(status),
                level_label(self.model.review.risk_level.as_deref()),
                level_label(self.model.review.user_authorization.as_deref()),
                time_label(Some(self.model.review.started_at_ms)),
                time_label(self.model.review.completed_at_ms)
            ))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                focus.focus(window, cx)
            })
            .focus_visible(move |s| {
                if details {
                    s.shadow(vec![
                        BoxShadow::new(px(0.), px(0.), rgba(0x99c8ffff).into())
                            .spread_radius(px(1.))
                            .inset(),
                    ])
                } else {
                    s.px(px(2.)).shadow(vec![
                        BoxShadow::new(px(0.), px(0.), rgba(0x3a83f7ff).into())
                            .spread_radius(px(2.))
                            .inset(),
                    ])
                }
            })
            .on_click(cx.listener(move |s, _, w, cx| s.toggle(details, w, cx)))
            .capture_key_down(cx.listener(move |s, e: &gpui::KeyDownEvent, w, cx| {
                if e.keystroke.key == "enter" && !e.keystroke.modifiers.modified() {
                    // GPUI also synthesizes a click on keyup. Prevent its
                    // activation here because browser buttons activate Enter on keydown.
                    w.prevent_default();
                    if e.is_held {
                        cx.stop_propagation();
                        return;
                    }
                    s.toggle(details, w, cx);
                    cx.stop_propagation();
                }
            }))
            .on_key_down(cx.listener(move |_, e: &gpui::KeyDownEvent, window, cx| {
                if navigate_tab(e, window, cx) {
                    return;
                }
                // Keep Space activation in GPUI's focus-aware keyup click path.
                if e.keystroke.key == "space" {
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .min_w(px(0.))
                    .gap(px(6.))
                    .when(!details, |d| {
                        d.child(
                            icon(
                                "auto-review-action",
                                if status == Status::Denied {
                                    theme.warning.into()
                                } else {
                                    theme.markdown_text.alpha(0.50).into()
                                },
                            )
                            .size(px(16.))
                            .flex_none(),
                        )
                    })
                    .child(if status == Status::InProgress {
                        shimmer::label(
                            label.split_whitespace().collect::<Vec<_>>().join(" "),
                            theme,
                        )
                        .into_any_element()
                    } else {
                        div()
                            // CoreText's 21 px line box differs by half a logical
                            // pixel from this browser button's measured baseline.
                            .relative()
                            .top(px(-0.25))
                            .min_w(px(0.))
                            .truncate()
                            .text_color(color)
                            .group_hover(group.clone(), move |s| s.text_color(theme.markdown_text))
                            .child(label)
                            .into_any_element()
                    }),
            )
            .child(
                icon(
                    "auto-review-chevron",
                    theme.markdown_text.alpha(0.50).into(),
                )
                .size(px(14.))
                .flex_none()
                .opacity(
                    if expanded || progress > 0. || (!details && keyboard_focus) {
                        1.
                    } else {
                        0.
                    },
                )
                .group_hover(group, |s| s.opacity(1.))
                .with_transformation(Transformation::rotate(radians(
                    std::f32::consts::FRAC_PI_2 * progress,
                ))),
            )
    }

    fn explanation(&mut self, text: String, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        if !text.is_char_boundary(self.selection.start)
            || !text.is_char_boundary(self.selection.end)
        {
            self.selection = 0..0;
        }
        let selected = self.selection.clone();
        let styled =
            StyledText::new(text.clone()).with_highlights((!selected.is_empty()).then_some((
                selected,
                HighlightStyle {
                    background_color: Some(rgba(0x316ac5aa).into()),
                    ..Default::default()
                },
            )));
        let layout = styled.layout().clone();
        let move_layout = layout.clone();
        let key_text = text.clone();
        let copy_text = text.clone();
        let text_len = text.len();
        div()
            .id("auto-review-explanation")
            .pt(px(4.))
            .max_w(px(711.586))
            .w_full()
            .font(ui_font())
            .text_size(px(14.))
            .line_height(px(22.75))
            .text_color(theme.markdown_text.alpha(0.60))
            .track_focus(&self.text_focus)
            .key_context("AutoApprovalReviewText")
            .role(Role::Label)
            .aria_label(text.clone())
            .cursor(gpui::CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |s, e: &gpui::MouseDownEvent, w, cx| {
                    let index = layout
                        .index_for_position(e.position)
                        .unwrap_or_else(|i| i)
                        .min(text.len());
                    s.text_focus.focus(w, cx);
                    if !e.modifiers.shift {
                        s.anchor = index;
                    }
                    s.selection = s.anchor.min(index)..s.anchor.max(index);
                    if e.click_count == 2 {
                        if let Some((start, word)) = text
                            .split_word_bound_indices()
                            .find(|(start, word)| index >= *start && index < start + word.len())
                        {
                            s.anchor = start;
                            s.selection = start..start + word.len();
                        }
                    } else if e.click_count >= 3 {
                        s.anchor = 0;
                        s.selection = 0..text.len();
                    }
                    s.selecting = true;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(move |s, e: &gpui::MouseMoveEvent, _, cx| {
                if s.selecting && e.pressed_button == Some(MouseButton::Left) {
                    let index = move_layout
                        .index_for_position(e.position)
                        .unwrap_or_else(|i| i);
                    s.selection = s.anchor.min(index)..s.anchor.max(index);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|s, _, _, cx| {
                    s.selecting = false;
                    cx.notify();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|s, _, _, _| s.selecting = false),
            )
            .on_action(cx.listener(move |s, _: &CopyReviewText, _, cx| {
                if let Some(selected) = copy_text.get(s.selection.clone()).filter(|s| !s.is_empty())
                {
                    cx.write_to_clipboard(ClipboardItem::new_string(selected.to_owned()));
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(move |s, _: &SelectAllReviewText, _, cx| {
                s.anchor = 0;
                s.selection = 0..text_len;
                s.selecting = false;
                cx.stop_propagation();
                cx.notify();
            }))
            .on_key_down(cx.listener(move |s, e: &gpui::KeyDownEvent, window, cx| {
                if navigate_tab(e, window, cx) {
                    return;
                }
                if e.keystroke.modifiers.platform && e.keystroke.key == "c" {
                    if let Some(selected) =
                        key_text.get(s.selection.clone()).filter(|s| !s.is_empty())
                    {
                        cx.write_to_clipboard(ClipboardItem::new_string(selected.to_owned()));
                    }
                    cx.stop_propagation();
                } else if e.keystroke.modifiers.platform && e.keystroke.key == "a" {
                    s.anchor = 0;
                    s.selection = 0..key_text.len();
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .child(div().relative().top(px(0.25)).child(styled))
    }
}

fn explanation_height(text: &str, width: f32, theme: Theme, window: &Window) -> f32 {
    let run = gpui::TextRun {
        len: text.len(),
        font: ui_font(),
        color: theme.markdown_text.into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_text(
            text.to_owned().into(),
            px(14.),
            &[run],
            Some(px(width.clamp(1., 711.586))),
            None,
        )
        .map(|lines| {
            lines
                .iter()
                .map(|line| f32::from(line.size(px(22.75)).height))
                .sum::<f32>()
        })
        .unwrap_or(22.75)
        + 4.
}

impl Render for AutoApprovalReviewView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_mode(self.mode);
        let status = self.model.status();
        let now = std::time::Instant::now();
        let (action_progress, action_animating) = self.action_transition.sample(
            self.expanded || self.attached,
            now,
            cx.reduce_motion() || self.attached,
        );
        if action_progress == 0. && !self.expanded && !self.attached {
            self.details_expanded = false;
            self.details_transition.sample(false, now, true);
            self.selection = 0..0;
        }
        let (details_progress, details_animating) =
            self.details_transition
                .sample(self.details_expanded, now, cx.reduce_motion());
        if action_animating || details_animating {
            window.request_animation_frame();
            if let Some(callback) = self.on_change.clone() {
                // List measurement cannot be invalidated reentrantly while a
                // virtual row is being laid out. Update it before the next draw.
                window.on_next_frame(move |window, cx| callback.emit((), window, cx));
            }
        }
        let action_keyboard_focus =
            window.last_input_was_keyboard() && self.action_focus.is_focused(window);
        let details_keyboard_focus =
            window.last_input_was_keyboard() && self.details_focus.is_focused(window);
        let width_state = self.content_width.clone();
        #[cfg(feature = "screenshot")]
        let capture_height = self.capture_height.clone();
        let view = cx.weak_entity();
        let mut result = div()
            .relative()
            .w_full()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .items_start()
            .font(ui_font())
            .text_size(px(14.))
            .line_height(px(21.));
        if !self.attached {
            result = result.child(self.header(
                action_label(&self.model.review.action),
                false,
                theme,
                action_keyboard_focus,
                action_progress,
                cx,
            ));
        }
        if action_progress > 0. || self.expanded || self.attached {
            let text = if status == Status::Denied {
                if self.model.review.risk_level.as_deref() == Some("high") {
                    "此操作被视为高风险，需要明确授权"
                } else {
                    "需要明确授权"
                }
                .to_owned()
            } else {
                self.model
                    .review
                    .rationale
                    .clone()
                    .filter(|text| !text.trim().is_empty())
                    .unwrap_or_else(|| fallback_explanation(status).to_owned())
            };
            let detail_height = explanation_height(&text, self.content_width.get(), theme, window);
            let body_height = if status == Status::Denied {
                detail_height
            } else {
                25. + detail_height * details_progress
            };
            let mut body = div()
                .w_full()
                .min_w(px(0.))
                .flex_none()
                .flex()
                .flex_col()
                .items_start();
            if status == Status::Denied {
                body = body.child(self.explanation(text, theme, cx).flex_none());
            } else {
                body = body.child(div().pt(px(4.)).flex_none().child(self.header(
                    status_label(status).to_owned(),
                    true,
                    theme,
                    details_keyboard_focus,
                    details_progress,
                    cx,
                )));
                if details_progress > 0. || self.details_expanded {
                    let interactive = self.details_expanded;
                    body = body.child(
                        div()
                            .id("review-animated-explanation")
                            .w_full()
                            .h(px(detail_height * details_progress))
                            .flex_none()
                            .overflow_hidden()
                            .opacity(details_progress)
                            .capture_any_mouse_down(move |_, _, cx| {
                                if !interactive {
                                    cx.stop_propagation();
                                }
                            })
                            .child(self.explanation(text, theme, cx).flex_none()),
                    );
                }
            }
            let interactive = self.expanded || self.attached;
            result = result.child(
                div()
                    .id("review-animated-body")
                    .w_full()
                    .h(px(body_height * action_progress))
                    .flex_none()
                    .overflow_hidden()
                    .opacity(action_progress)
                    .capture_any_mouse_down(move |_, _, cx| {
                        if !interactive {
                            cx.stop_propagation();
                        }
                    })
                    .child(body),
            );
        }
        result.child(
            gpui::canvas(
                move |bounds, window, _| {
                    #[cfg(feature = "screenshot")]
                    capture_height.set(f32::from(bounds.size.height));
                    let width = f32::from(bounds.size.width);
                    if (width_state.replace(width) - width).abs() > 0.1 {
                        window.on_next_frame(move |_, cx| {
                            let _ = view.update(cx, |_, cx| cx.notify());
                        });
                    }
                },
                |_, (), _, _| {},
            )
            .absolute()
            .inset_0(),
        )
    }
}

pub(crate) fn strict_review(requirement: &StrictReviewPresentation, theme: Theme) -> Stateful<Div> {
    let label = "此请求需要额外的安全检查，可能需要更多时间。";
    div()
        .id(SharedString::from(format!(
            "strict-review-{:?}",
            requirement.requirement
        )))
        .role(Role::Status)
        .aria_label(label)
        .w_full()
        .flex()
        .items_start()
        .gap(px(6.))
        .font(ui_font())
        .text_size(px(14.))
        .line_height(px(21.))
        .text_color(theme.markdown_text.alpha(0.60))
        .child(
            icon("auto-review-shield", theme.markdown_text.alpha(0.60).into())
                .size(px(16.))
                .mt(px(2.5))
                .flex_none(),
        )
        .child(div().min_w(px(0.)).relative().top(px(-0.25)).child(label))
}

pub(crate) fn guardian_warning(warning: &AgentGuardianWarning, theme: Theme) -> Stateful<Div> {
    let interrupted = warning
        .message
        .starts_with("Automatic approval review rejected too many approval requests for this turn");
    let message = if interrupted {
        "本轮操作已被自动审查终止"
    } else {
        &warning.message
    };
    div()
        .id("guardian-warning")
        .role(Role::Status)
        .aria_label(message)
        .aria_description(warning.message.clone())
        .w_full()
        .flex()
        .items_center()
        .gap(px(8.))
        .font(ui_font())
        .text_size(px(14.))
        .line_height(px(21.))
        .text_color(theme.markdown_text.alpha(0.65))
        .when(interrupted, |row| {
            row.child(div().flex_1().h(px(1.)).bg(theme.border))
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .min_w(px(0.))
                .max_w_full()
                .when(interrupted, |group| group.flex_none())
                .when(!interrupted, |group| group.flex_1())
                .child(
                    icon("auto-review-shield", theme.markdown_text.alpha(0.65).into())
                        .size(px(16.))
                        .flex_none(),
                )
                .child(
                    div()
                        .min_w(px(0.))
                        .when(!interrupted, |text| text.flex_1())
                        .relative()
                        .top(px(-0.25))
                        .child(message.to_owned()),
                )
                .when(interrupted, |group| group.child(warning_info(theme))),
        )
        .when(interrupted, |row| {
            row.child(div().flex_1().h(px(1.)).bg(theme.border))
        })
}

fn warning_info(theme: Theme) -> Stateful<Div> {
    div()
        .id("auto-review-warning-info")
        .relative()
        .size(px(14.))
        .role(Role::Label)
        .aria_label("自动审查停止说明")
        .on_hover(|_, window, _| window.refresh())
        .child(icon("auto-review-info", theme.markdown_text.alpha(0.65).into()).size(px(14.)))
        .child(
            gpui::canvas(
                move |bounds, window, cx| {
                    if !bounds.contains(&window.mouse_position()) {
                        return;
                    }
                    // Match the desktop hint above the icon. GPUI's general tooltip
                    // defaults to the bottom-right of the mouse pointer.
                    let center = bounds.center();
                    let width = 274.;
                    let height = 3. * (130. / 7.) + 14.;
                    let x = (f32::from(center.x) - width / 2.)
                        .clamp(
                            8.,
                            (f32::from(window.viewport_size().width) - width - 8.).max(8.),
                        )
                        .floor();
                    let above = f32::from(center.y) - height - 8.;
                    let y = if above >= 8. {
                        above
                    } else {
                        f32::from(center.y) + 8.
                    }
                    .floor();
                    window.set_tooltip(gpui::AnyTooltip {
                        view: cx.new(|_| WarningTooltip { theme }).into(),
                        mouse_position: gpui::point(px(x - 1.), px(y - 1.)),
                        check_visible_and_update: std::rc::Rc::new(move |_, window, _| {
                            bounds.contains(&window.mouse_position())
                        }),
                    });
                },
                |_, (), _, _| {},
            )
            .absolute()
            .inset_0(),
        )
}

struct WarningTooltip {
    theme: Theme,
}
impl Render for WarningTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("auto-review-warning-tooltip")
            .role(Role::Tooltip)
            .w(px(274.))
            .px(px(8.))
            .py(px(6.))
            .rounded(px(20.))
            .bg(self.theme.control)
            .border(px(1.))
            .border_color(self.theme.border)
            .font(ui_font())
            .text_size(px(13.))
            .line_height(px(130. / 7.))
            .text_color(self.theme.markdown_text)
            .text_center()
            .child(
                "因多次被驳回，自动审查已停止本轮操作。请添加更多上下文或选择其他权限模式以继续。",
            )
    }
}
