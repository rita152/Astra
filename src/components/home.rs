use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use gpui::{
    App, Bounds, ContentMask, Context, Div, Entity, FontWeight, MouseButton, PathBuilder, Pixels,
    Render, ScrollHandle, ShapedLine, SharedString, TextAlign, TextRun, Transformation, Window,
    canvas, div, point, prelude::*, px, radians, relative, rgba,
};

use crate::{
    agent::{CommandExecution, CommandExecutionStatus},
    components::{
        composer::{
            ComposerView, ConversationActivity, ConversationChanged, ConversationPhase,
            RequestFullAccess,
        },
        icons::{icon, suggestion_icon},
    },
    theme::{Theme, ThemeMode, UI_MONOSPACE_FONT_FAMILY},
};

pub struct HomeView {
    mode: ThemeMode,
    composer: Entity<ComposerView>,
    suggestion_scale: [f32; 2],
    suggestion_animation_from: [f32; 2],
    suggestion_animation_to: [f32; 2],
    suggestion_animation_started_at: [Option<Instant>; 2],
    suggestion_animation_duration: [Duration; 2],
    suggestion_animation_running: bool,
    thinking_shimmer_progress: f32,
    thinking_shimmer_cycle: u64,
    thinking_shimmer_running: bool,
    response_feedback: i8,
    user_message_actions_visible_for_capture: bool,
    expanded_commands: HashSet<String>,
    command_scroll_handles: HashMap<String, ScrollHandle>,
}

impl gpui::EventEmitter<RequestFullAccess> for HomeView {}

const SUGGESTION_PRESSED_SCALE: f32 = 0.99;
const SUGGESTION_TRANSITION_DURATION: Duration = Duration::from_millis(150);
const THINKING_SHIMMER_DURATION: Duration = Duration::from_secs(1);
const THINKING_SHIMMER_STEPS: f32 = 48.0;
const THINKING_SHIMMER_FRAME_INTERVAL: Duration = Duration::from_micros(20_833);
const THINKING_SHIMMER_WIDTH: f32 = 56.0;
const THINKING_SHIMMER_BAND_SCALE: f32 = 0.5;
const THINKING_SHIMMER_ALPHA_LEVELS: usize = 32;
const USER_MESSAGE_BUBBLE_RADIUS: f32 = 22.0;
const USER_MESSAGE_BUBBLE_SUPERELLIPSE: f32 = 1.5;
const USER_MESSAGE_FOOTER_OFFSET: f32 = 3.0;
const USER_MESSAGE_FOOTER_HEIGHT: f32 = 26.0;
const USER_MESSAGE_FOOTER_SIDE_MARGIN: f32 = 4.0;
const USER_MESSAGE_FOOTER_GAP: f32 = 8.0;
const USER_MESSAGE_TIME_SIZE: f32 = 12.0;
const USER_MESSAGE_TIME_LINE_HEIGHT: f32 = 16.0;
const RESPONSE_ACTION_ICON_SIZE: f32 = 16.0;
const RESPONSE_ACTION_FOOTER_OFFSET: f32 = 6.0;
const RESPONSE_ACTION_FOOTER_HEIGHT: f32 = 20.0;
const RESPONSE_ACTION_FOOTER_ELECTRON_SHIFT: f32 = -4.0;
const RESPONSE_ACTION_GAP: f32 = 2.0;
const RESPONSE_TIME_MARGIN: f32 = 6.0;
const RESPONSE_TIME_SIZE: f32 = 12.0;
const RESPONSE_TIME_LINE_HEIGHT: f32 = 16.0;

fn superellipse_corner_points(
    center_x: f32,
    center_y: f32,
    radius: f32,
    start_angle: f32,
) -> impl Iterator<Item = gpui::Point<Pixels>> {
    const SEGMENTS: usize = 12;
    let exponent = 2.0_f32.powf(USER_MESSAGE_BUBBLE_SUPERELLIPSE);
    (1..=SEGMENTS).map(move |step| {
        let angle = start_angle + std::f32::consts::FRAC_PI_2 * step as f32 / SEGMENTS as f32;
        let cosine = angle.cos();
        let sine = angle.sin();
        point(
            px(center_x + cosine.signum() * cosine.abs().powf(2.0 / exponent) * radius),
            px(center_y + sine.signum() * sine.abs().powf(2.0 / exponent) * radius),
        )
    })
}

fn user_message_bubble_path(bounds: Bounds<Pixels>) -> gpui::Path<Pixels> {
    let left = f32::from(bounds.left());
    let top = f32::from(bounds.top());
    let right = f32::from(bounds.right());
    let bottom = f32::from(bounds.bottom());
    let radius = USER_MESSAGE_BUBBLE_RADIUS
        .min((right - left) * 0.5)
        .min((bottom - top) * 0.5);
    let mut builder = PathBuilder::fill();
    builder.move_to(point(px(left + radius), px(top)));
    builder.line_to(point(px(right - radius), px(top)));
    for point in superellipse_corner_points(
        right - radius,
        top + radius,
        radius,
        -std::f32::consts::FRAC_PI_2,
    ) {
        builder.line_to(point);
    }
    builder.line_to(point(px(right), px(bottom - radius)));
    for point in superellipse_corner_points(right - radius, bottom - radius, radius, 0.0) {
        builder.line_to(point);
    }
    builder.line_to(point(px(left + radius), px(bottom)));
    for point in superellipse_corner_points(
        left + radius,
        bottom - radius,
        radius,
        std::f32::consts::FRAC_PI_2,
    ) {
        builder.line_to(point);
    }
    builder.line_to(point(px(left), px(top + radius)));
    for point in
        superellipse_corner_points(left + radius, top + radius, radius, std::f32::consts::PI)
    {
        builder.line_to(point);
    }
    builder.close();
    builder
        .build()
        .expect("user message superellipse should tessellate")
}

fn thinking_shimmer_step(progress: f32) -> f32 {
    (progress.clamp(0.0, 1.0) * THINKING_SHIMMER_STEPS).floor() / THINKING_SHIMMER_STEPS
}

