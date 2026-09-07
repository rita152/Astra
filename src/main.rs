mod agent;
mod app;
mod components;
mod settings;
mod theme;
mod typography;
mod workspace;

use std::{borrow::Cow, fs, path::PathBuf};

#[cfg(feature = "screenshot")]
use std::path::Path;
#[cfg(feature = "screenshot")]
use std::time::{Duration, Instant};

#[cfg(feature = "screenshot")]
const RESUMED_THREAD_STABLE_FRAMES: usize = 3;

use anyhow::Result;
use app::ChatApp;
use components::prompt_input::{
    Backspace, Copy, Cut, Delete, End, Home, Left, Paste, Right, SelectAll, SelectLeft,
    SelectRight, Submit,
};
use gpui::{
    App, AppContext, AssetSource, Bounds, SharedString, WindowAppearance,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, px, size,
};
use gpui_platform::application;
use theme::ThemeMode;

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)] // objc 0.2 macros probe the legacy cargo-clippy cfg.
fn configure_native_blur_sampling(window: &mut gpui::Window) {
    fn schedule(window: &mut gpui::Window, attempts_remaining: usize) {
        window.on_next_frame(move |window, _| {
            let configured = unsafe { apply() };
            if !configured && attempts_remaining > 1 {
                schedule(window, attempts_remaining - 1);
            }
        });
    }

    #[allow(unexpected_cfgs)]
    unsafe fn apply() -> bool {
        use cocoa::{
            appkit::{
                NSView, NSViewHeightSizable, NSViewWidthSizable, NSVisualEffectBlendingMode,
                NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
            },
            base::{id, nil},
        };
        use objc::{class, msg_send, runtime::Object, sel, sel_impl};

        let application: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        let mut native_window: *mut Object = msg_send![application, keyWindow];
        if native_window.is_null() {
            let windows: *mut Object = msg_send![application, windows];
            let count: usize = msg_send![windows, count];
            if count == 0 {
                return false;
            }
            native_window = msg_send![windows, objectAtIndex: 0usize];
        }

        let content_view: *mut Object = msg_send![native_window, contentView];
        let subviews: *mut Object = msg_send![content_view, subviews];
        let count: usize = msg_send![subviews, count];
        for index in 0..count {
            let view: *mut Object = msg_send![subviews, objectAtIndex: index];
            let is_visual_effect: bool = msg_send![view, isKindOfClass: class!(NSVisualEffectView)];
            if is_visual_effect {
                // Electron's primary ChatGPT window is transparent and calls
                // setVibrancy("menu"). GPUI's built-in BlurredView instead
                // uses Selection material and strips AppKit's tint and
                // saturation filters in updateLayer, so changing its material
                // in place still produces a visibly different light sidebar.
                // Keep GPUI's owned view alive but hidden, then install a plain
                // NSVisualEffectView with Electron's exact semantic material.
                unsafe {
                    let frame = NSView::bounds(content_view);
                    let menu_view: id =
                        NSVisualEffectView::initWithFrame_(NSVisualEffectView::alloc(nil), frame);
                    menu_view.setMaterial_(NSVisualEffectMaterial::Menu);
                    menu_view.setBlendingMode_(NSVisualEffectBlendingMode::BehindWindow);
                    menu_view.setState_(NSVisualEffectState::Active);
                    menu_view.setAutoresizingMask_(NSViewWidthSizable | NSViewHeightSizable);
                    let _: () = msg_send![
                        content_view,
                        addSubview: menu_view
                        positioned: -1isize
                        relativeTo: nil
                    ];
                    let _: id = msg_send![menu_view, autorelease];
                    let _: () = msg_send![view, setHidden: true];
                }
                return true;
            }
        }
        false
    }

    schedule(window, 8);
}

#[cfg(not(target_os = "macos"))]
fn configure_native_blur_sampling(_window: &mut gpui::Window) {}

struct Assets {
    base: PathBuf,
}

#[cfg(feature = "screenshot")]
fn save_screenshot(window: &gpui::Window, path: &str) -> anyhow::Result<()> {
    if let Some(parent) = Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    window.render_to_image()?.save(path)?;
    Ok(())
}

