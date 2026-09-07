//! Dictation behavior and presentation for the prompt composer.

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, Context, IntoElement, Transformation, div, prelude::*, px, radians,
    rgba,
};

use super::{ComposerView, DictationState};
use crate::{components::icons::icon, theme::Theme};

impl ComposerView {
    #[cfg(test)]
    pub(super) fn dictation_state_name(&self) -> &'static str {
        match self.dictation_state {
            DictationState::Idle => "idle",
            DictationState::Recording => "recording",
            DictationState::Transcribing => "transcribing",
        }
    }
    pub(super) fn start_dictation(&mut self, cx: &mut Context<Self>) {
        self.dictation_cycle = self.dictation_cycle.wrapping_add(1);
        self.dictation_state = DictationState::Recording;
        self.menu_open = false;
        self.submenu = None;
        cx.notify();
    }
    pub(super) fn cancel_dictation(&mut self, cx: &mut Context<Self>) {
        self.dictation_cycle = self.dictation_cycle.wrapping_add(1);
        self.dictation_state = DictationState::Idle;
        cx.notify();
    }
    pub(super) fn stop_dictation(&mut self, cx: &mut Context<Self>) {
        if self.dictation_state != DictationState::Recording {
            return;
        }
        self.dictation_state = DictationState::Transcribing;
        self.dictation_cycle = self.dictation_cycle.wrapping_add(1);
        let cycle = self.dictation_cycle;
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            // CDP sampling showed the processing controls for roughly one second
            // before the ordinary composer footer returned.
            executor.timer(Duration::from_millis(1_050)).await;
            let _ = this.update(cx, |this, cx| {
                if this.dictation_state == DictationState::Transcribing
                    && this.dictation_cycle == cycle
                {
                    this.dictation_state = DictationState::Idle;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn dictation_waveform(&self, theme: Theme) -> impl IntoElement {
        div()
            .id("composer-dictation-waveform")
            .h(px(28.0))
            .min_w(px(0.0))
            .flex_1()
            .with_animation(
                "composer-dictation-waveform-motion",
                Animation::new(Duration::from_secs(36)).repeat(),
                move |waveform, progress| {
                    waveform.child(
                        gpui::canvas(
                            |bounds, _, _| bounds,
                            move |bounds, _, window, _| {
                                // The desktop app paints a 2px pill every 6px on a
                                // transparent 28px canvas. Quiet samples are 2px tall at
                                // 20% text opacity; voiced samples grow to 22px and 55%.
                                let width = f32::from(bounds.size.width).max(0.0);
                                let count = (width / 6.0).ceil() as usize;
                                let phase = progress * 610.0;
                                for index in 0..count {
                                    let sample = index as f32 + phase;
                                    let envelope = ((sample * 0.097).sin().abs().powf(22.0)
                                        * (0.45 + 0.55 * (sample * 0.271).sin().abs()))
                                    .max(
                                        (sample * 0.043 + 1.7).sin().abs().powf(34.0)
                                            * (sample * 0.191).sin().abs(),
                                    );
                                    let height = 2.0 + 20.0 * envelope;
                                    let x = index as f32 * 6.0 + 1.0;
                                    if x >= width {
                                        break;
                                    }
                                    let bar = gpui::Bounds {
                                        origin: gpui::point(
                                            bounds.origin.x + px(x),
                                            bounds.origin.y + px((28.0 - height) * 0.5),
                                        ),
                                        size: gpui::size(px(2.0), px(height)),
                                    };
                                    window.paint_quad(gpui::quad(
                                        bar,
                                        px(1.0),
                                        theme.text.alpha(0.20 + 0.35 * envelope),
                                        px(0.0),
                                        rgba(0x00000000),
                                        Default::default(),
                                    ));
                                }
                            },
                        )
                        .size_full(),
                    )
                },
            )
    }
    pub(super) fn dictation_footer(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let transcribing = self.dictation_state == DictationState::Transcribing;
        let soft_fill = theme.text.alpha(0.05);
        let strong_fill = theme.text.alpha(0.10);

        let cancel = div()
            .id("composer-dictation-cancel")
            .size(px(28.0))
            .flex_none()
            .rounded_full()
            .bg(soft_fill)
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(move |style| style.bg(strong_fill))
            .on_click(cx.listener(|this, _, _, cx| this.cancel_dictation(cx)))
            .child(icon("dictation-cancel", theme.text.into()).size(px(16.0)));

        let stop_icon = if transcribing {
            icon("dictation-spinner", theme.text.into())
                .size(px(16.0))
                .with_animation(
                    "composer-dictation-spinner-motion",
                    Animation::new(Duration::from_millis(800)).repeat(),
                    |spinner, progress| {
                        spinner.with_transformation(Transformation::rotate(radians(
                            progress * std::f32::consts::TAU,
                        )))
                    },
                )
                .into_any_element()
        } else {
            icon("dictation-stop", theme.text.into())
                .size(px(16.0))
                .into_any_element()
        };

        let stop = div()
            .id("composer-dictation-stop")
            .size(px(28.0))
            .flex_none()
            .rounded_full()
            .bg(if transcribing { soft_fill } else { strong_fill })
            .flex()
            .items_center()
            .justify_center()
            .when(!transcribing, |button| {
                button
                    .cursor_pointer()
                    .hover(move |style| style.bg(strong_fill))
                    .on_click(cx.listener(|this, _, _, cx| this.stop_dictation(cx)))
            })
            .child(stop_icon);

        let send = div()
            .id("composer-dictation-send")
            .size(px(28.0))
            .flex_none()
            .rounded_full()
            .bg(theme.button)
            .when(transcribing, |button| button.opacity(0.4))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| this.stop_dictation(cx)))
            .child(icon("dictation-send", theme.button_text.into()).size(px(16.0)));

        div()
            .id("composer-dictation-footer")
            .h(px(36.0))
            .relative()
            .top(px(7.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(cancel)
            .child(self.dictation_waveform(theme))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(stop)
                    .child(send),
            )
    }
}