fn thinking_shimmer_progress(elapsed: Duration) -> f32 {
    (elapsed.as_secs_f32() / THINKING_SHIMMER_DURATION.as_secs_f32()).clamp(0.0, 1.0)
}

fn thinking_shimmer_band_left(progress: f32, text_width: f32) -> f32 {
    // CSS background-position percentages are relative to the remaining width.
    // With a 50%-wide image, -100%..250% resolves to -0.5w..1.25w.
    let remaining_width = text_width * (1.0 - THINKING_SHIMMER_BAND_SCALE);
    remaining_width * (-1.0 + 3.5 * thinking_shimmer_step(progress))
}

fn thinking_shimmer_alpha(position: f32) -> f32 {
    let position = position.clamp(0.0, 1.0);
    if position < 0.4 {
        position / 0.4 * 0.75
    } else if position <= 0.6 {
        0.75
    } else {
        (1.0 - position) / 0.4 * 0.75
    }
}

fn suggestion_transition_ease(progress: f32) -> f32 {
    fn bezier(t: f32, first: f32, second: f32) -> f32 {
        let inverse = 1.0 - t;
        3.0 * inverse * inverse * t * first + 3.0 * inverse * t * t * second + t * t * t
    }

    let progress = progress.clamp(0.0, 1.0);
    if progress == 0.0 || progress == 1.0 {
        return progress;
    }

    // chat-reference: --ease-enter-snappy: cubic-bezier(.23, 1, .32, 1)
    let (mut lower, mut upper) = (0.0, 1.0);
    for _ in 0..12 {
        let parameter = (lower + upper) * 0.5;
        if bezier(parameter, 0.23, 0.32) < progress {
            lower = parameter;
        } else {
            upper = parameter;
        }
    }
    bezier((lower + upper) * 0.5, 1.0, 1.0)
}

impl HomeView {
    pub fn new(mode: ThemeMode, cx: &mut Context<Self>) -> Self {
        let composer = cx.new(|cx| ComposerView::new(mode, cx));
        cx.subscribe(&composer, |_, _, _: &RequestFullAccess, cx| {
            cx.emit(RequestFullAccess);
        })
        .detach();
        cx.subscribe(&composer, |this, composer, _: &ConversationChanged, cx| {
            let phase = composer.read(cx).conversation_phase();
            this.sync_thinking_shimmer(phase, cx);
            cx.notify();
        })
        .detach();
        Self {
            mode,
            composer,
            suggestion_scale: [1.0; 2],
            suggestion_animation_from: [1.0; 2],
            suggestion_animation_to: [1.0; 2],
            suggestion_animation_started_at: [None; 2],
            suggestion_animation_duration: [Duration::ZERO; 2],
            suggestion_animation_running: false,
            thinking_shimmer_progress: 0.0,
            thinking_shimmer_cycle: 0,
            thinking_shimmer_running: false,
            response_feedback: 0,
            user_message_actions_visible_for_capture: false,
            expanded_commands: HashSet::new(),
            command_scroll_handles: HashMap::new(),
        }
    }

    fn sync_thinking_shimmer(&mut self, phase: ConversationPhase, cx: &mut Context<Self>) {
        if phase == ConversationPhase::Thinking {
            if !self.thinking_shimmer_running {
                self.start_thinking_shimmer(cx);
            }
        } else if self.thinking_shimmer_running || self.thinking_shimmer_progress != 0.0 {
            self.thinking_shimmer_cycle = self.thinking_shimmer_cycle.wrapping_add(1);
            self.thinking_shimmer_progress = 0.0;
            self.thinking_shimmer_running = false;
        }
    }

