use std::{path::PathBuf, time::Duration};

use gpui::{
    AppContext, Bounds, MouseButton, TestApp, TestAppWindow, WindowBounds, WindowOptions, point,
    px, size,
};

use super::{
    ChatApp, ConversationKey, image_preview::finder_reveal_command,
    render::startup_loading_logo_opacity,
};
use crate::{
    agent::{
        HistoryItemDetail, HistoryTurnStatus, ThreadActivity, ThreadHistory, ThreadHistoryItem,
        ThreadSummary, ThreadTurn,
    },
    components::{
        composer::{ComposerView, ModelCatalogLoadFinished},
        file_change::DiffReviewEvent,
        home::OpenSubAgentPanel,
        sidebar::OpenSettings,
    },
    conversation::ConversationPhase,
    theme::ThemeMode,
};

fn simulate_next_frame(app: &mut TestApp, window: &TestAppWindow<ChatApp>, elapsed_ms: u64) {
    app.advance_clock(Duration::from_millis(elapsed_ms));
    let handle = window.handle();
    app.update(|cx| {
        cx.update_window(handle.into(), |_, window, cx| {
            window.simulate_next_frame(cx)
        })
        .unwrap()
    });
}

#[test]
fn startup_logo_blinks_in_place_without_disappearing() {
    assert_eq!(startup_loading_logo_opacity(0.0), 1.0);
    assert!((startup_loading_logo_opacity(0.5) - 0.32).abs() < f32::EPSILON);
    assert_eq!(startup_loading_logo_opacity(1.0), 1.0);
}

#[test]
fn diff_review_copy_and_open_events_have_native_actions() {
    let path = "/tmp/a file with spaces.txt";
    let command = finder_reveal_command(path);
    assert_eq!(command.get_program(), "open");
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        vec![std::ffi::OsStr::new("-R"), std::ffi::OsStr::new(path)]
    );

    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
    );
    window.update(|chat, _, cx| {
        chat.handle_diff_review_event(DiffReviewEvent::CopyPath(path.to_owned()), cx)
    });
    assert_eq!(
        app.read_from_clipboard().and_then(|item| item.text()),
        Some(path.to_owned())
    );
}

#[test]
fn image_preview_download_copies_the_source_and_close_unmounts_the_overlay() {
    let suffix = std::process::id();
    let source = std::env::temp_dir().join(format!("gpui-image-preview-source-{suffix}.png"));
    let destination =
        std::env::temp_dir().join(format!("gpui-image-preview-download-{suffix}.png"));
    std::fs::write(&source, b"real image payload").unwrap();
    let _ = std::fs::remove_file(&destination);

    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
    );
    window.update(|chat, _, cx| {
        cx.bind_keys([gpui::KeyBinding::new(
            "escape",
            super::DismissPermissionUi,
            None,
        )]);
        chat.startup_model_catalog_resolved = true;
        chat.startup_sidebar_resolved = true;
        chat.startup_minimum_duration_elapsed = true;
        chat.image_preview.path = Some(source.clone());
        chat.image_preview.dimensions = Some((1024, 1024));
        chat.image_preview.zoom = 1.75;
        cx.notify();
    });
    window.draw();

    // CDP geometry scaled to the 900px test viewport: download is the
    // left 40px control and close is the right 40px control.
    window.simulate_click(point(px(812.0), px(32.0)), MouseButton::Left);
    assert!(app.did_prompt_for_new_path());
    assert_eq!(
        window.read(|chat, _| chat.image_preview.path.clone()),
        Some(source.clone())
    );
    app.simulate_new_path_selection(|_| Some(destination.clone()));
    app.run_until_parked();
    assert_eq!(std::fs::read(&destination).unwrap(), b"real image payload");

    window.simulate_click(point(px(866.0), px(32.0)), MouseButton::Left);
    window.read(|chat, _| {
        assert_eq!(chat.image_preview.path, None);
        assert_eq!(chat.image_preview.dimensions, None);
        assert_eq!(chat.image_preview.zoom, 1.0);
    });

    window.draw();
    window.update(|chat, _, cx| {
        chat.image_preview.path = Some(source.clone());
        cx.notify();
    });
    window.draw();
    window.simulate_keystroke("escape");
    assert!(window.read(|chat, _| chat.image_preview.path.is_none()));
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(destination).unwrap();
}

