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
    /// Theme underlay tint above the native blurred window material.
    pub surface_underlay: Rgba,
    /// The sidebar's actual translucent paint, matching the Electron shell.
    pub sidebar_surface: Rgba,
    pub surface_under: Rgba,
    pub elevated: Rgba,
    pub model_picker_surface: Rgba,
    pub project_dialog_surface: Rgba,
    pub control: Rgba,
    pub control_soft: Rgba,
    pub sidebar_hover: Rgba,
    pub sidebar_icon_muted: Rgba,
    /// Product-title foreground, kept separate for the OpenAI Sans substitute.
    pub sidebar_title_text: Rgba,
    /// Secondary sidebar foreground measured from the ChatGPT desktop app.
    pub sidebar_text_muted: Rgba,
    pub sidebar_resize_hover: Rgba,
    pub sidebar_resize_active: Rgba,
    pub text: Rgba,
    pub sidebar_text: Rgba,
    pub text_secondary: Rgba,
    pub text_tertiary: Rgba,
    pub home_mark: Rgba,
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
                // A light underlay stabilizes the material before the stronger
                // sidebar tint is composited above it.
                surface_underlay: rgba(0xf9f9f995),
                // Keep the native material visible without letting a bright
                // desktop dominate the sidebar. Combined with the underlay,
                // roughly 23% of the sampled background remains visible.
                sidebar_surface: rgba(0xededed73),
                // Resolved sidebar color over the canonical underlay. Sticky
                // overlays need this opaque value to avoid double compositing.
                surface_under: rgba(0xf6f6f6ff),
                elevated: rgba(0xffffffff),
                model_picker_surface: rgba(0xfafafaff),
                project_dialog_surface: rgba(0xfafafaff),
                control: rgba(0xffffffff),
                control_soft: rgba(0xffffffff),
                // chat-reference: --color-background-primary-ghost-hover
                // rgba(26, 28, 31, 0.053), quantized to an 8-bit alpha.
                sidebar_hover: rgba(0x1a1c1f0e),
                sidebar_icon_muted: rgba(0x1a1c1f7f),
                sidebar_title_text: rgba(0x1a1c1fd9),
                sidebar_text_muted: rgba(0x1a1c1f7f),
                sidebar_resize_hover: rgba(0x8b92994d),
                sidebar_resize_active: rgba(0x8b929999),
                text: rgba(0x1a1c1fff),
                sidebar_text: rgba(0x1a1c1fd9),
                text_secondary: rgba(0x5d5d5dff),
                text_tertiary: rgba(0x1a1c1f7e),
                home_mark: rgba(0xb8b9baff),
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
                // Reduce native-material transparency from 30% to 18%. This
                // retains the glass effect while preventing bright content
                // behind the window from washing the dark sidebar toward gray.
                surface_underlay: rgba(0x00000000),
                // Preserve the original #282828 material color while using a
                // stronger alpha for reliable contrast over bright desktops.
                sidebar_surface: rgba(0x282828d1),
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
                sidebar_icon_muted: rgba(0xffffff7f),
                sidebar_title_text: rgba(0xdfdfdfd9),
                sidebar_text_muted: rgba(0xffffff7f),
                sidebar_resize_hover: rgba(0x8b92994d),
                sidebar_resize_active: rgba(0x8b929999),
                text: rgba(0xdfdfdfff),
                sidebar_text: rgba(0xdfdfdfd9),
                text_secondary: rgba(0xc3c3c3ff),
                text_tertiary: rgba(0xffffff80),
                home_mark: rgba(0x565656ff),
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

#[cfg(test)]
mod tests {
    use gpui::Rgba;

    use super::{Theme, ThemeMode};

    fn composite(foreground: Rgba, background: Rgba) -> Rgba {
        let alpha = foreground.a + background.a * (1.0 - foreground.a);
        let channel = |foreground_channel: f32, background_channel: f32| {
            (foreground_channel * foreground.a
                + background_channel * background.a * (1.0 - foreground.a))
                / alpha
        };

        Rgba {
            r: channel(foreground.r, background.r),
            g: channel(foreground.g, background.g),
            b: channel(foreground.b, background.b),
            a: alpha,
        }
    }

    fn relative_luminance(color: Rgba) -> f32 {
        let linear = |channel: f32| {
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
    }

    fn contrast_ratio(a: Rgba, b: Rgba) -> f32 {
        let (lighter, darker) = if relative_luminance(a) > relative_luminance(b) {
            (relative_luminance(a), relative_luminance(b))
        } else {
            (relative_luminance(b), relative_luminance(a))
        };
        (lighter + 0.05) / (darker + 0.05)
    }

    #[test]
    fn sidebar_surfaces_keep_reduced_transparency() {
        let dark = Theme::for_mode(ThemeMode::Dark);
        let light = Theme::for_mode(ThemeMode::Light);

        assert!((dark.sidebar_surface.a - 209.0 / 255.0).abs() < f32::EPSILON);
        assert!((light.sidebar_surface.a - 115.0 / 255.0).abs() < f32::EPSILON);
        assert!(dark.sidebar_surface.a < 1.0);
        assert!(light.sidebar_surface.a < 1.0);
        assert_eq!(dark.surface_underlay.a, 0.0);
        assert!((light.surface_underlay.a - 149.0 / 255.0).abs() < f32::EPSILON);
    }

    #[test]
    fn sidebar_foreground_alpha_matches_the_desktop_app() {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let theme = Theme::for_mode(mode);
            assert!((theme.sidebar_text.a - 217.0 / 255.0).abs() < f32::EPSILON);
            assert!((theme.sidebar_title_text.a - 217.0 / 255.0).abs() < f32::EPSILON);
            assert!((theme.sidebar_text_muted.a - 127.0 / 255.0).abs() < f32::EPSILON);
            assert!((theme.sidebar_icon_muted.a - 127.0 / 255.0).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn sidebar_text_remains_legible_when_native_material_samples_white() {
        let white = Rgba {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        };

        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let theme = Theme::for_mode(mode);
            let underlay = composite(theme.surface_underlay, white);
            let sidebar = composite(theme.sidebar_surface, underlay);
            let primary_contrast = contrast_ratio(composite(theme.sidebar_text, sidebar), sidebar);
            let muted_contrast =
                contrast_ratio(composite(theme.sidebar_text_muted, sidebar), sidebar);
            let icon_contrast =
                contrast_ratio(composite(theme.sidebar_icon_muted, sidebar), sidebar);
            assert!(primary_contrast >= 4.5, "{mode:?}: {primary_contrast}");
            assert!(muted_contrast >= 3.0, "{mode:?}: {muted_contrast}");
            assert!(icon_contrast >= 3.0, "{mode:?}: {icon_contrast}");
        }
    }
}
