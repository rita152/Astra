use super::*;
use gpui::{Bounds, MouseButton, TestApp, WindowBounds, WindowOptions, point, size};

fn fixture(cx: &mut Context<ReviewPanel>) -> ReviewPanel {
    let mut panel = ReviewPanel::new(std::env::temp_dir(), ThemeMode::Light, cx);
    panel.generation += 1;
    panel.loading = true;
    panel.tree_open = false;
    let patch = "diff --git a/demo.rs b/demo.rs\n--- a/demo.rs\n+++ b/demo.rs\n@@ -1,3 +1,3 @@\n alpha βeta\n-old 中文\n+new 中文🙂\n omega\n";
    panel.snapshot = Arc::new(Snapshot {
        files: git_review::parse_unified(patch),
        root: std::env::temp_dir(),
        ..Default::default()
    });
    panel.rebuild(cx);
    panel
}

#[test]
fn native_keyboard_menu_and_select_all_copy_work_in_the_diff() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(0.), px(0.)),
                size(px(800.), px(500.)),
            ))),
            ..Default::default()
        },
        |_, cx| fixture(cx),
    );
    window.draw();
    window.simulate_click(point(px(40.), px(64.)), MouseButton::Left);
    window.draw();
    window.simulate_keystrokes("down down");
    assert_eq!(window.read(|p, _| p.menu_selected), 2);
    window.simulate_keystrokes("escape");
    assert!(window.read(|p, _| p.menu.is_none()));
    window.simulate_click(point(px(100.), px(151.)), MouseButton::Left);
    window.simulate_keystrokes("cmd-a cmd-c");
    assert_eq!(
        app.read_from_clipboard().and_then(|c| c.text()),
        Some("alpha βeta\nold 中文\nnew 中文🙂\nomega".into())
    );
}

#[test]
fn comment_and_commit_drafts_have_independent_multiline_editors() {
    let mut app = TestApp::new();
    app.update(crate::components::file_editor::init);
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(0.), px(0.)),
                size(px(800.), px(500.)),
            ))),
            ..Default::default()
        },
        |_, cx| fixture(cx),
    );
    window.update(|p, _, cx| {
        p.begin_comment(
            Draft {
                file: 0,
                start: 2,
                end: 2,
                old: false,
            },
            cx,
        )
    });
    window.draw();
    window.simulate_input("第一行 🙂");
    window.simulate_keystrokes("enter");
    window.simulate_input("第二行");
    window.simulate_keystrokes("cmd-z");
    assert_eq!(
        window.read(|p, cx| p.input.read(cx).text().to_owned()),
        "第一行 🙂\n"
    );
    window.update(|p, _, cx| {
        p.open_commit(cx);
        p.commit_input
            .update(cx, |i, cx| i.set_text_silently("Commit message", cx));
    });
    assert_eq!(
        window.read(|p, cx| p.input.read(cx).text().to_owned()),
        "第一行 🙂\n"
    );
    window.update(|p, _, cx| {
        p.commit_open = false;
        p.save_comment(cx);
    });
    assert_eq!(window.read(|p, _| p.comments[0].text.clone()), "第一行 🙂");
}

#[test]
fn history_updates_in_the_same_turn_do_not_read_current_files() {
    let mut app = TestApp::new();
    let panel = app.new_entity(fixture);
    app.update_entity(&panel,|p,cx|{
        for text in ["one","two"]{let patch=format!("diff --git a/history b/history\n--- a/history\n+++ b/history\n@@ -9 +9 @@\n-old\n+{text}\n");
            p.set_last_turn(DiffReviewPresentation::from_unified_diff("same-turn","上一轮",&patch,None),true,cx);
            p.generation+=1;p.loading=true;
            assert_eq!(p.snapshot.files[0].hunks[0].lines[1].text,text);assert_eq!(p.snapshot.files[0].patch,patch);
        }
    });
}