#[test]
fn startup_loading_screen_waits_for_catalog_sidebar_and_one_second_minimum() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
    );

    window.update(|chat, _, cx| {
        chat.startup_model_catalog_resolved = false;
        chat.startup_sidebar_resolved = false;
        chat.startup_minimum_duration_elapsed = false;
        cx.notify();
    });
    window.draw();

    let sidebar_toggle_center = point(px(102.0), px(25.0));
    window.simulate_click(sidebar_toggle_center, MouseButton::Left);
    assert!(!window.read(|chat, _| chat.sidebar_layout.collapsed));

    let home = window.read(|chat, _| chat.home.clone());
    app.update(|cx| home.update(cx, |_, cx| cx.emit(ModelCatalogLoadFinished)));
    assert!(window.read(|chat, _| chat.startup_model_catalog_resolved));
    assert!(!window.read(|chat, _| chat.startup_minimum_duration_elapsed));

    app.advance_clock(Duration::from_millis(999));
    app.run_until_parked();
    assert!(!window.read(|chat, _| chat.startup_minimum_duration_elapsed));

    app.advance_clock(Duration::from_millis(1));
    app.run_until_parked();
    assert!(window.read(|chat, _| chat.startup_minimum_duration_elapsed));

    window.draw();
    window.simulate_click(sidebar_toggle_center, MouseButton::Left);
    assert!(!window.read(|chat, _| chat.sidebar_layout.collapsed));

    window.update(|chat, _, cx| {
        chat.startup_sidebar_resolved = true;
        cx.notify();
    });
    window.draw();
    window.simulate_click(sidebar_toggle_center, MouseButton::Left);
    assert!(window.read(|chat, _| chat.sidebar_layout.collapsed));
}

#[test]
fn sidebar_toggle_collapses_and_restores_without_moving_the_titlebar_control() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
    );

    window.draw();
    assert!(!window.read(|app, _| app.sidebar_layout.collapsed));

    // The control remains at left: 88px, centered at y=25px in both states.
    let toggle_center = point(px(102.0), px(25.0));
    window.simulate_click(toggle_center, MouseButton::Left);
    assert!(window.read(|app, _| app.sidebar_layout.collapsed));
    assert!(window.read(|app, _| app.sidebar_layout.animation_running));

    simulate_next_frame(&mut app, &window, 200);
    let collapsed_midpoint = window.read(|app, _| app.sidebar_layout.reveal);
    assert!(collapsed_midpoint > 0.0 && collapsed_midpoint < 1.0);

    simulate_next_frame(&mut app, &window, 200);
    assert_eq!(window.read(|app, _| app.sidebar_layout.reveal), 0.0);
    assert!(!window.read(|app, _| app.sidebar_layout.animation_running));

    window.draw();
    window.simulate_click(toggle_center, MouseButton::Left);
    assert!(!window.read(|app, _| app.sidebar_layout.collapsed));
    assert!(window.read(|app, _| app.sidebar_layout.animation_running));

    simulate_next_frame(&mut app, &window, 400);
    assert_eq!(window.read(|app, _| app.sidebar_layout.reveal), 1.0);
    assert!(!window.read(|app, _| app.sidebar_layout.animation_running));
}

#[test]
fn terminal_shortcut_restores_focus_and_preserves_the_session() {
    let mut app = TestApp::new();
    app.update(|cx| {
        crate::components::terminal::init(cx);
        cx.bind_keys([gpui::KeyBinding::new("ctrl-`", super::ToggleTerminal, None)]);
    });
    let mut window = app.open_window(|_, cx| ChatApp::new(ThemeMode::Dark, false, cx));
    window.update(|chat, window, cx| {
        chat.complete_startup_for_capture(cx);
        chat.conversation_hosts[&chat.active_conversation]
            .composer
            .read(cx)
            .prompt_focus_handle(cx)
            .focus(window, cx);
    });
    window.draw();
    window.simulate_keystroke("ctrl-`");
    window.draw();
    let terminal =
        window.read(|chat, _| chat.terminal_panels[&chat.active_conversation].entity_id());
    assert!(window.read(|chat, _| chat.right_panel.open));
    window.simulate_keystroke("ctrl-`");
    window.draw();
    assert!(!window.read(|chat, _| chat.right_panel.open));
    window.simulate_keystroke("ctrl-`");
    window.draw();
    assert!(window.read(|chat, _| chat.right_panel.open));
    assert_eq!(
        window.read(|chat, _| chat.terminal_panels[&chat.active_conversation].entity_id()),
        terminal
    );
}

#[test]
fn right_panel_stays_open_until_its_titlebar_toggle_is_clicked_again() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
    );

    window.draw();
    let trigger = point(px(878.0), px(23.0));
    window.simulate_click(trigger, MouseButton::Left);
    assert!(window.read(|chat, _| chat.right_panel.open));

    window.draw();
    window.simulate_click(point(px(400.0), px(400.0)), MouseButton::Left);
    assert!(window.read(|chat, _| chat.right_panel.open));

    window.draw();
    window.simulate_keystroke("escape");
    assert!(window.read(|chat, _| chat.right_panel.open));

    window.draw();
    window.simulate_click(trigger, MouseButton::Left);
    assert!(!window.read(|chat, _| chat.right_panel.open));
}