#[cfg(feature = "screenshot")]
fn schedule_screenshot(window: &mut gpui::Window, path: String, frames: usize) {
    window.on_next_frame(move |window, cx| {
        if frames > 1 {
            // `on_next_frame` callbacks run before GPUI paints that frame.
            // Dirty each counted frame so the delay represents real draws.
            window.refresh();
            schedule_screenshot(window, path, frames - 1);
        } else {
            match save_screenshot(window, &path) {
                Ok(()) => println!("{path}"),
                Err(error) => {
                    eprintln!("failed to save screenshot: {error:#}");
                    std::process::exit(1);
                }
            }
            cx.quit();
        }
    });
}

#[cfg(feature = "screenshot")]
fn schedule_resumed_thread_screenshot(
    window: &mut gpui::Window,
    app: gpui::Entity<ChatApp>,
    path: String,
    thread_id: String,
    deadline: Instant,
    scroll_from_bottom: Option<f32>,
    stable_frames_remaining: usize,
) {
    window.on_next_frame(move |window, cx| {
        let readiness = app.read(cx).resumed_thread_ready(&thread_id, cx);
        match readiness {
            Err(error) => {
                eprintln!("failed to load resumed thread {thread_id}: {error}");
                std::process::exit(1);
            }
            Ok(true) if scroll_from_bottom.is_some() => {
                let distance = scroll_from_bottom.expect("guarded capture scroll distance");
                app.update(cx, |app, cx| {
                    app.set_conversation_scroll_from_bottom_for_capture(distance, cx)
                });
                window.refresh();
                schedule_resumed_thread_screenshot(
                    window,
                    app,
                    path,
                    thread_id,
                    deadline,
                    None,
                    RESUMED_THREAD_STABLE_FRAMES,
                );
            }
            Ok(true) if stable_frames_remaining > 0 => {
                // Readiness can flip in this callback, before that frame is
                // painted. Force a draw for every counted stable frame.
                window.refresh();
                schedule_resumed_thread_screenshot(
                    window,
                    app,
                    path,
                    thread_id,
                    deadline,
                    scroll_from_bottom,
                    stable_frames_remaining - 1,
                );
            }
            Ok(true) => {
                match save_screenshot(window, &path) {
                    Ok(()) => println!("{path}"),
                    Err(error) => {
                        eprintln!("failed to save screenshot: {error:#}");
                        std::process::exit(1);
                    }
                }
                let audit = app.read(cx).resumed_render_audit(&thread_id, cx);
                if let Err(error) = std::fs::write(
                    format!("{path}.render.json"),
                    serde_json::to_vec_pretty(&audit).expect("capture audit is JSON"),
                ) {
                    eprintln!("failed to save resumed render audit: {error}");
                    std::process::exit(1);
                }
                cx.quit();
            }
            Ok(false) if Instant::now() < deadline => schedule_resumed_thread_screenshot(
                window,
                app,
                path,
                thread_id,
                deadline,
                scroll_from_bottom,
                RESUMED_THREAD_STABLE_FRAMES,
            ),
            Ok(false) => {
                eprintln!("timed out waiting for resumed thread {thread_id}");
                std::process::exit(1);
            }
        }
    });
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        fs::read(self.base.join(path))
            .map(Cow::Owned)
            .map(Some)
            .map_err(Into::into)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(fs::read_dir(self.base.join(path))?
            .filter_map(|entry| {
                entry
                    .ok()?
                    .file_name()
                    .into_string()
                    .ok()
                    .map(SharedString::from)
            })
            .collect())
    }
}

