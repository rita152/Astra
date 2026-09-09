use super::*;
use crate::agent::{AgentAutoApprovalReview, AgentAutoApprovalReviewKey};
use gpui::{Bounds, KeyUpEvent, Keystroke, TestApp, WindowBounds, WindowOptions, point, size};

fn model(status: Status) -> AutoApprovalReviewPresentation {
    AutoApprovalReviewPresentation {
        review: AgentAutoApprovalReview {
            key: AgentAutoApprovalReviewKey {
                thread_id: "thread".into(),
                turn_id: "turn".into(),
                review_id: "review".into(),
            },
            target_item_id: None,
            action: Action::NetworkAccess {
                host: "example.com".into(),
                port: 443,
                protocol: "https".into(),
                target: "example.com:443".into(),
            },
            status,
            rationale: Some("This action only reads public information.".into()),
            risk_level: Some("low".into()),
            user_authorization: Some("high".into()),
            started_at_ms: 100,
            completed_at_ms: (status != Status::InProgress).then_some(200),
            decision_source: (status != Status::InProgress).then(|| "agent".into()),
        },
        closed_locally: false,
        attached_to_item: false,
    }
}

fn options() -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(800.), px(400.)),
        })),
        ..Default::default()
    }
}

#[test]
fn auto_approval_pointer_focus_enter_and_space_release_toggle_the_disclosure() {
    let mut app = TestApp::new();
    app.update(|cx| cx.set_reduce_motion(true));
    let mut window = app.open_window_with_options(options(), |_, cx| {
        AutoApprovalReviewView::new(model(Status::InProgress), ThemeMode::Dark, cx)
    });
    window.draw();
    window.simulate_click(point(px(1.), px(10.)), MouseButton::Left);
    assert!(window.read(|s, _| s.expanded));
    window.draw();
    window.simulate_keystrokes("enter");
    assert!(!window.read(|s, _| s.expanded));
    window.draw();
    window.simulate_keystrokes("space");
    assert!(!window.read(|s, _| s.expanded));
    window.update(|s, w, _| {
        assert!(
            s.action_focus.is_focused(w),
            "action focus lost after Space"
        )
    });
    window.simulate_event(KeyUpEvent {
        keystroke: Keystroke::parse("space").unwrap(),
    });
    assert!(window.read(|s, _| s.expanded));
    window.draw();
    window.simulate_click(point(px(700.), px(10.)), MouseButton::Left);
    assert!(window.read(|s, _| s.expanded));
    window.simulate_click(point(px(80.), px(10.)), MouseButton::Left);
    assert!(!window.read(|s, _| s.expanded));
}

#[test]
fn auto_approval_completion_preserves_disclosures_and_copy_uses_only_selected_text() {
    let mut app = TestApp::new();
    app.update(|cx| cx.set_reduce_motion(true));
    app.update(init);
    let mut window = app.open_window_with_options(options(), |_, cx| {
        AutoApprovalReviewView::new(model(Status::InProgress), ThemeMode::Light, cx)
    });
    window.draw();
    window.simulate_click(point(px(40.), px(10.)), MouseButton::Left);
    window.draw();
    window.simulate_keystrokes("tab enter");
    window.draw();
    assert!(window.read(|s, _| s.details_expanded));
    window.update(|s, _, cx| s.sync(model(Status::TimedOut), ThemeMode::Light, cx));
    window.draw();
    assert!(window.read(|s, _| s.expanded && s.details_expanded));
    window.simulate_click(point(px(10.), px(58.)), MouseButton::Left);
    window.draw();
    window.simulate_keystrokes("cmd-a cmd-c");
    app.update(|cx| {
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("This action only reads public information.")
        )
    });
}

#[test]
fn auto_approval_labels_match_observed_action_variants() {
    assert_eq!(
        action_label(&Action::Command {
            command: "cat README.md".into(),
            cwd: "/tmp".into(),
            source: "shell".into()
        }),
        "cat README.md"
    );
    assert_eq!(
        action_label(&Action::Execve {
            program: "/bin/ls".into(),
            argv: vec!["ls".into(), "a b".into()],
            cwd: "/tmp".into(),
            source: "unifiedExec".into()
        }),
        "/bin/ls ls a b"
    );
    assert_eq!(
        action_label(&Action::WriteStdin {
            approval_id: "child".into(),
            process_id: "123".into(),
            stdin: "answer\n".into(),
            cwd: "/tmp".into()
        }),
        "向进程 123 发送输入：answer\n"
    );
    assert_eq!(
        action_label(&Action::ApplyPatch {
            files: vec!["/tmp/a".into(), "/tmp/b".into()],
            cwd: "/tmp".into()
        }),
        "正在编辑 2 个文件"
    );
    assert_eq!(status_label(Status::TimedOut), "自动审核超时");
    assert_eq!(status_label(Status::Aborted), "自动审核已停止");
}

#[test]
fn auto_approval_closing_the_action_resets_its_nested_disclosure() {
    let mut app = TestApp::new();
    app.update(|cx| cx.set_reduce_motion(true));
    let mut window = app.open_window_with_options(options(), |_, cx| {
        AutoApprovalReviewView::new(model(Status::InProgress), ThemeMode::Dark, cx)
    });
    window.update(|view, window, cx| {
        view.toggle(false, window, cx);
        view.toggle(true, window, cx);
    });
    assert!(window.read(|v, _| v.expanded && v.details_expanded));
    window.update(|view, window, cx| view.toggle(false, window, cx));
    window.update(|view, window, cx| view.toggle(false, window, cx));
    assert!(window.read(|v, _| v.expanded && !v.details_expanded));
}
