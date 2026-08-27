mod app;
mod components;
mod settings;
mod theme;

use std::{borrow::Cow, fs, path::PathBuf};

use anyhow::Result;
use app::ChatApp;
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
                // NSVisualEffectBlendingModeBehindWindow = 0. GPUI creates
                // this view but otherwise leaves AppKit's WithinWindow mode,
                // which samples only our own clear surface and looks opaque.
                let _: () = msg_send![view, setBlendingMode: 0isize];
                let _: () = msg_send![view, setState: 1isize];
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
fn schedule_screenshot(window: &mut gpui::Window, path: String, frames: usize) {
    window.on_next_frame(move |window, cx| {
        if frames > 1 {
            schedule_screenshot(window, path, frames - 1);
        } else {
            match window
                .render_to_image()
                .and_then(|image| image.save(&path).map_err(anyhow::Error::from))
            {
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--theme=").map(ThemeMode::from_name))
        .unwrap_or(ThemeMode::Dark);
    let screenshot_path = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--screenshot=").map(ToOwned::to_owned));
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
        arg.strip_prefix("--project-menu-open=")?
            .parse::<usize>()
            .ok()
    });
    let project_create_open = args.iter().any(|arg| arg == "--project-create-open");
    let project_create_remote = args.iter().any(|arg| arg == "--project-create-remote");
    let activity_open = args.iter().any(|arg| arg == "--activity-open");
    let activity_scroll = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--activity-scroll=")?.parse::<f32>().ok());
    let activity_hover_recent = args.iter().find_map(|arg| {
        arg.strip_prefix("--activity-hover-recent=")?
            .parse::<usize>()
            .ok()
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
    let permission_confirmation_open = args
        .iter()
        .any(|arg| arg == "--permission-confirmation-open");
    let settings_open = args.iter().any(|arg| arg == "--settings-open");
    let settings_page = args.iter().find_map(|arg| {
        arg.strip_prefix("--settings-page=")
            .map(|slug| Box::leak(slug.to_owned().into_boxed_str()) as &'static str)
    });

    application()
        .with_assets(Assets {
            base: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"),
        })
        .run(move |cx: &mut App| {
            cx.set_window_appearance(Some(match mode {
                ThemeMode::Light => WindowAppearance::VibrantLight,
                ThemeMode::Dark => WindowAppearance::VibrantDark,
            }));
            cx.bind_keys([gpui::KeyBinding::new(
                "escape",
                app::DismissPermissionUi,
                None,
            )]);
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
                    window_min_size: Some(size(px(900.0), px(620.0))),
                    ..Default::default()
                },
                move |window, cx| {
                    configure_native_blur_sampling(window);
                    if maximize_after_open {
                        window.on_next_frame(|window, _| window.zoom_window());
                    }
                    #[cfg(feature = "screenshot")]
                    if let Some(path) = screenshot_path.clone() {
                        schedule_screenshot(window, path, if maximize_after_open { 90 } else { 2 });
                    }
                    cx.new(|cx| {
                        let mut app = ChatApp::new(mode, sidebar_bottom, cx);
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
                        if let Some(index) = project_menu_open {
                            app.open_project_menu_for_capture(index, cx);
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
                        if let Some(index) = activity_hover_recent {
                            app.open_activity(cx);
                            app.set_activity_hovered_recent_for_capture(index, cx);
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
                            app.set_permission_mode(mode, cx);
                        }
                        if permission_menu_open {
                            app.open_permission_menu(cx);
                        }
                        if permission_confirmation_open {
                            app.open_permission_confirmation(cx);
                        }
                        if let Some(slug) = settings_page {
                            app.open_settings_page(slug, cx);
                        } else if settings_open {
                            app.open_settings(cx);
                        }
                        app
                    })
                },
            )
            .expect("failed to open Codex window");
            cx.activate(true);
        });
}