#[test]
fn collaboration_event_opens_a_read_only_subagent_panel_without_switching_parent() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(1_440.0), px(900.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Light, false, cx),
    );
    window.update(|chat, _, cx| {
        let backend = chat.agent_backend.clone();
        let child = cx.new(|cx| ComposerView::new_with_backend(chat.mode, backend, cx));
        let child_cwd = PathBuf::from("/tmp/collab-evidence-child");
        child.update(cx, |composer, cx| {
            composer.set_workspace_context(
                child_cwd.clone(),
                None,
                Some("thread-collab-evidence-child".to_owned()),
                cx,
            )
        });
        chat.conversation_hosts.insert(
            ConversationKey::Thread("thread-collab-evidence-child".to_owned()),
            super::ConversationHost {
                composer: child,
                cwd: child_cwd,
                project_id: None,
            },
        );
    });
    let (parent_key, parent_composer, home) = window.read(|chat, cx| {
        (
            chat.active_conversation.clone(),
            chat.home.read(cx).composer_entity(),
            chat.home.clone(),
        )
    });

    app.update(|cx| {
        home.update(cx, |_, cx| {
            cx.emit(OpenSubAgentPanel {
                thread_id: "thread-collab-evidence-child".to_owned(),
                name: "Collab evidence child".to_owned(),
            })
        })
    });

    window.read(|chat, cx| {
        assert!(chat.right_panel.open);
        assert_eq!(chat.active_conversation, parent_key);
        assert!(chat.home.read(cx).composer_entity() == parent_composer);
        let panel = chat
            .right_panel
            .subagent
            .as_ref()
            .expect("collaboration row should mount a subagent panel");
        assert_eq!(panel.thread_id, "thread-collab-evidence-child");
        assert_eq!(panel.name, "Collab evidence child");
        assert_eq!(
            panel.home.read(cx).composer_entity().read(cx).thread_id(),
            Some("thread-collab-evidence-child")
        );
    });

    window.draw();
    // At 1440px the 603px panel begins at x=837; the live 156×28 tab
    // occupies x=845..1001 and toggles the same information popover.
    window.simulate_click(point(px(900.0), px(23.0)), MouseButton::Left);
    assert!(window.read(|chat, _| chat.right_panel.subagent_menu_open));
    window.simulate_keystroke("escape");
    assert!(window.read(|chat, _| chat.right_panel.open));
    assert!(!window.read(|chat, _| chat.right_panel.subagent_menu_open));

    window.simulate_keystroke("escape");
    window.read(|chat, _| {
        assert!(!chat.right_panel.open);
        assert!(chat.right_panel.subagent.is_none());
        assert_eq!(chat.active_conversation, parent_key);
    });
}

#[test]
fn bottom_panel_matches_the_persistent_native_titlebar_toggle() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
    );

    window.draw();
    let trigger = point(px(844.0), px(23.0));
    window.simulate_click(trigger, MouseButton::Left);
    assert!(window.read(|chat, _| chat.bottom_panel.open));
    assert_eq!(
        window.read(|chat, _| chat.bottom_panel.tabs.clone()),
        vec![super::BottomPanelMode::Terminal]
    );
    assert_eq!(window.read(|chat, _| chat.bottom_panel.active_tab), Some(0));

    window.draw();
    window.simulate_click(point(px(400.0), px(300.0)), MouseButton::Left);
    assert!(window.read(|chat, _| chat.bottom_panel.open));
    window.simulate_keystroke("escape");
    assert!(window.read(|chat, _| chat.bottom_panel.open));

    window.draw();
    window.simulate_click(trigger, MouseButton::Left);
    assert!(!window.read(|chat, _| chat.bottom_panel.open));
}

#[test]
fn bottom_panel_add_menu_closes_on_toggle_outside_and_escape() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
    );

    window.update(|chat, _, cx| {
        chat.open_bottom_panel(cx);
        chat.toggle_bottom_panel_menu(cx);
    });
    assert!(window.read(|chat, _| chat.bottom_panel.add_menu_open));

    window.update(|chat, _, cx| chat.toggle_bottom_panel_menu(cx));
    assert!(!window.read(|chat, _| chat.bottom_panel.add_menu_open));

    window.update(|chat, _, cx| chat.toggle_bottom_panel_menu(cx));
    window.draw();
    window.simulate_click(point(px(400.0), px(300.0)), MouseButton::Left);
    assert!(!window.read(|chat, _| chat.bottom_panel.add_menu_open));

    window.update(|chat, _, cx| chat.toggle_bottom_panel_menu(cx));
    window.draw();
    window.simulate_keystroke("escape");
    assert!(!window.read(|chat, _| chat.bottom_panel.add_menu_open));
    assert!(window.read(|chat, _| chat.bottom_panel.open));
}

