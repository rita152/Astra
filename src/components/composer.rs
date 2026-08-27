use std::time::Duration;

use gpui::{
    Animation, AnimationExt, BoxShadow, Context, Div, MouseButton, Render, Transformation, Window,
    deferred, div, hsla, linear_color_stop, linear_gradient, prelude::*, px, radians, rgba,
};

use crate::{
    components::icons::icon,
    theme::{Theme, ThemeMode},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PickerSubmenu {
    Model,
    Effort,
    Speed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DictationState {
    #[default]
    Idle,
    Recording,
    Transcribing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PermissionMode {
    Request,
    Assist,
    Full,
    Custom,
}

pub struct RequestFullAccess;
impl gpui::EventEmitter<RequestFullAccess> for ComposerView {}

const MODEL_PICKER_WIDTH: f32 = 224.0;
const MODEL_PICKER_SUBMENU_GAP: f32 = 1.0;
const MODEL_PICKER_RIGHT_INSET: f32 = 63.0;
const MODEL_PICKER_MIN_SUBMENU_WIDTH: f32 = 180.0;
const HOME_COMPOSER_MAX_WIDTH: f32 = 786.0;
const APP_SIDEBAR_WIDTH: f32 = 256.125;
const PARTICLE_TIMELINE_MS: f32 = 120_000.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct SubmenuLayout {
    open_left: bool,
    width: f32,
}

fn submenu_layout(viewport_width: f32, natural_width: f32) -> SubmenuLayout {
    let main_width = (viewport_width - APP_SIDEBAR_WIDTH).max(0.0);
    let composer_width = main_width.min(HOME_COMPOSER_MAX_WIDTH);
    let trailing_margin = ((main_width - composer_width) * 0.5).max(0.0);
    let available_right = trailing_margin + MODEL_PICKER_RIGHT_INSET - MODEL_PICKER_SUBMENU_GAP;

    if available_right >= MODEL_PICKER_MIN_SUBMENU_WIDTH {
        SubmenuLayout {
            open_left: false,
            width: natural_width.min(available_right),
        }
    } else {
        SubmenuLayout {
            open_left: true,
            width: natural_width,
        }
    }
}

fn particle_transition_ease(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    if progress == 0.0 || progress == 1.0 {
        return progress;
    }

    // Invert x for the reference cubic-bezier(.45, 0, .55, 1), then evaluate y.
    let mut lower = 0.0;
    let mut upper = 1.0;
    for _ in 0..10 {
        let parameter = (lower + upper) * 0.5;
        let inverse = 1.0 - parameter;
        let x = 3.0 * inverse * inverse * parameter * 0.45
            + 3.0 * inverse * parameter * parameter * 0.55
            + parameter * parameter * parameter;
        if x < progress {
            lower = parameter;
        } else {
            upper = parameter;
        }
    }
    let parameter = (lower + upper) * 0.5;
    let inverse = 1.0 - parameter;
    3.0 * inverse * parameter * parameter + parameter * parameter * parameter
}

fn particle_noise(index: usize, step: u32, channel: u32) -> f32 {
    let mut value = (index as u32 + 1)
        .wrapping_mul(0x9e37_79b9)
        .wrapping_add(step.wrapping_mul(0x85eb_ca6b))
        .wrapping_add(channel.wrapping_mul(0xc2b2_ae35));
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^= value >> 16;
    value as f32 / u32::MAX as f32
}

fn max_particle_drift(progress: f32, index: usize, duration_ms: u64) -> (f32, f32) {
    let segment_count = (PARTICLE_TIMELINE_MS / duration_ms as f32).round() as u32;
    let local = progress * segment_count as f32 + index as f32 * 0.37;
    let segment = local.floor() as u32 % segment_count;
    let next_segment = (segment + 1) % segment_count;
    let eased = particle_transition_ease(local.fract());
    let interpolate = |from: f32, to: f32| from + (to - from) * eased;

    let x = interpolate(
        particle_noise(index, segment, 0),
        particle_noise(index, next_segment, 0),
    );
    let y = interpolate(
        particle_noise(index, segment, 1),
        particle_noise(index, next_segment, 1),
    );
    ((x - 0.5) * 6.0, (y - 0.5) * 8.0)
}

fn particle_layers(slider_index: usize, fast_mode: bool) -> (bool, bool) {
    let show_fast_particles = fast_mode;
    // CDP: at data-max=true + data-fast-mode=true, MaxEffects retains only
    // its gradient canvas; the drifting TrackParticles layer is unmounted.
    let show_max_particles = slider_index == 5 && !fast_mode;
    (show_max_particles, show_fast_particles)
}

pub struct ComposerView {
    mode: ThemeMode,
    menu_open: bool,
    advanced_expanded: bool,
    submenu: Option<PickerSubmenu>,
    selected_model: &'static str,
    selected_effort: &'static str,
    selected_speed: &'static str,
    slider_index: usize,
    slider_dragging: bool,
    dictation_state: DictationState,
    dictation_cycle: u64,
    permission_mode: PermissionMode,
    permission_menu_open: bool,
}

impl ComposerView {
    pub fn new(mode: ThemeMode) -> Self {
        Self {
            mode,
            menu_open: false,
            advanced_expanded: true,
            submenu: None,
            selected_model: "5.6 Sol",
            selected_effort: "中",
            selected_speed: "快速",
            slider_index: 2,
            slider_dragging: false,
            dictation_state: DictationState::Idle,
            dictation_cycle: 0,
            permission_mode: PermissionMode::Full,
            permission_menu_open: false,
        }
    }

    pub fn set_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.mode = mode;
        cx.notify();
    }

    pub fn close_picker(&mut self, cx: &mut Context<Self>) {
        if self.menu_open {
            self.menu_open = false;
            self.submenu = None;
            cx.notify();
        }
        if self.permission_menu_open {
            self.permission_menu_open = false;
            cx.notify();
        }
    }

    pub fn set_permission_mode(&mut self, mode: &str, cx: &mut Context<Self>) {
        self.permission_mode = match mode {
            "request" => PermissionMode::Request,
            "assist" => PermissionMode::Assist,
            "custom" => PermissionMode::Custom,
            _ => PermissionMode::Full,
        };
        self.permission_menu_open = false;
        cx.notify();
    }

    pub fn open_permission_menu(&mut self, cx: &mut Context<Self>) {
        self.menu_open = false;
        self.submenu = None;
        self.permission_menu_open = true;
        cx.notify();
    }

    pub fn open_picker(&mut self, cx: &mut Context<Self>) {
        self.menu_open = true;
        cx.notify();
    }

    pub fn open_picker_submenu(&mut self, name: &str, cx: &mut Context<Self>) {
        self.menu_open = true;
        self.advanced_expanded = name != "simple";
        self.submenu = match name {
            "model" => Some(PickerSubmenu::Model),
            "effort" => Some(PickerSubmenu::Effort),
            "speed" => Some(PickerSubmenu::Speed),
            _ => None,
        };
        cx.notify();
    }

    pub fn open_picker_slider_at(&mut self, index: usize, fast: bool, cx: &mut Context<Self>) {
        self.menu_open = true;
        self.advanced_expanded = false;
        self.submenu = None;
        self.selected_speed = if fast { "快速" } else { "标准" };
        self.set_slider_index(index);
        cx.notify();
    }

    pub fn set_dictation_state_for_capture(&mut self, state: &str, cx: &mut Context<Self>) {
        self.dictation_state = match state {
            "recording" => DictationState::Recording,
            "transcribing" => DictationState::Transcribing,
            _ => DictationState::Idle,
        };
        cx.notify();
    }

    #[cfg(test)]
    fn dictation_state_name(&self) -> &'static str {
        match self.dictation_state {
            DictationState::Idle => "idle",
            DictationState::Recording => "recording",
            DictationState::Transcribing => "transcribing",
        }
    }

    #[cfg(test)]
    fn permission_mode_name(&self) -> &'static str {
        match self.permission_mode {
            PermissionMode::Request => "request",
            PermissionMode::Assist => "assist",
            PermissionMode::Full => "full",
            PermissionMode::Custom => "custom",
        }
    }

    fn set_slider_index(&mut self, index: usize) {
        self.slider_index = index.min(5);
        let (model, effort) = match self.slider_index {
            0 => ("5.6 Terra", "轻度"),
            1 => ("5.6 Sol", "轻度"),
            2 => ("5.6 Sol", "中"),
            3 => ("5.6 Sol", "高"),
            4 => ("5.6 Sol", "极高"),
            _ => ("5.6 Sol", "Ultra"),
        };
        self.selected_model = model;
        self.selected_effort = effort;
    }

    fn start_dictation(&mut self, cx: &mut Context<Self>) {
        self.dictation_cycle = self.dictation_cycle.wrapping_add(1);
        self.dictation_state = DictationState::Recording;
        self.menu_open = false;
        self.submenu = None;
        cx.notify();
    }

    fn cancel_dictation(&mut self, cx: &mut Context<Self>) {
        self.dictation_cycle = self.dictation_cycle.wrapping_add(1);
        self.dictation_state = DictationState::Idle;
        cx.notify();
    }

    fn stop_dictation(&mut self, cx: &mut Context<Self>) {
        if self.dictation_state != DictationState::Recording {
            return;
        }
        self.dictation_state = DictationState::Transcribing;
        self.dictation_cycle = self.dictation_cycle.wrapping_add(1);
        let cycle = self.dictation_cycle;
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            // CDP sampling showed the processing controls for roughly one second
            // before the ordinary composer footer returned.
            executor.timer(Duration::from_millis(1_050)).await;
            let _ = this.update(cx, |this, cx| {
                if this.dictation_state == DictationState::Transcribing
                    && this.dictation_cycle == cycle
                {
                    this.dictation_state = DictationState::Idle;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
}

fn utility(
    id: &'static str,
    label: &'static str,
    glyph: &'static str,
    horizontal_padding: f32,
    theme: Theme,
) -> impl IntoElement {
    let hover_fill = theme.text.alpha(0.05);

    div()
        .id(id)
        .h(px(28.0))
        .px(px(horizontal_padding))
        .rounded_full()
        .flex()
        .items_center()
        .gap(px(4.0))
        .text_size(px(13.0))
        .line_height(px(18.0))
        .font_weight(gpui::FontWeight(445.0))
        .text_color(theme.text)
        .cursor_pointer()
        .hover(move |style| style.bg(hover_fill))
        .child(icon(glyph, theme.text.into()).size(px(16.0)))
        .child(label)
}

fn project_utility(theme: Theme) -> impl IntoElement {
    let group = "composer-project-hover";
    let hover_fill = theme.text.alpha(0.05);

    div()
        .id("composer-project")
        .group(group)
        .relative()
        .h(px(28.0))
        .rounded_full()
        .flex_none()
        .child(
            div()
                .h(px(28.0))
                .px(px(8.0))
                .rounded_full()
                .flex()
                .items_center()
                .gap(px(4.0))
                .text_size(px(13.0))
                .line_height(px(18.0))
                .font_weight(gpui::FontWeight(445.0))
                .text_color(theme.text)
                .cursor_pointer()
                .group_hover(group, move |style| style.bg(hover_fill))
                .child(
                    icon("utility-folder", theme.text.into())
                        .size(px(16.0))
                        .group_hover(group, |style| style.invisible()),
                )
                .child("coda"),
        )
        // The reference overlays a 28px clear-project control on the leading
        // edge and swaps it with the folder whenever the project trigger is
        // hovered. Only this nested control promotes tertiary -> primary.
        .child(
            div()
                .id("composer-clear-project")
                .absolute()
                .top_0()
                .left_0()
                .size(px(28.0))
                .group("composer-clear-project-icon")
                .invisible()
                .group_hover(group, |style| style.visible())
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.text_tertiary)
                .cursor_pointer()
                .hover(move |style| style.bg(hover_fill).text_color(theme.text))
                .child(
                    icon("clear-project", theme.text_tertiary.into())
                        .size(px(16.0))
                        .group_hover("composer-clear-project-icon", move |style| {
                            style.text_color(theme.text)
                        }),
                ),
        )
}

fn context_toolbar(theme: Theme) -> Div {
    div()
        .relative()
        .h(px(38.0))
        .mx(px(13.0))
        // The reference toolbar continues underneath the Composer. Keeping
        // the extension in a separate background layer preserves the text's
        // 38px alignment while the later Composer layer masks its lower edge.
        .child(
            div()
                .absolute()
                .top(px(4.0))
                .left_0()
                .w_full()
                .h(px(52.0))
                .rounded(px(16.0))
                .bg(theme.surface_under),
        )
        .child(
            div()
                .relative()
                .top(px(4.0))
                .h(px(38.0))
                .w_full()
                .px(px(6.0))
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(project_utility(theme))
                .child(utility("composer-location", "本地", "local", 8.0, theme))
                // Although the HTML includes a trailing `px-0` class, the
                // home-placement `px-2` rule is emitted later in the bundled
                // stylesheet and wins the cascade. The computed button has
                // 8px inline padding on both sides.
                .child(utility("composer-branch", "main", "branch", 8.0, theme)),
        )
}

impl ComposerView {
    fn picker_row(
        &self,
        id: &'static str,
        label: &'static str,
        value: &'static str,
        submenu: PickerSubmenu,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let selected = self.submenu == Some(submenu);
        div()
            .id(id)
            .h(px(28.5625))
            .px(px(8.0))
            .rounded(px(12.5))
            .flex()
            .items_center()
            .text_size(px(13.0))
            .line_height(px(18.5625))
            .font_weight(gpui::FontWeight::NORMAL)
            .text_color(theme.text)
            .cursor_pointer()
            .when(selected, |row| row.bg(theme.sidebar_hover))
            .hover(move |style| style.bg(theme.sidebar_hover))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.submenu = if this.submenu == Some(submenu) {
                    None
                } else {
                    Some(submenu)
                };
                cx.notify();
            }))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered && this.submenu != Some(submenu) {
                    this.submenu = Some(submenu);
                    cx.notify();
                }
            }))
            .child(div().flex_1().child(label))
            .child(div().text_color(theme.text_tertiary).child(value))
            .child(
                icon("chevron-down", theme.text_tertiary.into())
                    .size(px(16.0))
                    .ml(px(12.0))
                    .with_transformation(Transformation::rotate(radians(
                        -std::f32::consts::FRAC_PI_2,
                    ))),
            )
    }

    fn option_row(
        &self,
        id: (&'static str, usize),
        title: &'static str,
        detail: Option<&'static str>,
        selected: bool,
        theme: Theme,
    ) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .min_h(px(if detail.is_some() { 47.125 } else { 28.5625 }))
            .px(px(8.0))
            .py(px(5.0))
            .rounded(px(12.5))
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .hover(move |style| style.bg(theme.sidebar_hover))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .text_size(px(13.0))
                    .line_height(px(18.5625))
                    .text_color(theme.text)
                    .child(title)
                    .when_some(detail, |column, detail| {
                        column.child(
                            div()
                                .text_size(px(12.0))
                                .line_height(px(18.5625))
                                .text_color(theme.text_tertiary)
                                .child(detail),
                        )
                    }),
            )
            .when(selected, |row| {
                row.child(
                    div()
                        .flex_none()
                        .w(px(16.0))
                        .text_size(px(13.0))
                        .text_color(theme.text)
                        .child("✓"),
                )
            })
    }

    fn submenu(
        &self,
        kind: PickerSubmenu,
        viewport_width: f32,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let (width, top): (f32, f32) = match kind {
            PickerSubmenu::Model => (280.0, -20.0),
            PickerSubmenu::Effort => (180.0, -36.0),
            PickerSubmenu::Speed => (233.0, 18.0),
        };
        let layout = submenu_layout(viewport_width, width);
        let mut menu = div()
            .id("model-picker-submenu")
            .absolute()
            .top(px(top))
            .w(px(layout.width))
            .when(layout.open_left, |menu| {
                menu.right(px(MODEL_PICKER_WIDTH + MODEL_PICKER_SUBMENU_GAP))
            })
            .when(!layout.open_left, |menu| {
                menu.left(px(MODEL_PICKER_WIDTH + MODEL_PICKER_SUBMENU_GAP))
            })
            .p(px(4.0))
            .rounded(px(15.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme.model_picker_surface)
            .shadow(vec![
                BoxShadow::new(px(0.0), px(8.0), hsla(0.0, 0.0, 0.0, 0.12))
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()));

        match kind {
            PickerSubmenu::Model => {
                const OPTIONS: [&str; 7] = [
                    "5.6 Sol",
                    "5.6 Terra",
                    "5.6 Luna",
                    "5.5",
                    "5.4",
                    "5.4 Mini",
                    "5.3 Codex Spark",
                ];
                for (index, option) in OPTIONS.into_iter().enumerate() {
                    menu = menu.child(
                        self.option_row(
                            ("model-option", index),
                            option,
                            None,
                            self.selected_model == option,
                            theme,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.selected_model = option;
                            if this.selected_effort == "轻度" {
                                this.slider_index = if option == "5.6 Terra" { 0 } else { 1 };
                            }
                            this.menu_open = false;
                            this.submenu = None;
                            cx.notify();
                        })),
                    );
                }
            }
            PickerSubmenu::Effort => {
                const OPTIONS: [(&str, Option<&str>); 6] = [
                    ("轻度", None),
                    ("中", None),
                    ("高", None),
                    ("极高", None),
                    ("最高", None),
                    ("Ultra", Some("更快消耗使用额度")),
                ];
                menu = menu.child(
                    div()
                        .h(px(26.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .text_size(px(13.0))
                        .line_height(px(18.5625))
                        .text_color(theme.text_tertiary)
                        .child("推理强度"),
                );
                for (index, (option, detail)) in OPTIONS.into_iter().enumerate() {
                    menu = menu.child(
                        self.option_row(
                            ("effort-option", index),
                            option,
                            detail,
                            self.selected_effort == option,
                            theme,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if option == "Ultra" {
                                this.slider_index = 5;
                                this.selected_effort = option;
                            } else {
                                let slider_index = match option {
                                    "轻度" if this.selected_model == "5.6 Terra" => 0,
                                    "轻度" => 1,
                                    "中" => 2,
                                    "高" => 3,
                                    "极高" => 4,
                                    _ => 5,
                                };
                                this.set_slider_index(slider_index);
                            }
                            this.menu_open = false;
                            this.submenu = None;
                            cx.notify();
                        })),
                    );
                }
            }
            PickerSubmenu::Speed => {
                const OPTIONS: [(&str, &str); 2] =
                    [("标准", "默认速度"), ("快速", "1.5 倍速度，用量更多")];
                menu = menu.child(
                    div()
                        .h(px(26.0))
                        .px(px(8.0))
                        .flex()
                        .items_center()
                        .text_size(px(13.0))
                        .line_height(px(18.5625))
                        .text_color(theme.text_tertiary)
                        .child("速度"),
                );
                for (index, (option, detail)) in OPTIONS.into_iter().enumerate() {
                    menu = menu.child(
                        self.option_row(
                            ("speed-option", index),
                            option,
                            Some(detail),
                            self.selected_speed == option,
                            theme,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.selected_speed = option;
                            this.menu_open = false;
                            this.submenu = None;
                            cx.notify();
                        })),
                    );
                }
            }
        }
        menu
    }

    fn view_controls(&self, show_fast_toggle: bool, theme: Theme, cx: &mut Context<Self>) -> Div {
        let controls = div()
            .h(px(32.0))
            .flex()
            .items_center()
            .child(
                div()
                    .id("model-picker-advanced")
                    .h(px(32.0))
                    .w(px(58.0))
                    .p(px(4.0))
                    .rounded(px(8.0))
                    .flex()
                    .items_center()
                    .text_color(theme.text_tertiary)
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.sidebar_hover))
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.advanced_expanded = !this.advanced_expanded;
                        this.submenu = None;
                        this.slider_dragging = false;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w_full()
                            .h_full()
                            .px(px(4.0))
                            .py(px(2.0))
                            .rounded(px(6.0))
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child("高级")
                            .child(
                                icon("chevron-down", theme.text_tertiary.into())
                                    .size(px(12.0))
                                    .with_transformation(Transformation::rotate(radians(
                                        if self.advanced_expanded {
                                            std::f32::consts::PI
                                        } else {
                                            0.0
                                        },
                                    ))),
                            ),
                    ),
            )
            .child(div().flex_1());

        if show_fast_toggle {
            controls.child(
                div()
                    .id("model-picker-fast-toggle")
                    .size(px(32.0))
                    .rounded(px(8.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.sidebar_hover))
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.selected_speed = if this.selected_speed == "快速" {
                            "标准"
                        } else {
                            "快速"
                        };
                        cx.notify();
                    }))
                    .child(
                        icon(
                            "model-fast",
                            if self.selected_speed == "快速" {
                                if self.slider_index == 5 {
                                    rgba(0xad7bf9ff).into()
                                } else {
                                    rgba(0x339cffff).into()
                                }
                            } else {
                                theme.text_tertiary.into()
                            },
                        )
                        .size(px(16.0)),
                    ),
            )
        } else {
            controls
        }
    }

    fn power_slider(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        const TRACK_WIDTH: f32 = 200.0;
        const TRACK_INSET: f32 = 13.0;
        const STEP: f32 = (TRACK_WIDTH - TRACK_INSET * 2.0) / 5.0;
        let thumb_center = TRACK_INSET + STEP * self.slider_index as f32;
        let thumb_size = if self.slider_dragging { 32.0 } else { 28.0 };
        let mut range = div()
            .absolute()
            .left_0()
            .top_0()
            .h_full()
            .w(px(thumb_center))
            .rounded(px(12.0))
            .overflow_hidden()
            .bg(rgba(0x339cffff));

        if self.slider_index == 5 {
            // Keep the first radius of the rounded range as solid blue. Starting the
            // rectangular gradient at the circle tangent prevents its square corners
            // from leaking through the rounded left cap.
            const RANGE_RADIUS: f32 = 12.0;
            range = range.child(
                div()
                    .absolute()
                    .left(px(RANGE_RADIUS))
                    .right_0()
                    .top_0()
                    .bottom_0()
                    .flex()
                    .child(div().h_full().w(px(TRACK_WIDTH * 0.55 - RANGE_RADIUS)).bg(
                        linear_gradient(
                            90.0,
                            linear_color_stop(rgba(0x339cffff), 0.0),
                            linear_color_stop(rgba(0xad7bf9ff), 1.0),
                        ),
                    ))
                    .child(div().h_full().flex_1().bg(linear_gradient(
                        90.0,
                        linear_color_stop(rgba(0xad7bf9ff), 0.0),
                        linear_color_stop(rgba(0x8b84fbff), 1.0),
                    ))),
            );
        }

        let (show_max_particles, show_fast_particles) =
            particle_layers(self.slider_index, self.selected_speed == "快速");
        if show_max_particles || show_fast_particles {
            const MAX_PARTICLES: [(f32, f32, f32, f32, f32, u64); 14] = [
                (0.50, 3.0, 17.0, 0.616, 0.405, 3102),
                (0.87, 3.0, 17.0, 0.740, 0.528, 2843),
                (0.72, 0.0, 17.0, 0.946, 0.837, 1352),
                (0.60, -3.0, 8.0, 0.595, 0.898, 1607),
                (0.48, -4.0, 15.0, 0.627, 0.993, 1738),
                (0.56, -1.0, 11.0, 0.617, 0.718, 1967),
                (0.18, -3.0, 12.0, 0.522, 0.788, 2066),
                (0.42, -2.0, 11.0, 0.682, 0.992, 1660),
                (0.29, -1.0, 12.0, 0.580, 0.755, 2481),
                (0.49, 0.0, 13.0, 0.564, 0.701, 1577),
                (0.90, -3.0, 12.0, 0.852, 0.614, 2675),
                (0.08, -3.0, 5.0, 0.523, 0.892, 2156),
                (0.13, -3.0, 14.0, 0.759, 0.773, 1553),
                (0.08, 2.0, 14.0, 0.563, 0.880, 1863),
            ];
            const FAST_PARTICLES: [(f32, f32, f32, u64, f32); 14] = [
                (8.94, 0.616, 0.405, 1701, 0.20),
                (16.65, 0.740, 0.528, 1708, 0.12),
                (20.50, 0.946, 0.837, 2259, 0.08),
                (7.68, 0.595, 0.898, 1928, 0.99),
                (10.22, 0.627, 0.993, 1843, 0.91),
                (19.24, 0.617, 0.718, 1629, 0.78),
                (6.07, 0.522, 0.788, 1963, 0.68),
                (15.18, 0.682, 0.992, 1851, 0.57),
                (10.92, 0.580, 0.755, 1745, 0.46),
                (3.44, 0.564, 0.701, 2014, 0.39),
                (18.27, 0.852, 0.614, 1843, 0.31),
                (13.25, 0.523, 0.892, 2292, 0.24),
                (20.65, 0.759, 0.773, 1982, 0.16),
                (14.98, 0.563, 0.880, 1809, 0.08),
            ];

            // One phase-locked clock drives the complete particle layer. Each particle
            // moves at canvas paint time, so animation frames neither fan out into 14/28
            // independent timers nor invalidate their layout positions.
            range = range.child(div().absolute().inset_0().with_animation(
                "model-slider-particles",
                Animation::new(Duration::from_secs(120)).repeat_synced(),
                move |layer, progress| {
                    layer.child(
                        gpui::canvas(
                            |bounds, _, _| bounds,
                            move |bounds, _, window, _| {
                                let mut paint_particle =
                                    |x: f32, y: f32, diameter: f32, opacity: f32| {
                                        let particle_bounds = gpui::Bounds {
                                            origin: gpui::point(
                                                bounds.origin.x + px(x),
                                                bounds.origin.y + px(y),
                                            ),
                                            size: gpui::size(px(diameter), px(diameter)),
                                        };
                                        let radius = px(diameter / 2.0);
                                        window.paint_drop_shadows(
                                            particle_bounds,
                                            radius.into(),
                                            &[BoxShadow::new(
                                                px(0.0),
                                                px(0.0),
                                                hsla(0.0, 0.0, 1.0, 0.34 * opacity),
                                            )
                                            .blur_radius(px(5.0))],
                                        );
                                        window.paint_quad(gpui::quad(
                                            particle_bounds,
                                            radius,
                                            hsla(0.0, 0.0, 1.0, 0.72 * opacity),
                                            px(0.0),
                                            hsla(0.0, 0.0, 1.0, 0.0),
                                            Default::default(),
                                        ));
                                    };

                                if show_max_particles {
                                    for (index, (position, offset, y, scale, opacity, duration)) in
                                        MAX_PARTICLES.into_iter().enumerate()
                                    {
                                        let (drift_x, drift_y) =
                                            max_particle_drift(progress, index, duration);
                                        paint_particle(
                                            position * TRACK_WIDTH + offset + drift_x,
                                            (y + drift_y).clamp(4.0, 20.0),
                                            3.0 * scale,
                                            opacity,
                                        );
                                    }
                                }

                                if show_fast_particles {
                                    for (y, scale, base_opacity, duration, phase) in FAST_PARTICLES
                                    {
                                        let loops =
                                            (PARTICLE_TIMELINE_MS / duration as f32).round();
                                        let travel = (progress * loops + phase).fract();
                                        let opacity = if travel < 0.08 {
                                            travel / 0.08
                                        } else if travel > 0.92 {
                                            (1.0 - travel) / 0.08
                                        } else {
                                            1.0
                                        };
                                        paint_particle(
                                            (1.0 - travel) * thumb_center,
                                            y,
                                            3.0 * scale,
                                            opacity * base_opacity,
                                        );
                                    }
                                }
                            },
                        )
                        .absolute()
                        .inset_0()
                        .size_full(),
                    )
                },
            ));
        }

        let mut track = div()
            .absolute()
            .left_0()
            .top(px(2.0))
            .w(px(TRACK_WIDTH))
            .h(px(24.0))
            .rounded(px(12.0))
            .overflow_hidden()
            .bg(theme.text.alpha(0.10))
            .border(px(0.5))
            .border_color(theme.border)
            .child(range);

        for index in 0..6 {
            let center = TRACK_INSET + STEP * index as f32;
            let selected = index <= self.slider_index;
            let hidden = self.slider_index == 5 || (self.selected_speed == "快速" && selected);
            track = track.child(
                div()
                    .absolute()
                    .left(px(center - if hidden { 1.5 } else { 2.0 }))
                    .top(px(if hidden { 10.5 } else { 10.0 }))
                    .size(px(if hidden { 3.0 } else { 4.0 }))
                    .rounded_full()
                    .bg(if selected {
                        rgba(0xffffff4d)
                    } else {
                        rgba(0xffffff40)
                    })
                    .opacity(if hidden { 0.0 } else { 1.0 }),
            );
        }

        let mut hit_areas = div().absolute().inset_0();
        for index in 0..6 {
            let center = TRACK_INSET + STEP * index as f32;
            let left = if index == 0 { 0.0 } else { center - STEP * 0.5 };
            let right = if index == 5 {
                TRACK_WIDTH
            } else {
                center + STEP * 0.5
            };
            hit_areas = hit_areas.child(
                div()
                    .id(("model-power-slider-step", index))
                    .absolute()
                    .left(px(left))
                    .top_0()
                    .w(px(right - left))
                    .h_full()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.slider_dragging = true;
                            this.set_slider_index(index);
                            cx.notify();
                        }),
                    )
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if *hovered && this.slider_dragging && this.slider_index != index {
                            this.set_slider_index(index);
                            cx.notify();
                        }
                    })),
            );
        }

        div()
            .id("model-power-slider")
            .relative()
            .h(px(32.0))
            .mx(px(2.0))
            .px(px(6.0))
            .py(px(2.0))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.slider_dragging = false;
                    cx.notify();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.slider_dragging = false;
                    cx.notify();
                }),
            )
            .child(
                div()
                    .relative()
                    .w(px(TRACK_WIDTH))
                    .h(px(28.0))
                    .child(track)
                    .child(
                        div()
                            .absolute()
                            .left(px(thumb_center - thumb_size * 0.5))
                            .top(px((28.0 - thumb_size) * 0.5))
                            .size(px(thumb_size))
                            .rounded_full()
                            .bg(rgba(0xffffffff))
                            .border(px(0.5))
                            .border_color(rgba(0xffffff28))
                            .shadow(vec![
                                BoxShadow::new(px(0.0), px(0.0), hsla(0.0, 0.0, 0.0, 0.10))
                                    .blur_radius(px(2.0)),
                            ]),
                    )
                    .child(hit_areas),
            )
    }

    fn model_menu(
        &self,
        viewport_width: f32,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let mut menu = div()
            .id("model-picker-menu")
            .absolute()
            .right(px(MODEL_PICKER_RIGHT_INSET))
            .bottom(px(46.0))
            .w(px(MODEL_PICKER_WIDTH))
            .p(px(4.0))
            .rounded(px(15.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme.model_picker_surface)
            .shadow(vec![
                BoxShadow::new(px(0.0), px(8.0), hsla(0.0, 0.0, 0.0, 0.12))
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .font_family(".SystemUIFont")
            .text_size(px(13.0))
            .font_weight(gpui::FontWeight::NORMAL)
            .text_color(theme.text)
            .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()));

        if self.advanced_expanded {
            menu = menu
                .child(self.picker_row(
                    "model-picker-model-row",
                    "模型",
                    self.selected_model,
                    PickerSubmenu::Model,
                    theme,
                    cx,
                ))
                .child(self.picker_row(
                    "model-picker-effort-row",
                    "推理强度",
                    self.selected_effort,
                    PickerSubmenu::Effort,
                    theme,
                    cx,
                ))
                .child(self.picker_row(
                    "model-picker-speed-row",
                    "速度",
                    self.selected_speed,
                    PickerSubmenu::Speed,
                    theme,
                    cx,
                ))
                .child(
                    div()
                        .h(px(8.0))
                        .px(px(8.0))
                        .py(px(3.5))
                        .child(div().h(px(1.0)).w_full().bg(theme.border)),
                )
                .child(self.view_controls(false, theme, cx));
        } else {
            menu = menu
                .child(self.view_controls(true, theme, cx))
                .child(div().h(px(4.0)))
                .child(self.power_slider(theme, cx))
                .child(div().h(px(8.0)));
        }

        if let Some(submenu) = self.submenu {
            menu = menu.child(deferred(self.submenu(submenu, viewport_width, theme, cx)));
        }
        menu
    }

    fn dictation_waveform(&self, theme: Theme) -> impl IntoElement {
        div()
            .id("composer-dictation-waveform")
            .h(px(28.0))
            .min_w(px(0.0))
            .flex_1()
            .with_animation(
                "composer-dictation-waveform-motion",
                Animation::new(Duration::from_secs(36)).repeat(),
                move |waveform, progress| {
                    waveform.child(
                        gpui::canvas(
                            |bounds, _, _| bounds,
                            move |bounds, _, window, _| {
                                // The desktop app paints a 2px pill every 6px on a
                                // transparent 28px canvas. Quiet samples are 2px tall at
                                // 20% text opacity; voiced samples grow to 22px and 55%.
                                let width = f32::from(bounds.size.width).max(0.0);
                                let count = (width / 6.0).ceil() as usize;
                                let phase = progress * 610.0;
                                for index in 0..count {
                                    let sample = index as f32 + phase;
                                    let envelope = ((sample * 0.097).sin().abs().powf(22.0)
                                        * (0.45 + 0.55 * (sample * 0.271).sin().abs()))
                                    .max(
                                        (sample * 0.043 + 1.7).sin().abs().powf(34.0)
                                            * (sample * 0.191).sin().abs(),
                                    );
                                    let height = 2.0 + 20.0 * envelope;
                                    let x = index as f32 * 6.0 + 1.0;
                                    if x >= width {
                                        break;
                                    }
                                    let bar = gpui::Bounds {
                                        origin: gpui::point(
                                            bounds.origin.x + px(x),
                                            bounds.origin.y + px((28.0 - height) * 0.5),
                                        ),
                                        size: gpui::size(px(2.0), px(height)),
                                    };
                                    window.paint_quad(gpui::quad(
                                        bar,
                                        px(1.0),
                                        theme.text.alpha(0.20 + 0.35 * envelope),
                                        px(0.0),
                                        rgba(0x00000000),
                                        Default::default(),
                                    ));
                                }
                            },
                        )
                        .size_full(),
                    )
                },
            )
    }

    fn dictation_footer(&self, theme: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let transcribing = self.dictation_state == DictationState::Transcribing;
        let soft_fill = theme.text.alpha(0.05);
        let strong_fill = theme.text.alpha(0.10);

        let cancel = div()
            .id("composer-dictation-cancel")
            .size(px(28.0))
            .flex_none()
            .rounded_full()
            .bg(soft_fill)
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(move |style| style.bg(strong_fill))
            .on_click(cx.listener(|this, _, _, cx| this.cancel_dictation(cx)))
            .child(icon("dictation-cancel", theme.text.into()).size(px(16.0)));

        let stop_icon = if transcribing {
            icon("dictation-spinner", theme.text.into())
                .size(px(16.0))
                .with_animation(
                    "composer-dictation-spinner-motion",
                    Animation::new(Duration::from_millis(800)).repeat(),
                    |spinner, progress| {
                        spinner.with_transformation(Transformation::rotate(radians(
                            progress * std::f32::consts::TAU,
                        )))
                    },
                )
                .into_any_element()
        } else {
            icon("dictation-stop", theme.text.into())
                .size(px(16.0))
                .into_any_element()
        };

        let stop = div()
            .id("composer-dictation-stop")
            .size(px(28.0))
            .flex_none()
            .rounded_full()
            .bg(if transcribing { soft_fill } else { strong_fill })
            .flex()
            .items_center()
            .justify_center()
            .when(!transcribing, |button| {
                button
                    .cursor_pointer()
                    .hover(move |style| style.bg(strong_fill))
                    .on_click(cx.listener(|this, _, _, cx| this.stop_dictation(cx)))
            })
            .child(stop_icon);

        let send = div()
            .id("composer-dictation-send")
            .size(px(28.0))
            .flex_none()
            .rounded_full()
            .bg(theme.button)
            .when(transcribing, |button| button.opacity(0.4))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| this.stop_dictation(cx)))
            .child(icon("dictation-send", theme.button_text.into()).size(px(16.0)));

        div()
            .id("composer-dictation-footer")
            .h(px(36.0))
            .relative()
            .top(px(7.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(cancel)
            .child(self.dictation_waveform(theme))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(stop)
                    .child(send),
            )
    }

    fn permission_label(&self) -> (&'static str, &'static str, gpui::Rgba) {
        let theme = Theme::for_mode(self.mode);
        match self.permission_mode {
            PermissionMode::Request => ("请求批准", "permission-request", theme.text_tertiary),
            PermissionMode::Assist => ("帮我批准", "permission-assist", theme.text_tertiary),
            PermissionMode::Full => ("完全访问", "permission", theme.warning),
            PermissionMode::Custom => ("自定义", "permission-custom", theme.text_tertiary),
        }
    }

    fn permission_row(
        &self,
        index: usize,
        mode: PermissionMode,
        title: &'static str,
        detail: &'static str,
        glyph: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let selected = self.permission_mode == mode;
        let warning = mode == PermissionMode::Full;
        let color = if warning { theme.warning } else { theme.text };
        div()
            .id(("permission-menu-item", index))
            .h(px(47.125))
            .px(px(8.0))
            .py(px(5.0))
            .rounded(px(12.5))
            .flex()
            .items_center()
            .cursor_pointer()
            .hover(move |style| style.bg(theme.sidebar_hover))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.permission_menu_open = false;
                if mode == PermissionMode::Full && this.permission_mode != PermissionMode::Full {
                    cx.emit(RequestFullAccess);
                } else {
                    this.permission_mode = mode;
                }
                cx.notify();
            }))
            .child(icon(glyph, color.into()).size(px(20.0)).opacity(0.75))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .ml(px(12.0))
                    // CoreText's Chinese glyph run is fractionally wider than
                    // Chromium's system-ui run sampled over CDP.
                    .text_size(px(12.75))
                    .line_height(px(18.5625))
                    .child(div().text_color(color).child(title))
                    .child(
                        div()
                            .text_color(if warning {
                                theme.warning
                            } else {
                                theme.text_tertiary
                            })
                            .child(detail),
                    ),
            )
            .when(selected, |row| {
                row.child(
                    icon("permission-check", color.into())
                        .size(px(17.0))
                        .ml(px(12.0))
                        .opacity(0.75),
                )
            })
    }

    fn permission_menu(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let width = if self.permission_mode == PermissionMode::Full {
            355.0
        } else {
            327.0
        };
        div()
            .id("permission-menu")
            .absolute()
            .left(px(42.0))
            // CDP: the menu's bottom edge is 1.5px above the 28px trigger.
            .bottom(px(36.5))
            .w(px(width))
            .h(px(222.5))
            .p(px(4.0))
            .rounded(px(15.0))
            .border(px(0.5))
            .border_color(theme.border)
            .bg(theme.model_picker_surface)
            .shadow(vec![
                BoxShadow::new(px(0.0), px(8.0), hsla(0.0, 0.0, 0.0, 0.12))
                    .blur_radius(px(16.0))
                    .spread_radius(px(-4.0)),
            ])
            .font_family(".SystemUIFont")
            .text_size(px(13.0))
            .font_weight(gpui::FontWeight::NORMAL)
            .text_color(theme.text)
            .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
            .child(
                div()
                    .h(px(26.0))
                    .px(px(8.0))
                    .py(px(5.0))
                    .flex()
                    .items_start()
                    .text_size(px(13.0))
                    .line_height(px(16.0))
                    .text_color(theme.text_tertiary)
                    .child(div().flex_1().child("应如何批准 ChatGPT 操作？"))
                    .child(
                        div()
                            .id("permission-learn-more")
                            .cursor_pointer()
                            .underline()
                            .child("了解更多"),
                    ),
            )
            .child(self.permission_row(
                0,
                PermissionMode::Request,
                "请求批准",
                "编辑外部文件和使用互联网时始终询问",
                "permission-request",
                theme,
                cx,
            ))
            .child(self.permission_row(
                1,
                PermissionMode::Assist,
                "帮我批准",
                "仅对检测到的风险操作请求批准",
                "permission-assist",
                theme,
                cx,
            ))
            .child(self.permission_row(
                2,
                PermissionMode::Full,
                "完全访问权限",
                "可不受限制地访问互联网和你电脑上的任何文件",
                "permission",
                theme,
                cx,
            ))
            .child(self.permission_row(
                3,
                PermissionMode::Custom,
                "自定义 (config.toml)",
                "使用 config.toml 中定义的权限",
                "permission-custom",
                theme,
                cx,
            ))
    }

    fn render_composer(&self, viewport_width: f32, theme: Theme, cx: &mut Context<Self>) -> Div {
        let (permission_label, permission_icon, permission_color) = self.permission_label();
        div()
            .w_full()
            .relative()
            .flex()
            .flex_col()
            .gap(px(0.0))
            .child(context_toolbar(theme))
            .child(
                div()
                    .h(px(100.0))
                    .w_full()
                    .rounded(px(24.0))
                    .bg(theme.control_soft)
                    .shadow(vec![
                        BoxShadow::new(px(0.0), px(0.0), theme.border.into())
                            .spread_radius(px(0.5)),
                        BoxShadow::new(px(0.0), px(3.0), hsla(0.0, 0.0, 0.0, 0.04))
                            .blur_radius(px(7.5)),
                        BoxShadow::new(px(0.0), px(0.0), hsla(0.0, 0.0, 0.0, 0.05))
                            .blur_radius(px(20.0)),
                        BoxShadow::new(px(0.0), px(0.0), theme.surface.into())
                            .spread_radius(px(0.5)),
                    ])
                    .flex()
                    .flex_col()
                    .px(px(8.0))
                    .py(px(12.0))
                    .child(div().flex_1())
                    .when(self.dictation_state == DictationState::Idle, |composer| {
                        composer.child(
                            div()
                                .h(px(36.0))
                                .relative()
                                .top(px(7.0))
                                .flex()
                                .items_center()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(5.0))
                                        .child(
                                            div()
                                                .id("composer-add-context")
                                                .size(px(28.0))
                                                .rounded_full()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .cursor_pointer()
                                                .hover(move |style| style.bg(theme.sidebar_hover))
                                                .child(
                                                    icon("add", theme.text.into()).size(px(16.0)),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .id("composer-permissions")
                                                .h(px(28.0))
                                                .px(px(8.0))
                                                .rounded_full()
                                                .flex()
                                                .items_center()
                                                .gap(px(4.0))
                                                .text_size(px(13.0))
                                                .line_height(px(18.0))
                                                .text_color(permission_color)
                                                .cursor_pointer()
                                                .when(self.permission_menu_open, |button| {
                                                    button.bg(theme.sidebar_hover)
                                                })
                                                .hover(move |style| style.bg(theme.sidebar_hover))
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(|this, _, _, cx| {
                                                        cx.stop_propagation();
                                                        this.menu_open = false;
                                                        this.submenu = None;
                                                        this.permission_menu_open =
                                                            !this.permission_menu_open;
                                                        cx.notify();
                                                    }),
                                                )
                                                .on_click(cx.listener(|_, _, _, cx| {
                                                    cx.stop_propagation();
                                                }))
                                                .child(
                                                    div()
                                                        .size(px(16.0))
                                                        .flex_none()
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .child(icon(
                                                            permission_icon,
                                                            permission_color.into(),
                                                        )),
                                                )
                                                .child(permission_label),
                                        ),
                                )
                                .child(div().flex_1())
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .min_w(px(0.0))
                                        .child(
                                            div()
                                                .id("composer-model-picker")
                                                .h(px(28.0))
                                                .px(px(8.0))
                                                .rounded_full()
                                                .flex()
                                                .items_center()
                                                .gap(px(4.0))
                                                .text_size(px(13.0))
                                                .line_height(px(18.0))
                                                .cursor_pointer()
                                                .when(self.menu_open, |button| {
                                                    button.bg(theme.sidebar_hover)
                                                })
                                                .hover(move |style| style.bg(theme.sidebar_hover))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    cx.stop_propagation();
                                                    this.permission_menu_open = false;
                                                    this.menu_open = !this.menu_open;
                                                    if !this.menu_open {
                                                        this.submenu = None;
                                                    }
                                                    cx.notify();
                                                }))
                                                .when(self.selected_speed == "快速", |button| {
                                                    button.child(
                                                        icon("model-fast", theme.text.into())
                                                            .size(px(14.0)),
                                                    )
                                                })
                                                .child(
                                                    div()
                                                        .text_color(theme.text)
                                                        .child(self.selected_model),
                                                )
                                                .child(
                                                    div()
                                                        .text_color(theme.effort)
                                                        .child(self.selected_effort),
                                                )
                                                .child(
                                                    icon(
                                                        "chevron-down",
                                                        theme.text_tertiary.into(),
                                                    )
                                                    .size(px(14.0)),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(8.0))
                                                .child(
                                                    div()
                                                        .id("composer-dictation")
                                                        .size(px(28.0))
                                                        .rounded_full()
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .cursor_pointer()
                                                        .hover(move |style| {
                                                            style.bg(theme.sidebar_hover)
                                                        })
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.start_dictation(cx)
                                                        }))
                                                        .child(
                                                            icon("dictation", theme.text.into())
                                                                .size(px(16.0)),
                                                        ),
                                                )
                                                .child(
                                                    div()
                                                        .id("composer-voice")
                                                        .size(px(28.0))
                                                        .flex_none()
                                                        .rounded_full()
                                                        .bg(theme.button)
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .cursor_pointer()
                                                        .child(
                                                            icon("voice", theme.button_text.into())
                                                                .size(px(16.0)),
                                                        ),
                                                ),
                                        ),
                                ),
                        )
                    })
                    .when(self.dictation_state != DictationState::Idle, |composer| {
                        composer.child(self.dictation_footer(theme, cx))
                    }),
            )
            .when(self.menu_open, |composer| {
                composer.child(deferred(self.model_menu(viewport_width, theme, cx)))
            })
            .when(self.permission_menu_open, |composer| {
                composer.child(deferred(self.permission_menu(theme, cx)))
            })
    }
}