#[test]
fn horizontal_scrollbar_drag_and_keyboard_reach_both_limits() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(0.), px(0.)),
                size(px(400.), px(300.)),
            ))),
            ..Default::default()
        },
        |_, cx| {
            let mut p = fixture(cx);
            Arc::make_mut(&mut p.snapshot).files[0].hunks[0].lines[0].text =
                "long source line ".repeat(40);
            p.rebuild(cx);
            p
        },
    );
    window.draw();
    window.draw();
    window.simulate_click(point(px(20.), px(254.)), MouseButton::Left);
    window.simulate_keystrokes("end");
    window.draw();
    assert!(window.read(|p, _| p.horizontal_offset) > 100.);
    window.simulate_keystrokes("home");
    window.draw();
    assert_eq!(window.read(|p, _| p.horizontal_offset), 0.);
    window.simulate_mouse_down(point(px(10.), px(254.)), MouseButton::Left);
    window.simulate_event(gpui::MouseMoveEvent {
        position: point(px(350.), px(254.)),
        pressed_button: Some(MouseButton::Left),
        modifiers: Default::default(),
    });
    window.simulate_mouse_up(point(px(350.), px(254.)), MouseButton::Left);
    window.draw();
    assert!(window.read(|p, _| p.horizontal_offset) > 100.);
}

#[test]
fn split_and_unified_layouts_keep_the_visible_source_line_after_layout() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(0.), px(0.)), size(px(800.), px(500.))))),
            ..Default::default()
        },
        |_, cx| {
            let mut p = fixture(cx);
            let mut patch = "diff --git a/long.rs b/long.rs\n--- a/long.rs\n+++ b/long.rs\n@@ -1,260 +1,260 @@\n".to_owned();
            for i in 1..=260 { patch.push_str(&format!("-old line {i}\n")); }
            for i in 1..=260 { patch.push_str(&format!("+new line {i}\n")); }
            Arc::make_mut(&mut p.snapshot).files = git_review::parse_unified(&patch);
            p.rebuild(cx);
            p
        },
    );
    window.draw();
    window.update(|p, _, _| {
        let item_ix = p
            .rows
            .iter()
            .position(
                |row| matches!(row, Row::Code { right: Some(line), .. } if line.new == Some(138)),
            )
            .unwrap();
        p.scroll.scroll_to(ListOffset {
            item_ix,
            offset_in_item: px(7.),
        });
    });
    window.draw();
    for split in [true, false] {
        window.update(|p, _, cx| {
            p.split = split;
            p.rebuild(cx);
        });
        window.draw();
        window.draw();
        window.read(|p, _| {
            let offset = p.scroll.logical_scroll_top();
            assert!(matches!(&p.rows[offset.item_ix], Row::Code { right: Some(line), .. } if line.new == Some(138)));
            assert_eq!(offset.offset_in_item, px(7.));
        });
    }
}

#[test]
fn opened_history_stays_pinned_until_latest_scope_is_selected() {
    let mut app = TestApp::new();
    let mut window = app.open_window(|_, cx| fixture(cx));
    let review = |name: &str| {
        DiffReviewPresentation::from_unified_diff(
            name,
            "上一轮",
            &format!(
                "diff --git a/history b/history\n--- a/history\n+++ b/history\n@@ -1 +1 @@\n-old\n+{name}\n"
            ),
            None,
        )
    };
    window.update(|p, w, cx| {
        p.set_last_turn(review("earlier"), true, cx);
        p.generation += 1;
        p.loading = true;
        p.set_last_turn(review("latest"), false, cx);
        assert_eq!(p.snapshot.files[0].hunks[0].lines[1].text, "earlier");
        p.action(controls::Action::Scope(Scope::LastTurn), w, cx);
        assert_eq!(p.snapshot.files[0].hunks[0].lines[1].text, "latest");
        assert!(!p.history_pinned);
    });
}