    fn start_thinking_shimmer(&mut self, cx: &mut Context<Self>) {
        self.thinking_shimmer_cycle = self.thinking_shimmer_cycle.wrapping_add(1);
        let cycle = self.thinking_shimmer_cycle;
        self.thinking_shimmer_progress = 0.0;
        self.thinking_shimmer_running = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let mut step = 1usize;
            loop {
                cx.background_executor()
                    .timer(THINKING_SHIMMER_FRAME_INTERVAL)
                    .await;
                let should_continue = this
                    .update(cx, |this, cx| {
                        if this.thinking_shimmer_cycle != cycle || !this.thinking_shimmer_running {
                            return false;
                        }
                        let elapsed =
                            THINKING_SHIMMER_DURATION.mul_f32(step as f32 / THINKING_SHIMMER_STEPS);
                        this.thinking_shimmer_progress = thinking_shimmer_progress(elapsed);
                        // A scheduled frame alone reuses the previous element
                        // tree. Notify the view so the Canvas captures the new
                        // cadence step before it paints.
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !should_continue {
                    return;
                }
                step = step % THINKING_SHIMMER_STEPS as usize + 1;
            }
        })
        .detach();
    }

    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.composer
            .update(cx, |composer, cx| composer.set_mode(mode, cx));
        cx.notify();
    }

    pub fn close_model_picker(&mut self, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |composer, cx| composer.close_picker(cx));
    }

    pub fn set_permission_mode(&mut self, mode: &str, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |composer, cx| composer.set_permission_mode(mode, cx));
    }

    pub fn open_permission_menu(&mut self, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |composer, cx| composer.open_permission_menu(cx));
    }

    pub fn open_model_picker(&mut self, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |composer, cx| composer.open_picker(cx));
    }

    pub fn open_model_picker_submenu(&mut self, name: &str, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |composer, cx| composer.open_picker_submenu(name, cx));
    }

    pub fn open_model_picker_slider_at(
        &mut self,
        index: usize,
        fast: bool,
        cx: &mut Context<Self>,
    ) {
        self.composer.update(cx, |composer, cx| {
            composer.open_picker_slider_at(index, fast, cx)
        });
    }

    pub fn set_dictation_state_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.composer.update(cx, |composer, cx| {
            composer.set_dictation_state_for_capture(state, cx)
        });
    }

    pub fn submit_prompt_for_capture(&mut self, prompt: &str, cx: &mut Context<Self>) {
        self.composer.update(cx, |composer, cx| {
            composer.submit_prompt_for_capture(prompt, cx)
        });
    }

    pub fn show_user_message_actions_for_capture(&mut self, cx: &mut Context<Self>) {
        self.user_message_actions_visible_for_capture = true;
        cx.notify();
    }

    pub fn set_command_tool_for_capture(
        &mut self,
        running: bool,
        expanded: bool,
        cx: &mut Context<Self>,
    ) {
        self.expanded_commands.clear();
        if expanded {
            self.expanded_commands
                .insert("exec-command-ui-capture".to_owned());
        }
        self.composer.update(cx, |composer, cx| {
            composer.set_command_tool_for_capture(running, cx)
        });
        cx.notify();
    }

    fn set_suggestion_pressed(
        &mut self,
        index: usize,
        pressed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = if pressed {
            SUGGESTION_PRESSED_SCALE
        } else {
            1.0
        };

        if cx.reduce_motion() || (target - self.suggestion_scale[index]).abs() <= f32::EPSILON {
            self.suggestion_scale[index] = target;
            self.suggestion_animation_from[index] = target;
            self.suggestion_animation_to[index] = target;
            self.suggestion_animation_started_at[index] = None;
            self.suggestion_animation_duration[index] = Duration::ZERO;
            cx.notify();
            return;
        }

        self.suggestion_animation_from[index] = self.suggestion_scale[index];
        self.suggestion_animation_to[index] = target;
        self.suggestion_animation_started_at[index] = Some(cx.background_executor().now());
        self.suggestion_animation_duration[index] = Duration::from_secs_f32(
            SUGGESTION_TRANSITION_DURATION.as_secs_f32()
                * (target - self.suggestion_scale[index]).abs()
                / (1.0 - SUGGESTION_PRESSED_SCALE),
        );

        let was_running = self.suggestion_animation_running;
        self.suggestion_animation_running = true;
        cx.notify();
        if !was_running {
            cx.on_next_frame(window, |this, window, cx| {
                this.advance_suggestion_animations(window, cx)
            });
        }
    }

    fn advance_suggestion_animations(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.suggestion_animation_running {
            return;
        }

        let now = cx.background_executor().now();
        let mut still_running = false;
        for index in 0..self.suggestion_scale.len() {
            let Some(started_at) = self.suggestion_animation_started_at[index] else {
                continue;
            };
            let duration = self.suggestion_animation_duration[index];
            let progress = if duration.is_zero() {
                1.0
            } else {
                now.saturating_duration_since(started_at).as_secs_f32() / duration.as_secs_f32()
            }
            .clamp(0.0, 1.0);
            self.suggestion_scale[index] = self.suggestion_animation_from[index]
                + (self.suggestion_animation_to[index] - self.suggestion_animation_from[index])
                    * suggestion_transition_ease(progress);

            if progress >= 1.0 || cx.reduce_motion() {
                self.suggestion_scale[index] = self.suggestion_animation_to[index];
                self.suggestion_animation_started_at[index] = None;
            } else {
                still_running = true;
            }
        }

        self.suggestion_animation_running = still_running;
        cx.notify();
        if still_running {
            cx.on_next_frame(window, |this, window, cx| {
                this.advance_suggestion_animations(window, cx)
            });
        }
    }

    fn suggestion(
        &self,
        index: usize,
        label: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let hover_group = format!("home-suggestion-{index}");
        let scale = self.suggestion_scale[index];

        div()
            .id(("home-suggestion-hit-area", index))
            .relative()
            .top(px(-11.0))
            .left(px(7.0))
            .h(px(40.0))
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.set_suggestion_pressed(index, true, window, cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.set_suggestion_pressed(index, false, window, cx);
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.set_suggestion_pressed(index, false, window, cx);
                }),
            )
            .child(
                div()
                    .id(("home-suggestion", index))
                    .group(hover_group.clone())
                    // GPUI does not expose a transform for arbitrary elements,
                    // so scale the same geometry around a fixed 40px hit area.
                    // The hit target therefore never moves while the rendered
                    // row matches the reference's active:scale-[0.99].
                    .w(relative(scale))
                    .h(px(40.0 * scale))
                    .px(px(6.0 * scale))
                    .rounded(px(8.0 * scale))
                    .flex()
                    .items_center()
                    .gap(px(7.0 * scale))
                    .text_size(px(13.0 * scale))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .text_color(theme.text_tertiary)
                    .hover(move |style| style.text_color(theme.text))
                    .child(
                        suggestion_icon(theme.text_tertiary.into())
                            .w(px(14.0 * scale))
                            .h(px(12.0 * scale))
                            .group_hover(hover_group, move |style| style.text_color(theme.text)),
                    )
                    .child(label),
            )
    }
}

impl Render for HomeView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_mode(self.mode);
        let (
            phase,
            user_message,
            user_message_time,
            assistant_message,
            assistant_message_time,
            conversation_activity,
        ) = self.composer.read(cx).conversation_render_snapshot();
        for activity in &conversation_activity {
            if let ConversationActivity::Command(command) = activity {
                let scroll_handle = self
                    .command_scroll_handles
                    .entry(command.id.clone())
                    .or_insert_with(ScrollHandle::new);
                if command.status == CommandExecutionStatus::InProgress {
                    scroll_handle.scroll_to_bottom();
                }
            }
        }
        home(
            cx.entity(),
            theme,
            self.composer.clone(),
            phase,
            user_message,
            user_message_time,
            assistant_message,
            assistant_message_time,
            conversation_activity,
            self.thinking_shimmer_progress,
            self.response_feedback,
            self.user_message_actions_visible_for_capture,
            self.expanded_commands.clone(),
            self.command_scroll_handles.clone(),
            self.suggestion(
                0,
                "Prove plugin upgrades never mutate an active run",
                theme,
                cx,
            ),
            self.suggestion(
                1,
                "Verify the full /plugins lifecycle in the interactive terminal",
                theme,
                cx,
            ),
        )
    }
}

