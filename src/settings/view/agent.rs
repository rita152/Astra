//! Agent settings presentation.

use gpui::{Context, IntoElement, div, prelude::*, px, svg};

use super::SettingsView;
use crate::{
    settings::PageSpec,
    theme::{Theme, ThemeMode},
};

impl SettingsView {
    pub(super) fn agent_select(
        &self,
        label: &'static str,
        width: f32,
        theme: Theme,
    ) -> gpui::AnyElement {
        div()
            .w(px(width))
            .h(px(28.0))
            .flex_none()
            .px(px(12.0))
            .rounded(px(12.5))
            .border_1()
            .border_color(theme.border)
            .bg(theme.settings_control)
            .flex()
            .items_center()
            .justify_between()
            .gap(px(6.0))
            .text_size(px(14.0))
            .line_height(px(18.0))
            .whitespace_nowrap()
            .child(label)
            .child(
                svg()
                    .path("icons/chevron-down.svg")
                    .size(px(16.0))
                    .text_color(theme.text_tertiary),
            )
            .into_any_element()
    }
    pub(super) fn agent_row(
        &self,
        title: &'static str,
        subtitle: &'static str,
        right: gpui::AnyElement,
        last: bool,
        theme: Theme,
    ) -> gpui::AnyElement {
        let (title_x, title_y, subtitle_x, subtitle_y) = match title {
            "批准策略" => (0.0, 1.0, 0.0, 0.0),
            "沙盒设置" => (0.0, 0.0, -1.0, 0.0),
            "网页搜索" => (0.0, 0.0, 0.0, -1.0),
            "输出详细程度" | "推理摘要" => (0.0, -1.0, -1.0, -1.0),
            "可用推理强度" => (0.0, 0.0, 0.0, -1.0),
            "模型选择器滑块中的 Ultra" => (0.0, -1.0, 0.0, -1.0),
            "Codex 依赖项" => (0.0, 0.0, -1.0, -1.0),
            _ => (0.0, 0.0, 0.0, 0.0),
        };
        let label = div()
            .min_w(px(0.0))
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .relative()
                    .left(px(title_x))
                    .top(px(title_y))
                    .text_size(px(13.0))
                    .line_height(px(18.5625))
                    .font_weight(gpui::FontWeight(500.0))
                    .text_color(theme.text)
                    .child(title),
            )
            .when(!subtitle.is_empty(), |column| {
                column.child(
                    div()
                        .relative()
                        .left(px(subtitle_x))
                        .top(px(subtitle_y))
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(theme.settings_description)
                        .child(subtitle),
                )
            });
        div()
            .h(px(60.5625))
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
                        .h(px(1.0))
                        .bg(if theme.surface == gpui::rgba(0x181818ff) {
                            gpui::rgba(0x313131ff)
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
            .overflow_hidden()
            .border_1()
            .border_color(if theme.surface == gpui::rgba(0x181818ff) {
                gpui::rgba(0x313131ff)
            } else {
                gpui::rgba(0xe9e9e9ff)
            })
            .bg(theme.settings_panel);
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
        let defaults = self.agent_card(
            vec![
                self.agent_row(
                    "批准策略",
                    "选择 ChatGPT 何时请求批准",
                    self.agent_select("按请求", 86.0, theme),
                    false,
                    theme,
                ),
                self.agent_row(
                    "沙盒设置",
                    "选择 ChatGPT 运行命令时的权限范围",
                    self.agent_select("完整访问权限", 128.0, theme),
                    false,
                    theme,
                ),
                self.agent_row(
                    "网页搜索",
                    "选择 ChatGPT 访问网络的方式",
                    self.agent_select("实时", 72.0, theme),
                    false,
                    theme,
                ),
                self.agent_row(
                    "输出详细程度",
                    "选择 ChatGPT 回复包含细节的详细程度",
                    self.agent_select("模型默认", 100.0, theme),
                    false,
                    theme,
                ),
                self.agent_row(
                    "推理摘要",
                    "选择 ChatGPT 总结其推理的方式",
                    self.agent_select("自动", 72.0, theme),
                    true,
                    theme,
                ),
            ],
            theme,
        );
        let model = self.agent_card(
            vec![
                self.agent_row(
                    "可用推理强度",
                    "选择在模型控件中显示哪些推理强度级别。可用性因模型而异",
                    self.agent_select("已选择 6 个", 116.34375, theme),
                    false,
                    theme,
                ),
                self.agent_row(
                    "模型选择器滑块中的 Ultra",
                    "将 Ultra 显示为滑块最高档选项",
                    self.reference_switch_control(true, (page.slug, 1, 1), theme, cx),
                    true,
                    theme,
                ),
            ],
            theme,
        );
        let dependencies = self.agent_card(
            vec![
                self.agent_row(
                    "Codex 依赖项",
                    "允许 ChatGPT 安装并提供随附的 Node.js 和 Python 工具",
                    self.reference_switch_control(true, (page.slug, 2, 0), theme, cx),
                    false,
                    theme,
                ),
                self.agent_row(
                    "诊断 Codex 工作空间中的问题",
                    "检查当前捆绑包并记录诊断日志",
                    self.reference_button(
                        "诊断",
                        64.0,
                        Some(("icons/search.svg", 16.0)),
                        false,
                        theme,
                    ),
                    false,
                    theme,
                ),
                self.agent_row(
                    "重置并安装工作空间",
                    "下载新的软件包并安装，然后重新加载工具",
                    self.reference_button(
                        "重新安装",
                        92.0,
                        Some(("icons/settings-import.svg", 20.0)),
                        true,
                        theme,
                    ),
                    true,
                    theme,
                ),
            ],
            theme,
        );
        let link_color = match self.mode {
            ThemeMode::Light => gpui::rgba(0x339cffff),
            ThemeMode::Dark => gpui::rgba(0x99ceffff),
        };

        div()
            .w_full()
            .max_w(px(768.0))
            .mx_auto()
            .pt(px(66.0))
            .pb(px(80.0))
            .child(
                div()
                    .text_size(px(24.0))
                    .line_height(px(28.8))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .child(page.label),
            )
            .child(
                div()
                    .mt(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .text_color(theme.settings_description)
                    .child("配置新聊天的权限、网页访问和智能体回复")
                    .child(div().text_color(link_color).child("了解更多")),
            )
            .child(
                div()
                    .mt(px(41.5))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child("智能体默认设置"),
            )
            .child(
                div()
                    .mt(px(21.5))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(self.agent_select("用户配置", 100.0, theme))
                    .child(
                        div()
                            .w(px(140.328_13))
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap(px(4.0))
                            .text_size(px(14.0))
                            .line_height(px(18.0))
                            .text_color(theme.text_tertiary)
                            .child("打开 config.toml")
                            .child(
                                svg()
                                    .path("icons/settings-external.svg")
                                    .size(px(16.0))
                                    .text_color(theme.text_tertiary),
                            ),
                    ),
            )
            .child(div().mt(px(12.0)).child(defaults))
            .child(
                div()
                    .mt(px(49.5))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child("模型功能"),
            )
            .child(div().mt(px(15.5)).child(model))
            .child(
                div()
                    .mt(px(49.5))
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .font_weight(gpui::FontWeight(500.0))
                    .child("工作空间依赖项"),
            )
            .child(div().mt(px(15.5)).child(dependencies))
            .child(
                div()
                    .mt(px(6.0))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(theme.settings_description)
                    .child("当前版本：")
                    .child("26.819.11345"),
            )
            .into_any_element()
    }
}