fn normalize_resume_thread_id(value: &str) -> String {
    value.strip_prefix("local:").unwrap_or(value).to_owned()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--theme=").map(ThemeMode::from_name))
        .unwrap_or(ThemeMode::Dark);
    let screenshot_path = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--screenshot=").map(ToOwned::to_owned));
    let screenshot_frames = args
        .iter()
        .find_map(|arg| {
            arg.strip_prefix("--screenshot-frames=")?
                .parse::<usize>()
                .ok()
        })
        .unwrap_or(2);
    let resume_thread = args.iter().find_map(|arg| {
        arg.strip_prefix("--resume-thread=")
            .map(normalize_resume_thread_id)
    });
    let resume_scroll_from_bottom = args.iter().find_map(|arg| {
        arg.strip_prefix("--resume-scroll-from-bottom=")?
            .parse::<f32>()
            .ok()
    });
    #[cfg(not(feature = "screenshot"))]
    let _ = (screenshot_frames, resume_scroll_from_bottom);
    let submit_prompt = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--submit-prompt=").map(ToOwned::to_owned));
    let command_tool_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--command-tool-state=")
            .map(ToOwned::to_owned)
    });
    let command_tool_expanded = args.iter().any(|arg| arg == "--command-tool-expanded");
    let tool_group_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--tool-group-state=")
            .map(ToOwned::to_owned)
    });
    let tool_group_expanded = args.iter().any(|arg| arg == "--tool-group-expanded");
    let reasoning_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--reasoning-ui-state=")
            .map(ToOwned::to_owned)
    });
    let reasoning_ui_expanded = args.iter().any(|arg| arg == "--reasoning-ui-expanded");
    let approval_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--approval-ui-state=")
            .map(ToOwned::to_owned)
    });
    let user_input_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--user-input-ui-state=")
            .map(ToOwned::to_owned)
    });
    let file_approval_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--file-approval-ui-state=")
            .map(ToOwned::to_owned)
    });
    let permissions_approval_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--permissions-approval-ui-state=")
            .map(ToOwned::to_owned)
    });
    let permissions_approval_ui_kind = args
        .iter()
        .find_map(|arg| {
            arg.strip_prefix("--permissions-approval-ui-kind=")
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "network".to_owned());
    let file_change_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--file-change-ui-state=")
            .map(ToOwned::to_owned)
    });
    let turn_diff_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--turn-diff-ui-state=")
            .map(ToOwned::to_owned)
    });
    let context_compaction_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--context-compaction-ui-state=")
            .map(ToOwned::to_owned)
    });
    let collaboration_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--collaboration-ui-state=")
            .map(ToOwned::to_owned)
    });
    let mcp_tool_call_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--mcp-tool-call-ui-state=")
            .map(ToOwned::to_owned)
    });
    let image_generation_ui_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--image-generation-ui-state=")
            .map(ToOwned::to_owned)
    });
    let image_generation_path = args.iter().find_map(|arg| {
        arg.strip_prefix("--image-generation-path=")
            .map(PathBuf::from)
    });
    let approval_ui_kind = args
        .iter()
        .find_map(|arg| {
            arg.strip_prefix("--approval-ui-kind=")
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "command".to_owned());
    let user_message_actions_visible = args
        .iter()
        .any(|arg| arg == "--user-message-actions-visible");
    #[cfg(not(feature = "screenshot"))]
    if screenshot_path.is_some() {
        eprintln!(
            "--screenshot requires the screenshot feature; rebuild with \
             `cargo run --release --features screenshot -- --screenshot=<path>`"
        );
        std::process::exit(2);
    }
    let start_maximized = args.iter().any(|arg| arg == "--maximized");
    let maximize_after_open = args.iter().any(|arg| arg == "--maximize-after-open");
    let window_width = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--window-width=")?.parse::<f32>().ok())
        .unwrap_or(1440.0);
    let window_height = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--window-height=")?.parse::<f32>().ok())
        .unwrap_or(900.0);
    let sidebar_bottom = args.iter().any(|arg| arg == "--sidebar-bottom");
    let profile_menu_open = args.iter().any(|arg| arg == "--profile-menu-open");
    let bottom_panel_open = args.iter().any(|arg| arg == "--bottom-panel-open");
    let bottom_panel_menu_open = args.iter().any(|arg| arg == "--bottom-panel-menu-open");
    let bottom_panel_append = args.iter().find_map(|arg| {
        arg.strip_prefix("--bottom-panel-append=")
            .map(ToOwned::to_owned)
    });
    let right_panel_open = args.iter().any(|arg| arg == "--right-panel-open");
    let projects_menu_open = args.iter().any(|arg| arg == "--projects-menu-open");
    let project_menu_open = args.iter().find_map(|arg| {
        arg.strip_prefix("--project-menu-open=")
            .map(ToOwned::to_owned)
    });
    let project_create_open = args.iter().any(|arg| arg == "--project-create-open");
    let project_create_remote = args.iter().any(|arg| arg == "--project-create-remote");
    let activity_open = args.iter().any(|arg| arg == "--activity-open");
    let activity_scroll = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--activity-scroll=")?.parse::<f32>().ok());
    let activity_hover_recent = args.iter().find_map(|arg| {
        arg.strip_prefix("--activity-hover-recent=")
            .map(ToOwned::to_owned)
    });
    let model_picker_open = args.iter().any(|arg| arg == "--model-picker-open");
    let model_picker_submenu = args.iter().find_map(|arg| {
        arg.strip_prefix("--model-picker-submenu=")
            .map(ToOwned::to_owned)
    });
    let model_picker_slider_index = args.iter().find_map(|arg| {
        arg.strip_prefix("--model-picker-slider-index=")?
            .parse::<usize>()
            .ok()
    });
    let model_picker_slider_fast = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--model-picker-slider-speed="))
        .is_some_and(|speed| speed == "fast");
    let dictation_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--dictation-state=")
            .map(ToOwned::to_owned)
    });
    let permission_mode = args.iter().find_map(|arg| {
        arg.strip_prefix("--permission-mode=")
            .map(ToOwned::to_owned)
    });
    let permission_menu_open = args.iter().any(|arg| arg == "--permission-menu-open");
    let permission_menu_state = args.iter().find_map(|arg| {
        arg.strip_prefix("--permission-menu-state=")
            .map(ToOwned::to_owned)
    });
    let permission_confirmation_open = args
        .iter()
        .any(|arg| arg == "--permission-confirmation-open");
    // Permission controls are visible in ordinary product launches. These
    // flags only select deterministic modes/states for visual capture; pixel
    // similarity is not a production visibility gate.
    let permission_ui_capture = screenshot_path.is_some()
        || permission_mode.is_some()
        || permission_menu_open
        || permission_menu_state.is_some()
        || permission_confirmation_open;
    let settings_open = args.iter().any(|arg| arg == "--settings-open");
    let settings_page = args.iter().find_map(|arg| {
        arg.strip_prefix("--settings-page=")
            .map(|slug| Box::leak(slug.to_owned().into_boxed_str()) as &'static str)
    });

    #[cfg(feature = "screenshot")]
    if components::markdown::capture_markdown(&args) {
        return;
    }
    #[cfg(feature = "screenshot")]
    if typography::capture_specimen(&args) {
        return;
    }
    typography::configure();
    application()
        .with_assets(Assets {
            base: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"),
        })
        .run(move |cx: &mut App| {
            typography::initialize_fonts(cx);
            cx.bind_keys([gpui::KeyBinding::new("ctrl-`", app::ToggleTerminal, None)]);
            cx.set_window_appearance(Some(match mode {
                ThemeMode::Light => WindowAppearance::Light,
                ThemeMode::Dark => WindowAppearance::Dark,
            }));
            cx.bind_keys([gpui::KeyBinding::new(
                "escape",
                app::DismissPermissionUi,
                None,
            )]);
            cx.bind_keys([
                gpui::KeyBinding::new("backspace", Backspace, Some("PromptInput")),
                gpui::KeyBinding::new("delete", Delete, Some("PromptInput")),
                gpui::KeyBinding::new("left", Left, Some("PromptInput")),
                gpui::KeyBinding::new("right", Right, Some("PromptInput")),
                gpui::KeyBinding::new("shift-left", SelectLeft, Some("PromptInput")),
                gpui::KeyBinding::new("shift-right", SelectRight, Some("PromptInput")),
                gpui::KeyBinding::new("cmd-a", SelectAll, Some("PromptInput")),
                gpui::KeyBinding::new("cmd-v", Paste, Some("PromptInput")),
                gpui::KeyBinding::new("cmd-c", Copy, Some("PromptInput")),
                gpui::KeyBinding::new("cmd-x", Cut, Some("PromptInput")),
                gpui::KeyBinding::new("home", Home, Some("PromptInput")),
                gpui::KeyBinding::new("end", End, Some("PromptInput")),
                gpui::KeyBinding::new("enter", Submit, Some("PromptInput")),
            ]);
            // Register terminal bindings after the application-wide Escape fallback.
            components::terminal::init(cx);
            let bounds = Bounds::centered(None, size(px(window_width), px(window_height)), cx);
            let initial_bounds = if start_maximized {
                WindowBounds::Maximized(bounds)
            } else {
                WindowBounds::Windowed(bounds)
            };
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(initial_bounds),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Codex".into()),
                        appears_transparent: true,
                        traffic_light_position: Some(gpui::point(px(18.0), px(18.0))),
                    }),
                    // Let the native macOS visual-effect layer participate in
                    // the translucent sidebar composition. Opaque main-pane
                    // content still masks the material on the right.
                    window_background: WindowBackgroundAppearance::Blurred,
                    window_min_size: Some(size(px(960.0), px(620.0))),
                    ..Default::default()
                },
                move |window, cx| {
                    configure_native_blur_sampling(window);
                    if maximize_after_open {
                        window.on_next_frame(|window, _| window.zoom_window());
                    }
                    let app = cx.new(|cx| {
                        cx.observe_window_activation(window, |_, _, cx| cx.notify())
                            .detach();
                        cx.observe_window_bounds(window, |_, _, cx| cx.notify())
                            .detach();
                        let mut app = ChatApp::new(mode, sidebar_bottom, cx);
                        if permission_ui_capture {
                            app.enable_permission_ui_for_capture(cx);
                        }
                        if approval_ui_state.is_some()
                            || tool_group_state.is_some()
                            || reasoning_ui_state.is_some()
                            || user_input_ui_state.is_some()
                            || file_approval_ui_state.is_some()
                            || permissions_approval_ui_state.is_some()
                            || file_change_ui_state.is_some()
                            || turn_diff_ui_state.is_some()
                            || context_compaction_ui_state.is_some()
                            || collaboration_ui_state.is_some()
                            || mcp_tool_call_ui_state.is_some()
                            || image_generation_ui_state.is_some()
                            || permission_mode.is_some()
                            || permission_menu_open
                            || permission_menu_state.is_some()
                            || permission_confirmation_open
                        {
                            app.complete_startup_for_capture(cx);
                        }
                        if profile_menu_open {
                            app.open_profile_menu(cx);
                        }
                        if bottom_panel_open {
                            app.open_bottom_panel(cx);
                        }
                        if bottom_panel_menu_open {
                            app.open_bottom_panel_menu(cx);
                        }
                        if let Some(name) = bottom_panel_append.as_deref() {
                            app.append_bottom_panel_item_for_capture(name, cx);
                        }
                        if right_panel_open {
                            app.open_right_panel(cx);
                        }
                        if projects_menu_open {
                            app.open_projects_section_menu(cx);
                        }
                        if let Some(project_id) = project_menu_open {
                            app.open_project_menu_for_capture(project_id, cx);
                        }
                        if project_create_open {
                            app.open_project_creation(cx);
                        }
                        if project_create_remote {
                            app.open_project_creation_remote_for_capture(cx);
                        }
                        if activity_open {
                            app.open_activity(cx);
                        }
                        if let Some(offset) = activity_scroll {
                            app.open_activity(cx);
                            app.set_activity_scroll_for_capture(offset, cx);
                        }
                        if let Some(thread_id) = activity_hover_recent {
                            app.open_activity(cx);
                            app.set_activity_hovered_thread_for_capture(thread_id, cx);
                        }
                        if model_picker_open {
                            app.open_model_picker(cx);
                        }
                        if let Some(submenu) = model_picker_submenu.as_deref() {
                            app.open_model_picker_submenu(submenu, cx);
                        }
                        if let Some(index) = model_picker_slider_index {
                            app.open_model_picker_slider_at(index, model_picker_slider_fast, cx);
                        }
                        if let Some(state) = dictation_state.as_deref() {
                            app.set_dictation_state_for_capture(state, cx);
                        }
                        if let Some(mode) = permission_mode.as_deref() {
                            app.set_permission_mode_for_capture(mode, cx);
                        }
                        if let Some(state) = permission_menu_state.as_deref() {
                            app.set_permission_menu_capture_state(state, cx);
                        } else if permission_menu_open {
                            app.open_permission_menu_for_capture(cx);
                        }
                        if permission_confirmation_open {
                            app.open_permission_confirmation_for_capture(cx);
                        }
                        if let Some(prompt) = submit_prompt.as_deref() {
                            app.submit_prompt_for_capture(prompt, cx);
                        }
                        if user_message_actions_visible {
                            app.show_user_message_actions_for_capture(cx);
                        }
                        if let Some(state) = command_tool_state.as_deref() {
                            app.set_command_tool_for_capture(
                                state.eq_ignore_ascii_case("running"),
                                command_tool_expanded,
                                cx,
                            );
                        }
                        if let Some(state) = context_compaction_ui_state.as_deref() {
                            app.set_context_compaction_for_capture(
                                state.eq_ignore_ascii_case("running"),
                                cx,
                            );
                        }
                        if let Some(state) = collaboration_ui_state.as_deref() {
                            app.set_collaboration_for_capture(state, cx);
                        }
                        if let Some(state) = mcp_tool_call_ui_state.as_deref() {
                            app.set_mcp_tool_call_for_capture(state, cx);
                        }
                        if let Some(state) = image_generation_ui_state.as_deref() {
                            app.set_image_generation_for_capture(
                                state,
                                image_generation_path.clone(),
                                cx,
                            );
                        }
                        if let Some(state) = tool_group_state.as_deref() {
                            app.set_tool_group_for_capture(
                                state.eq_ignore_ascii_case("running"),
                                tool_group_expanded,
                                cx,
                            );
                        }
                        if let Some(state) = reasoning_ui_state.as_deref() {
                            app.set_reasoning_for_capture(state, reasoning_ui_expanded, cx);
                        }
                        if let Some(state) = approval_ui_state.as_deref() {
                            app.set_approval_for_capture(&approval_ui_kind, state, cx);
                        }
                        if let Some(state) = user_input_ui_state.as_deref() {
                            app.set_user_input_for_capture(state, cx);
                        }
                        if let Some(state) = file_approval_ui_state.as_deref() {
                            app.set_file_approval_for_capture(state, cx);
                        }
                        if let Some(state) = permissions_approval_ui_state.as_deref() {
                            app.set_permissions_approval_for_capture(
                                &permissions_approval_ui_kind,
                                state,
                                cx,
                            );
                        }
                        if let Some(state) = file_change_ui_state.as_deref() {
                            app.set_file_change_for_capture(state, cx);
                        }
                        if let Some(state) = turn_diff_ui_state.as_deref() {
                            app.set_turn_diff_for_capture(state, cx);
                        }
                        if let Some(slug) = settings_page {
                            app.open_settings_page(slug, cx);
                        } else if settings_open {
                            app.open_settings(cx);
                        }
                        if let Some(thread_id) = resume_thread.as_deref() {
                            app.resume_thread_for_capture(thread_id.to_owned(), cx);
                        }
                        app
                    });
                    #[cfg(feature = "screenshot")]
                    if let Some(path) = screenshot_path.clone() {
                        if let Some(thread_id) = resume_thread.clone() {
                            schedule_resumed_thread_screenshot(
                                window,
                                app.clone(),
                                path,
                                thread_id,
                                Instant::now() + Duration::from_secs(60),
                                resume_scroll_from_bottom,
                                RESUMED_THREAD_STABLE_FRAMES,
                            );
                        } else {
                            schedule_screenshot(
                                window,
                                path,
                                if maximize_after_open {
                                    screenshot_frames.max(90)
                                } else {
                                    screenshot_frames
                                },
                            );
                        }
                    }
                    app
                },
            )
            .expect("failed to open Codex window");
            cx.activate(true);
        });
}

#[cfg(test)]
mod tests {
    use super::normalize_resume_thread_id;

    #[test]
    fn resume_thread_id_accepts_chatgpt_sidebar_identity() {
        assert_eq!(
            normalize_resume_thread_id("local:01a06013-458c-7733-8aea-4f36df979feb"),
            "01a06013-458c-7733-8aea-4f36df979feb"
        );
        assert_eq!(
            normalize_resume_thread_id("01a06013-458c-7733-8aea-4f36df979feb"),
            "01a06013-458c-7733-8aea-4f36df979feb"
        );
    }
}
