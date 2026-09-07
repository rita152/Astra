//! Sidebar behavior and presentation for the application shell.

use std::time::Duration;

use gpui::{
    Context, Div, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Window, canvas,
    prelude::*,
};

use super::{
    ChatApp, RIGHT_PANEL_MAIN_MIN_WIDTH, RIGHT_PANEL_MIN_WIDTH, SIDEBAR_MAX_WIDTH,
    SIDEBAR_MIN_WIDTH, SIDEBAR_TRANSITION_DURATION, render::panel_resize_handle,
};
use crate::theme::Theme;

impl ChatApp {
    pub(super) fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_layout.resize_hovered = false;
        self.sidebar_layout.resize_dragging = false;
        self.sidebar_layout.collapsed = !self.sidebar_layout.collapsed;
        let target = if self.sidebar_layout.collapsed {
            0.0
        } else {
            1.0
        };

        if cx.reduce_motion() || (target - self.sidebar_layout.reveal).abs() <= f32::EPSILON {
            self.sidebar_layout.reveal = target;
            self.sidebar_layout.animation_from = target;
            self.sidebar_layout.animation_to = target;
            self.sidebar_layout.animation_started_at = None;
            self.sidebar_layout.animation_duration = Duration::ZERO;
            self.sidebar_layout.animation_running = false;
            cx.notify();
            return;
        }

        let was_running = self.sidebar_layout.animation_running;
        self.sidebar_layout.animation_from = self.sidebar_layout.reveal;
        self.sidebar_layout.animation_to = target;
        self.sidebar_layout.animation_started_at = Some(cx.background_executor().now());
        self.sidebar_layout.animation_duration = Duration::from_secs_f32(
            SIDEBAR_TRANSITION_DURATION.as_secs_f32() * (target - self.sidebar_layout.reveal).abs(),
        );
        self.sidebar_layout.animation_running = true;
        cx.notify();

        if !was_running {
            cx.on_next_frame(window, |this, window, cx| {
                this.advance_sidebar_animation(window, cx)
            });
        }
    }
    pub(super) fn advance_sidebar_animation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.sidebar_layout.animation_running {
            return;
        }

        let elapsed = self
            .sidebar_layout
            .animation_started_at
            .map_or(Duration::ZERO, |started| {
                cx.background_executor()
                    .now()
                    .saturating_duration_since(started)
            });
        let progress = if self.sidebar_layout.animation_duration.is_zero() {
            1.0
        } else {
            elapsed.as_secs_f32() / self.sidebar_layout.animation_duration.as_secs_f32()
        }
        .clamp(0.0, 1.0);

        self.sidebar_layout.reveal = self.sidebar_layout.animation_from
            + (self.sidebar_layout.animation_to - self.sidebar_layout.animation_from)
                * sidebar_transition_ease(progress);

        if progress >= 1.0 || cx.reduce_motion() {
            self.sidebar_layout.reveal = self.sidebar_layout.animation_to;
            self.sidebar_layout.animation_running = false;
            self.sidebar_layout.animation_started_at = None;
        }
        cx.notify();

        if self.sidebar_layout.animation_running {
            cx.on_next_frame(window, |this, window, cx| {
                this.advance_sidebar_animation(window, cx)
            });
        }
    }
    pub(super) fn sidebar_resize_handle(
        &self,
        theme: Theme,
        width: f32,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let entity = cx.entity();
        let line_visible =
            self.sidebar_layout.resize_hovered || self.sidebar_layout.resize_dragging;
        let input_layer = canvas(
            |bounds, _, _| bounds,
            move |bounds, _, window, _| {
                let mouse_down_entity = entity.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, _, _, cx| {
                    if event.button != MouseButton::Left || !bounds.contains(&event.position) {
                        return;
                    }
                    mouse_down_entity.update(cx, |this, cx| {
                        let divider_x = f32::from(bounds.origin.x) + 8.0;
                        this.sidebar_layout.resize_dragging = true;
                        this.sidebar_layout.resize_hovered = true;
                        this.sidebar_layout.resize_pointer_offset =
                            divider_x - f32::from(event.position.x);
                        cx.notify();
                    });
                });

                let mouse_move_entity = entity.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, _, window, cx| {
                    let pointer_inside = bounds.contains(&event.position);
                    mouse_move_entity.update(cx, |this, cx| {
                        let mut changed = false;
                        if this.sidebar_layout.resize_dragging {
                            let viewport_width = f32::from(window.viewport_size().width);
                            let reserved_right_width = if this.right_panel.open {
                                RIGHT_PANEL_MIN_WIDTH
                            } else {
                                0.0
                            };
                            let max_width = (viewport_width
                                - reserved_right_width
                                - RIGHT_PANEL_MAIN_MIN_WIDTH)
                                .clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
                            let next_width = (f32::from(event.position.x)
                                + this.sidebar_layout.resize_pointer_offset)
                                .clamp(SIDEBAR_MIN_WIDTH, max_width);
                            this.sidebar
                                .update(cx, |sidebar, cx| sidebar.set_width(next_width, cx));
                            changed = true;
                        }
                        let next_hovered = pointer_inside || this.sidebar_layout.resize_dragging;
                        if this.sidebar_layout.resize_hovered != next_hovered {
                            this.sidebar_layout.resize_hovered = next_hovered;
                            changed = true;
                        }
                        if changed {
                            cx.notify();
                        }
                    });
                });

                let mouse_up_entity = entity.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, _, _, cx| {
                    if event.button != MouseButton::Left {
                        return;
                    }
                    mouse_up_entity.update(cx, |this, cx| {
                        if !this.sidebar_layout.resize_dragging {
                            return;
                        }
                        this.sidebar_layout.resize_dragging = false;
                        this.sidebar_layout.resize_hovered = bounds.contains(&event.position);
                        cx.notify();
                    });
                });
            },
        )
        .absolute()
        .inset_0();

        panel_resize_handle(
            "sidebar-resize-handle",
            width - 8.0,
            line_visible,
            theme,
            input_layer,
        )
    }
}

pub(super) fn sidebar_transition_ease(progress: f32) -> f32 {
    fn bezier(t: f32, first: f32, second: f32) -> f32 {
        let inverse = 1.0 - t;
        3.0 * inverse * inverse * t * first + 3.0 * inverse * t * t * second + t * t * t
    }

    let progress = progress.clamp(0.0, 1.0);
    if progress == 0.0 || progress == 1.0 {
        return progress;
    }

    // chat-reference: --cubic-enter: cubic-bezier(.19, 1, .22, 1)
    let (mut lower, mut upper) = (0.0, 1.0);
    for _ in 0..12 {
        let parameter = (lower + upper) * 0.5;
        if bezier(parameter, 0.19, 0.22) < progress {
            lower = parameter;
        } else {
            upper = parameter;
        }
    }
    bezier((lower + upper) * 0.5, 1.0, 1.0)
}