#[test]
fn bottom_panel_add_menu_supports_keyboard_selection() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
    );

    window.update(|chat, _, cx| {
        chat.open_bottom_panel(cx);
        chat.toggle_bottom_panel_menu(cx);
    });
    window.draw();
    window.simulate_keystroke("down");
    window.simulate_keystroke("down");
    assert_eq!(window.read(|chat, _| chat.bottom_panel.focused_item), 1);
    window.simulate_keystroke("enter");
    assert_eq!(
        window.read(|chat, _| chat.bottom_panel.tabs.clone()),
        vec![
            super::BottomPanelMode::Terminal,
            super::BottomPanelMode::Terminal
        ]
    );
    assert_eq!(window.read(|chat, _| chat.bottom_panel.active_tab), Some(1));
    assert!(!window.read(|chat, _| chat.bottom_panel.add_menu_open));
}

#[test]
fn bottom_panel_add_menu_appends_after_the_default_terminal() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
    );

    window.update(|chat, _, cx| {
        chat.open_bottom_panel(cx);
        chat.select_bottom_panel_item(2, cx);
    });

    assert_eq!(
        window.read(|chat, _| chat.bottom_panel.tabs.clone()),
        vec![
            super::BottomPanelMode::Terminal,
            super::BottomPanelMode::Browser
        ]
    );
    assert_eq!(window.read(|chat, _| chat.bottom_panel.active_tab), Some(1));
    assert_eq!(
        super::bottom_panel::bottom_panel_tab_spec(super::BottomPanelMode::Browser).0,
        "新标签页"
    );
}

#[test]
fn right_panel_menu_supports_keyboard_selection() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
    );

    window.update(|chat, _, cx| chat.open_right_panel(cx));
    window.draw();
    window.simulate_keystroke("down");
    window.simulate_keystroke("down");
    assert_eq!(window.read(|chat, _| chat.right_panel.focused_item), 1);
    window.simulate_keystroke("enter");
    assert_eq!(
        window.read(|chat, _| chat.right_panel.mode),
        Some(super::RightPanelMode::Browser)
    );
}

#[test]
fn sidebar_resize_handle_supports_full_hit_area_limits_and_collapse() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(1440.0), px(900.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
    );
    // Both edges of the 16px hit area work along its full height, with no
    // jump when grabbing away from the divider's center.
    for (offset, y) in [(-7.0, 10.0), (7.0, 450.0), (0.0, 890.0)] {
        window.update(|chat, _, cx| {
            chat.sidebar
                .update(cx, |sidebar, cx| sidebar.set_width(300.0, cx));
        });
        window.draw();
        window.simulate_mouse_move(point(px(300.0 + offset), px(y)));
        assert!(window.read(|chat, _| chat.sidebar_layout.resize_hovered));
        window.simulate_mouse_down(point(px(300.0 + offset), px(y)), MouseButton::Left);
        window.simulate_mouse_move(point(px(380.0 + offset), px(y)));
        window.simulate_mouse_up(point(px(380.0 + offset), px(y)), MouseButton::Left);
        assert!((window.read(|chat, cx| chat.sidebar.read(cx).width()) - 380.0).abs() < 0.2);
        assert!(!window.read(|chat, _| chat.sidebar_layout.resize_dragging));
    }
    window.draw();
    window.simulate_mouse_down(point(px(380.0), px(450.0)), MouseButton::Left);
    window.simulate_mouse_move(point(px(1200.0), px(450.0)));
    window.simulate_mouse_up(point(px(1200.0), px(450.0)), MouseButton::Left);
    assert_eq!(
        window.read(|chat, cx| chat.sidebar.read(cx).width()),
        super::SIDEBAR_MAX_WIDTH
    );
    window.draw();
    window.simulate_mouse_down(point(px(480.0), px(450.0)), MouseButton::Left);
    window.simulate_mouse_move(point(px(10.0), px(450.0)));
    window.simulate_mouse_up(point(px(10.0), px(450.0)), MouseButton::Left);
    assert_eq!(
        window.read(|chat, cx| chat.sidebar.read(cx).width()),
        super::SIDEBAR_MIN_WIDTH
    );
    window.simulate_mouse_move(point(px(600.0), px(450.0)));
    assert!(!window.read(|chat, _| chat.sidebar_layout.resize_hovered));
    window.update(|chat, win, cx| {
        chat.sidebar
            .update(cx, |sidebar, cx| sidebar.set_width(360.0, cx));
        chat.toggle_sidebar(win, cx);
        chat.toggle_sidebar(win, cx);
        assert_eq!(chat.sidebar.read(cx).width(), 360.0);
        assert!(!chat.sidebar_layout.resize_dragging);
        chat.open_right_panel(cx);
    });
    window.draw();
    window.simulate_mouse_down(point(px(360.0), px(450.0)), MouseButton::Left);
    window.simulate_mouse_move(point(px(420.0), px(450.0)));
    window.simulate_mouse_up(point(px(420.0), px(450.0)), MouseButton::Left);
    assert!((window.read(|chat, cx| chat.sidebar.read(cx).width()) - 420.0).abs() < 0.2);
    assert!(window.read(|chat, _| chat.right_panel.open));
}