fn home(
    home_entity: Entity<HomeView>,
    theme: Theme,
    composer: Entity<ComposerView>,
    phase: ConversationPhase,
    user_message: Option<String>,
    user_message_time: Option<String>,
    assistant_message: String,
    assistant_message_time: Option<String>,
    conversation_activity: Vec<ConversationActivity>,
    thinking_shimmer_progress: f32,
    response_feedback: i8,
    user_message_actions_visible_for_capture: bool,
    expanded_commands: HashSet<String>,
    command_scroll_handles: HashMap<String, ScrollHandle>,
    first_suggestion: impl IntoElement,
    second_suggestion: impl IntoElement,
) -> Div {
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .relative()
        .when(phase == ConversationPhase::Empty, |root| {
            root.child(
                div()
                    .absolute()
                    // These are component boundaries, not a viewport-specific
                    // heading coordinate. GPUI centers the group in between them.
                    .top(px(46.0))
                    .bottom(px(153.0))
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(768.0))
                            .px(px(24.0))
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(12.0))
                            .child(
                                icon("home-mark", theme.home_mark.into())
                                    .size(px(56.0))
                                    .relative()
                                    .top(px(-2.0)),
                            )
                            .child(
                                div()
                                    .relative()
                                    .top(px(2.0))
                                    .left(px(-6.5))
                                    .text_size(px(29.2))
                                    .font_weight(gpui::FontWeight(350.0))
                                    .text_color(theme.text)
                                    .child("你想让我们在 coda 中构建什么？"),
                            ),
                    ),
            )
        })
        .when(phase != ConversationPhase::Empty, |root| {
            root.child(conversation(
                home_entity,
                theme,
                phase,
                user_message.unwrap_or_default(),
                user_message_time.unwrap_or_default(),
                assistant_message,
                assistant_message_time,
                conversation_activity,
                thinking_shimmer_progress,
                response_feedback,
                user_message_actions_visible_for_capture,
                expanded_commands,
                command_scroll_handles,
            ))
        })
        .child(
            div()
                .absolute()
                .bottom(px(15.0))
                .w_full()
                .max_w(px(748.0))
                // Match the reference composition at every window size: the
                // composer sits 6px inside its responsive container, while
                // its utility strip adds its own 14px inset.
                .px(px(6.0))
                .flex()
                .flex_col()
                .justify_end()
                .gap(px(8.0))
                .when(phase == ConversationPhase::Empty, |container| {
                    container.child(
                        div()
                            .min_h(px(80.0))
                            .px(px(19.0))
                            .flex()
                            .flex_col()
                            .justify_end()
                            .child(first_suggestion)
                            .child(second_suggestion),
                    )
                })
                .child(composer),
        )
}