#[test]
fn comments_remain_editable_after_their_file_leaves_the_diff() {
    let mut app = TestApp::new();
    app.update(crate::components::file_editor::init);
    let mut window = app.open_window(|_, cx| fixture(cx));
    window.update(|p, _, cx| {
        p.begin_comment(
            Draft {
                file: 0,
                start: 2,
                end: 2,
                old: false,
            },
            cx,
        );
        p.input
            .update(cx, |input, cx| input.set_text_silently("原始评论", cx));
        p.save_comment(cx);
        Arc::make_mut(&mut p.snapshot).files.clear();
        p.rebuild(cx);
        assert!(p.rows.iter().any(|row| matches!(row, Row::Comment(1))));
        p.edit_comment(1, cx);
    });
    window.draw();
    window.simulate_keystrokes("cmd-a");
    window.simulate_input("修改后的评论 🙂");
    window.simulate_keystrokes("enter");
    window.simulate_input("第二行");
    window.simulate_keystrokes("cmd-enter");
    window.read(|p, _| {
        assert_eq!(p.comments.len(), 1);
        assert_eq!(p.comments[0].text, "修改后的评论 🙂\n第二行");
        assert_eq!(p.comments[0].path, "demo.rs");
        assert!(p.editing_comment.is_none());
    });
    window.update(|p, w, cx| {
        p.edit_comment(1, cx);
        p.input
            .update(cx, |input, cx| input.set_text_silently("取消的编辑", cx));
        p.dismiss_transient(w, cx);
        assert_eq!(p.comments[0].text, "修改后的评论 🙂\n第二行");
    });
}

#[test]
fn shift_wheel_over_code_scrolls_horizontally_without_moving_the_source_line() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(0.), px(0.)),
                size(px(400.), px(300.)),
            ))),
            ..Default::default()
        },
        |_, cx| {
            let mut p = fixture(cx);
            let hunk = &mut Arc::make_mut(&mut p.snapshot).files[0].hunks[0];
            hunk.lines = (1..=100)
                .map(|n| Line {
                    old: Some(n),
                    new: Some(n),
                    text: "long source ".repeat(40),
                    kind: LineKind::Context,
                })
                .collect();
            p.rebuild(cx);
            p
        },
    );
    window.draw();
    window.draw();
    let before = window.read(|p, _| p.scroll.logical_scroll_top());
    window.simulate_event(gpui::ScrollWheelEvent {
        position: point(px(200.), px(170.)),
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-100.))),
        modifiers: gpui::Modifiers {
            shift: true,
            ..Default::default()
        },
        ..Default::default()
    });
    window.draw();
    window.read(|p, _| {
        assert!(p.horizontal_offset > 0.);
        let after = p.scroll.logical_scroll_top();
        assert_eq!(after.item_ix, before.item_ix);
        assert_eq!(after.offset_in_item, before.offset_in_item);
    });
}

#[test]
fn five_and_six_digit_line_numbers_do_not_double_the_diff_row_height() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(0.), px(0.)), size(px(710.), px(500.))))),
            ..Default::default()
        },
        |_, cx| {
            let mut panel = fixture(cx);
            Arc::make_mut(&mut panel.snapshot).files = git_review::parse_unified(
                "diff --git a/long.rs b/long.rs\n--- a/long.rs\n+++ b/long.rs\n@@ -99999,3 +99999,3 @@\n line 99999\n line 100000\n line 100001\n",
            );
            panel.rebuild(cx);
            panel
        },
    );
    for split in [false, true] {
        window.update(|p, _, cx| {
            p.split = split;
            p.rebuild(cx);
        });
        window.draw();
        window.draw();
        window.read(|p, _| {
            for (i, row) in p.rows.iter().enumerate() {
                if matches!(row, Row::Code { .. }) {
                    let height = f32::from(p.scroll.bounds_for_item(i).unwrap().size.height);
                    // Layout snaps the 21.6-point row to the display pixel grid.
                    assert!(
                        (height - 21.6).abs() <= 0.5,
                        "source row unexpectedly wrapped: {height}"
                    );
                }
            }
        });
    }
}
