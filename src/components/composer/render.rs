//! Render behavior and presentation for the prompt composer.

use gpui::{
    BoxShadow, Context, Div, MouseButton, Render, Window, deferred, div, hsla, prelude::*, px, rgba,
};

use super::{
    COMPOSER_CORNER_RADIUS, ComposerView, DictationState, MODEL_PICKER_TRIGGER_GAP,
    MODEL_PICKER_WIDTH,
};
use crate::{
    components::icons::icon,
    conversation::ConversationPhase,
    theme::{Theme, ThemeMode},
};

impl ComposerView {
    pub(super) fn render_composer(
        &self,
        viewport_width: f32,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let (permission_label, permission_icon, permission_color) = self.permission_label();
        let effective_model_label = self.effective_model_label();
        let model_status_active = self.conversation.model_status.is_some();
        let effort_or_status_label = self
            .conversation
            .model_status
            .clone()
            .unwrap_or_else(|| self.selected_effort_label());
        let fast_tier_selected = self.conversation.selected_service_tier.is_some();
        let trigger_label = div()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap(px(MODEL_PICKER_TRIGGER_GAP))
            .when(self.menu_open, |label| label.flex_1().justify_center())
            .child(div().text_color(theme.text).child(effective_model_label))
            .child(
                div()
                    .text_color(if model_status_active {
                        theme.effort
                    } else {
                        theme.text_tertiary
                    })
                    .child(effort_or_status_label),
            );
        let trigger_value = div()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .when(self.menu_open, |value| {
                value.flex_1().child(
                    div()
                        .w(px(18.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(fast_tier_selected, |indicator| {
                            indicator.child(icon("model-fast", theme.text.into()).size(px(14.0)))
                        }),
                )
            })
            .when(!self.menu_open && fast_tier_selected, |value| {
                // ChatGPT CDP: the closed trigger uses a 14px fast glyph with
                // exactly 4px between its right edge and the model label.
                value
                    .gap(px(MODEL_PICKER_TRIGGER_GAP))
                    .child(icon("model-fast", theme.text.into()).size(px(14.0)))
            })
            .child(trigger_label);
        let prompt_is_empty = self.prompt_input.read(cx).text().is_empty();
        let conversation_started = self.conversation.phase != ConversationPhase::Empty;
        let generation_active = matches!(
            self.conversation.phase,
            ConversationPhase::Starting
                | ConversationPhase::Thinking
                | ConversationPhase::Streaming
                | ConversationPhase::Stopping
        );
        div()
            .w_full()
            .relative()
            .flex()
            .flex_col()
            .gap(px(0.0))
            .when(!conversation_started, |composer| {
                composer.child(context_toolbar(theme))
            })
            .child(
                div()
                    .h(px(98.0))
                    .w_full()
                    .rounded(px(COMPOSER_CORNER_RADIUS))
                    .bg(theme.control_soft)
                    .shadow(vec![
                        BoxShadow::new(px(0.0), px(0.0), theme.border.into())
                            .spread_radius(px(0.5)),
                        BoxShadow::new(px(0.0), px(3.0), hsla(0.0, 0.0, 0.0, 0.04))
                            .blur_radius(px(7.5)),
                        BoxShadow::new(px(0.0), px(0.0), hsla(0.0, 0.0, 0.0, 0.05))
                            .blur_radius(px(20.0)),
                        BoxShadow::new(px(0.0), px(0.0), theme.surface.into())
                            .spread_radius(px(0.5)),
                    ])
                    .flex()
                    .flex_col()
                    // Composer labels are uniformly system-ui 400 in the
                    // reference (placeholder 14/20; controls 13/18).
                    .font_weight(gpui::FontWeight::NORMAL)
                    .px(px(8.0))
                    .py(px(12.0))
                    .child(self.prompt_input.clone())
                    .when(self.dictation_state == DictationState::Idle, |composer| {
                        composer.child(
                            div()
                                .h(px(36.0))
                                .relative()
                                .top(px(7.0))
                                .flex()
                                .items_center()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(5.0))
                                        .child(
                                            div()
                                                .id("composer-add-context")
                                                .size(px(28.0))
                                                .rounded_full()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .cursor_pointer()
                                                .hover(move |style| style.bg(theme.sidebar_hover))
                                                .child(
                                                    icon("add", theme.text.into()).size(px(16.0)),
                                                ),
                                        )
                                        .when(self.permission_ui_enabled, |controls| {
                                            controls.child(
                                                div()
                                                    .id("composer-permissions")
                                                    .h(px(28.0))
                                                    .px(px(8.0))
                                                    .rounded_full()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(4.0))
                                                    .text_size(px(13.0))
                                                    .line_height(px(18.0))
                                                    .text_color(permission_color)
                                                    .cursor_pointer()
                                                    .when(!self.permission_menu_open, |button| {
                                                        button.track_focus(
                                                            &self.permission_menu_focus,
                                                        )
                                                    })
                                                    .on_key_down(
                                                        cx.listener(
                                                            Self::handle_permission_menu_key,
                                                        ),
                                                    )
                                                    .when(self.permission_menu_open, |button| {
                                                        button.bg(theme.sidebar_hover)
                                                    })
                                                    .hover(move |style| {
                                                        style.bg(theme.sidebar_hover)
                                                    })
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(|this, _, window, cx| {
                                                            cx.stop_propagation();
                                                            this.menu_open = false;
                                                            this.submenu = None;
                                                            this.permission_menu_open =
                                                                !this.permission_menu_open;
                                                            this.permission_menu_keyboard_focus =
                                                                false;
                                                            if this.permission_menu_open {
                                                                window.focus(
                                                                    &this.permission_menu_focus,
                                                                    cx,
                                                                );
                                                            }
                                                            cx.notify();
                                                        }),
                                                    )
                                                    .on_click(cx.listener(|_, _, _, cx| {
                                                        cx.stop_propagation();
                                                    }))
                                                    .child(
                                                        div()
                                                            .size(px(16.0))
                                                            .flex_none()
                                                            .flex()
                                                            .items_center()
                                                            .justify_center()
                                                            .child(icon(
                                                                permission_icon,
                                                                permission_color.into(),
                                                            )),
                                                    )
                                                    .child(permission_label),
                                            )
                                        }),
                                )
                                .child(div().flex_1())
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .min_w(px(0.0))
                                        .child(
                                            div()
                                                .id("composer-model-picker")
                                                .h(px(28.0))
                                                .px(px(8.0))
                                                .rounded_full()
                                                .when(self.menu_open, |button| {
                                                    button.w(px(MODEL_PICKER_WIDTH)).flex_none()
                                                })
                                                .flex()
                                                .items_center()
                                                .gap(px(4.0))
                                                .text_size(px(13.0))
                                                .line_height(px(18.0))
                                                .cursor_pointer()
                                                .when(self.menu_open, |button| {
                                                    button.bg(theme.sidebar_hover)
                                                })
                                                .hover(move |style| style.bg(theme.sidebar_hover))
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    cx.stop_propagation();
                                                    this.permission_menu_open = false;
                                                    this.permission_menu_keyboard_focus = false;
                                                    this.menu_open = !this.menu_open;
                                                    if !this.menu_open {
                                                        this.submenu = None;
                                                    } else {
                                                        this.model_menu_keyboard_focus = false;
                                                        this.submenu_keyboard_focus = false;
                                                        window.focus(&this.model_menu_focus, cx);
                                                    }
                                                    cx.notify();
                                                }))
                                                .child(trigger_value)
                                                .child(
                                                    icon(
                                                        "chevron-down",
                                                        theme.text_tertiary.into(),
                                                    )
                                                    .size(px(14.0)),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(8.0))
                                                .child(
                                                    div()
                                                        .id("composer-dictation")
                                                        .size(px(28.0))
                                                        .rounded_full()
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .cursor_pointer()
                                                        .hover(move |style| {
                                                            style.bg(theme.sidebar_hover)
                                                        })
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.start_dictation(cx)
                                                        }))
                                                        .child(
                                                            icon("dictation", theme.text.into())
                                                                .size(px(16.0)),
                                                        ),
                                                )
                                                .child(
                                                    div()
                                                        .id("composer-voice")
                                                        .size(px(28.0))
                                                        .flex_none()
                                                        .rounded_full()
                                                        .bg(theme.button)
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .when(
                                                            prompt_is_empty
                                                                && conversation_started
                                                                && !generation_active,
                                                            |button| button.opacity(0.4),
                                                        )
                                                        .when(
                                                            !prompt_is_empty
                                                                || generation_active
                                                                || !conversation_started,
                                                            |button| button.cursor_pointer(),
                                                        )
                                                        .when(!generation_active, |button| {
                                                            button.on_click(cx.listener(
                                                                |this, _, _, cx| {
                                                                    this.prompt_input.update(
                                                                        cx,
                                                                        |input, cx| {
                                                                            input.submit(cx)
                                                                        },
                                                                    );
                                                                },
                                                            ))
                                                        })
                                                        .when(generation_active, |button| {
                                                            button.on_click(cx.listener(
                                                                |this, _, _, cx| {
                                                                    this.stop_generation(cx)
                                                                },
                                                            ))
                                                        })
                                                        .child(
                                                            icon(
                                                                if generation_active {
                                                                    "composer-stop"
                                                                } else if !prompt_is_empty
                                                                    || conversation_started
                                                                {
                                                                    "dictation-send"
                                                                } else {
                                                                    "voice"
                                                                },
                                                                theme.button_text.into(),
                                                            )
                                                            .size(px(
                                                                if generation_active
                                                                    || !prompt_is_empty
                                                                    || conversation_started
                                                                {
                                                                    20.0
                                                                } else {
                                                                    16.0
                                                                },
                                                            )),
                                                        ),
                                                ),
                                        ),
                                ),
                        )
                    })
                    .when(self.dictation_state != DictationState::Idle, |composer| {
                        composer.child(self.dictation_footer(theme, cx))
                    }),
            )
            .when(self.menu_open, |composer| {
                composer.child(deferred(self.model_menu(viewport_width, theme, cx)))
            })
            .when(
                self.permission_ui_enabled && self.permission_menu_open,
                |composer| composer.child(deferred(self.permission_menu(theme, cx))),
            )
            .when(
                self.approval_resolved_capture && self.mode == ThemeMode::Dark,
                |composer| {
                    // CDP 12 was captured while the real model trigger's
                    // tooltip was visible. Keep that interaction state in the
                    // resolved-only fixture instead of changing the live
                    // composer or reusing a synthetic blank crop.
                    composer.child(
                        div()
                            .absolute()
                            .left(px(539.0))
                            .top(px(26.0))
                            .w(px(122.796875))
                            .h(px(32.5625))
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(px(12.5))
                            .border_1()
                            .border_color(rgba(0xdfdfdfff))
                            .bg(rgba(0xdfdfdfff))
                            .font_family(".SystemUIFont")
                            .text_size(px(13.0))
                            .line_height(px(18.5714))
                            .font_weight(gpui::FontWeight::NORMAL)
                            .text_color(rgba(0x2d2d2dff))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(div().flex_none().child("选择模型"))
                            .child(
                                div()
                                    .w(px(42.0))
                                    .h(px(16.0))
                                    .flex_none()
                                    .rounded(px(6.0))
                                    .bg(rgba(0x2d2d2d1a))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(12.0))
                                    .line_height(px(12.0))
                                    .child("⌃⇧M"),
                            ),
                    )
                },
            )
    }
}