#[test]
fn right_panel_resize_handle_matches_reference_limits_without_hiding_the_panel() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(1440.0), px(900.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
    );
    window.update(|chat, _, cx| chat.open_right_panel(cx));
    window.draw();

    // CDP: a 16 px hit area is centered on the one-pixel divider.
    window.simulate_mouse_move(point(px(773.09375), px(300.0)));
    assert!(window.read(|chat, _| chat.right_panel.resize_hovered));
    window.simulate_mouse_down(point(px(773.09375), px(300.0)), MouseButton::Left);
    window.simulate_mouse_move(point(px(1100.0), px(300.0)));
    window.simulate_mouse_up(point(px(1100.0), px(300.0)), MouseButton::Left);
    let narrow_width = window.read(|chat, _| chat.right_panel.width.unwrap());
    assert!((narrow_width - 339.09375).abs() < 0.2, "{narrow_width}");

    window.draw();
    window.simulate_mouse_down(point(px(1100.0), px(300.0)), MouseButton::Left);
    window.simulate_mouse_move(point(px(400.0), px(300.0)));
    window.simulate_mouse_up(point(px(400.0), px(300.0)), MouseButton::Left);
    let sidebar_width = window.read(|chat, cx| chat.sidebar.read(cx).width());
    let expected_max = 1440.0 - sidebar_width - super::RIGHT_PANEL_MAIN_MIN_WIDTH;
    assert!((window.read(|chat, _| chat.right_panel.width.unwrap()) - expected_max).abs() < 0.2);

    window.draw();
    let divider_x = 1440.0 - expected_max;
    window.simulate_mouse_down(point(px(divider_x), px(300.0)), MouseButton::Left);
    window.simulate_mouse_move(point(px(1300.0), px(300.0)));
    window.simulate_mouse_up(point(px(1300.0), px(300.0)), MouseButton::Left);
    assert!(window.read(|chat, _| chat.right_panel.open));
    assert_eq!(
        window.read(|chat, _| chat.right_panel.width),
        Some(super::RIGHT_PANEL_MIN_WIDTH)
    );
}

#[test]
fn settings_event_from_profile_menu_navigates_to_settings() {
    let mut app = TestApp::new();
    let window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
    );
    let sidebar = window.read(|app, _| app.sidebar.clone());
    app.update(|cx| sidebar.update(cx, |_, cx| cx.emit(OpenSettings)));
    app.run_until_parked();
    assert!(window.read(|app, _| app.showing_settings));
}

#[test]
fn permission_confirmation_buttons_close_the_modal() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
    );

    window.update(|chat, _, cx| chat.open_permission_confirmation_for_capture(cx));
    window.draw();
    window.simulate_click(point(px(553.0), px(500.0)), MouseButton::Left);
    assert!(!window.read(|chat, _| chat.permission_confirmation_open));

    window.update(|chat, _, cx| chat.open_permission_confirmation_for_capture(cx));
    window.draw();
    window.simulate_click(point(px(645.0), px(500.0)), MouseButton::Left);
    assert!(!window.read(|chat, _| chat.permission_confirmation_open));
}

#[test]
fn clicking_settings_in_profile_menu_opens_settings_surface() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
    );
    window.draw();
    window.simulate_click(point(px(80.0), px(677.0)), MouseButton::Left);
    window.draw();
    // CDP: the single 30 px settings row is anchored 43 px above the
    // bottom of the 700 px window, with the menu's four-pixel inset.
    window.simulate_click(point(px(80.0), px(638.0)), MouseButton::Left);
    assert!(window.read(|app, _| app.showing_settings));
}

#[test]
fn projects_menu_closes_when_the_main_surface_is_clicked() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| {
            let mut app = ChatApp::new(ThemeMode::Dark, false, cx);
            app.open_projects_section_menu(cx);
            app
        },
    );

    window.draw();
    assert!(window.read(|app, cx| { app.sidebar.read(cx).projects_section_menu_is_open() }));
    window.simulate_click(point(px(600.0), px(350.0)), MouseButton::Left);
    assert!(!window.read(|app, cx| { app.sidebar.read(cx).projects_section_menu_is_open() }));
}