fn conversation(
    home_entity: Entity<HomeView>,
    theme: Theme,
    phase: ConversationPhase,
    user_message: String,
    user_message_time: String,
    assistant_message: String,
    assistant_message_time: Option<String>,
    conversation_activity: Vec<ConversationActivity>,
    thinking_shimmer_progress: f32,
    response_feedback: i8,
    user_message_actions_visible_for_capture: bool,
    expanded_commands: HashSet<String>,
    command_scroll_handles: HashMap<String, ScrollHandle>,
) -> Div {
    let status = conversation_status(phase);
    let user_message_hover_group: SharedString = "user-message-hover".into();
    let assistant_message_hover_group: SharedString = "assistant-message-hover".into();
    let copied_user_message = user_message.clone();
    let complete = matches!(
        phase,
        ConversationPhase::Complete | ConversationPhase::Failed
    );

    div()
        .absolute()
        .top(px(78.0))
        .w_full()
        .max_w(px(736.0))
        .flex()
        .flex_col()
        .child(
            div()
                .h(px(72.0))
                .w_full()
                .flex()
                .flex_col()
                .items_end()
                .child(
                    div()
                        .group(user_message_hover_group.clone())
                        .flex()
                        .flex_col()
                        .items_end()
                        .child(
                            div()
                                .max_w(px(600.0))
                                .px(px(16.0))
                                .py(px(10.0))
                                .relative()
                                .text_size(px(14.0))
                                .line_height(px(22.0))
                                .text_color(theme.text)
                                // CDP reports border-radius:22px plus
                                // corner-shape:superellipse(1.5), which cannot be
                                // represented by GPUI's circular rounded corners.
                                .child(
                                    canvas(
                                        |bounds, _, _| user_message_bubble_path(bounds),
                                        move |_, path, window, _| {
                                            window.paint_path(path, theme.text.alpha(0.05));
                                        },
                                    )
                                    .absolute()
                                    .inset_0(),
                                )
                                .child(div().relative().child(user_message)),
                        )
                        .child(
                            div()
                                .mt(px(USER_MESSAGE_FOOTER_OFFSET))
                                .mx(px(USER_MESSAGE_FOOTER_SIDE_MARGIN))
                                .h(px(USER_MESSAGE_FOOTER_HEIGHT))
                                .flex()
                                .items_center()
                                .gap(px(USER_MESSAGE_FOOTER_GAP))
                                .child(
                                    div()
                                        .text_size(px(USER_MESSAGE_TIME_SIZE))
                                        .line_height(px(USER_MESSAGE_TIME_LINE_HEIGHT))
                                        .text_color(theme.text_tertiary)
                                        .opacity(if user_message_actions_visible_for_capture {
                                            1.0
                                        } else {
                                            0.0
                                        })
                                        .group_hover(user_message_hover_group.clone(), |time| {
                                            time.opacity(1.0)
                                        })
                                        .child(user_message_time),
                                )
                                .child(
                                    div()
                                        .id("user-message-copy")
                                        .size(px(26.0))
                                        .rounded(px(10.0))
                                        .opacity(if user_message_actions_visible_for_capture {
                                            1.0
                                        } else {
                                            0.0
                                        })
                                        .group_hover(user_message_hover_group, |button| {
                                            button.opacity(1.0)
                                        })
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .hover(move |button| button.bg(theme.sidebar_hover))
                                        .active(move |button| button.bg(theme.text.alpha(0.12)))
                                        .on_click(move |_, _, cx| {
                                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                                copied_user_message.clone(),
                                            ));
                                        })
                                        .child(
                                            icon("message-copy", theme.text_tertiary.into())
                                                .size(px(RESPONSE_ACTION_ICON_SIZE)),
                                        ),
                                ),
                        ),
                ),
        )
        .child(
            div()
                .group(assistant_message_hover_group.clone())
                .mt(px(16.0))
                .w_full()
                .min_h(px(48.0))
                // GPUI group hover is based on the group's own hitbox. The
                // reference's 26px buttons overflow a 20px footer by 3px, and
                // CSS :hover still includes those descendants. This invisible
                // trailing hit area preserves that behavior for every icon px.
                .pb(px(3.0))
                .text_size(px(14.0))
                .line_height(px(22.0))
                .text_color(theme.text)
                .when_some(status, |answer, _| {
                    answer.child(thinking_shimmer(theme, thinking_shimmer_progress))
                })
                .when(
                    !assistant_message.is_empty() || !conversation_activity.is_empty(),
                    |answer| {
                        if conversation_activity.is_empty() {
                            answer.child(div().w_full().child(assistant_message.clone()))
                        } else {
                            answer.child(activity_stream(
                                home_entity.clone(),
                                conversation_activity,
                                expanded_commands,
                                command_scroll_handles,
                                theme,
                            ))
                        }
                    },
                )
                .when(complete, |answer| {
                    answer.child(
                        div()
                            .relative()
                            .left(px(RESPONSE_ACTION_FOOTER_ELECTRON_SHIFT))
                            .mt(px(RESPONSE_ACTION_FOOTER_OFFSET))
                            .w_full()
                            .h(px(RESPONSE_ACTION_FOOTER_HEIGHT))
                            .flex()
                            .items_center()
                            .gap(px(RESPONSE_ACTION_GAP))
                            .child(
                                div()
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .gap(px(RESPONSE_ACTION_GAP))
                                    .child(message_action(
                                        "message-copy",
                                        "response-copy",
                                        0,
                                        false,
                                        assistant_message.clone(),
                                        home_entity.clone(),
                                        theme,
                                    ))
                                    .child(message_action(
                                        "message-thumb-up",
                                        "response-thumb-up",
                                        1,
                                        response_feedback == 1,
                                        assistant_message.clone(),
                                        home_entity.clone(),
                                        theme,
                                    ))
                                    .child(message_action(
                                        "message-thumb-down",
                                        "response-thumb-down",
                                        2,
                                        response_feedback == -1,
                                        assistant_message.clone(),
                                        home_entity.clone(),
                                        theme,
                                    ))
                                    .child(message_action(
                                        "message-branch",
                                        "response-branch",
                                        3,
                                        false,
                                        assistant_message,
                                        home_entity,
                                        theme,
                                    )),
                            )
                            .when_some(assistant_message_time, |footer, completed_at| {
                                footer.child(
                                    div()
                                        .ml(px(RESPONSE_TIME_MARGIN))
                                        .h_full()
                                        .flex()
                                        .items_center()
                                        .opacity(0.0)
                                        .group_hover(assistant_message_hover_group, |time| {
                                            time.opacity(1.0)
                                        })
                                        .child(
                                            div()
                                                .text_size(px(RESPONSE_TIME_SIZE))
                                                .line_height(px(RESPONSE_TIME_LINE_HEIGHT))
                                                .font_weight(gpui::FontWeight::NORMAL)
                                                .text_color(theme.text_tertiary)
                                                .child(completed_at),
                                        ),
                                )
                            }),
                    )
                }),
        )
}

fn activity_stream(
    home_entity: Entity<HomeView>,
    activities: Vec<ConversationActivity>,
    expanded_commands: HashSet<String>,
    command_scroll_handles: HashMap<String, ScrollHandle>,
    theme: Theme,
) -> Div {
    activities.into_iter().fold(
        div().w_full().flex().flex_col().gap(px(16.0)),
        |stream, activity| match activity {
            ConversationActivity::AssistantMessage { text, .. } if !text.is_empty() => {
                stream.child(div().w_full().child(text))
            }
            ConversationActivity::Command(command) => {
                let expanded = expanded_commands.contains(&command.id);
                let scroll_handle = command_scroll_handles
                    .get(&command.id)
                    .cloned()
                    .unwrap_or_else(ScrollHandle::new);
                stream.child(command_activity(
                    home_entity.clone(),
                    command,
                    expanded,
                    scroll_handle,
                    theme,
                ))
            }
            ConversationActivity::Error { message } => stream.child(
                div()
                    .w_full()
                    .px(px(12.0))
                    .py(px(9.0))
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .rounded(px(8.0))
                    .border(px(1.0))
                    .border_color(theme.warning.alpha(0.24))
                    .bg(theme.warning.alpha(0.08))
                    .text_color(theme.warning)
                    .child(
                        icon("settings-warning", theme.warning.into())
                            .size(px(16.0))
                            .mt(px(2.0))
                            .flex_none(),
                    )
                    .child(div().min_w(px(0.0)).child(message)),
            ),
            _ => stream,
        },
    )
}

