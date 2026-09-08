use super::controls::Action;
use super::*;
use crate::{components::icons::icon, theme::Theme};
use gpui::{MouseButton, Render, Role, div, list, prelude::*, rgba};

impl Render for ReviewPanel {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_cache.prepare(&self.snapshot, self.mode);
        self.last_render = std::time::Instant::now();
        let t = Theme::for_mode(self.mode);
        if self.focus_pending {
            self.focus_pending = false;
            self.focus.focus(w, cx);
        }
        if self.focus_input {
            self.focus_input = false;
            if self.commit_open {
                self.commit_input.read(cx).focus_handle(cx).focus(w, cx);
            } else {
                self.input.read(cx).focus_handle(cx).focus(w, cx);
            }
        }
        let entity = cx.entity();
        let width_entity = entity.clone();
        let rows_empty = self.rows.is_empty();
        // List holds a mutable layout borrow while asking for rows. Cache the
        // width here so headers never read ListState from its row callback.
        self.diff_width = f32::from(self.scroll.viewport_bounds().size.width);
        if !rows_empty && self.scroll.viewport_bounds().size.width == px(0.) {
            let panel = entity.clone();
            w.on_next_frame(move |_, cx| panel.update(cx, |_, cx| cx.notify()));
        }
        let (track_width, thumb_width, max_horizontal) = self.horizontal_metrics();
        self.horizontal_offset = self.horizontal_offset.clamp(0., max_horizontal);
        let content = div()
            .min_w(px(0.))
            .flex_1()
            .h_full()
            .overflow_hidden()
            .flex()
            .flex_col()
            .when(rows_empty, |d| {
                d.items_center()
                    .justify_center()
                    .gap(px(12.))
                    .child(icon("panel-review", t.text_tertiary.into()).size(px(64.)))
                    .child(
                        div()
                            .text_size(px(16.))
                            .text_color(t.text)
                            .child(if self.loading {
                                "正在加载更改…"
                            } else if self.error.is_some() {
                                "无法加载更改"
                            } else {
                                "尚无文件更改"
                            }),
                    )
                    .child(
                        div()
                            .px(px(20.))
                            .text_size(px(13.))
                            .text_color(t.text_tertiary)
                            .child(if self.scope == Scope::LastTurn {
                                "此轮没有文件更改。"
                            } else {
                                "此项目中的更改将显示在此处。"
                            }),
                    )
            })
            .when(!rows_empty, |d| {
                d.child(
                    div()
                        .id("review-horizontal-scroll")
                        .w_full()
                        .min_h(px(0.))
                        .flex_1()
                        .overflow_hidden()
                        .on_scroll_wheel(cx.listener(Self::scroll_diff_wheel))
                        .child(
                            list(self.scroll.clone(), move |index, _, cx| {
                                entity.update(cx, |s, cx| s.render_row(index, cx))
                            })
                            .w_full()
                            .h_full()
                            .min_w(px(0.)),
                        ),
                )
            })
            .when(
                !self.wrap && !rows_empty && max_horizontal > 0. && track_width > 1.,
                |d| {
                    d.child(
                        div()
                            .id("review-horizontal-bar")
                            .role(Role::ScrollBar)
                            .aria_label("水平滚动条")
                            .aria_value(self.horizontal_offset.to_string())
                            .track_focus(&self.horizontal_focus)
                            .tab_stop(true)
                            .h(px(12.))
                            .w_full()
                            .flex_none()
                            .relative()
                            .bg(t.text.alpha(0.025))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |s, e: &gpui::MouseDownEvent, w, cx| {
                                    let local =
                                        f32::from(e.position.x - s.scroll.viewport_bounds().left());
                                    s.horizontal_offset = ((local - thumb_width / 2.)
                                        / (track_width - thumb_width).max(1.)
                                        * max_horizontal)
                                        .clamp(0., max_horizontal);
                                    s.horizontal_drag = Some((e.position.x, s.horizontal_offset));
                                    s.horizontal_focus.focus(w, cx);
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            )
                            .on_key_down(cx.listener(|s, e: &KeyDownEvent, _, cx| {
                                let (_, _, max) = s.horizontal_metrics();
                                s.horizontal_offset = match e.keystroke.key.as_str() {
                                    "left" => (s.horizontal_offset - 40.).max(0.),
                                    "right" => (s.horizontal_offset + 40.).min(max),
                                    "home" => 0.,
                                    "end" => max,
                                    _ => return,
                                };
                                cx.stop_propagation();
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .id("review-horizontal-thumb")
                                    .absolute()
                                    .top(px(3.))
                                    .left(px(self.horizontal_offset / max_horizontal
                                        * (track_width - thumb_width)))
                                    .w(px(thumb_width))
                                    .h(px(6.))
                                    .rounded_full()
                                    .bg(t.text.alpha(0.25))
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|s, e: &gpui::MouseDownEvent, w, cx| {
                                            s.horizontal_drag =
                                                Some((e.position.x, s.horizontal_offset));
                                            s.horizontal_focus.focus(w, cx);
                                            cx.stop_propagation();
                                        }),
                                    ),
                            ),
                    )
                },
            )
            .when(
                self.scope.editable() && !self.snapshot.files.is_empty(),
                |d| {
                    d.child(
                        div()
                            .h(px(40.))
                            .flex_none()
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                self.button(
                                    "review-discard-all",
                                    "还原全部",
                                    None,
                                    Action::Confirm(Mutation::DiscardAll),
                                    cx,
                                )
                                .h(px(24.)),
                            )
                            .child(
                                self.button(
                                    "review-stage-all",
                                    if self.scope == Scope::Staged {
                                        "对全部取消暂存"
                                    } else {
                                        "暂存全部"
                                    },
                                    None,
                                    Action::Mutation(if self.scope == Scope::Staged {
                                        Mutation::Unstage(None)
                                    } else {
                                        Mutation::Stage(None)
                                    }),
                                    cx,
                                )
                                .h(px(24.))
                                .border_1()
                                .border_color(t.border),
                            ),
                    )
                },
            );
        div()
            .id("workspace-review")
            .role(Role::Region)
            .aria_label("审查文件更改")
            .size_full()
            .min_w(px(0.))
            .min_h(px(0.))
            .overflow_hidden()
            .relative()
            .flex()
            .flex_col()
            .bg(t.surface)
            .text_color(t.text)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .child(
                gpui::canvas(
                    move |bounds, _, cx| {
                        width_entity.update(cx, |panel, cx| {
                            let width = f32::from(bounds.size.width);
                            if (panel.panel_width - width).abs() > 1. {
                                panel.panel_width = width;
                                cx.notify();
                            }
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .on_mouse_move(cx.listener(|s, e: &gpui::MouseMoveEvent, _, cx| {
                if let Some((x, start)) = s.horizontal_drag
                    && e.pressed_button == Some(MouseButton::Left)
                {
                    let (width, thumb, max) = s.horizontal_metrics();
                    s.horizontal_offset = (start
                        + f32::from(e.position.x - x) / (width - thumb).max(1.) * max)
                        .clamp(0., max);
                    cx.stop_propagation();
                    cx.notify();
                } else if e.pressed_button.is_none() {
                    s.horizontal_drag = None;
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|s, _, _, cx| {
                    s.horizontal_drag = None;
                    s.selecting = false;
                    if let Some(d) = s.gutter_drag.take() {
                        s.begin_comment(d, cx);
                    }
                }),
            )
            .child(
                div()
                    .h(px(46.))
                    .flex_none()
                    .px(px(8.))
                    .pr(px(78.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .border_b_1()
                    .border_color(t.border)
                    .children(
                        self.file_tabs
                            .iter()
                            .enumerate()
                            .filter(|_| !self.compact())
                            .map(|(i, path)| {
                                self.button(
                                    format!("review-file-tab-{i}"),
                                    PathBuf::from(path)
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .into_owned(),
                                    None,
                                    Action::OpenTab(path.clone()),
                                    cx,
                                )
                                .max_w(px(156.))
                                .truncate()
                            }),
                    )
                    .child(
                        div()
                            .h(px(28.))
                            .w(px(156.))
                            .px(px(8.))
                            .rounded(px(10.))
                            .bg(t.text.alpha(0.05))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(icon("panel-review", t.text.into()))
                            .child(div().flex_1().text_size(px(13.)).child("审查"))
                            .child(
                                self.button(
                                    "review-close",
                                    "关闭审查标签页",
                                    Some("close-dialog"),
                                    Action::Close,
                                    cx,
                                )
                                .size(px(20.)),
                            ),
                    )
                    .child(self.button(
                        "review-new-tab",
                        "打开侧边面板标签页",
                        Some("review-plus"),
                        Action::AddTab,
                        cx,
                    ))
                    .child(div().flex_1())
                    .child(self.button(
                        "review-fullscreen",
                        "进入或退出全屏",
                        Some("settings-external"),
                        Action::Fullscreen,
                        cx,
                    )),
            )
            .child(self.toolbar(cx))
            .when(
                matches!(self.scope, Scope::Branch(_) | Scope::Commit(_)),
                |d| {
                    let label = match &self.scope {
                        Scope::Branch(base) => format!("{}  →  {base}  ⌄", self.snapshot.branch),
                        Scope::Commit(sha) => format!("{}  ⌄", &sha[..8.min(sha.len())]),
                        _ => String::new(),
                    };
                    d.child(
                        div()
                            .h(px(29.))
                            .flex_none()
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .border_b_1()
                            .border_color(t.border)
                            .child(self.button(
                                "review-base",
                                label,
                                None,
                                Action::Menu(if matches!(self.scope, Scope::Branch(_)) {
                                    Menu::Branch
                                } else {
                                    Menu::Commits
                                }),
                                cx,
                            )),
                    )
                },
            )
            .when_some(
                self.display_error().filter(|_| !self.commit_open),
                |d, e| {
                    d.child(
                        div()
                            .px(px(12.))
                            .py(px(8.))
                            .flex_none()
                            .text_size(px(12.))
                            .text_color(deletion_color(self.mode))
                            .child(e),
                    )
                },
            )
            .child(
                div()
                    .min_h(px(0.))
                    .flex_1()
                    .w_full()
                    .flex()
                    .overflow_hidden()
                    .child(content)
                    .when(self.tree_open && !self.compact(), |d| {
                        d.child(self.tree(cx))
                    }),
            )
            .when_some(self.notice.clone(), |d, n| {
                d.child(
                    div()
                        .id("review-toast")
                        .role(Role::Status)
                        .aria_label(n.clone())
                        .absolute()
                        .bottom(px(48.))
                        .right(px(16.))
                        .px(px(12.))
                        .py(px(6.))
                        .rounded(px(10.))
                        .bg(t.elevated)
                        .border_1()
                        .border_color(t.border)
                        .text_size(px(12.))
                        .text_color(t.text_secondary)
                        .child(n),
                )
            })
            .when_some(
                self.menu
                    .clone()
                    .filter(|m| !matches!(m, Menu::CommitBranch | Menu::PullRequestBase)),
                |d, m| d.child(self.popup(m, cx)),
            )
    }
}

impl ReviewPanel {
    pub(super) fn horizontal_metrics(&self) -> (f32, f32, f32) {
        let track = f32::from(self.scroll.viewport_bounds().size.width).max(1.);
        let visible =
            (track / if self.split { 2. } else { 1. } - self.render_cache.gutter_width - 14.4492)
                .max(1.);
        let max = (self.max_line_width - visible).max(0.);
        let thumb = (track * visible / self.max_line_width.max(1.)).clamp(track.min(28.), track);
        (track, thumb, max)
    }
}

pub(super) fn addition_color(mode: ThemeMode) -> gpui::Rgba {
    if mode == ThemeMode::Dark {
        rgba(0x40c977ff)
    } else {
        rgba(0x00a240ff)
    }
}
pub(super) fn deletion_color(mode: ThemeMode) -> gpui::Rgba {
    if mode == ThemeMode::Dark {
        rgba(0xfa423eff)
    } else {
        rgba(0xe02e2aff)
    }
}