pub(super) fn utility(
    id: &'static str,
    label: &'static str,
    glyph: &'static str,
    horizontal_padding: f32,
    theme: Theme,
) -> impl IntoElement {
    let hover_fill = theme.text.alpha(0.05);

    div()
        .id(id)
        .h(px(28.0))
        .px(px(horizontal_padding))
        .rounded_full()
        .flex()
        .items_center()
        .gap(px(4.0))
        .text_size(px(13.0))
        .line_height(px(18.0))
        .font_weight(gpui::FontWeight::NORMAL)
        .text_color(theme.text)
        .cursor_pointer()
        .hover(move |style| style.bg(hover_fill))
        .child(icon(glyph, theme.text.into()).size(px(16.0)))
        .child(label)
}

pub(super) fn project_utility(theme: Theme) -> impl IntoElement {
    let group = "composer-project-hover";
    let hover_fill = theme.text.alpha(0.05);

    div()
        .id("composer-project")
        .group(group)
        .relative()
        .h(px(28.0))
        .rounded_full()
        .flex_none()
        .child(
            div()
                .h(px(28.0))
                .px(px(8.0))
                .rounded_full()
                .flex()
                .items_center()
                .gap(px(4.0))
                .text_size(px(13.0))
                .line_height(px(18.0))
                .font_weight(gpui::FontWeight::NORMAL)
                .text_color(theme.text)
                .cursor_pointer()
                .group_hover(group, move |style| style.bg(hover_fill))
                .child(
                    icon("utility-folder", theme.text.into())
                        .size(px(16.0))
                        .group_hover(group, |style| style.invisible()),
                )
                .child("coda"),
        )
        // The reference overlays a 28px clear-project control on the leading
        // edge and swaps it with the folder whenever the project trigger is
        // hovered. Only this nested control promotes tertiary -> primary.
        .child(
            div()
                .id("composer-clear-project")
                .absolute()
                .top_0()
                .left_0()
                .size(px(28.0))
                .group("composer-clear-project-icon")
                .invisible()
                .group_hover(group, |style| style.visible())
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.text_tertiary)
                .cursor_pointer()
                .hover(move |style| style.bg(hover_fill).text_color(theme.text))
                .child(
                    icon("clear-project", theme.text_tertiary.into())
                        .size(px(16.0))
                        .group_hover("composer-clear-project-icon", move |style| {
                            style.text_color(theme.text)
                        }),
                ),
        )
}

pub(super) fn context_toolbar(theme: Theme) -> Div {
    div()
        .relative()
        .h(px(38.0))
        .mx(px(13.0))
        // The reference toolbar continues underneath the Composer. Keeping
        // the extension in a separate background layer preserves the text's
        // 38px alignment while the later Composer layer masks its lower edge.
        .child(
            div()
                .absolute()
                .top(px(4.0))
                .left_0()
                .w_full()
                .h(px(52.0))
                .rounded(px(16.0))
                .bg(theme.surface_under),
        )
        .child(
            div()
                .relative()
                .top(px(4.0))
                .h(px(38.0))
                .w_full()
                .px(px(6.0))
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(project_utility(theme))
                .child(utility("composer-location", "本地", "local", 8.0, theme))
                // Although the HTML includes a trailing `px-0` class, the
                // home-placement `px-2` rule is emitted later in the bundled
                // stylesheet and wins the cascade. The computed button has
                // 8px inline padding on both sides.
                .child(utility("composer-branch", "main", "branch", 8.0, theme)),
        )
}

impl Render for ComposerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_composer(
            f32::from(window.viewport_size().width),
            Theme::for_mode(self.mode),
            cx,
        )
    }
}