fn command_activity(
    home_entity: Entity<HomeView>,
    command: CommandExecution,
    expanded: bool,
    scroll_handle: ScrollHandle,
    theme: Theme,
) -> Div {
    let item_id = command.id.clone();
    let output_scroll_id: SharedString = format!("command-output-{item_id}").into();
    let hover_group: SharedString = format!("command-activity-{item_id}").into();
    let command_label = match command.status {
        CommandExecutionStatus::InProgress => "正在运行",
        CommandExecutionStatus::Completed => "已运行",
        CommandExecutionStatus::Failed => "运行失败",
    };
    let status_label = match command.status {
        CommandExecutionStatus::InProgress => "运行中",
        CommandExecutionStatus::Completed => "成功",
        CommandExecutionStatus::Failed => "失败",
    };
    let status_icon = match command.status {
        CommandExecutionStatus::Failed => "settings-warning",
        _ => "check",
    };
    let status_color = if command.status == CommandExecutionStatus::Failed {
        theme.warning
    } else {
        theme.text_tertiary
    };
    let display_command = if command.command.is_empty() {
        "命令".to_owned()
    } else {
        command.command.clone()
    };
    let header_text = format!("{command_label} {display_command}");
    let output = if command.output.is_empty() {
        if command.status == CommandExecutionStatus::InProgress {
            "等待输出…".to_owned()
        } else {
            "（无输出）".to_owned()
        }
    } else {
        command.output.clone()
    };
    let command_for_body = display_command.clone();
    let scroll_handle_for_click = scroll_handle.clone();

    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .items_start()
        .child(
            div()
                .id(SharedString::from(format!("command-activity-{item_id}")))
                .group(hover_group.clone())
                .h(px(21.0))
                .max_w_full()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(4.0))
                .rounded(px(6.0))
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    home_entity.update(cx, |home, cx| {
                        if !home.expanded_commands.remove(&item_id) {
                            home.expanded_commands.insert(item_id.clone());
                            scroll_handle_for_click.scroll_to_bottom();
                        }
                        cx.notify();
                    });
                })
                .child(
                    icon("panel-terminal", theme.text_tertiary.into())
                        .size(px(16.0))
                        .flex_none(),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .max_w(px(690.0))
                        .truncate()
                        .text_size(px(14.0))
                        .line_height(px(21.0))
                        .font_family(".SystemUIFont")
                        .text_color(theme.text_tertiary)
                        .child(header_text),
                )
                .child(
                    icon("settings-chevron-right", theme.text_tertiary.into())
                        .size(px(12.0))
                        .flex_none()
                        .opacity(if expanded { 1.0 } else { 0.0 })
                        .group_hover(hover_group, |chevron| chevron.opacity(1.0))
                        .when(expanded, |chevron| {
                            chevron.with_transformation(Transformation::rotate(radians(
                                std::f32::consts::FRAC_PI_2,
                            )))
                        }),
                ),
        )
        .when(expanded, |activity| {
            activity.child(
                div().w_full().pt(px(8.0)).pb(px(4.0)).child(
                    div()
                        .w_full()
                        .overflow_hidden()
                        .rounded(px(8.0))
                        .border(px(1.0))
                        .border_color(theme.text.alpha(0.15))
                        .bg(theme.text.alpha(0.04))
                        .child(
                            div()
                                .h(px(29.0))
                                .px(px(8.0))
                                .flex()
                                .items_center()
                                .text_size(px(14.0))
                                .line_height(px(21.0))
                                .font_family(".SystemUIFont")
                                .text_color(theme.text_tertiary)
                                .child("Shell"),
                        )
                        .child(
                            div()
                                .px(px(8.0))
                                .pt(px(8.0))
                                .text_size(px(13.0))
                                .line_height(px(20.0))
                                .font_family(UI_MONOSPACE_FONT_FAMILY)
                                .text_color(theme.text_secondary)
                                .child(
                                    div()
                                        .flex()
                                        .items_start()
                                        .child(
                                            div()
                                                .mr(px(8.0))
                                                .text_color(theme.text_tertiary)
                                                .child("$"),
                                        )
                                        .child(
                                            div().min_w(px(0.0)).flex_1().child(command_for_body),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .id(output_scroll_id)
                                .max_h(px(144.0))
                                .overflow_scroll()
                                .restrict_scroll_to_axis()
                                .scrollbar_width(px(0.0))
                                .track_scroll(&scroll_handle)
                                .p(px(8.0))
                                .text_size(px(13.0))
                                .line_height(px(20.0))
                                .font_weight(FontWeight::MEDIUM)
                                .font_family(UI_MONOSPACE_FONT_FAMILY)
                                .text_color(theme.text_secondary)
                                .child(output),
                        )
                        .child(
                            div()
                                .h(px(27.0))
                                .px(px(10.0))
                                .pt(px(2.0))
                                .pb(px(4.0))
                                .flex()
                                .items_center()
                                .justify_end()
                                .gap(px(4.0))
                                .text_size(px(14.0))
                                .line_height(px(21.0))
                                .text_color(status_color)
                                .child(icon(status_icon, status_color.into()).size(px(12.0)))
                                .child(status_label),
                        ),
                ),
            )
        })
}

fn conversation_status(phase: ConversationPhase) -> Option<&'static str> {
    match phase {
        ConversationPhase::Thinking => Some("正在思考"),
        _ => None,
    }
}

