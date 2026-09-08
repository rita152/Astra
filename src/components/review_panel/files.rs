use super::*;
use super::{
    controls::Action,
    render::{addition_color, deletion_color},
};
use crate::{
    components::{
        icons::icon,
        markdown::{MarkdownPreview, parse_markdown},
    },
    theme::{Theme, UI_MONOSPACE_FONT_FAMILY},
};
use gpui::{
    AnyElement, Div, MouseButton, Role, SharedString, Stateful, StyledText, div, prelude::*, rgba,
};

impl ReviewPanel {
    pub(super) fn tree(&self, cx: &Context<Self>) -> Stateful<Div> {
        let t = Theme::for_mode(self.mode);
        let mut rows = div()
            .id("review-tree-scroll")
            .min_h(px(0.))
            .flex_1()
            .overflow_y_scroll()
            .p(px(8.))
            .flex()
            .flex_col();
        let mut folders = HashSet::new();
        let matched = self.matching_files(&self.query);
        for i in matched.iter().copied() {
            let file = &self.snapshot.files[i];
            let parts = file.path.split('/').collect::<Vec<_>>();
            let mut hidden = false;
            for depth in 0..parts.len().saturating_sub(1) {
                let path = parts[..=depth].join("/");
                if hidden {
                    break;
                }
                if folders.insert(path.clone()) {
                    let collapsed = self.folder_collapsed.contains(&path);
                    let id: SharedString = format!("review-folder-{path}").into();
                    let toggle = path.clone();
                    rows = rows.child(
                        div()
                            .id(id)
                            .role(Role::TreeItem)
                            .aria_label(path.clone())
                            .h(px(28.))
                            .pl(px(6. + depth as f32 * 14.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .rounded(px(6.))
                            .text_size(px(13.))
                            .text_color(t.text_secondary)
                            .cursor_pointer()
                            .hover(move |b| b.bg(t.sidebar_hover))
                            .on_click(cx.listener(move |s, _, _, cx| {
                                if !s.folder_collapsed.remove(&toggle) {
                                    s.folder_collapsed.insert(toggle.clone());
                                }
                                cx.notify();
                            }))
                            .child(
                                icon(
                                    if collapsed {
                                        "settings-chevron-right"
                                    } else {
                                        "chevron-down"
                                    },
                                    t.text_tertiary.into(),
                                )
                                .size(px(12.)),
                            )
                            .child(parts[depth].to_owned()),
                    );
                }
                if self.folder_collapsed.contains(&path) {
                    hidden = true;
                }
            }
            if hidden {
                continue;
            }
            let selected = i == self.selected_file;
            let path = file.path.clone();
            rows = rows.child(
                div()
                    .id(("review-tree-file", i))
                    .role(Role::TreeItem)
                    .aria_label(path)
                    .focusable()
                    .tab_stop(true)
                    .h(px(28.))
                    .pl(px(7. + parts.len().saturating_sub(1) as f32 * 14.))
                    .pr(px(5.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(6.))
                    .text_size(px(13.))
                    .text_color(t.text)
                    .cursor_pointer()
                    .when(selected, |b| b.bg(t.sidebar_hover))
                    .hover(move |b| b.bg(t.sidebar_hover))
                    .on_click(cx.listener(move |s, _, _, cx| s.jump_to(i, cx)))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |s, _, _, cx| {
                            s.toggle_menu(Menu::File(i), cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_key_down(cx.listener(move |s, e: &KeyDownEvent, _, cx| {
                        match e.keystroke.key.as_str() {
                            "enter" | "space" => s.jump_to(i, cx),
                            "down" => s.jump_to((i + 1).min(s.snapshot.files.len() - 1), cx),
                            "up" => s.jump_to(i.saturating_sub(1), cx),
                            _ => return,
                        }
                        cx.stop_propagation();
                    }))
                    .child(icon(file_glyph(&file.path), t.text_secondary.into()).size(px(16.)))
                    .child(
                        div()
                            .min_w(px(0.))
                            .flex_1()
                            .truncate()
                            .child(parts.last().unwrap_or(&"").to_string()),
                    )
                    .child(
                        div()
                            .text_color(if file.status == 'A' {
                                addition_color(self.mode)
                            } else {
                                rgba(0xff9658ff)
                            })
                            .child(match file.status {
                                'A' => "⊞",
                                'D' => "⊟",
                                _ => "⊡",
                            }),
                    ),
            );
        }
        if matched.is_empty() {
            rows = rows.child(
                div()
                    .p(px(8.))
                    .text_size(px(13.))
                    .text_color(t.text_tertiary)
                    .child("没有匹配的文件"),
            );
        }
        div()
            .id("review-file-tree")
            .role(Role::Tree)
            .aria_label("审查文件")
            .w(px(250.))
            .min_w(px(160.))
            .max_w(gpui::relative(0.4))
            .h_full()
            .flex_none()
            .border_l_1()
            .border_color(t.border)
            .flex()
            .flex_col()
            .child(
                div()
                    .m(px(8.))
                    .h(px(28.))
                    .px(px(8.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(t.border)
                    .bg(t.text.alpha(0.03))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(icon("review-jump", t.text_tertiary.into()).size(px(14.)))
                    .child(div().min_w(px(0.)).flex_1().child(self.filter.clone())),
            )
            .child(rows)
    }

    pub(super) fn file_header(&self, i: usize, cx: &Context<Self>) -> Stateful<Div> {
        let t = Theme::for_mode(self.mode);
        let file = &self.snapshot.files[i];
        let path = file.path.clone();
        let collapsed = self.collapsed.contains(&path);
        div()
            .id(("review-file-header", i))
            .role(Role::Group)
            .aria_label(path.clone())
            .group("review-file-header")
            .h(px(32.))
            .w_full()
            .flex_none()
            .px(px(8.))
            .bg(t.surface)
            .border_b_1()
            .border_color(t.border)
            .flex()
            .items_center()
            .gap(px(4.))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |s, _, _, cx| s.toggle_menu(Menu::File(i), cx)),
            )
            .child(icon(file_glyph(&path), t.text_secondary.into()).size(px(16.)))
            .child(
                self.button(
                    format!("review-file-path-{i}"),
                    path.clone(),
                    None,
                    Action::Toggle(i),
                    cx,
                )
                .min_w(px(0.))
                .flex_shrink(1.)
                .truncate()
                .text_size(px(14.)),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .gap(px(4.))
                    .text_size(px(13.))
                    .when(self.diff_width < 260., |d| d.hidden())
                    .child(
                        div()
                            .text_color(addition_color(self.mode))
                            .child(format!("+{}", file.additions)),
                    )
                    .child(
                        div()
                            .text_color(deletion_color(self.mode))
                            .child(format!("-{}", file.deletions)),
                    ),
            )
            .child(div().flex_1().min_w(px(0.)))
            .child(
                self.button(
                    format!("review-copy-{i}"),
                    "复制路径",
                    Some("message-copy"),
                    Action::Copy(path.clone()),
                    cx,
                )
                .w(px(24.))
                .opacity(0.)
                .group_hover("review-file-header", |b| b.opacity(1.))
                .when(self.diff_width < 280., |b| b.hidden()),
            )
            .child(
                self.button(
                    format!("review-fold-{i}"),
                    "切换文件差异对比",
                    Some(if collapsed {
                        "settings-chevron-right"
                    } else {
                        "chevron-down"
                    }),
                    Action::Toggle(i),
                    cx,
                )
                .w(px(20.)),
            )
            .child(
                self.button(
                    format!("review-open-{i}"),
                    "打开位置",
                    Some("review-open"),
                    Action::Open(i),
                    cx,
                )
                .w(px(20.))
                .opacity(0.)
                .group_hover("review-file-header", |b| b.opacity(1.))
                .when(self.diff_width < 280., |b| b.hidden()),
            )
            .when(self.scope.editable(), |d| {
                d.child(
                    self.button(
                        format!("review-restore-{i}"),
                        "还原文件",
                        Some("review-restore"),
                        Action::Confirm(Mutation::Discard(file.path.clone())),
                        cx,
                    )
                    .size(px(20.)),
                )
                .child(
                    self.button(
                        format!("review-stage-{i}"),
                        if self.scope == Scope::Staged {
                            "对文件取消暂存"
                        } else {
                            "暂存文件"
                        },
                        Some(if self.scope == Scope::Staged {
                            "review-minus"
                        } else {
                            "review-plus"
                        }),
                        Action::Mutation(if self.scope == Scope::Staged {
                            Mutation::Unstage(Some(file.path.clone()))
                        } else {
                            Mutation::Stage(Some(file.path.clone()))
                        }),
                        cx,
                    )
                    .size(px(20.)),
                )
            })
            .when(
                matches!(self.scope, Scope::Branch(_) | Scope::Commit(_)),
                |d| {
                    d.child(
                        self.button(
                            format!("review-viewed-{i}"),
                            if self.viewed.get(&file.path) == Some(&file.patch) {
                                "✓ 已查看"
                            } else {
                                "标记为已查看"
                            },
                            None,
                            Action::Viewed(i),
                            cx,
                        )
                        .h(px(24.))
                        .text_size(px(12.)),
                    )
                },
            )
    }

    pub(super) fn code_cell(
        &mut self,
        index: usize,
        file: usize,
        line: Option<&Line>,
        old: bool,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let t = Theme::for_mode(self.mode);
        let dark = self.mode == ThemeMode::Dark;
        let Some(line) = line else {
            return div()
                .id(("review-empty-cell", index * 2 + old as usize))
                .on_scroll_wheel(cx.listener(Self::scroll_diff_wheel))
                .flex_1()
                .min_w(px(0.))
                .h(px(21.6))
                .bg(t.text.alpha(0.025));
        };
        let added = line.kind == LineKind::Added;
        let deleted = line.kind == LineKind::Deleted;
        let accent = if added {
            addition_color(self.mode)
        } else if deleted {
            deletion_color(self.mode)
        } else {
            t.text_tertiary
        };
        let bg = if added {
            if dark {
                rgba(0x1e3225ff)
            } else {
                rgba(0xe7f4e7ff)
            }
        } else if deleted {
            if dark {
                rgba(0x412121ff)
            } else {
                rgba(0xffe9e9ff)
            }
        } else {
            t.surface
        };
        let number = if old { line.old } else { line.new.or(line.old) }.unwrap_or(0);
        let comment_old = old || line.kind == LineKind::Deleted;
        let draft = Draft {
            file,
            start: number,
            end: number,
            old: comment_old,
        };
        let move_draft = draft.clone();
        let selected = self.selection.filter(|s| s.old == old).and_then(|s| {
            let a = s.anchor.min(s.head);
            let b = s.anchor.max(s.head);
            (index >= a.row && index <= b.row).then_some(
                (if index == a.row {
                    a.byte.min(line.text.len())
                } else {
                    0
                })..(if index == b.row {
                    b.byte.min(line.text.len())
                } else {
                    line.text.len()
                }),
            )
        });
        let code = line.text.clone();
        let spans = self.render_cache.syntax_runs(file, line, t);
        let word_span = if self.words && (added || deleted) {
            self.render_cache.word_span(file, line)
        } else {
            None
        };
        let runs = decorate_runs(
            &spans,
            word_span.map(|r| (r, accent.alpha(0.25).into())),
            selected.map(|r| (r, t.accent.alpha(0.4).into())),
        );
        let content = StyledText::new(code.clone()).with_runs(runs);
        let down_layout = content.layout().clone();
        let move_layout = down_layout.clone();
        div()
            .id(("review-code-cell", index * 2 + old as usize))
            .on_scroll_wheel(cx.listener(Self::scroll_diff_wheel))
            .min_w(px(0.))
            .flex_1()
            .min_h(px(21.6))
            .flex()
            .items_stretch()
            .bg(bg)
            .font_family(UI_MONOSPACE_FONT_FAMILY)
            .text_size(px(12.))
            .line_height(px(21.6))
            .text_color(t.file_editor_text)
            .child(
                div()
                    .id(("review-line-number", index * 2 + old as usize))
                    .role(Role::Button)
                    .aria_label(format!(
                        "在 {} 第 {}{} 行添加评论",
                        self.snapshot.files[file].path,
                        if comment_old { "L" } else { "R" },
                        number
                    ))
                    .w(px(self.render_cache.gutter_width))
                    .flex_none()
                    .min_h(px(21.6))
                    .border_l(px(4.))
                    .border_color(if added || deleted { accent } else { t.surface })
                    .px(px(7.))
                    .text_right()
                    .whitespace_nowrap()
                    .text_color(accent)
                    .cursor_pointer()
                    .hover(move |s| s.bg(t.accent.alpha(0.25)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |s, _, _, cx| {
                            s.gutter_drag = Some(draft.clone());
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_move(cx.listener(move |s, e: &gpui::MouseMoveEvent, _, cx| {
                        if e.pressed_button == Some(MouseButton::Left)
                            && let Some(d) = s.gutter_drag.as_mut()
                            && d.file == move_draft.file
                            && d.old == move_draft.old
                        {
                            d.end = move_draft.end;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        if let Some(d) = s.gutter_drag.take() {
                            s.begin_comment(d, cx);
                        }
                        cx.stop_propagation();
                    }))
                    .child(number.to_string()),
            )
            .child(
                div()
                    .id(("review-line-text", index * 2 + old as usize))
                    .min_w(px(0.))
                    .flex_1()
                    .overflow_hidden()
                    .px(px(7.2246))
                    .cursor_text()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |s, e: &gpui::MouseDownEvent, w, cx| {
                            let byte = down_layout
                                .index_for_position(e.position)
                                .unwrap_or_else(|i| i)
                                .min(code.len());
                            let cursor = TextCursor { row: index, byte };
                            if e.modifiers.shift && s.selection.is_some_and(|v| v.old == old) {
                                s.selection.as_mut().unwrap().head = cursor;
                            } else {
                                s.selection = Some(Selection {
                                    anchor: cursor,
                                    head: cursor,
                                    old,
                                });
                            }
                            if e.click_count >= 3 {
                                s.selection = Some(Selection {
                                    anchor: TextCursor {
                                        row: index,
                                        byte: 0,
                                    },
                                    head: TextCursor {
                                        row: index,
                                        byte: code.len(),
                                    },
                                    old,
                                });
                            } else if e.click_count == 2 {
                                let r = word_at(&code, byte);
                                s.selection = Some(Selection {
                                    anchor: TextCursor {
                                        row: index,
                                        byte: r.start,
                                    },
                                    head: TextCursor {
                                        row: index,
                                        byte: r.end,
                                    },
                                    old,
                                });
                            }
                            s.selecting = true;
                            s.focus.focus(w, cx);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .on_mouse_move(cx.listener(move |s, e: &gpui::MouseMoveEvent, _, cx| {
                        if s.selecting
                            && e.pressed_button == Some(MouseButton::Left)
                            && let Some(selection) = s.selection.as_mut()
                            && selection.old == old
                        {
                            selection.head = TextCursor {
                                row: index,
                                byte: move_layout
                                    .index_for_position(e.position)
                                    .unwrap_or_else(|i| i),
                            };
                            cx.notify();
                        }
                    }))
                    .child(
                        div()
                            .when(self.wrap, |d| d.w_full().whitespace_normal())
                            .when(!self.wrap, |d| {
                                d.w(px(self.max_line_width))
                                    .relative()
                                    .left(px(-self.horizontal_offset))
                                    .whitespace_nowrap()
                            })
                            .child(content),
                    ),
            )
    }

    pub(super) fn render_row(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let t = Theme::for_mode(self.mode);
        match self.rows[index].clone() {
            Row::Header(i) => self.file_header(i, cx).into_any_element(),
            Row::Hunk(i, h) => {
                let file = &self.snapshot.files[i];
                let reverse = self.scope == Scope::Staged;
                let path = file.path.clone();
                div()
                    .w_full()
                    .h(px(26.))
                    .px(px(8.))
                    .bg(t.accent.alpha(0.05))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.))
                    .text_color(t.text_tertiary)
                    .child(
                        div()
                            .min_w(px(0.))
                            .flex_1()
                            .truncate()
                            .child(file.hunks[h].header.clone()),
                    )
                    .child(
                        self.button(
                            format!("review-context-{i}-{h}"),
                            if self.expanded_files.contains(&file.path) {
                                "收起上下文"
                            } else {
                                "展开上下文"
                            },
                            Some("chevron-down"),
                            Action::Context(i),
                            cx,
                        )
                        .h(px(22.)),
                    )
                    .when(self.scope.editable() && file.status == 'M', |d| {
                        d.child(
                            self.button(
                                format!("review-hunk-{i}-{h}"),
                                if reverse {
                                    "取消暂存差异块"
                                } else {
                                    "暂存差异块"
                                },
                                Some(if reverse {
                                    "review-minus"
                                } else {
                                    "review-plus"
                                }),
                                Action::Mutation(Mutation::Hunk {
                                    path,
                                    index: h,
                                    reverse,
                                }),
                                cx,
                            )
                            .h(px(22.)),
                        )
                    })
                    .into_any_element()
            }
            Row::Code { file, left, right } => {
                let mut row = div().w_full().min_h(px(21.6)).flex().items_stretch();
                if self.split {
                    row = row
                        .child(self.code_cell(index, file, left.as_ref(), true, cx))
                        .child(div().w(px(1.)).bg(t.border));
                }
                row.child(self.code_cell(index, file, right.as_ref(), false, cx))
                    .into_any_element()
            }
            Row::Comment(id) => {
                if let Some(c) = self.comments.iter().find(|c| c.id == id) {
                    self.comment_card(c.clone(), self.editing_comment == Some(id), cx)
                        .into_any_element()
                } else {
                    div().into_any_element()
                }
            }
            Row::Draft => {
                let d = self.draft.as_ref().expect("draft row");
                self.comment_card(
                    Comment {
                        id: 0,
                        path: String::new(),
                        start: d.start.min(d.end),
                        end: d.start.max(d.end),
                        old: d.old,
                        text: String::new(),
                    },
                    true,
                    cx,
                )
                .into_any_element()
            }
            Row::Binary(i) => div()
                .p(px(28.))
                .text_size(px(13.))
                .text_color(t.text_tertiary)
                .child("二进制文件内容已更改")
                .child(self.button(
                    format!("review-binary-open-{i}"),
                    "打开文件",
                    None,
                    Action::Open(i),
                    cx,
                ))
                .into_any_element(),
            Row::Empty(i) => div()
                .p(px(20.))
                .text_size(px(13.))
                .text_color(t.text_tertiary)
                .child(if self.snapshot.files[i].old_path.is_some() {
                    "文件已重命名，内容未更改"
                } else {
                    "没有文本差异"
                })
                .into_any_element(),
            Row::Preview(i) => {
                let file = &self.snapshot.files[i];
                let path = file.path.clone();
                let mode = self.mode;
                let text = file.new_text.clone().unwrap_or_default();
                let preview = self.previews.entry(path).or_insert_with(|| {
                    cx.new(|cx| MarkdownPreview::new(parse_markdown(&text), mode, cx))
                });
                div()
                    .w_full()
                    .h(px(440.))
                    .overflow_hidden()
                    .child(preview.clone())
                    .into_any_element()
            }
        }
    }
}

fn file_glyph(path: &str) -> &'static str {
    if path.ends_with(".rs") {
        "markdown-file-rust"
    } else if path.ends_with(".md") {
        "markdown-file-document"
    } else {
        "panel-files"
    }
}

fn decorate_runs(
    spans: &[(std::ops::Range<usize>, gpui::TextRun)],
    word: Option<(std::ops::Range<usize>, gpui::Hsla)>,
    selection: Option<(std::ops::Range<usize>, gpui::Hsla)>,
) -> Vec<gpui::TextRun> {
    let mut result = Vec::new();
    for (span, run) in spans {
        let mut cuts = vec![span.start, span.end];
        for (range, _) in [&word, &selection].into_iter().flatten() {
            if range.start > span.start && range.start < span.end {
                cuts.push(range.start);
            }
            if range.end > span.start && range.end < span.end {
                cuts.push(range.end);
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        for p in cuts.windows(2) {
            let mut r = run.clone();
            r.len = p[1] - p[0];
            for (range, color) in [&word, &selection].into_iter().flatten() {
                if range.contains(&p[0]) {
                    r.background_color = Some(*color);
                }
            }
            result.push(r);
        }
    }
    result
}
fn word_at(text: &str, byte: usize) -> std::ops::Range<usize> {
    let mut a = byte.min(text.len());
    while !text.is_char_boundary(a) {
        a -= 1;
    }
    let mut b = a;
    while a > 0 {
        let (i, c) = text[..a].char_indices().next_back().unwrap();
        if !c.is_alphanumeric() && c != '_' {
            break;
        }
        a = i;
    }
    while b < text.len() {
        let c = text[b..].chars().next().unwrap();
        if !c.is_alphanumeric() && c != '_' {
            break;
        }
        b += c.len_utf8();
    }
    a..b
}