#[test]
fn switching_drafts_preserves_the_background_conversation_host() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
    );
    let (first_key, first_composer) = window.read(|chat, _| {
        (
            chat.active_conversation.clone(),
            chat.conversation_hosts[&chat.active_conversation]
                .composer
                .clone(),
        )
    });
    app.update(|cx| {
        first_composer.update(cx, |composer, cx| {
            composer.set_command_tool_for_capture(true, cx)
        });
    });

    window.update(|chat, _, cx| {
        chat.start_draft(
            Some("project-stable-id".to_owned()),
            PathBuf::from("/tmp/second-project"),
            cx,
        );
    });
    assert_ne!(
        window.read(|chat, _| chat.active_conversation.clone()),
        first_key
    );
    assert_eq!(window.read(|chat, _| chat.conversation_hosts.len()), 2);
    assert_eq!(
        app.read_entity(&first_composer, |composer, _| composer.conversation_phase()),
        ConversationPhase::Streaming
    );

    window.update(|chat, _, cx| chat.switch_home_to(first_key.clone(), cx));
    let active_composer = window.read(|chat, cx| chat.home.read(cx).composer_entity());
    assert!(active_composer == first_composer);
    assert_eq!(
        window.read(|chat, _| chat.active_conversation.clone()),
        first_key
    );
    assert!(matches!(first_key, ConversationKey::Draft(_)));
    assert_eq!(
        app.read_entity(&active_composer, |composer, _| composer
            .conversation_phase()),
        ConversationPhase::Streaming
    );
}

#[test]
fn thread_created_rekeys_the_draft_without_replacing_its_host() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
    );
    let (draft_key, composer) = window.read(|chat, _| {
        (
            chat.active_conversation.clone(),
            chat.conversation_hosts[&chat.active_conversation]
                .composer
                .clone(),
        )
    });
    app.update(|cx| {
        composer.update(cx, |composer, cx| {
            composer.set_workspace_context(
                PathBuf::from("/tmp/project"),
                Some("project-stable-id".to_owned()),
                Some("thread-stable-id".to_owned()),
                cx,
            );
        });
    });

    window.update(|chat, _, cx| chat.rekey_created_thread("thread-stable-id".to_owned(), cx));
    let thread_key = ConversationKey::Thread("thread-stable-id".to_owned());
    assert!(!window.read(|chat, _| chat.conversation_hosts.contains_key(&draft_key)));
    assert_eq!(
        window.read(|chat, _| chat.active_conversation.clone()),
        thread_key
    );
    let rekeyed = window.read(|chat, _| chat.conversation_hosts[&thread_key].composer.clone());
    assert!(rekeyed == composer);
}

fn history_fixture(thread_id: &str, message: &str) -> ThreadHistory {
    ThreadHistory {
        thread: ThreadSummary {
            thread_id: thread_id.to_owned(),
            title: format!("Thread {thread_id}"),
            preview: message.to_owned(),
            cwd: PathBuf::from("/tmp/project"),
            project_id: None,
            section: None,
            created_at: 1,
            updated_at: 2,
            recency_at: Some(2),
            activity: ThreadActivity::Idle,
        },
        turns: vec![ThreadTurn {
            turn_id: format!("turn-{thread_id}"),
            status: HistoryTurnStatus::Completed,
            items_view: HistoryItemDetail::Full,
            items: vec![
                ThreadHistoryItem::UserMessage {
                    images: Vec::new(),
                    item_id: format!("user-{thread_id}"),
                    text: message.to_owned(),
                },
                ThreadHistoryItem::AssistantMessage {
                    item_id: format!("assistant-{thread_id}"),
                    text: format!("answer {message}"),
                    phase: None,
                },
            ],
            started_at: Some(1),
            completed_at: Some(2),
            duration_ms: Some(1),
            error: None,
        }],
        next_turn_cursor: None,
        backwards_turn_cursor: None,
    }
}