fn thinking_shimmer(theme: Theme, progress: f32) -> impl IntoElement {
    div()
        .id("thinking-shimmer")
        .w(px(THINKING_SHIMMER_WIDTH))
        .h(px(21.0))
        .child(
            canvas(
                move |_, window, _| {
                    let mut font = window.text_style().font();
                    font.family = ".SystemUIFont".into();
                    font.weight = FontWeight::NORMAL;
                    let shape = |color| {
                        window.text_system().shape_line(
                            "正在思考".into(),
                            px(14.0),
                            &[TextRun {
                                len: "正在思考".len(),
                                font: font.clone(),
                                color,
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            }],
                            None,
                        )
                    };
                    let base = shape(theme.text.alpha(0.385).into());
                    let highlights = (1..=THINKING_SHIMMER_ALPHA_LEVELS)
                        .map(|level| {
                            shape(
                                rgba(0xffffff00)
                                    .alpha(
                                        0.75 * level as f32 / THINKING_SHIMMER_ALPHA_LEVELS as f32,
                                    )
                                    .into(),
                            )
                        })
                        .collect::<Vec<_>>();
                    (base, highlights)
                },
                move |bounds,
                      (base, highlights): (ShapedLine, Vec<ShapedLine>),
                      window: &mut Window,
                      cx: &mut App| {
                    let origin = bounds.origin;
                    base.paint(origin, px(21.0), TextAlign::Left, None, window, cx)
                        .expect("thinking shimmer base glyphs should paint");

                    let text_width = f32::from(bounds.size.width);
                    let band_width = text_width * THINKING_SHIMMER_BAND_SCALE;
                    let band_left = f32::from(bounds.origin.x)
                        + thinking_shimmer_band_left(progress, text_width);
                    let first_x = band_left.floor().max(f32::from(bounds.left()));
                    let last_x = (band_left + band_width)
                        .ceil()
                        .min(f32::from(bounds.right()));

                    for x in first_x as i32..last_x as i32 {
                        let band_position = (x as f32 + 0.5 - band_left) / band_width;
                        let alpha = thinking_shimmer_alpha(band_position);
                        if alpha <= 0.0 {
                            continue;
                        }
                        let level = ((alpha / 0.75 * THINKING_SHIMMER_ALPHA_LEVELS as f32).ceil()
                            as usize)
                            .clamp(1, THINKING_SHIMMER_ALPHA_LEVELS);
                        let mask = Bounds::from_corners(
                            point(px(x as f32), bounds.top()),
                            point(px(x as f32 + 1.0), bounds.bottom()),
                        );
                        window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
                            highlights[level - 1]
                                .paint(origin, px(21.0), TextAlign::Left, None, window, cx)
                                .expect("thinking shimmer highlight glyphs should paint");
                        });
                    }
                },
            )
            .size_full(),
        )
}

