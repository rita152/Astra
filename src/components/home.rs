use std::time::{Duration, Instant};

use gpui::{Context, Div, Entity, MouseButton, Render, Window, div, prelude::*, px, relative};

use crate::{
    components::{
        composer::{ComposerView, RequestFullAccess},
        icons::{icon, suggestion_icon},
    },
    theme::{Theme, ThemeMode},
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
}

impl gpui::EventEmitter<RequestFullAccess> for HomeView {}

const SUGGESTION_PRESSED_SCALE: f32 = 0.99;
const SUGGESTION_TRANSITION_DURATION: Duration = Duration::from_millis(150);

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
        let composer = cx.new(|_| ComposerView::new(mode));
        cx.subscribe(&composer, |_, _, _: &RequestFullAccess, cx| {
            cx.emit(RequestFullAccess);
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
        }
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
                    .font_weight(gpui::FontWeight(445.0))
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
        home(
            theme,
            self.composer.clone(),
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
    theme: Theme,
    composer: Entity<ComposerView>,
    first_suggestion: impl IntoElement,
    second_suggestion: impl IntoElement,
) -> Div {
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .relative()
        .child(
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
        .child(
            div()
                .absolute()
                .bottom(px(15.0))
                .w_full()
                .max_w(px(786.0))
                // Match the reference composition at every window size: the
                // composer sits 6px inside its responsive container, while
                // its utility strip adds its own 14px inset.
                .px(px(6.0))
                .flex()
                .flex_col()
                .justify_end()
                .gap(px(8.0))
                .child(
                    div()
                        .min_h(px(80.0))
                        .px(px(19.0))
                        .flex()
                        .flex_col()
                        .justify_end()
                        .child(first_suggestion)
                        .child(second_suggestion),
                )
                .child(composer),
        )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gpui::{
        AppContext, Bounds, TestApp, TestAppWindow, WindowBounds, WindowOptions, point, px, size,
    };

    use super::{HomeView, SUGGESTION_PRESSED_SCALE};
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
}
