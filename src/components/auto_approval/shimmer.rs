//! ChatGPT's 600 ms delay / 1 s sweep / 4 s cadence, with 48 discrete steps.

use crate::theme::{Theme, ui_font};
use gpui::{
    Animation, AnimationExt, App, Bounds, ContentMask, IntoElement, ShapedLine, TextAlign, TextRun,
    Window, canvas, div, point, prelude::*, px, rgba,
};
use std::{cell::Cell, rc::Rc, time::Duration};

pub(super) fn label(text: String, theme: Theme) -> impl IntoElement {
    let progress = Rc::new(Cell::new(0_f32));
    let paint_progress = progress.clone();
    let shape_text = text.clone();
    div()
        .relative()
        .top(px(-0.25))
        .min_w(px(0.))
        .truncate()
        .text_color(theme.markdown_text.alpha(0.273_922))
        .child(text)
        .child(
            canvas(
                move |_, window, _| {
                    (1..=32)
                        .map(|level| {
                            window.text_system().shape_line(
                                shape_text.clone().into(),
                                px(14.),
                                &[TextRun {
                                    len: shape_text.len(),
                                    font: ui_font(),
                                    color: rgba(0xffffff00).alpha(0.75 * level as f32 / 32.).into(),
                                    background_color: None,
                                    underline: None,
                                    strikethrough: None,
                                }],
                                None,
                            )
                        })
                        .collect::<Vec<_>>()
                },
                move |bounds, lines: Vec<ShapedLine>, window: &mut Window, cx: &mut App| {
                    let time = paint_progress.get() * 4.;
                    if !(0.6..1.6).contains(&time) || bounds.size.width <= px(0.) {
                        return;
                    }
                    let phase = ((time - 0.6) * 48.).floor() / 48.;
                    let width = f32::from(bounds.size.width);
                    let left = f32::from(bounds.left()) + width * (-0.5 + 1.75 * phase);
                    for x in f32::from(bounds.left()).floor() as i32
                        ..f32::from(bounds.right()).ceil() as i32
                    {
                        let position = (x as f32 + 0.5 - left) / width;
                        let alpha = match position {
                            p if (0.0..0.2).contains(&p) => p / 0.2,
                            p if (0.2..0.3).contains(&p) => 1.,
                            p if (0.3..0.5).contains(&p) => (0.5 - p) / 0.2,
                            _ => 0.,
                        };
                        if alpha <= 0. {
                            continue;
                        }
                        let index = (alpha * 32.).ceil().clamp(1., 32.) as usize - 1;
                        let mask = Bounds::from_corners(
                            point(px(x as f32), bounds.top()),
                            point(px(x as f32 + 1.), bounds.bottom()),
                        );
                        window.with_content_mask(Some(ContentMask { bounds: mask }), |w| {
                            lines[index]
                                .paint(bounds.origin, px(21.), TextAlign::Left, None, w, cx)
                                .expect("paint review shimmer");
                        });
                    }
                },
            )
            .absolute()
            .inset_0(),
        )
        .with_animation(
            "auto-review-shimmer",
            Animation::new(Duration::from_secs(4))
                .repeat()
                .with_max_fps(48.),
            move |label, phase| {
                progress.set(phase);
                label
            },
        )
}