fn message_action(
    glyph: &'static str,
    id: &'static str,
    action: usize,
    active: bool,
    assistant_message: String,
    home_entity: Entity<HomeView>,
    theme: Theme,
) -> impl IntoElement {
    div()
        .id(id)
        .size(px(26.0))
        .rounded(px(10.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .when(active, |button| {
            button.bg(theme.sidebar_hover).text_color(theme.text)
        })
        .hover(move |style| style.bg(theme.sidebar_hover))
        .active(move |style| style.bg(theme.text.alpha(0.12)))
        .on_click(move |_, _, cx| match action {
            0 => cx.write_to_clipboard(gpui::ClipboardItem::new_string(assistant_message.clone())),
            1 | 2 => {
                let value = if action == 1 { 1 } else { -1 };
                home_entity.update(cx, |home, cx| {
                    home.response_feedback = if home.response_feedback == value {
                        0
                    } else {
                        value
                    };
                    cx.notify();
                });
            }
            _ => {}
        })
        .child(icon(glyph, theme.text_tertiary.into()).size(px(RESPONSE_ACTION_ICON_SIZE)))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gpui::{
        AppContext, Bounds, MouseButton, TestApp, TestAppWindow, WindowBounds, WindowOptions,
        point, px, size,
    };

    use super::{
        HomeView, RESPONSE_ACTION_FOOTER_ELECTRON_SHIFT, RESPONSE_ACTION_FOOTER_HEIGHT,
        RESPONSE_ACTION_FOOTER_OFFSET, RESPONSE_ACTION_GAP, RESPONSE_ACTION_ICON_SIZE,
        RESPONSE_TIME_LINE_HEIGHT, RESPONSE_TIME_MARGIN, RESPONSE_TIME_SIZE,
        SUGGESTION_PRESSED_SCALE, THINKING_SHIMMER_DURATION, THINKING_SHIMMER_FRAME_INTERVAL,
        THINKING_SHIMMER_STEPS, THINKING_SHIMMER_WIDTH, USER_MESSAGE_BUBBLE_RADIUS,
        USER_MESSAGE_BUBBLE_SUPERELLIPSE, USER_MESSAGE_FOOTER_GAP, USER_MESSAGE_FOOTER_HEIGHT,
        USER_MESSAGE_FOOTER_OFFSET, USER_MESSAGE_FOOTER_SIDE_MARGIN, USER_MESSAGE_TIME_LINE_HEIGHT,
        USER_MESSAGE_TIME_SIZE, conversation_status, thinking_shimmer_alpha,
        thinking_shimmer_band_left, thinking_shimmer_progress, thinking_shimmer_step,
    };
    use crate::components::composer::ConversationPhase;
    use crate::theme::ThemeMode;

    fn simulate_next_frame(app: &mut TestApp, window: &TestAppWindow<HomeView>, elapsed_ms: u64) {
        app.advance_clock(Duration::from_millis(elapsed_ms));
        let handle = window.handle();
        app.update(|cx| {
            cx.update_window(handle.into(), |_, window, cx| {
                window.simulate_next_frame(cx)
            })
            .unwrap()
        });
    }

    #[test]
    fn suggestion_press_uses_the_reference_scale_and_interruptible_transition() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| HomeView::new(ThemeMode::Dark, cx),
        );

        window.update(|home, window, cx| {
            home.set_suggestion_pressed(0, true, window, cx);
        });
        simulate_next_frame(&mut app, &window, 75);
        let pressed_midpoint = window.read(|home, _| home.suggestion_scale[0]);
        assert!(pressed_midpoint > SUGGESTION_PRESSED_SCALE && pressed_midpoint < 1.0);

        // Releasing halfway through must reverse from the rendered value,
        // rather than jumping to either endpoint.
        window.update(|home, window, cx| {
            home.set_suggestion_pressed(0, false, window, cx);
        });
        assert_eq!(
            window.read(|home, _| home.suggestion_scale[0]),
            pressed_midpoint
        );
        simulate_next_frame(&mut app, &window, 150);
        assert_eq!(window.read(|home, _| home.suggestion_scale[0]), 1.0);
        assert!(!window.read(|home, _| home.suggestion_animation_running));
    }

    #[test]
    fn starting_a_prompt_does_not_render_a_synthetic_status_message() {
        assert_eq!(conversation_status(ConversationPhase::Starting), None);
        assert_eq!(
            conversation_status(ConversationPhase::Thinking),
            Some("正在思考")
        );
    }

    #[test]
    fn thinking_shimmer_matches_the_cdp_animation_geometry() {
        assert_eq!(THINKING_SHIMMER_DURATION, Duration::from_secs(1));
        assert_eq!(thinking_shimmer_progress(Duration::ZERO), 0.0);
        assert_eq!(thinking_shimmer_progress(Duration::from_millis(500)), 0.5);
        assert_eq!(thinking_shimmer_progress(Duration::from_secs(1)), 1.0);
        assert_eq!(thinking_shimmer_step(0.02), 0.0);
        assert_eq!(thinking_shimmer_step(1.0 / 48.0), 1.0 / 48.0);
        assert_eq!(
            thinking_shimmer_band_left(0.0, THINKING_SHIMMER_WIDTH),
            -28.0
        );
        assert_eq!(
            thinking_shimmer_band_left(1.0, THINKING_SHIMMER_WIDTH),
            70.0
        );
        assert_eq!(thinking_shimmer_alpha(0.0), 0.0);
        assert_eq!(thinking_shimmer_alpha(0.4), 0.75);
        assert_eq!(thinking_shimmer_alpha(0.6), 0.75);
        assert_eq!(thinking_shimmer_alpha(1.0), 0.0);
    }

    #[test]
    fn thinking_shimmer_timer_advances_and_loops_the_rendered_phase() {
        let mut app = TestApp::new();
        let home = app.new_entity(|cx| HomeView::new(ThemeMode::Dark, cx));

        app.update_entity(&home, |home, cx| home.start_thinking_shimmer(cx));
        assert_eq!(
            app.read_entity(&home, |home, _| home.thinking_shimmer_progress),
            0.0
        );

        app.advance_clock(THINKING_SHIMMER_FRAME_INTERVAL);
        app.run_until_parked();
        let progress = app.read_entity(&home, |home, _| home.thinking_shimmer_progress);
        assert!((progress - 1.0 / THINKING_SHIMMER_STEPS).abs() < 0.000_001);

        for _ in 1..THINKING_SHIMMER_STEPS as usize {
            app.advance_clock(THINKING_SHIMMER_FRAME_INTERVAL);
            app.run_until_parked();
        }
        assert_eq!(
            app.read_entity(&home, |home, _| home.thinking_shimmer_progress),
            1.0
        );

        app.advance_clock(THINKING_SHIMMER_FRAME_INTERVAL);
        app.run_until_parked();
        let wrapped_progress = app.read_entity(&home, |home, _| home.thinking_shimmer_progress);
        assert!((wrapped_progress - 1.0 / THINKING_SHIMMER_STEPS).abs() < 0.000_001);
        assert!(app.read_entity(&home, |home, _| home.thinking_shimmer_running));

        app.update_entity(&home, |home, cx| {
            home.sync_thinking_shimmer(ConversationPhase::Streaming, cx)
        });
        app.advance_clock(THINKING_SHIMMER_FRAME_INTERVAL);
        app.run_until_parked();
        assert_eq!(
            app.read_entity(&home, |home, _| home.thinking_shimmer_progress),
            0.0
        );
        assert!(!app.read_entity(&home, |home, _| home.thinking_shimmer_running));
    }

    #[test]
    fn user_bubble_uses_the_live_cdp_corner_radius() {
        assert_eq!(USER_MESSAGE_BUBBLE_RADIUS, 22.0);
        assert_eq!(USER_MESSAGE_BUBBLE_SUPERELLIPSE, 1.5);
    }

    #[test]
    fn response_action_icons_use_the_css_resolved_size() {
        assert_eq!(RESPONSE_ACTION_ICON_SIZE, 16.0);
    }

    #[test]
    fn assistant_footer_matches_the_live_cdp_geometry() {
        assert_eq!(RESPONSE_ACTION_FOOTER_OFFSET, 6.0);
        assert_eq!(RESPONSE_ACTION_FOOTER_HEIGHT, 20.0);
        assert_eq!(RESPONSE_ACTION_FOOTER_ELECTRON_SHIFT, -4.0);
        assert_eq!(RESPONSE_ACTION_GAP, 2.0);
        assert_eq!(RESPONSE_TIME_MARGIN, 6.0);
        assert_eq!(RESPONSE_ACTION_GAP + RESPONSE_TIME_MARGIN, 8.0);
        assert_eq!(RESPONSE_TIME_SIZE, 12.0);
        assert_eq!(RESPONSE_TIME_LINE_HEIGHT, 16.0);
    }

    #[test]
    fn user_message_footer_matches_the_live_cdp_geometry() {
        assert_eq!(USER_MESSAGE_FOOTER_OFFSET, 3.0);
        assert_eq!(USER_MESSAGE_FOOTER_HEIGHT, 26.0);
        assert_eq!(USER_MESSAGE_FOOTER_SIDE_MARGIN, 4.0);
        assert_eq!(USER_MESSAGE_FOOTER_GAP, 8.0);
        assert_eq!(USER_MESSAGE_TIME_SIZE, 12.0);
        assert_eq!(USER_MESSAGE_TIME_LINE_HEIGHT, 16.0);
    }

    #[test]
    fn user_message_copy_button_copies_the_submitted_prompt() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(900.0), px(700.0)),
                })),
                ..Default::default()
            },
            |_, cx| HomeView::new(ThemeMode::Dark, cx),
        );

        window.update(|home, _, cx| home.submit_prompt_for_capture("clipboard prompt", cx));
        window.draw();

        // The 736px conversation column is centered in this 900px test
        // window. ChatGPT insets the trailing 26px action by 4px, so it
        // occupies x=788..814 and y=123..149.
        window.simulate_mouse_move(point(px(801.0), px(136.0)));
        window.simulate_click(point(px(801.0), px(136.0)), MouseButton::Left);

        assert_eq!(
            app.read_from_clipboard().and_then(|item| item.text()),
            Some("clipboard prompt".to_owned())
        );
    }
}