#[cfg(feature = "screenshot")]
#[test]
fn resumed_thread_readiness_tracks_loading_completion_and_errors() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(WindowOptions::default(), |_, cx| {
        ChatApp::new(ThemeMode::Dark, false, cx)
    });
    let composer = window.read(|chat, _| {
        chat.conversation_hosts[&chat.active_conversation]
            .composer
            .clone()
    });
    app.update(|cx| {
        composer.update(cx, |composer, cx| {
            composer.set_workspace_context(
                PathBuf::from("/tmp/capture"),
                None,
                Some("thread-capture".to_owned()),
                cx,
            );
            composer.set_history_loading(true, cx);
        });
    });
    window.update(|chat, _, cx| chat.rekey_created_thread("thread-capture".to_owned(), cx));
    window.update(|chat, _, cx| chat.resume_thread_for_capture("thread-capture".to_owned(), cx));

    assert_eq!(
        window.read(|chat, cx| chat.resumed_thread_ready("thread-capture", cx)),
        Ok(false)
    );

    app.update(|cx| {
        composer.update(cx, |composer, cx| {
            composer.hydrate_history(history_fixture("thread-capture", "captured history"), cx)
        });
    });
    assert_eq!(
        window.read(|chat, cx| chat.resumed_thread_ready("thread-capture", cx)),
        Ok(true)
    );

    app.update(|cx| {
        composer.update(cx, |composer, cx| {
            composer.set_history_error("history failed".to_owned(), cx)
        });
    });
    assert_eq!(
        window.read(|chat, cx| chat.resumed_thread_ready("thread-capture", cx)),
        Err("history failed".to_owned())
    );
}

#[test]
fn rapid_thread_switch_keeps_late_history_scoped_to_its_original_host() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(WindowOptions::default(), |_, cx| {
        ChatApp::new(ThemeMode::Dark, false, cx)
    });
    let thread_a = window.read(|chat, _| {
        chat.conversation_hosts[&chat.active_conversation]
            .composer
            .clone()
    });
    app.update(|cx| {
        thread_a.update(cx, |composer, cx| {
            composer.set_workspace_context(
                PathBuf::from("/tmp/project-a"),
                None,
                Some("thread-a".to_owned()),
                cx,
            );
            composer.set_history_loading(true, cx);
        });
    });
    window.update(|chat, _, cx| {
        chat.rekey_created_thread("thread-a".to_owned(), cx);
        chat.start_draft(None, PathBuf::from("/tmp/project-b"), cx);
    });
    let thread_b = window.read(|chat, _| {
        chat.conversation_hosts[&chat.active_conversation]
            .composer
            .clone()
    });

    app.update(|cx| {
        thread_a.update(cx, |composer, cx| {
            composer.hydrate_history(history_fixture("thread-a", "old selection"), cx)
        });
    });

    assert!(window.read(|chat, cx| chat.home.read(cx).composer_entity()) == thread_b);
    assert_eq!(
        app.read_entity(&thread_b, |composer, _| composer.conversation_phase()),
        ConversationPhase::Empty
    );
    assert_eq!(
        app.read_entity(&thread_a, |composer, _| composer.conversation_phase()),
        ConversationPhase::Complete
    );
}

#[test]
fn switching_from_a_running_thread_does_not_stop_its_background_turn() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(WindowOptions::default(), |_, cx| {
        ChatApp::new(ThemeMode::Dark, false, cx)
    });
    let running = window.read(|chat, _| {
        chat.conversation_hosts[&chat.active_conversation]
            .composer
            .clone()
    });
    app.update(|cx| {
        running.update(cx, |composer, cx| {
            composer.set_workspace_context(
                PathBuf::from("/tmp/running"),
                None,
                Some("thread-running".to_owned()),
                cx,
            );
            composer.set_command_tool_for_capture(true, cx);
        });
    });
    window.update(|chat, _, cx| {
        chat.rekey_created_thread("thread-running".to_owned(), cx);
        chat.start_draft(None, PathBuf::from("/tmp/other"), cx);
    });

    assert_eq!(
        app.read_entity(&running, |composer, _| composer.conversation_phase()),
        ConversationPhase::Streaming
    );
    assert!(window.read(|chat, _| {
        chat.conversation_hosts
            .contains_key(&ConversationKey::Thread("thread-running".to_owned()))
            && chat.conversation_hosts.len() == 2
    }));
}

#[test]
fn an_empty_backend_does_not_expose_a_phantom_project_menu() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| {
            let mut app = ChatApp::new(ThemeMode::Dark, false, cx);
            app.open_project_menu_for_capture("missing-project".to_owned(), cx);
            app
        },
    );

    window.draw();
    assert!(!window.read(|app, cx| app.sidebar.read(cx).project_menu_is_open()));
}

#[test]
fn an_empty_backend_does_not_expose_a_phantom_pinned_menu() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
    );

    window.draw();
    assert!(!window.read(|app, cx| app.sidebar.read(cx).pinned_menu_is_open()));
}

