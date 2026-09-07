//! Animation presentation and interaction for the conversation view.

use std::time::Duration;

use gpui::{
    App, Bounds, ContentMask, FontWeight, ShapedLine, TextAlign, TextRun, Window, canvas, div,
    point, prelude::*, px, rgba,
};

use super::{
    THINKING_SHIMMER_ALPHA_LEVELS, THINKING_SHIMMER_BAND_SCALE, THINKING_SHIMMER_DURATION,
    THINKING_SHIMMER_STEPS, THINKING_SHIMMER_WIDTH,
};
use crate::theme::Theme;

pub(super) fn thinking_shimmer_step(progress: f32) -> f32 {
    (progress.clamp(0.0, 1.0) * THINKING_SHIMMER_STEPS).floor() / THINKING_SHIMMER_STEPS
}

pub(super) fn thinking_shimmer_progress(elapsed: Duration) -> f32 {
    (elapsed.as_secs_f32() / THINKING_SHIMMER_DURATION.as_secs_f32()).clamp(0.0, 1.0)
}

pub(super) fn thinking_shimmer_band_left(progress: f32, text_width: f32) -> f32 {
    // CSS background-position percentages are relative to the remaining width.
    // With a 50%-wide image, -100%..250% resolves to -0.5w..1.25w.
    let remaining_width = text_width * (1.0 - THINKING_SHIMMER_BAND_SCALE);
    remaining_width * (-1.0 + 3.5 * thinking_shimmer_step(progress))
}

pub(super) fn thinking_shimmer_alpha(position: f32) -> f32 {
    let position = position.clamp(0.0, 1.0);
    if position < 0.4 {
        position / 0.4 * 0.75
    } else if position <= 0.6 {
        0.75
    } else {
        (1.0 - position) / 0.4 * 0.75
    }
}

pub(super) fn suggestion_transition_ease(progress: f32) -> f32 {
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

pub(super) fn reasoning_transition_ease(progress: f32) -> f32 {
    cubic_bezier_ease(progress, 0.19, 1.0, 0.22, 1.0)
}

pub(super) fn tool_group_chevron_transition_ease(progress: f32) -> f32 {
    cubic_bezier_ease(progress, 0.4, 0.0, 0.2, 1.0)
}

pub(super) fn cubic_bezier_ease(progress: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    if progress == 0.0 || progress == 1.0 {
        return progress;
    }

    // Invert x for the reference cubic Bézier, then evaluate y.
    let mut lower = 0.0;
    let mut upper = 1.0;
    for _ in 0..10 {
        let parameter = (lower + upper) * 0.5;
        let inverse = 1.0 - parameter;
        let x = 3.0 * inverse * inverse * parameter * x1
            + 3.0 * inverse * parameter * parameter * x2
            + parameter * parameter * parameter;
        if x < progress {
            lower = parameter;
        } else {
            upper = parameter;
        }
    }
    let parameter = (lower + upper) * 0.5;
    let inverse = 1.0 - parameter;
    3.0 * inverse * inverse * parameter * y1
        + 3.0 * inverse * parameter * parameter * y2
        + parameter * parameter * parameter
}

pub(super) fn thinking_shimmer(theme: Theme, progress: f32) -> impl IntoElement {
    div().id("thinking-shimmer").child(shimmer_label(
        "正在思考",
        THINKING_SHIMMER_WIDTH,
        theme,
        progress,
    ))
}

pub(super) fn shimmer_label(
    label: &'static str,
    width: f32,
    theme: Theme,
    progress: f32,
) -> impl IntoElement {
    div().w(px(width)).h(px(21.0)).child(
        canvas(
            move |_, window, _| {
                let mut font = window.text_style().font();
                font.family = ".SystemUIFont".into();
                font.weight = FontWeight::NORMAL;
                let shape = |color| {
                    window.text_system().shape_line(
                        label.into(),
                        px(14.0),
                        &[TextRun {
                            len: label.len(),
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
                                .alpha(0.75 * level as f32 / THINKING_SHIMMER_ALPHA_LEVELS as f32)
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
                    .expect("context compaction shimmer base glyphs should paint");

                let text_width = f32::from(bounds.size.width);
                let band_width = text_width * THINKING_SHIMMER_BAND_SCALE;
                let band_left =
                    f32::from(bounds.origin.x) + thinking_shimmer_band_left(progress, text_width);
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
                            .expect("context compaction shimmer highlight glyphs should paint");
                    });
                }
            },
        )
        .size_full(),
    )
}
