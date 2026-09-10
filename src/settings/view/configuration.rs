//! Configuration I/O and interactions belong to the view; drafts live in configuration.rs.
use super::SettingsView;
use crate::{
    agent::{AgentConfigError, AgentConfigErrorKind, config_value},
    configuration::{ConfigEditor, ConfigOperation},
    theme::Theme,
};
use gpui::{
    ClipboardItem, Context, Div, Focusable, KeyDownEvent, Role, SharedString, Stateful, Window,
    deferred, div, prelude::*, px,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone)]
pub(super) enum ConfigAction {
    Reload,
    Save,
    Discard,
    Review,
    Sources,
    Advanced,
    Copy,
    Menu(String),
    Choose(String, Value),
    Target(PathBuf),
    Custom(String),
    ApplyCustom,
}

impl SettingsView {
    pub fn dismiss_transient(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(key) = self
            .config_menu
            .take()
            .or_else(|| self.config_custom_key.take())
            && let Some(focus) = self.config_field_focus.get(&key)
        {
            focus.focus(window, cx);
        }
        self.config_menu_focus_pending = false;
        cx.notify();
    }

    pub fn advance_focus(&mut self, backwards: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.config_menu.is_some() {
            self.dismiss_transient(window, cx);
        }
        if backwards {
            window.focus_prev(cx);
        } else {
            window.focus_next(cx);
        }
    }

    pub fn set_config_context(&mut self, cwd: PathBuf, cx: &mut Context<Self>) {
        if self.config_cwd != cwd {
            let previous = std::mem::replace(&mut self.config_cwd, cwd.clone());
            let editor = std::mem::take(&mut self.config_editor);
            self.config_drafts.insert(previous, editor);
            self.config_editor = self.config_drafts.remove(&cwd).unwrap_or_default();
            self.config_menu = None;
        }
        self.reload_config(cx);
    }

