//! Preserve the visible file and source line when rebuilding the virtual list.

use super::*;

#[derive(Clone)]
pub(super) struct ScrollAnchor {
    path: Option<String>,
    kind: AnchorKind,
    offset: gpui::Pixels,
}

#[derive(Clone)]
enum AnchorKind {
    Header,
    Hunk(String),
    Line { old: bool, number: u32 },
    Body,
    Comment(u64),
    Draft,
}

impl ReviewPanel {
    pub(super) fn scroll_diff_wheel(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(px(21.6));
        if delta.x.abs() > delta.y.abs() || event.modifiers.shift {
            if !self.wrap {
                let horizontal = if event.modifiers.shift && delta.y != px(0.) {
                    delta.y
                } else {
                    delta.x
                };
                let (_, _, maximum) = self.horizontal_metrics();
                self.horizontal_offset =
                    (self.horizontal_offset - f32::from(horizontal)).clamp(0., maximum);
                cx.notify();
            }
            // Run on code cells before the virtual list consumes the Y delta.
            cx.stop_propagation();
        }
    }
    pub(super) fn scroll_anchor(&self) -> Option<ScrollAnchor> {
        let offset = self.scroll.logical_scroll_top();
        let row = self.rows.get(offset.item_ix)?;
        let (file, kind) = match row {
            Row::Header(file) => (Some(*file), AnchorKind::Header),
            Row::Hunk(file, hunk) => (
                Some(*file),
                AnchorKind::Hunk(
                    self.snapshot
                        .files
                        .get(*file)?
                        .hunks
                        .get(*hunk)?
                        .header
                        .clone(),
                ),
            ),
            Row::Code { file, left, right } => {
                let line = right.as_ref().or(left.as_ref())?;
                let (old, number) = if let Some(n) = line.new {
                    (false, n)
                } else {
                    (true, line.old?)
                };
                (Some(*file), AnchorKind::Line { old, number })
            }
            Row::Binary(file) | Row::Empty(file) | Row::Preview(file) => {
                (Some(*file), AnchorKind::Body)
            }
            Row::Comment(id) => (None, AnchorKind::Comment(*id)),
            Row::Draft => (None, AnchorKind::Draft),
        };
        Some(ScrollAnchor {
            path: file
                .and_then(|i| self.snapshot.files.get(i))
                .map(|f| f.path.clone()),
            kind,
            offset: offset.offset_in_item,
        })
    }

    pub(super) fn restore_scroll(&self, anchor: Option<ScrollAnchor>) {
        let found = anchor.and_then(|anchor| {
            let file = anchor
                .path
                .as_ref()
                .and_then(|path| self.snapshot.files.iter().position(|f| &f.path == path));
            let exact = self.rows.iter().position(|row| match (&anchor.kind, row) {
                (AnchorKind::Header, Row::Header(i)) => Some(*i) == file,
                (AnchorKind::Body, Row::Binary(i) | Row::Empty(i) | Row::Preview(i)) => {
                    Some(*i) == file
                }
                (AnchorKind::Hunk(header), Row::Hunk(i, h)) => {
                    Some(*i) == file && self.snapshot.files[*i].hunks[*h].header == *header
                }
                (
                    AnchorKind::Line { old, number },
                    Row::Code {
                        file: i,
                        left,
                        right,
                    },
                ) => {
                    Some(*i) == file
                        && [left, right]
                            .into_iter()
                            .flatten()
                            .any(|line| if *old { line.old } else { line.new } == Some(*number))
                }
                (AnchorKind::Comment(id), Row::Comment(other)) => id == other,
                (AnchorKind::Draft, Row::Draft) => true,
                _ => false,
            });
            exact.map(|i| (i, anchor.offset)).or_else(|| {
                self.rows
                    .iter()
                    .position(|row| matches!(row, Row::Header(i) if Some(*i) == file))
                    .map(|i| (i, px(0.)))
            })
        });
        let (item_ix, offset_in_item) = found.unwrap_or((0, px(0.)));
        self.scroll.scroll_to(ListOffset {
            item_ix,
            offset_in_item,
        });
    }
}