impl Render for ComposerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_composer(
            f32::from(window.viewport_size().width),
            Theme::for_mode(self.mode),
            cx,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ComposerView, SubmenuLayout, max_particle_drift, particle_layers, particle_transition_ease,
        submenu_layout,
    };
    use crate::theme::ThemeMode;
    use gpui::{Bounds, MouseButton, TestApp, WindowBounds, WindowOptions, point, px, size};
    use std::time::Duration;

    #[test]
    fn submenu_clamps_to_the_trailing_edge_at_reference_width() {
        let layout = submenu_layout(1440.0, 280.0);
        assert!(!layout.open_left);
        assert!(layout.width > 260.0 && layout.width < 262.0);

        assert_eq!(
            submenu_layout(1440.0, 180.0),
            SubmenuLayout {
                open_left: false,
                width: 180.0,
            }
        );
    }

    #[test]
    fn submenu_flips_before_it_can_overflow_a_small_window() {
        assert_eq!(
            submenu_layout(900.0, 280.0),
            SubmenuLayout {
                open_left: true,
                width: 280.0,
            }
        );
    }

    #[test]
    fn slider_positions_match_the_cdp_observed_model_and_effort_labels() {
        let mut composer = ComposerView::new(ThemeMode::Dark);
        let expected = [
            ("5.6 Terra", "轻度"),
            ("5.6 Sol", "轻度"),
            ("5.6 Sol", "中"),
            ("5.6 Sol", "高"),
            ("5.6 Sol", "极高"),
            ("5.6 Sol", "Ultra"),
        ];

        for (index, (model, effort)) in expected.into_iter().enumerate() {
            composer.set_slider_index(index);
            assert_eq!(composer.selected_model, model);
            assert_eq!(composer.selected_effort, effort);
        }
    }

    #[test]
    fn permission_modes_update_the_label_and_outside_close_dismisses_the_menu() {
        let mut app = TestApp::new();
        let composer = app.new_entity(|_| ComposerView::new(ThemeMode::Dark));

        assert_eq!(
            app.read_entity(&composer, |c, _| c.permission_mode_name()),
            "full"
        );
        app.update_entity(&composer, |composer, cx| {
            composer.set_permission_mode("assist", cx);
            composer.open_permission_menu(cx);
        });
        assert_eq!(
            app.read_entity(&composer, |c, _| c.permission_mode_name()),
            "assist"
        );
        assert!(app.read_entity(&composer, |c, _| c.permission_menu_open));

        app.update_entity(&composer, |composer, cx| composer.close_picker(cx));
        assert!(!app.read_entity(&composer, |c, _| c.permission_menu_open));
    }

    #[test]
    fn particle_drift_uses_the_reference_ease_and_has_a_seamless_loop() {
        assert_eq!(particle_transition_ease(0.0), 0.0);
        assert_eq!(particle_transition_ease(1.0), 1.0);
        assert!((particle_transition_ease(0.5) - 0.5).abs() < 0.002);

        for index in 0..14 {
            let start = max_particle_drift(0.0, index, 1_701);
            let end = max_particle_drift(1.0, index, 1_701);
            assert!((start.0 - end.0).abs() < 0.001);
            assert!((start.1 - end.1).abs() < 0.001);
        }
    }

    #[test]
    fn ultra_fast_uses_only_the_fast_particle_layer_observed_over_cdp() {
        assert_eq!(particle_layers(5, false), (true, false));
        assert_eq!(particle_layers(5, true), (false, true));
        assert_eq!(particle_layers(4, true), (false, true));
    }

    #[test]
    fn dictation_can_start_and_cancel_without_leaving_transcribed_content() {
        let mut app = TestApp::new();
        let composer = app.new_entity(|_| ComposerView::new(ThemeMode::Dark));

        app.update_entity(&composer, |composer, cx| composer.start_dictation(cx));
        assert_eq!(
            app.read_entity(&composer, |composer, _| composer.dictation_state_name()),
            "recording"
        );

        app.update_entity(&composer, |composer, cx| composer.cancel_dictation(cx));
        assert_eq!(
            app.read_entity(&composer, |composer, _| composer.dictation_state_name()),
            "idle"
        );
    }

    #[test]
    fn stopping_dictation_shows_processing_then_returns_to_idle() {
        let mut app = TestApp::new();
        let composer = app.new_entity(|_| ComposerView::new(ThemeMode::Dark));

        app.update_entity(&composer, |composer, cx| composer.start_dictation(cx));
        app.update_entity(&composer, |composer, cx| composer.stop_dictation(cx));
        assert_eq!(
            app.read_entity(&composer, |composer, _| composer.dictation_state_name()),
            "transcribing"
        );

        app.advance_clock(Duration::from_millis(1_050));
        app.run_until_parked();
        assert_eq!(
            app.read_entity(&composer, |composer, _| composer.dictation_state_name()),
            "idle"
        );
    }

    #[test]
    fn rendered_microphone_cancel_and_stop_hit_targets_drive_the_state_machine() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(786.0), px(138.0)),
                })),
                ..Default::default()
            },
            |_, _| ComposerView::new(ThemeMode::Dark),
        );

        window.draw();
        window.simulate_mouse_move(point(px(716.0), px(110.0)));
        window.simulate_mouse_down(point(px(716.0), px(110.0)), MouseButton::Left);
        window.simulate_mouse_up(point(px(716.0), px(110.0)), MouseButton::Left);
        assert_eq!(
            window.read(|composer, _| composer.dictation_state_name()),
            "recording"
        );

        window.draw();
        window.simulate_click(point(px(22.0), px(110.0)), MouseButton::Left);
        assert_eq!(
            window.read(|composer, _| composer.dictation_state_name()),
            "idle"
        );

        window.draw();
        window.simulate_click(point(px(716.0), px(110.0)), MouseButton::Left);
        window.draw();
        window.simulate_click(point(px(716.0), px(110.0)), MouseButton::Left);
        assert_eq!(
            window.read(|composer, _| composer.dictation_state_name()),
            "transcribing"
        );
    }

    #[test]
    fn permission_trigger_opens_on_mouse_down_like_the_radix_reference() {
        let mut app = TestApp::new();
        let mut window = app.open_window_with_options(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.0), px(0.0)),
                    size: size(px(786.0), px(138.0)),
                })),
                ..Default::default()
            },
            |_, _| ComposerView::new(ThemeMode::Dark),
        );

        window.draw();
        window.simulate_mouse_move(point(px(83.0), px(111.0)));
        window.simulate_mouse_down(point(px(83.0), px(111.0)), MouseButton::Left);
        assert!(window.read(|composer, _| composer.permission_menu_open));
        window.simulate_mouse_up(point(px(83.0), px(111.0)), MouseButton::Left);
        assert!(window.read(|composer, _| composer.permission_menu_open));
    }
}