    fn reload_config(&mut self, cx: &mut Context<Self>) {
        if self.config_editor.busy() {
            return;
        }
        let cycle = self.config_editor.begin_read();
        let cwd = self.config_cwd.clone();
        let result = self.backend.read_config(cwd.clone());
        let profiles = self.backend.load_permission_profiles(cwd.clone());
        self.config_profiles_error = None;
        cx.spawn(async move |this, cx| {
            let result = result.recv().await.unwrap_or_else(|_| {
                Err(AgentConfigError {
                    kind: AgentConfigErrorKind::Connection,
                    message: "读取配置的连接已关闭".into(),
                    data: None,
                    outcome_unknown: false,
                })
            });
            let _ = this.update(cx, |this, cx| {
                let editor = if this.config_cwd == cwd {
                    &mut this.config_editor
                } else {
                    this.config_drafts.entry(cwd.clone()).or_default()
                };
                editor.accept_read(cycle, result);
                cx.notify();
            });
            let profiles = profiles
                .recv()
                .await
                .unwrap_or_else(|_| Err("权限列表连接已关闭".into()));
            let _ = this.update(cx, |this, cx| {
                if this.config_cwd != cwd || this.config_editor.cycle != cycle {
                    return;
                }
                match profiles {
                    Ok(profiles) => this.config_profiles = profiles,
                    Err(error) => {
                        this.config_profiles.clear();
                        this.config_profiles_error = Some(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        let models = self.backend.load_model_catalog();
        cx.spawn(async move |this, cx| {
            let models = models.recv().await;
            let _ = this.update(cx, |this, cx| {
                if let Ok(Ok(catalog)) = models {
                    this.config_models = catalog.models;
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn save_config(&mut self, cx: &mut Context<Self>) {
        let (cycle, write) = match self.config_editor.prepare_write(&self.config_choices) {
            Ok(write) => write,
            Err(error) => {
                self.config_editor.feedback = Some(error);
                cx.notify();
                return;
            }
        };
        let cwd = self.config_cwd.clone();
        let result = self.backend.write_config(write);
        cx.spawn(async move |this, cx| {
            let result = result.recv().await.unwrap_or_else(|_| {
                Err(AgentConfigError {
                    kind: AgentConfigErrorKind::Connection,
                    message: "保存连接已关闭；写入结果未知，请重新读取后核对".into(),
                    data: None,
                    outcome_unknown: true,
                })
            });
            let _ = this.update(cx, |this, cx| {
                let editor = if this.config_cwd == cwd {
                    &mut this.config_editor
                } else {
                    this.config_drafts.entry(cwd).or_default()
                };
                editor.accept_save(cycle, result);
                cx.emit(super::ConfigSaveFinished);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn config_action(&mut self, action: ConfigAction, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            ConfigAction::Reload => self.reload_config(cx),
            ConfigAction::Save => self.save_config(cx),
            ConfigAction::Discard => self.config_editor.discard(),
            ConfigAction::Review => self.config_editor.confirm_review(),
            ConfigAction::Sources => self.config_sources_open = !self.config_sources_open,
            ConfigAction::Advanced => self.config_advanced_open = !self.config_advanced_open,
            ConfigAction::Copy => {
                cx.write_to_clipboard(ClipboardItem::new_string(self.config_diagnostics()))
            }
            ConfigAction::Menu(key) => {
                if self.config_editor.busy() {
                    return;
                }
                if self.config_menu.as_ref() == Some(&key) {
                    self.config_menu = None;
                } else {
                    let selected = self.config_editor.value(&key).unwrap_or(&Value::Null);
                    self.config_menu_index=self.config_options(&key).iter().position(|(_,action,_)|matches!(action,ConfigAction::Choose(_,value) if value==selected)).unwrap_or(0);
                    self.config_menu_scroll
                        .scroll_to_item(self.config_menu_index);
                    self.config_menu = Some(key);
                    self.config_menu_focus_pending = true;
                }
            }
            ConfigAction::Choose(key, value) => {
                if let Err(error) = self.config_editor.edit(&key, value) {
                    self.config_editor.feedback = Some(error);
                }
                self.config_menu = None;
                if let Some(focus) = self.config_field_focus.get(&key) {
                    focus.focus(window, cx);
                }
            }
            ConfigAction::Target(path) => {
                if let Err(error) = self.config_editor.select_target(path) {
                    self.config_editor.feedback = Some(error);
                }
                self.config_menu = None;
                if let Some(focus) = self.config_field_focus.get("source") {
                    focus.focus(window, cx);
                }
            }
            ConfigAction::Custom(key) => {
                let text = self
                    .config_editor
                    .value(&key)
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                self.config_input
                    .update(cx, |input, cx| input.set_text_silently(text, cx));
                self.config_custom_key = Some(key);
                self.config_menu = None;
                self.config_input.focus_handle(cx).focus(window, cx);
            }
            ConfigAction::ApplyCustom => {
                if let Some(key) = self.config_custom_key.clone() {
                    let text = self.config_input.read(cx).text().trim().to_owned();
                    if text.is_empty() {
                        self.config_editor.feedback =
                            Some("请输入非空值，或在菜单中选择继承".into());
                    } else if let Err(error) = self.config_editor.edit(&key, json!(text)) {
                        self.config_editor.feedback = Some(error);
                    } else {
                        self.config_custom_key = None;
                    }
                }
            }
        }
        cx.notify();
    }

    pub(super) fn config_button(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        action: ConfigAction,
        disabled: bool,
        theme: Theme,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let label = label.into();
        let keyboard = action.clone();
        div()
            .id(id.into())
            .role(Role::Button)
            .aria_label(label.clone())
            .focusable()
            .tab_stop(!disabled)
            .min_h(px(28.))
            .max_w(px(360.))
            .min_w(px(0.))
            .whitespace_nowrap()
            .px(px(12.))
            .rounded(px(12.5))
            .border_1()
            .border_color(theme.border)
            .bg(if self.mode == crate::theme::ThemeMode::Dark {
                gpui::rgba(0x2a2a2aff)
            } else {
                theme.settings_control
            })
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.))
            .text_size(px(14.))
            .line_height(px(18.))
            .text_color(theme.markdown_text)
            .when(disabled, |button| button.opacity(0.45))
            .when(!disabled, |button| {
                button
                    .cursor_pointer()
                    .hover(move |s| s.bg(theme.sidebar_hover))
            })
            .focus_visible(move |s| s.border_color(theme.accent))
            .on_click(cx.listener(move |this, _, window, cx| {
                if !disabled {
                    this.config_action(action.clone(), window, cx);
                }
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if !disabled && matches!(event.keystroke.key.as_str(), "enter" | "space" | "down") {
                    cx.stop_propagation();
                    this.config_action(keyboard.clone(), window, cx);
                } else {
                    cx.propagate();
                }
            }))
            .child(div().min_w(px(0.)).truncate().child(label))
    }

    fn config_options(&self, key: &str) -> Vec<(String, ConfigAction, Option<String>)> {
        if key == "source" {
            return self
                .config_editor
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.layers.as_ref())
                .into_iter()
                .flatten()
                .filter_map(|layer| {
                    let path = layer.source.file_path()?;
                    Some((
                        layer.source.label(),
                        ConfigAction::Target(path),
                        layer.disabled_reason.clone().or_else(|| {
                            (!layer.source.writable())
                                .then(|| "本机服务端仅允许写入用户配置；此层只读".into())
                        }),
                    ))
                })
                .collect();
        }
        let mut values = vec![Value::Null];
        if let Some(schema) = self.config_choices.iter().find(|field| field.key == key) {
            values.extend(schema.values.clone());
        }
        match key {
            "default_permissions" => {
                values.extend(self.config_profiles.iter().map(|profile| json!(profile.id)))
            }
            "model" => values.extend(self.config_models.iter().map(|model| json!(model.model))),
            "model_reasoning_effort" | "plan_mode_reasoning_effort" | "service_tier" => {
                let model = self
                    .config_editor
                    .value("model")
                    .filter(|value| !value.is_null())
                    .or_else(|| {
                        self.config_editor
                            .snapshot
                            .as_ref()
                            .and_then(|snapshot| snapshot.effective.get("model"))
                    })
                    .and_then(Value::as_str);
                let models: Vec<_> = self
                    .config_models
                    .iter()
                    .filter(|entry| model.is_none_or(|model| entry.model == model))
                    .collect();
                if key == "service_tier" {
                    values.extend(
                        models.iter().flat_map(|model| {
                            model.service_tiers.iter().map(|tier| json!(tier.id))
                        }),
                    );
                } else {
                    values.extend(models.iter().flat_map(|model| {
                        model
                            .supported_reasoning_efforts
                            .iter()
                            .map(|effort| json!(effort.id))
                    }));
                }
            }
            _ => {}
        }
        if let Some(allowed) = self
            .config_editor
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.requirements.as_ref())
            .and_then(|requirements| requirements.allowed.get(key))
        {
            values.extend(allowed.iter().cloned());
        }
        if let Some(value) = self.config_editor.value(key) {
            values.push(value.clone());
        }
        if let Some(value) = self
            .config_editor
            .snapshot
            .as_ref()
            .and_then(|snapshot| config_value(&snapshot.effective, key))
        {
            values.push(value.clone());
        }
        let mut seen = Vec::new();
        values.retain(|value| {
            if seen.contains(value) {
                false
            } else {
                seen.push(value.clone());
                true
            }
        });
        let mut options: Vec<_> = values
            .into_iter()
            .map(|value| {
                let mut reason = self.config_editor.restriction(key, &value);
                if key == "default_permissions"
                    && let Some(id) = value.as_str()
                    && self
                        .config_profiles
                        .iter()
                        .find(|profile| profile.id == id)
                        .is_none_or(|profile| !profile.allowed)
                {
                    reason = Some("服务端未允许使用此权限配置".into());
                }
                (
                    config_label(key, &value),
                    ConfigAction::Choose(key.into(), value),
                    reason,
                )
            })
            .collect();
        if self
            .config_choices
            .iter()
            .any(|field| field.key == key && field.allows_custom_string)
        {
            options.push((
                "输入其他值…".into(),
                ConfigAction::Custom(key.into()),
                self.config_editor.restriction(key, &Value::Null),
            ));
        }
        options
    }

    fn config_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.config_menu.clone() else {
            return;
        };
        let options = self.config_options(&key);
        let count = options.len();
        if count == 0 {
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                self.config_menu = None;
                if let Some(focus) = self.config_field_focus.get(&key) {
                    focus.focus(window, cx);
                }
            }
            "down" => self.config_menu_index = (self.config_menu_index + 1) % count,
            "up" => self.config_menu_index = (self.config_menu_index + count - 1) % count,
            "home" => self.config_menu_index = 0,
            "end" => self.config_menu_index = count - 1,
            "tab" => {
                self.config_menu = None;
                if let Some(focus) = self.config_field_focus.get(&key) {
                    focus.focus(window, cx);
                }
                if event.keystroke.modifiers.shift {
                    window.focus_prev(cx);
                } else {
                    window.focus_next(cx);
                }
                cx.stop_propagation();
                cx.notify();
                return;
            }
            "enter" | "space" => {
                if let Some((_, action, None)) = options.get(self.config_menu_index) {
                    self.config_action(action.clone(), window, cx);
                }
            }
            _ => {
                cx.propagate();
                return;
            }
        }
        self.config_menu_scroll
            .scroll_to_item(self.config_menu_index);
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn config_control(
        &self,
        key: &str,
        theme: Theme,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let busy = self.config_editor.busy() || self.config_editor.snapshot.is_none();
        let label = match &self.config_editor.operation {
            ConfigOperation::Loading if self.config_editor.snapshot.is_none() => "读取中…".into(),
            ConfigOperation::Failed(_) | ConfigOperation::ReadFailed(_)
                if self.config_editor.snapshot.is_none() =>
            {
                "读取失败".into()
            }
            ConfigOperation::Unavailable => "不可用".into(),
            _ if key == "source" => self
                .config_editor
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.layer(self.config_editor.target.as_ref()?))
                .map(|layer| {
                    if layer.source.kind() == "project" {
                        "项目配置"
                    } else {
                        "用户配置"
                    }
                })
                .unwrap_or("无可写配置")
                .into(),
            _ if self
                .config_editor
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.requirements.as_ref())
                .is_some_and(|requirements| requirements.enforced.contains_key(key)) =>
            {
                let value = &self
                    .config_editor
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .requirements
                    .as_ref()
                    .unwrap()
                    .enforced[key];
                format!("{} · 受管", config_label(key, value))
            }
            _ => {
                let value = self.config_editor.value(key).unwrap_or(&Value::Null);
                let mut label = config_label(key, value);
                if value.is_null()
                    && let Some(effective) = self
                        .config_editor
                        .snapshot
                        .as_ref()
                        .and_then(|snapshot| config_value(&snapshot.effective, key))
                        .filter(|value| !value.is_null())
                {
                    label = format!("继承 · {}", config_label(key, effective));
                }
                if self.config_editor.edits.contains_key(key) {
                    label.push_str(" ·");
                } else if !value.is_null()
                    && let Some(snapshot) = &self.config_editor.snapshot
                    && config_value(&snapshot.effective, key) != Some(value)
                {
                    label.push_str(" · 被覆盖");
                }
                label
            }
        };
        let accessible_label = format!("{}：{}", config_field_name(key), label);
        let key_string = key.to_owned();
        let disabled = busy
            || (key != "source" && self.config_editor.target.is_none())
            || (key != "source"
                && self
                    .config_options(key)
                    .iter()
                    .all(|(_, _, reason)| reason.is_some()));
        let bounds_state = self.config_control_bounds.clone();
        let bounds_key = key.to_owned();
        let outside_key = key.to_owned();
        let mut control = div().relative().flex_none().child(
            self.config_button(
                format!("config-{key}"),
                label,
                ConfigAction::Menu(key_string),
                disabled,
                theme,
                cx,
            )
            .aria_label(accessible_label)
            .when_some(self.config_field_focus.get(key), |button, focus| {
                button.track_focus(focus)
            })
            .child(
                crate::components::icons::icon("chevron-down", theme.text_tertiary.into())
                    .size(px(16.)),
            ),
        );
        control = control.child(
            gpui::canvas(
                move |bounds, _, _| bounds,
                move |_, bounds, _, _| {
                    bounds_state.borrow_mut().insert(bounds_key.clone(), bounds);
                },
            )
            .absolute()
            .size_full(),
        );
        if self.config_menu.as_deref() == Some(key) {
            let options = self.config_options(key);
            let mut menu = div()
                .id("config-choice-menu")
                .role(Role::Menu)
                .track_focus(&self.config_menu_focus)
                .absolute()
                .right_0()
                .top(px(32.))
                .w(px(360.))
                .max_h(px(320.))
                .overflow_y_scroll()
                .track_scroll(&self.config_menu_scroll)
                .p(px(4.))
                .rounded(px(20.))
                .border_1()
                .border_color(theme.border)
                .bg(theme.model_picker_surface)
                .shadow(vec![
                    gpui::BoxShadow::new(px(0.), px(8.), gpui::hsla(0., 0., 0., 0.16))
                        .blur_radius(px(16.)),
                ])
                .on_key_down(cx.listener(Self::config_menu_key))
                .on_mouse_down_out(cx.listener(
                    move |this, event: &gpui::MouseDownEvent, _, cx| {
                        if this
                            .config_control_bounds
                            .borrow()
                            .get(&outside_key)
                            .is_some_and(|bounds| bounds.contains(&event.position))
                        {
                            return;
                        }
                        this.config_menu = None;
                        cx.notify();
                    },
                ));
            for (index, (label, action, reason)) in options.into_iter().enumerate() {
                let enabled = reason.is_none();
                let click_action = action.clone();
                let selected = matches!(&action,ConfigAction::Choose(_,value) if self.config_editor.value(key).unwrap_or(&Value::Null)==value);
                let detail = match &action {
                    ConfigAction::Choose(key, value) => config_choice_detail(key, value),
                    _ => None,
                };
                let row = div()
                    .id(("config-option", index))
                    .role(Role::MenuItem)
                    .aria_label(label.clone())
                    .when_some(reason.clone(), |row, reason| row.aria_description(reason))
                    .aria_selected(selected)
                    .when(index == self.config_menu_index, |row| {
                        row.aria_active_descendant()
                    })
                    .min_h(px(38.))
                    .px(px(10.))
                    .py(px(7.))
                    .rounded(px(12.))
                    .text_size(px(13.))
                    .line_height(px(18.))
                    .when(index == self.config_menu_index, |row| {
                        row.bg(theme.sidebar_hover)
                    })
                    .when(enabled, |row| {
                        row.cursor_pointer()
                            .hover(move |s| s.bg(theme.sidebar_hover))
                    })
                    .when(!enabled, |row| row.opacity(0.5))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        if enabled {
                            this.config_action(click_action.clone(), window, cx);
                        }
                    }))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .child(label)
                            .when(selected, |row| row.child("✓")),
                    )
                    .when_some(detail, |row, detail| {
                        row.child(
                            div()
                                .text_size(px(12.))
                                .line_height(px(16.))
                                .text_color(theme.text_tertiary)
                                .child(detail),
                        )
                    })
                    .when_some(reason, |row, reason| {
                        row.child(
                            div()
                                .text_size(px(11.))
                                .text_color(theme.text_tertiary)
                                .child(reason),
                        )
                    });
                menu = menu.child(row);
            }
            control = control.child(deferred(menu));
        }
        control.into_any_element()
    }

    pub(super) fn config_diagnostics(&self) -> String {
        let Some(snapshot) = &self.config_editor.snapshot else {
            return "尚未读取配置".into();
        };
        let mut lines = vec![format!("工作目录：{}", snapshot.cwd.display())];
        for layer in snapshot.layers.as_ref().into_iter().flatten() {
            lines.push(format!(
                "{}\n版本：{}{}",
                layer.source.label(),
                layer.source.version,
                layer
                    .disabled_reason
                    .as_ref()
                    .map(|reason| format!("\n不可用：{reason}"))
                    .unwrap_or_default()
            ));
        }
        for field in &self.config_choices {
            let value = config_value(&snapshot.effective, &field.key)
                .map(Value::to_string)
                .unwrap_or_else(|| "未设置".into());
            let source = snapshot
                .origin(&field.key)
                .map(|source| source.label())
                .unwrap_or_else(|| "未指定来源／服务端默认".into());
            lines.push(format!(
                "{} = {}\n来源：{}{}",
                field.key,
                value,
                source,
                if field.session_static {
                    "；会话静态默认值"
                } else {
                    ""
                }
            ));
        }
        if let Some(layer) = self
            .config_editor
            .target
            .as_ref()
            .and_then(|path| snapshot.layer(path))
        {
            for field in &self.config_choices {
                let stored = config_value(&layer.config, &field.key)
                    .map(Value::to_string)
                    .unwrap_or_else(|| "未设置（继承）".into());
                lines.push(format!(
                    "用户文件中的 {}：{}{}",
                    field.key,
                    stored,
                    self.config_editor
                        .edits
                        .get(&field.key)
                        .map(|value| format!("；草稿：{value}"))
                        .unwrap_or_default()
                ));
            }
        }
        if let Some(requirements) = &snapshot.requirements {
            lines.push(format!(
                "受管限制：\n{}",
                serde_json::to_string_pretty(&requirements.raw).unwrap_or_default()
            ));
        } else {
            lines.push("未配置受管限制".into());
        }
        if let Some(receipt) = &self.config_editor.receipt {
            lines.push(format!(
                "保存状态：{}\n写入版本：{}\n覆盖信息：{}",
                receipt.status,
                receipt.version,
                receipt
                    .overridden
                    .as_ref()
                    .map(Value::to_string)
                    .unwrap_or_else(|| "无".into())
            ));
        }
        if let ConfigOperation::Failed(error) | ConfigOperation::ReadFailed(error) =
            &self.config_editor.operation
        {
            lines.push(format!(
                "操作错误：{}\n原始错误载荷：{}",
                error.user_message(),
                error
                    .data
                    .as_ref()
                    .map(Value::to_string)
                    .unwrap_or_else(|| "无".into())
            ));
        }
        lines.join("\n\n")
    }
}

pub(super) fn config_label(key: &str, value: &Value) -> String {
    let Some(value) = value.as_str() else {
        return if value.is_null() {
            "继承 / 未设置".into()
        } else {
            format!("精细策略：{value}")
        };
    };
    match (key, value) {
        ("approval_policy", "on-request") => "按请求",
        ("approval_policy", "untrusted") => "不受信任",
        ("approval_policy", "never") => "从不",
        ("sandbox_mode", "read-only") => "只读",
        ("sandbox_mode", "workspace-write") => "工作区写入",
        ("sandbox_mode", "danger-full-access") => "完整访问权限",
        ("web_search", "disabled") => "已禁用",
        ("web_search", "cached") => "已缓存",
        ("web_search", "indexed") => "已索引",
        ("web_search", "live") => "实时",
        ("model_verbosity", "low") => "低",
        ("model_verbosity", "medium") => "中",
        ("model_verbosity", "high") => "高",
        ("model_reasoning_summary", "auto") => "自动",
        ("model_reasoning_summary", "concise") => "简洁",
        ("model_reasoning_summary", "detailed") => "详细",
        ("model_reasoning_summary", "none") => "无",
        ("approvals_reviewer", "user") => "用户批准",
        ("approvals_reviewer", "auto_review") => "自动复核",
        ("approvals_reviewer", "guardian_subagent") => "自动复核（兼容旧值）",
        _ => value,
    }
    .into()
}

pub(super) type ConfigDrafts = BTreeMap<PathBuf, ConfigEditor>;

fn config_choice_detail(key: &str, value: &Value) -> Option<&'static str> {
    if value.is_null() {
        return Some("移除此文件中的值，使用继承配置或服务端默认值");
    }
    match (key, value.as_str()?) {
        ("web_search", "disabled") => Some("不允许网页搜索"),
        ("web_search", "cached") => Some("使用 OpenAI 维护的搜索索引"),
        ("web_search", "indexed") => Some("允许访问已索引的外部网页"),
        ("web_search", "live") => Some("允许不受限制地访问当前网页"),
        _ => None,
    }
}

fn config_field_name(key: &str) -> &str {
    match key {
        "source" => "配置来源",
        "approval_policy" => "批准策略",
        "sandbox_mode" => "沙盒设置",
        "web_search" => "网页搜索",
        "model_verbosity" => "输出详细程度",
        "model_reasoning_summary" => "推理摘要",
        "approvals_reviewer" => "批准方式",
        "default_permissions" => "默认权限配置",
        "model" => "默认模型",
        "model_reasoning_effort" => "默认推理强度",
        "plan_mode_reasoning_effort" => "Plan 推理强度",
        "service_tier" => "服务等级",
        "personality" => "个性默认值",
        other => other,
    }
}
