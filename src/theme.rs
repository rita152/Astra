use gpui::{Rgba, rgba};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
}

impl ThemeMode {
    pub fn from_name(value: &str) -> Self {
        if value.eq_ignore_ascii_case("light") {
            Self::Light
        } else {
            Self::Dark
        }
    }
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub surface: Rgba,
    pub surface_under: Rgba,
    pub elevated: Rgba,
    pub model_picker_surface: Rgba,
    pub project_dialog_surface: Rgba,
    pub control: Rgba,
    pub control_soft: Rgba,
    pub sidebar_hover: Rgba,
    pub sidebar_icon_muted: Rgba,
    pub sidebar_resize_hover: Rgba,
    pub sidebar_resize_active: Rgba,
    pub text: Rgba,
    pub sidebar_text: Rgba,
    pub text_secondary: Rgba,
    pub text_tertiary: Rgba,
    pub home_mark: Rgba,
    pub simple_scrim: Rgba,
    pub border: Rgba,
    pub accent: Rgba,
    pub warning: Rgba,
    pub effort: Rgba,
    pub button: Rgba,
    pub button_text: Rgba,
    pub scrollbar_thumb: Rgba,
    pub profile_menu_surface: Rgba,
    pub profile_menu_focus: Rgba,
    pub profile_menu_shadow: Rgba,
    pub settings_sidebar: Rgba,
    pub settings_panel: Rgba,
    pub settings_switch_off: Rgba,
    pub settings_search: Rgba,
    pub settings_accent: Rgba,
    pub settings_description: Rgba,
    pub settings_control: Rgba,
    pub settings_button: Rgba,
}

impl Theme {
    pub fn for_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Light => Self {
                surface: rgba(0xffffffff),
                surface_under: rgba(0xfcfcfcff),
                elevated: rgba(0xffffffff),
                model_picker_surface: rgba(0xfafafaff),
                project_dialog_surface: rgba(0xfafafaff),
                control: rgba(0xffffffff),
                control_soft: rgba(0xffffffff),
                // chat-reference: --color-background-primary-ghost-hover
                // rgba(26, 28, 31, 0.053), quantized to an 8-bit alpha.
                sidebar_hover: rgba(0x1a1c1f0e),
                sidebar_icon_muted: rgba(0x1a1c1f80),
                sidebar_resize_hover: rgba(0x8b92994d),
                sidebar_resize_active: rgba(0x8b929999),
                text: rgba(0x1a1c1fff),
                sidebar_text: rgba(0x1a1c1fd9),
                text_secondary: rgba(0x5d5d5dff),
                text_tertiary: rgba(0x1a1c1f7e),
                home_mark: rgba(0xb8b9baff),
                simple_scrim: rgba(0x0000001a),
                border: rgba(0x1a1c1f14),
                accent: rgba(0x339cffff),
                warning: rgba(0xe25507ff),
                effort: rgba(0x924ff7ff),
                button: rgba(0x1a1c1fff),
                button_text: rgba(0xffffffff),
                scrollbar_thumb: rgba(0xeaeaeaff),
                // The reference composes a 90% translucent elevated surface
                // over the light sidebar. Use the resolved color because the
                // GPUI surface is intentionally native and does not blur HTML.
                profile_menu_surface: rgba(0xfafafaff),
                profile_menu_focus: rgba(0xeeeeeeff),
                // chat-reference: --shadow-xl, 0 8px 16px -4px #0000001f.
                profile_menu_shadow: rgba(0x0000001f),
                settings_sidebar: rgba(0xf6f6f6ff),
                settings_panel: rgba(0xfbfbfbff),
                settings_switch_off: rgba(0x1a1c1f1a),
                settings_search: rgba(0xebebebff),
                settings_accent: rgba(0x539af8ff),
                settings_description: rgba(0x1a1c1fa6),
                settings_control: rgba(0xf7f7f7ff),
                settings_button: rgba(0xf0f0f0ff),
            },
            ThemeMode::Dark => Self {
                surface: rgba(0x181818ff),
                surface_under: rgba(0x222222ff),
                elevated: rgba(0x363636ff),
                // Resolved result of elevated-secondary/90 over #181818.
                model_picker_surface: rgba(0x2c2c2cff),
                // Captured opaque result of elevated-secondary/90 over the
                // canonical new-conversation background.
                project_dialog_surface: rgba(0x2b2b2bff),
                control: rgba(0x2d2d2dff),
                control_soft: rgba(0x2d2d2dff),
                // chat-reference: --color-background-primary-ghost-hover
                // rgba(255, 255, 255, 0.078), quantized to an 8-bit alpha.
                sidebar_hover: rgba(0xffffff14),
                sidebar_icon_muted: rgba(0xdfdfdf80),
                sidebar_resize_hover: rgba(0x8b92994d),
                sidebar_resize_active: rgba(0x8b929999),
                text: rgba(0xdfdfdfff),
                sidebar_text: rgba(0xdfdfdfd9),
                text_secondary: rgba(0xc3c3c3ff),
                text_tertiary: rgba(0xffffff80),
                home_mark: rgba(0x565656ff),
                simple_scrim: rgba(0xffffff1a),
                border: rgba(0xffffff14),
                accent: rgba(0x83c3ffff),
                warning: rgba(0xff8549ff),
                effort: rgba(0xad7bf9ff),
                button: rgba(0xdfdfdfff),
                button_text: rgba(0x2d2d2dff),
                scrollbar_thumb: rgba(0x343434ff),
                // The captured opaque result at the menu's unhighlighted
                // edges is #181818. Keep the native GPUI surface identical.
                profile_menu_surface: rgba(0x181818ff),
                profile_menu_focus: rgba(0x2e2e2eff),
                profile_menu_shadow: rgba(0x0000001f),
                settings_sidebar: rgba(0x171717ff),
                settings_panel: rgba(0x1f1f1fff),
                settings_switch_off: rgba(0xffffff1a),
                settings_search: rgba(0x232323ff),
                settings_accent: rgba(0x539af8ff),
                settings_description: rgba(0xdfdfdfa6),
                settings_control: rgba(0x262626ff),
                settings_button: rgba(0x292929ff),
            },
        }
    }
}
