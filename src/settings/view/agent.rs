//! Agent settings presentation.

use gpui::{Context, IntoElement, div, prelude::*, px};

use super::SettingsView;
use crate::{settings::PageSpec, theme::Theme};

impl SettingsView {
    pub(super) fn agent_row(
        &self,
        title: &'static str,
        subtitle: &'static str,
        right: gpui::AnyElement,
        last: bool,
        theme: Theme,
    ) -> gpui::AnyElement {
        let field_key = match title {
            "批准策略" => "approval_policy",
            "沙盒设置" => "sandbox_mode",
            "网页搜索" => "web_search",
            "输出详细程度" => "model_verbosity",
            "推理摘要" => "model_reasoning_summary",
            "批准方式" => "approvals_reviewer",
            "默认权限配置" => "default_permissions",
            "默认模型" => "model",
            "默认推理强度" => "model_reasoning_effort",
            "Plan 推理强度" => "plan_mode_reasoning_effort",
            "服务等级" => "service_tier",
            "个性默认值" => "personality",
            _ => "",
        };
        let field_error = if self.config_editor.edits.len() == 1
            && self.config_editor.edits.contains_key(field_key)
        {
            if let crate::configuration::ConfigOperation::Failed(error) =
                &self.config_editor.operation
            {
                Some(error.user_message())
            } else {
                None
            }
        } else {
            None
        };
        let label = div()
            .min_w(px(0.0))
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .line_height(px(18.5625))
                    .font_weight(gpui::FontWeight(500.0))
                    .text_color(theme.markdown_text)
                    .child(title),
            )
            .when(!subtitle.is_empty(), |column| {
                column.child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(theme.settings_description)
                        .child(subtitle),
                )
            });
        let label = label.when_some(field_error, |column, error| {
            column.child(
                div()
                    .id(format!("config-field-error-{field_key}"))
                    .role(gpui::Role::Alert)
                    .aria_label(error.clone())
                    .mt(px(2.))
                    .text_size(px(13.))
                    .line_height(px(18.5625))
                    .text_color(if self.mode == crate::theme::ThemeMode::Dark {
                        gpui::rgba(0xff6764ff)
                    } else {
                        gpui::rgba(0xd62f2aff)
                    })
                    .child(error),
            )
        });
        div()
            .min_h(px(60.5625))
            .py(px(12.))
            .flex_none()
            .px(px(16.0))
            .relative()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(24.0))
            .when(!last, |row| {
                row.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left(px(16.0))
                        .right(px(16.0))
                        .h(px(0.5))
                        .bg(if theme.surface == gpui::rgba(0x181818ff) {
                            gpui::rgba(0x353535ff)
                        } else {
                            gpui::rgba(0xe9e9e9ff)
                        }),
                )
            })
            .child(label)
            .child(right)
            .into_any_element()
    }
    pub(super) fn agent_card(&self, rows: Vec<gpui::AnyElement>, theme: Theme) -> gpui::AnyElement {
        let mut card = div()
            .w_full()
            .rounded(px(20.0))
            .border_1()
            .border_color(if theme.surface == gpui::rgba(0x181818ff) {
                gpui::rgba(0x353535ff)
            } else {
                gpui::rgba(0xe9e9e9ff)
            })
            .bg(if self.mode == crate::theme::ThemeMode::Dark {
                gpui::rgba(0x232323ff)
            } else {
                theme.settings_panel
            });
        for row in rows {
            card = card.child(row);
        }
        card.into_any_element()
    }
    pub(super) fn agent_content(
        &self,
        page: &'static PageSpec,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        use super::configuration::ConfigAction;
        use crate::configuration::ConfigOperation;
        let busy = self.config_editor.busy();
        let rows = [
            ("approval_policy", "批准策略", "选择 ChatGPT 何时请求批准"),
            (
                "sandbox_mode",
                "沙盒设置",
                "选择 ChatGPT 运行命令时的权限范围",
            ),
            ("web_search", "网页搜索", "选择 ChatGPT 访问网络的方式"),
            (
                "model_verbosity",
                "输出详细程度",
                "选择 ChatGPT 回复包含细节的详细程度",
            ),
            (
                "model_reasoning_summary",
                "推理摘要",
                "选择 ChatGPT 总结其推理的方式",
            ),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (key, title, description))| {
            self.agent_row(
                title,
                description,
                self.config_control(key, theme, cx),
                index == 4,
                theme,
            )
        })
        .collect();
        let mut content = div()
            .w_full()
            .max_w(px(768.))
            .mx_auto()
            .pt(px(66.))
            .pb(px(80.))
            .child(
                div()
                    .text_size(px(24.))
                    .line_height(px(28.8))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .child(page.label),
            )
            .child(
                div()
                    .mt(px(6.))
                    .text_size(px(14.))
                    .line_height(px(21.))
                    .text_color(theme.settings_description)
                    .child("配置新聊天的权限、网页访问和智能体回复"),
            )
            .child(
                div()
                    .mt(px(41.5))
                    .text_size(px(14.))
                    .line_height(px(21.))
                    .font_weight(gpui::FontWeight(500.))
                    .child("智能体默认设置"),
            )
            .child(
                div()
                    .mt(px(21.5))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(self.config_control("source", theme, cx))
                    .child(self.config_button(
                        "config-reload",
                        if self.config_editor.operation == ConfigOperation::Loading {
                            "读取中…"
                        } else {
                            "重新读取"
                        },
                        ConfigAction::Reload,
                        busy,
                        theme,
                        cx,
                    )),
            )
            .child(div().mt(px(12.)).child(self.agent_card(rows, theme)));
        if let ConfigOperation::Failed(error) | ConfigOperation::ReadFailed(error) =
            &self.config_editor.operation
            && (matches!(self.config_editor.operation, ConfigOperation::ReadFailed(_))
                || self.config_editor.edits.len() != 1)
        {
            let text = error.user_message();
            content = content.child(
                div()
                    .id("config-error")
                    .role(gpui::Role::Alert)
                    .aria_label(text.clone())
                    .mt(px(12.))
                    .text_size(px(13.))
                    .text_color(theme.warning)
                    .child(text),
            );
        }
        if let Some(feedback) = &self.config_editor.feedback {
            content = content.child(
                div()
                    .id("config-feedback")
                    .role(gpui::Role::Status)
                    .aria_label(feedback.clone())
                    .mt(px(12.))
                    .text_size(px(13.))
                    .line_height(px(20.))
                    .child(feedback.clone()),
            );
        }
        if !self.config_editor.edits.is_empty()
            || matches!(self.config_editor.operation, ConfigOperation::Saving)
        {
            content = content.child(
                div()
                    .mt(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.))
                            .text_color(theme.text_tertiary)
                            .child(format!("{} 项未保存的修改", self.config_editor.edits.len())),
                    )
                    .when(self.config_editor.needs_review, |row| {
                        row.child(self.config_button(
                            "config-review",
                            "确认已核对草稿",
                            ConfigAction::Review,
                            busy || self.config_editor.operation != ConfigOperation::Ready,
                            theme,
                            cx,
                        ))
                    })
                    .child(self.config_button(
                        "config-discard",
                        "放弃修改",
                        ConfigAction::Discard,
                        busy,
                        theme,
                        cx,
                    ))
                    .child(self.config_button(
                        "config-save",
                        if matches!(self.config_editor.operation, ConfigOperation::Saving) {
                            "保存中…"
                        } else {
                            "保存"
                        },
                        ConfigAction::Save,
                        busy || self.config_editor.needs_review
                            || matches!(
                                self.config_editor.operation,
                                ConfigOperation::ReadFailed(_)
                            ),
                        theme,
                        cx,
                    )),
            );
        }
        content = content.child(
            div()
                .mt(px(24.))
                .flex()
                .gap(px(8.))
                .child(self.config_button(
                    "config-advanced",
                    "权限与会话默认值",
                    ConfigAction::Advanced,
                    false,
                    theme,
                    cx,
                ))
                .child(self.config_button(
                    "config-sources",
                    "配置来源与受管限制",
                    ConfigAction::Sources,
                    false,
                    theme,
                    cx,
                )),
        );
        if self.config_advanced_open {
            let fields = [
                ("approvals_reviewer", "批准方式", "用户批准或服务端自动复核"),
                (
                    "default_permissions",
                    "默认权限配置",
                    "仅可使用服务器允许的配置；新聊天继承此设置",
                ),
                (
                    "model",
                    "默认模型",
                    "仅用于新会话；已有会话可在模型菜单中修改",
                ),
                (
                    "model_reasoning_effort",
                    "默认推理强度",
                    "使用模型目录提供的强度",
                ),
                (
                    "plan_mode_reasoning_effort",
                    "Plan 推理强度",
                    "仅用于新会话的 Plan 默认值",
                ),
                (
                    "service_tier",
                    "服务等级",
                    "仅用于新会话；合法值由模型目录提供",
                ),
                ("personality", "个性默认值", "仅用于新会话"),
            ];
            let rows = fields
                .into_iter()
                .enumerate()
                .map(|(index, (key, title, description))| {
                    self.agent_row(
                        title,
                        description,
                        self.config_control(key, theme, cx),
                        index == 6,
                        theme,
                    )
                })
                .collect();
            content = content.child(div().mt(px(12.)).child(self.agent_card(rows, theme)));
            if let Some(error) = &self.config_profiles_error {
                content = content.child(
                    div()
                        .mt(px(8.))
                        .text_size(px(13.))
                        .text_color(theme.warning)
                        .child(format!("权限配置列表不可用：{error}")),
                );
            }
        }
        if let Some(key) = &self.config_custom_key {
            content = content.child(
                div()
                    .mt(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().text_size(px(13.)).child(key.clone()))
                    .child(div().flex_1().child(self.config_input.clone()))
                    .child(self.config_button(
                        "config-apply-custom",
                        "应用到草稿",
                        ConfigAction::ApplyCustom,
                        busy,
                        theme,
                        cx,
                    )),
            );
        }
        if self.config_sources_open {
            content = content.child(
                div()
                    .mt(px(16.))
                    .child(self.config_button(
                        "config-copy",
                        "复制配置来源与诊断",
                        ConfigAction::Copy,
                        false,
                        theme,
                        cx,
                    ))
                    .child(
                        div()
                            .id("config-source-details")
                            .mt(px(12.))
                            .text_size(px(12.))
                            .line_height(px(18.))
                            .text_color(theme.text_tertiary)
                            .child(crate::components::markdown::render_selectable_plan(
                                &self.config_diagnostics(),
                                theme,
                                "config-source-details-text",
                            )),
                    ),
            );
        }
        content.child(div().mt(px(24.)).text_size(px(12.)).line_height(px(18.)).text_color(theme.settings_description)
            .child(format!("工作目录：{}",self.config_cwd.display())))
            .child(div().mt(px(6.)).text_size(px(12.)).line_height(px(18.)).text_color(theme.settings_description)
                .child("保存后会回读有效配置。配置默认值与当前线程权限分别管理；线程权限变更用于后续轮次。"))
            .into_any_element()
    }
}