#[test]
fn project_creation_dialog_matches_reference_close_and_keyboard_behavior() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
    );

    window.update(|chat, _, cx| chat.open_project_creation(cx));
    window.draw();
    assert!(window.read(|chat, _| chat.project_creation.open));
    assert_eq!(
        window.read(|chat, _| chat.project_creation.kind),
        super::state::ProjectCreationKind::Local
    );

    window.simulate_keystroke("tab");
    assert_eq!(window.read(|chat, _| chat.project_creation.focused_item), 1);
    window.simulate_keystroke("space");
    assert_eq!(
        window.read(|chat, _| chat.project_creation.kind),
        super::state::ProjectCreationKind::Remote
    );
    window.simulate_keystroke("tab");
    window.simulate_keystroke("tab");
    assert_eq!(window.read(|chat, _| chat.project_creation.focused_item), 3);
    window.simulate_keystroke("enter");
    assert!(!window.read(|chat, _| chat.project_creation.open));

    window.update(|chat, _, cx| chat.open_project_creation(cx));
    window.draw();
    window.simulate_click(point(px(50.0), px(80.0)), MouseButton::Left);
    assert!(!window.read(|chat, _| chat.project_creation.open));

    window.update(|chat, _, cx| chat.open_project_creation(cx));
    window.draw();
    window.simulate_keystroke("escape");
    assert!(!window.read(|chat, _| chat.project_creation.open));
}

#[test]
fn local_project_next_closes_the_project_type_dialog() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
    );

    window.update(|chat, _, cx| chat.open_project_creation(cx));
    window.draw();
    window.simulate_click(point(px(732.0), px(493.0)), MouseButton::Left);
    assert!(!window.read(|chat, _| chat.project_creation.open));
}

#[test]
fn remote_project_step_is_preserved_by_dismiss_and_reset_by_cancel() {
    let mut app = TestApp::new();
    let mut window = app.open_window(|_, cx| ChatApp::new(ThemeMode::Dark, false, cx));

    window.update(|chat, _, cx| {
        chat.open_project_creation(cx);
        chat.project_creation.kind = super::state::ProjectCreationKind::Remote;
        chat.advance_project_creation(cx);
        chat.close_project_creation(cx);
        chat.open_project_creation(cx);
    });
    assert_eq!(
        window.read(|chat, _| chat.project_creation.step),
        super::state::ProjectCreationStep::Remote
    );

    window.update(|chat, _, cx| chat.cancel_project_creation(cx));
    window.update(|chat, _, cx| chat.open_project_creation(cx));
    assert_eq!(
        window.read(|chat, _| chat.project_creation.step),
        super::state::ProjectCreationStep::Kind
    );
    assert_eq!(
        window.read(|chat, _| chat.project_creation.kind),
        super::state::ProjectCreationKind::Local
    );
}

#[test]
fn remote_project_keyboard_order_matches_the_native_dialog() {
    let mut app = TestApp::new();
    let mut window = app.open_window(|_, cx| {
        let mut chat = ChatApp::new(ThemeMode::Dark, false, cx);
        chat.open_project_creation_remote_for_capture(cx);
        chat
    });

    window.draw();
    window.simulate_keystroke("tab");
    assert_eq!(window.read(|chat, _| chat.project_creation.focused_item), 1);
    window.simulate_keystroke("tab");
    assert_eq!(window.read(|chat, _| chat.project_creation.focused_item), 2);
    window.simulate_keystroke("enter");
    assert!(!window.read(|chat, _| chat.project_creation.open));
    assert_eq!(
        window.read(|chat, _| chat.project_creation.step),
        super::state::ProjectCreationStep::Kind
    );
}

#[test]
fn project_creation_trigger_opens_and_the_same_screen_position_closes_on_the_overlay() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(1440.0), px(900.0)),
            })),
            ..Default::default()
        },
        |_, cx| ChatApp::new(ThemeMode::Dark, false, cx),
    );

    window.draw();
    // CDP-derived layout: 46 px titlebar safe area, 38 px brand header,
    // 31 px new-chat row, and the restored four-row navigation block.
    let trigger = point(px(214.0), px(282.5));
    window.simulate_mouse_move(trigger);
    window.draw();
    window.simulate_click(trigger, MouseButton::Left);
    app.run_until_parked();
    assert!(window.read(|chat, _| chat.project_creation.open));

    window.draw();
    window.simulate_click(trigger, MouseButton::Left);
    assert!(!window.read(|chat, _| chat.project_creation.open));
}

#[test]
fn appearance_cards_change_the_application_theme() {
    let mut app = TestApp::new();
    let mut window = app.open_window_with_options(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(900.0), px(700.0)),
            })),
            ..Default::default()
        },
        |_, cx| {
            let mut app = ChatApp::new(ThemeMode::Dark, false, cx);
            app.open_settings_page("appearance", cx);
            app
        },
    );

    window.draw();
    window.simulate_click(point(px(583.0), px(250.0)), MouseButton::Left);
    assert_eq!(window.read(|app, _| app.mode), ThemeMode::Light);

    window.draw();
    window.simulate_click(point(px(772.0), px(250.0)), MouseButton::Left);
    assert_eq!(window.read(|app, _| app.mode), ThemeMode::Dark);
}
