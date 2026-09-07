//! Picker behavior and presentation for the prompt composer.

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, BoxShadow, Context, Div, KeyDownEvent, MouseButton, Transformation,
    Window, deferred, div, hsla, linear_color_stop, linear_gradient, prelude::*, px, radians, rgba,
};

use super::{
    ComposerView, MODEL_PICKER_DETAIL_ROW_HEIGHT, MODEL_PICKER_RIGHT_INSET,
    MODEL_PICKER_ROW_HEIGHT, MODEL_PICKER_SUBMENU_BOTTOM_OFFSET, MODEL_PICKER_SUBMENU_GAP,
    MODEL_PICKER_SUBMENU_HEADER_HEIGHT, MODEL_PICKER_SUBMENU_MAX_HEIGHT,
    MODEL_PICKER_SUBMENU_VERTICAL_PADDING, MODEL_PICKER_WIDTH, PARTICLE_TIMELINE_MS, PickerSubmenu,
    layout::{max_particle_drift, particle_layers, submenu_layout},
};
use crate::{
    agent::{AgentModel, AgentModelCatalog},
    components::icons::icon,
    theme::Theme,
};

impl ComposerView {
    pub(super) fn apply_model_catalog(&mut self, catalog: AgentModelCatalog) {
        self.conversation.apply_model_catalog(catalog)
    }
    pub(super) fn apply_model_selection(
        &mut self,
        index: usize,
        preferred_effort: Option<String>,
        preferred_service_tier: Option<String>,
        preserve_standard_tier: bool,
    ) {
        self.conversation.apply_model_selection(
            index,
            preferred_effort,
            preferred_service_tier,
            preserve_standard_tier,
        )
    }
    pub(super) fn select_model_at(&mut self, index: usize) {
        self.apply_model_selection(index, None, None, false);
    }
    pub(super) fn select_effort_at(&mut self, index: usize) {
        let Some(effort) = self
            .selected_model_entry()
            .and_then(|model| model.supported_reasoning_efforts.get(index))
            .map(|effort| effort.id.clone())
        else {
            return;
        };
        self.conversation.selected_effort = effort;
        self.conversation.slider_index = index;
        self.conversation.actual_model = None;
        self.conversation.model_status = None;
        self.conversation.safety_buffering = false;
    }
    pub(super) fn select_service_tier_at(&mut self, index: usize) {
        self.conversation.selected_service_tier = if index == 0 {
            None
        } else {
            self.selected_model_entry()
                .and_then(|model| model.service_tiers.get(index - 1))
                .map(|tier| tier.id.clone())
        };
        self.conversation.actual_model = None;
        self.conversation.model_status = None;
        self.conversation.safety_buffering = false;
    }
    pub(super) fn selected_model_entry(&self) -> Option<&AgentModel> {
        self.conversation.selected_model_entry()
    }
    pub(super) fn model_display_name<'a>(&'a self, model_name: &'a str) -> &'a str {
        self.conversation.model_display_name(model_name)
    }
    pub(super) fn selected_model_label(&self) -> String {
        if self.conversation.selected_model.is_empty() {
            "模型不可用".to_owned()
        } else {
            self.model_display_name(&self.conversation.selected_model)
                .to_owned()
        }
    }
    pub(super) fn effective_model_label(&self) -> String {
        self.conversation
            .actual_model
            .as_deref()
            .map(|model| self.model_display_name(model).to_owned())
            .unwrap_or_else(|| self.selected_model_label())
    }
    pub(super) fn effort_label(effort: &str) -> &str {
        match effort {
            "none" => "无",
            "minimal" => "最小",
            "low" => "轻度",
            "medium" => "中",
            "high" => "高",
            "xhigh" => "极高",
            "max" => "最高",
            "ultra" => "Ultra",
            other => other,
        }
    }
    pub(super) fn effort_detail(effort: &str) -> Option<&'static str> {
        (effort == "ultra").then_some("更快消耗使用额度")
    }
    pub(super) fn selected_effort_label(&self) -> String {
        if self.conversation.selected_effort.is_empty() {
            "—".to_owned()
        } else {
            Self::effort_label(&self.conversation.selected_effort).to_owned()
        }
    }
    pub(super) fn selected_service_tier_label(&self) -> String {
        let Some(selected) = self.conversation.selected_service_tier.as_deref() else {
            return "标准".to_owned();
        };
        self.selected_model_entry()
            .and_then(|model| model.service_tiers.iter().find(|tier| tier.id == selected))
            .map(|tier| tier.name.clone())
            .unwrap_or_else(|| selected.to_owned())
    }
    pub(super) fn default_model_index(&self) -> Option<usize> {
        self.conversation
            .models
            .iter()
            .position(|model| model.is_default)
            .or((!self.conversation.models.is_empty()).then_some(0))
    }
    pub(super) fn selection_is_default(&self) -> bool {
        let Some(model) = self
            .default_model_index()
            .and_then(|index| self.conversation.models.get(index))
        else {
            return true;
        };
        let default_effort = model
            .supported_reasoning_efforts
            .iter()
            .find(|option| option.id == model.default_reasoning_effort)
            .or_else(|| model.supported_reasoning_efforts.first())
            .map(|option| option.id.as_str())
            .unwrap_or(model.default_reasoning_effort.as_str());
        let default_service_tier = model
            .default_service_tier
            .as_deref()
            .filter(|tier| model.service_tiers.iter().any(|option| option.id == *tier));

        self.conversation.selected_model == model.model
            && self.conversation.selected_effort == default_effort
            && self.conversation.selected_service_tier.as_deref() == default_service_tier
    }
    pub(super) fn reset_model_selection(&mut self) {
        if let Some(index) = self.default_model_index() {
            self.apply_model_selection(index, None, None, false);
        }
        self.advanced_expanded = false;
        self.submenu = None;
        self.submenu_keyboard_focus = false;
        self.slider_dragging = false;
    }
    pub(super) fn submenu_option_count(&self, submenu: PickerSubmenu) -> usize {
        match submenu {
            PickerSubmenu::Model => self.conversation.models.len(),
            PickerSubmenu::Effort => self
                .selected_model_entry()
                .map(|model| model.supported_reasoning_efforts.len())
                .unwrap_or(0),
            PickerSubmenu::ServiceTier => self
                .selected_model_entry()
                .map(|model| model.service_tiers.len() + 1)
                .unwrap_or(0),
        }
    }
    pub(super) fn toggle_accelerated_service_tier(&mut self) {
        if self.conversation.selected_service_tier.is_some() {
            self.conversation.selected_service_tier = None;
        } else {
            self.conversation.selected_service_tier = self
                .selected_model_entry()
                .and_then(|model| model.service_tiers.first())
                .map(|tier| tier.id.clone());
        }
        self.conversation.actual_model = None;
        self.conversation.model_status = None;
        self.conversation.safety_buffering = false;
    }
    pub(super) fn handle_model_menu_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.menu_open {
            return;
        }
        let key = event.keystroke.key.as_str();
        if let Some(submenu) = self.submenu {
            let count = self.submenu_option_count(submenu);
            if count == 0 {
                if matches!(key, "left" | "escape") {
                    self.submenu = None;
                    self.submenu_keyboard_focus = false;
                    cx.stop_propagation();
                    cx.notify();
                }
                return;
            }
            match key {
                "down" => {
                    self.submenu_focused_item = if self.submenu_keyboard_focus {
                        (self.submenu_focused_item + 1) % count
                    } else {
                        0
                    };
                    self.submenu_keyboard_focus = true;
                }
                "up" => {
                    self.submenu_focused_item = if self.submenu_keyboard_focus {
                        (self.submenu_focused_item + count - 1) % count
                    } else {
                        count - 1
                    };
                    self.submenu_keyboard_focus = true;
                }
                "home" => {
                    self.submenu_focused_item = 0;
                    self.submenu_keyboard_focus = true;
                }
                "end" => {
                    self.submenu_focused_item = count - 1;
                    self.submenu_keyboard_focus = true;
                }
                "left" | "escape" => {
                    self.submenu = None;
                    self.submenu_keyboard_focus = false;
                }
                "enter" | "space" => {
                    if !self.submenu_keyboard_focus {
                        return;
                    }
                    match submenu {
                        PickerSubmenu::Model => self.select_model_at(self.submenu_focused_item),
                        PickerSubmenu::Effort => self.select_effort_at(self.submenu_focused_item),
                        PickerSubmenu::ServiceTier => {
                            self.select_service_tier_at(self.submenu_focused_item)
                        }
                    }
                    self.menu_open = false;
                    self.submenu = None;
                }
                "tab" => return,
                _ => return,
            }
        } else {
            match key {
                "down" => {
                    self.model_menu_focused_item = if self.model_menu_keyboard_focus {
                        (self.model_menu_focused_item + 1) % 4
                    } else {
                        0
                    };
                    self.model_menu_keyboard_focus = true;
                }
                "up" => {
                    self.model_menu_focused_item = if self.model_menu_keyboard_focus {
                        (self.model_menu_focused_item + 3) % 4
                    } else {
                        3
                    };
                    self.model_menu_keyboard_focus = true;
                }
                "home" => {
                    self.model_menu_focused_item = 0;
                    self.model_menu_keyboard_focus = true;
                }
                "end" => {
                    self.model_menu_focused_item = 3;
                    self.model_menu_keyboard_focus = true;
                }
                "right" | "enter" | "space" if self.model_menu_focused_item < 3 => {
                    self.submenu = Some(match self.model_menu_focused_item {
                        0 => PickerSubmenu::Model,
                        1 => PickerSubmenu::Effort,
                        _ => PickerSubmenu::ServiceTier,
                    });
                    self.submenu_keyboard_focus = false;
                }
                "enter" | "space" => {
                    if self.selection_is_default() {
                        self.advanced_expanded = !self.advanced_expanded;
                    } else {
                        self.reset_model_selection();
                    }
                }
                "escape" | "tab" => {
                    self.menu_open = false;
                    self.submenu = None;
                }
                _ => return,
            }
        }
        cx.stop_propagation();
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
            self.permission_menu_keyboard_focus = false;
            cx.notify();
        }
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
            "speed" | "service-tier" => Some(PickerSubmenu::ServiceTier),
            _ => None,
        };
        cx.notify();
    }
    pub fn open_picker_slider_at(&mut self, index: usize, fast: bool, cx: &mut Context<Self>) {
        self.menu_open = true;
        self.advanced_expanded = false;
        self.submenu = None;
        self.conversation.selected_service_tier = if fast {
            self.selected_model_entry()
                .and_then(|model| model.service_tiers.first())
                .map(|tier| tier.id.clone())
        } else {
            None
        };
        self.set_slider_index(index);
        cx.notify();
    }
    pub(super) fn set_slider_index(&mut self, index: usize) {
        let effort_count = self
            .selected_model_entry()
            .map(|model| model.supported_reasoning_efforts.len())
            .unwrap_or(0);
        if effort_count == 0 {
            self.conversation.slider_index = 0;
            return;
        }
        self.select_effort_at(index.min(effort_count - 1));
    }
    pub(super) fn picker_row(
        &self,
        index: usize,
        field: ModelPickerField<'_>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let ModelPickerField {
            id,
            label,
            value,
            submenu,
        } = field;
        let selected = self.submenu == Some(submenu);
        let focused = self.model_menu_keyboard_focus && self.model_menu_focused_item == index;
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
            .when(selected || focused, |row| row.bg(theme.sidebar_hover))
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
            .child(div().flex_1().child(label.to_owned()))
            .child(
                div()
                    .text_color(theme.text_tertiary)
                    .child(value.to_owned()),
            )
            .child(
                icon("chevron-down", theme.text_tertiary.into())
                    .size(px(16.0))
                    .ml(px(12.0))
                    .with_transformation(Transformation::rotate(radians(
                        -std::f32::consts::FRAC_PI_2,
                    ))),
            )
    }
    pub(super) fn option_row(&self, option: PickerOption<'_>, theme: Theme) -> gpui::Stateful<Div> {
        let PickerOption {
            id,
            title,
            detail,
            truncate_detail,
            selected,
            focused,
        } = option;
        div()
            .id(id)
            .min_h(px(if detail.is_some() {
                MODEL_PICKER_DETAIL_ROW_HEIGHT
            } else {
                MODEL_PICKER_ROW_HEIGHT
            }))
            .px(px(8.0))
            .py(px(5.0))
            .rounded(px(12.5))
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .when(focused, |row| row.bg(theme.sidebar_hover))
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
                    .child(title.to_owned())
                    .when_some(detail, |column, detail| {
                        column.child(
                            div()
                                .min_w(px(0.0))
                                .w_full()
                                .text_size(px(12.0))
                                .line_height(px(18.5625))
                                .text_color(theme.text_tertiary)
                                .when(truncate_detail, |detail| detail.truncate())
                                .child(detail.to_owned()),
                        )
                    }),
            )
            .when(selected, |row| {
                row.child(
                    icon("check", theme.text.into())
                        .size(px(17.0))
                        .flex_none()
                        .opacity(0.75),
                )
            })
    }
    pub(super) fn submenu(
        &self,
        kind: PickerSubmenu,
        viewport_width: f32,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let width = match kind {
            PickerSubmenu::Model => 280.0,
            PickerSubmenu::Effort => 180.0,
            PickerSubmenu::ServiceTier => 233.0,
        };
        let estimated_height = match kind {
            PickerSubmenu::Model => {
                self.conversation.models.len().max(1) as f32 * MODEL_PICKER_ROW_HEIGHT
                    + MODEL_PICKER_SUBMENU_VERTICAL_PADDING
            }
            PickerSubmenu::Effort => {
                self.selected_model_entry()
                    .map(|model| {
                        model
                            .supported_reasoning_efforts
                            .iter()
                            .map(|option| {
                                if option.id == "ultra" {
                                    MODEL_PICKER_DETAIL_ROW_HEIGHT
                                } else {
                                    MODEL_PICKER_ROW_HEIGHT
                                }
                            })
                            .sum::<f32>()
                    })
                    .unwrap_or(MODEL_PICKER_ROW_HEIGHT)
                    + MODEL_PICKER_SUBMENU_HEADER_HEIGHT
                    + MODEL_PICKER_SUBMENU_VERTICAL_PADDING
            }
            PickerSubmenu::ServiceTier => {
                self.submenu_option_count(kind).max(1) as f32 * MODEL_PICKER_DETAIL_ROW_HEIGHT
                    + MODEL_PICKER_SUBMENU_HEADER_HEIGHT
                    + MODEL_PICKER_SUBMENU_VERTICAL_PADDING
            }
        }
        .min(MODEL_PICKER_SUBMENU_MAX_HEIGHT);
        let top = MODEL_PICKER_SUBMENU_BOTTOM_OFFSET - estimated_height;
        let layout = submenu_layout(viewport_width, width);
        let mut menu = div()
            .id("model-picker-submenu")
            .absolute()
            .top(px(top))
            .w(px(layout.width))
            .max_h(px(MODEL_PICKER_SUBMENU_MAX_HEIGHT))
            .overflow_y_scroll()
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
                if self.conversation.models.is_empty() {
                    let message = self
                        .conversation
                        .model_catalog_error
                        .clone()
                        .unwrap_or_else(|| "没有可用模型".to_owned());
                    menu = menu.child(self.option_row(
                        PickerOption {
                            id: ("model-option", 0),
                            title: &message,
                            detail: None,
                            truncate_detail: false,
                            selected: false,
                            focused: false,
                        },
                        theme,
                    ));
                }
                for (index, model) in self.conversation.models.iter().cloned().enumerate() {
                    let model_name = model.model.clone();
                    menu = menu.child(
                        self.option_row(
                            PickerOption {
                                id: ("model-option", index),
                                title: &model.display_name,
                                detail: None,
                                truncate_detail: false,
                                selected: self.conversation.selected_model == model_name,
                                focused: self.submenu_keyboard_focus
                                    && self.submenu_focused_item == index,
                            },
                            theme,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(index) = this.conversation.models.iter().position(|model| {
                                model.model == model_name || model.id == model_name
                            }) {
                                this.select_model_at(index);
                            }
                            this.menu_open = false;
                            this.submenu = None;
                            cx.notify();
                        })),
                    );
                }
            }
            PickerSubmenu::Effort => {
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
                let options = self
                    .selected_model_entry()
                    .map(|model| model.supported_reasoning_efforts.clone())
                    .unwrap_or_default();
                for (index, option) in options.into_iter().enumerate() {
                    let effort_id = option.id.clone();
                    let title = Self::effort_label(&effort_id).to_owned();
                    let detail = Self::effort_detail(&effort_id);
                    menu = menu.child(
                        self.option_row(
                            PickerOption {
                                id: ("effort-option", index),
                                title: &title,
                                detail,
                                truncate_detail: true,
                                selected: self.conversation.selected_effort == effort_id,
                                focused: self.submenu_keyboard_focus
                                    && self.submenu_focused_item == index,
                            },
                            theme,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(index) = this.selected_model_entry().and_then(|model| {
                                model
                                    .supported_reasoning_efforts
                                    .iter()
                                    .position(|option| option.id == effort_id)
                            }) {
                                this.select_effort_at(index);
                            }
                            this.menu_open = false;
                            this.submenu = None;
                            cx.notify();
                        })),
                    );
                }
            }
            PickerSubmenu::ServiceTier => {
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
                let service_tiers = self
                    .selected_model_entry()
                    .map(|model| model.service_tiers.clone())
                    .unwrap_or_default();
                menu = menu.child(
                    self.option_row(
                        PickerOption {
                            id: ("service-tier-option", 0),
                            title: "标准",
                            detail: Some("默认速度"),
                            truncate_detail: false,
                            selected: self.conversation.selected_service_tier.is_none(),
                            focused: self.submenu_keyboard_focus && self.submenu_focused_item == 0,
                        },
                        theme,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.select_service_tier_at(0);
                        this.menu_open = false;
                        this.submenu = None;
                        cx.notify();
                    })),
                );
                for (offset, tier) in service_tiers.into_iter().enumerate() {
                    let index = offset + 1;
                    let tier_id = tier.id.clone();
                    menu = menu.child(
                        self.option_row(
                            PickerOption {
                                id: ("service-tier-option", index),
                                title: &tier.name,
                                detail: Some(&tier.description),
                                truncate_detail: false,
                                selected: self.conversation.selected_service_tier.as_deref()
                                    == Some(tier_id.as_str()),
                                focused: self.submenu_keyboard_focus
                                    && self.submenu_focused_item == index,
                            },
                            theme,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(index) = this.selected_model_entry().and_then(|model| {
                                model
                                    .service_tiers
                                    .iter()
                                    .position(|tier| tier.id == tier_id)
                            }) {
                                this.select_service_tier_at(index + 1);
                            }
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
    pub(super) fn view_controls(
        &self,
        show_fast_toggle: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let focused = self.model_menu_keyboard_focus && self.model_menu_focused_item == 3;
        let mut controls = div().h(px(32.0)).flex().items_center();

        if self.selection_is_default() {
            controls = controls
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
                        .when(focused, |row| row.bg(theme.sidebar_hover))
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
        } else {
            controls = controls.child(
                div()
                    .id("model-picker-reset")
                    .h(px(28.0))
                    .when(show_fast_toggle, |row| row.flex_1())
                    .when(!show_fast_toggle, |row| row.w_full())
                    .px(px(8.0))
                    .rounded(px(12.5))
                    .flex()
                    .items_center()
                    .text_size(px(13.0))
                    .line_height(px(18.5625))
                    .text_color(theme.text_tertiary)
                    .cursor_pointer()
                    .when(focused, |row| row.bg(theme.sidebar_hover))
                    .hover(move |style| style.bg(theme.sidebar_hover))
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.reset_model_selection();
                        cx.notify();
                    }))
                    .child(div().flex_1().child("重置为默认设置"))
                    .child(icon("model-reset", theme.text_tertiary.into()).size(px(14.0))),
            );
        }

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
                        this.toggle_accelerated_service_tier();
                        cx.notify();
                    }))
                    .child(
                        icon(
                            "model-fast",
                            if self.conversation.selected_service_tier.is_some() {
                                if self.conversation.selected_effort == "ultra" {
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
    pub(super) fn power_slider(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        const TRACK_WIDTH: f32 = 200.0;
        const TRACK_INSET: f32 = 13.0;
        let effort_count = self
            .selected_model_entry()
            .map(|model| model.supported_reasoning_efforts.len())
            .unwrap_or(0)
            .max(1);
        let step = if effort_count > 1 {
            (TRACK_WIDTH - TRACK_INSET * 2.0) / (effort_count - 1) as f32
        } else {
            0.0
        };
        let step_center = |index: usize| {
            if effort_count > 1 {
                TRACK_INSET + step * index as f32
            } else {
                TRACK_WIDTH * 0.5
            }
        };
        let slider_index = self.conversation.slider_index.min(effort_count - 1);
        let thumb_center = step_center(slider_index);
        let ultra_mode = self.conversation.selected_effort == "ultra";
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

        if ultra_mode {
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

        let (show_max_particles, show_fast_particles) = particle_layers(
            ultra_mode,
            self.conversation.selected_service_tier.is_some(),
        );
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

        for index in 0..effort_count {
            let center = step_center(index);
            let selected = index <= slider_index;
            let hidden =
                ultra_mode || (self.conversation.selected_service_tier.is_some() && selected);
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
        for index in 0..effort_count {
            let center = step_center(index);
            let left = if index == 0 { 0.0 } else { center - step * 0.5 };
            let right = if index + 1 == effort_count {
                TRACK_WIDTH
            } else {
                center + step * 0.5
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
                        if *hovered
                            && this.slider_dragging
                            && this.conversation.slider_index != index
                        {
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
    pub(super) fn model_menu(
        &self,
        viewport_width: f32,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let selected_model_label = self.selected_model_label();
        let selected_effort_label = self.selected_effort_label();
        let selected_service_tier_label = self.selected_service_tier_label();
        let selection_is_default = self.selection_is_default();
        let mut menu = div()
            .id("model-picker-menu")
            .track_focus(&self.model_menu_focus)
            .on_key_down(cx.listener(Self::handle_model_menu_key))
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
                    0,
                    ModelPickerField {
                        id: "model-picker-model-row",
                        label: "模型",
                        value: &selected_model_label,
                        submenu: PickerSubmenu::Model,
                    },
                    theme,
                    cx,
                ))
                .child(self.picker_row(
                    1,
                    ModelPickerField {
                        id: "model-picker-effort-row",
                        label: "推理强度",
                        value: &selected_effort_label,
                        submenu: PickerSubmenu::Effort,
                    },
                    theme,
                    cx,
                ))
                .child(self.picker_row(
                    2,
                    ModelPickerField {
                        id: "model-picker-service-tier-row",
                        label: "速度",
                        value: &selected_service_tier_label,
                        submenu: PickerSubmenu::ServiceTier,
                    },
                    theme,
                    cx,
                ))
                .child(if selection_is_default {
                    div()
                        .h(px(8.0))
                        .px(px(8.0))
                        .py(px(3.5))
                        .child(div().h(px(1.0)).w_full().bg(theme.border))
                } else {
                    div().h(px(4.0))
                })
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
}

#[derive(Clone, Copy)]
pub(super) struct ModelPickerField<'a> {
    pub(super) id: &'static str,
    pub(super) label: &'a str,
    pub(super) value: &'a str,
    pub(super) submenu: PickerSubmenu,
}

#[derive(Clone, Copy)]
pub(super) struct PickerOption<'a> {
    pub(super) id: (&'static str, usize),
    pub(super) title: &'a str,
    pub(super) detail: Option<&'a str>,
    pub(super) truncate_detail: bool,
    pub(super) selected: bool,
    pub(super) focused: bool,
}
