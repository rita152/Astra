//! Hook input and the post-turn hook tooltip observed in the desktop renderer.

use super::{HomeView, OpenHookSettings};
use crate::{
    agent::{AgentHookRun, AgentScopedHookPrompt},
    components::{
        icons::icon,
        markdown::{
            MarkdownBlock, MarkdownDocument, MarkdownInline, parse_markdown,
            render_selectable_markdown_document,
        },
    },
    conversation::ConversationActivity,
    theme::Theme,
};
use gpui::{
    App, ClipboardItem, Context, Entity, FontWeight, IntoElement, Render, RenderOnce, Role,
    SharedString, Window, div, prelude::*, px,
};

#[derive(Clone, PartialEq, gpui::Action)]
#[action(no_json)]
pub(crate) struct ActivateHookControl;

pub(crate) fn init_keyboard(cx: &mut App) {
    for key in ["enter", "space"] {
        cx.bind_keys([gpui::KeyBinding::new(
            key,
            ActivateHookControl,
            Some("HookControl"),
        )]);
    }
}

pub(super) fn hook_runs(activities: &[ConversationActivity]) -> Vec<AgentHookRun> {
    activities
        .iter()
        .find_map(|a| match a {
            ConversationActivity::HookSummary(hooks) => Some(hooks.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn source_label(source: &str) -> &'static str {
    match source {
        "user" => "用户",
        "project" => "项目",
        "plugin" => "插件",
        "sessionFlags" => "会话",
        "system"
        | "mdm"
        | "cloudRequirements"
        | "cloudManagedConfig"
        | "legacyManagedConfigFile"
        | "legacyManagedConfigMdm" => "管理员",
        _ => "未知",
    }
}

fn event_label(event: &str) -> String {
    let mut chars = event.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

struct HookTooltip {
    hooks: Vec<AgentHookRun>,
    theme: Theme,
}

impl Render for HookTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .id("hook-statistics-tooltip")
            .role(Role::Tooltip)
            .max_w(px(512.))
            .px(px(12.))
            .py(px(8.))
            .rounded(px(20.))
            .border_1()
            .border_color(theme.markdown_text.alpha(0.082))
            .bg(theme.control)
            .font_family(".SystemUIFont")
            .font_weight(FontWeight(430.))
            .text_size(px(13.))
            .line_height(px(130. / 7.))
            .text_color(theme.markdown_text)
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(div().font_weight(FontWeight::MEDIUM).child("钩子"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .children(self.hooks.iter().map(|hook| {
                        div()
                            .flex()
                            .items_start()
                            .gap(px(16.))
                            .child(div().flex_none().child(event_label(&hook.event_name)))
                            .child(
                                div()
                                    .min_w(px(0.))
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .text_color(theme.markdown_text.alpha(0.65))
                                            .child(source_label(&hook.source)),
                                    )
                                    .when_some(
                                        hook.status_message
                                            .clone()
                                            .filter(|s| !s.trim().is_empty()),
                                        |v, s| {
                                            v.child(
                                                div()
                                                    .text_color(theme.markdown_text.alpha(0.65))
                                                    .child(s),
                                            )
                                        },
                                    )
                                    .children(
                                        hook.entries.iter().filter(|e| e.kind != "context").map(
                                            |entry| {
                                                div()
                                                    .text_color(if entry.kind == "warning" {
                                                        theme.markdown_text.alpha(0.65)
                                                    } else {
                                                        theme.warning
                                                    })
                                                    .child(entry.text.clone())
                                            },
                                        ),
                                    ),
                            )
                    })),
            )
    }
}

pub(super) fn hook_button(
    scope: &str,
    hooks: Vec<AgentHookRun>,
    theme: Theme,
    home: Entity<HomeView>,
    cx: &mut App,
) -> impl IntoElement {
    let id = hooks
        .first()
        .map(|hook| format!("hook-statistics-{:?}-{:?}", hook.thread_id, hook.turn_id))
        .unwrap_or_else(|| format!("hook-statistics-{scope}"));
    let keyboard_open = home.read(cx).expanded_commands.contains(&id);
    let keyboard_focus = home.read(cx).hook_tooltip_focus.clone();
    let tooltip_hooks = hooks.clone();
    let description = hooks
        .iter()
        .map(|hook| {
            let mut lines = vec![
                event_label(&hook.event_name),
                source_label(&hook.source).to_owned(),
            ];
            lines.extend(hook.status_message.clone());
            lines.extend(
                hook.entries
                    .iter()
                    .filter(|entry| entry.kind != "context")
                    .map(|entry| entry.text.clone()),
            );
            lines.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let tooltip_home = home.clone();
    let tooltip_id = id.clone();
    let hover_home = home.clone();
    let hover_id = id.clone();
    let action_home = home.clone();
    let action_id = id.clone();
    let focus = hook_control_focus(&home, &id, cx);
    let click_focus = focus.clone();
    div()
        .id(SharedString::from(id.clone()))
        .role(Role::Button)
        .aria_label("钩子")
        .aria_description(description)
        .track_focus(&focus)
        .tab_stop(true)
        .key_context("HookControl")
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            focus.focus(window, cx)
        })
        .on_click(move |_, window, cx| click_focus.focus(window, cx))
        .on_action(move |_: &ActivateHookControl, window, cx| {
            toggle_hook_tooltip(&action_home, &action_id, window, cx);
            cx.stop_propagation();
        })
        .size(px(26.))
        .rounded(px(10.))
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .focus_visible(|v| v.bg(theme.sidebar_hover).text_color(theme.text))
        .on_hover(move |hovered, _, cx| {
            if *hovered {
                hover_home.update(cx, |home, cx| {
                    home.hook_hover_started
                        .insert(hover_id.clone(), std::time::Instant::now());
                    cx.spawn(async move |home, cx| {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(700))
                            .await;
                        let _ = home.update(cx, |home, cx| {
                            home.conversation_cache_dirty = true;
                            cx.notify();
                        });
                    })
                    .detach();
                });
            } else {
                hover_home.update(cx, |home, cx| {
                    home.dismissed_hook_tooltips.remove(&hover_id);
                    home.hook_hover_started.remove(&hover_id);
                    cx.notify();
                });
            }
        })
        .on_key_down(
            move |event, window, cx| match event.keystroke.key.as_str() {
                "enter" | "space" => {
                    toggle_hook_tooltip(&home, &id, window, cx);
                    cx.stop_propagation();
                }
                "escape" => {
                    home.update(cx, |home, cx| {
                        home.expanded_commands.remove(&id);
                        home.dismissed_hook_tooltips.insert(id.clone());
                        home.conversation_cache_dirty = true;
                        cx.notify();
                    });
                    cx.stop_propagation();
                }
                "tab" => {
                    home.update(cx, |home, cx| {
                        home.expanded_commands.remove(&id);
                        home.conversation_cache_dirty = true;
                        cx.notify();
                    });
                    if event.keystroke.modifiers.shift {
                        window.focus_prev(cx)
                    } else {
                        window.focus_next(cx)
                    };
                    cx.stop_propagation();
                }
                _ => {}
            },
        )
        .child(icon("hook", theme.text_tertiary.into()).size(px(18.)))
        .child(
            gpui::canvas(
                move |bounds, window, cx| {
                    if tooltip_home
                        .read(cx)
                        .dismissed_hook_tooltips
                        .contains(&tooltip_id)
                    {
                        return;
                    }
                    let keyboard_open = keyboard_open
                        && keyboard_focus
                            .as_ref()
                            .is_some_and(|focus| focus.is_focused(window));
                    let hover_ready = bounds.contains(&window.mouse_position())
                        && tooltip_home
                            .read(cx)
                            .hook_hover_started
                            .get(&tooltip_id)
                            .is_some_and(|time| {
                                time.elapsed() >= std::time::Duration::from_millis(700)
                            });
                    if !keyboard_open && !hover_ready {
                        return;
                    }
                    let lines = tooltip_hooks
                        .iter()
                        .map(|hook| {
                            1 + usize::from(hook.status_message.is_some())
                                + hook.entries.iter().filter(|e| e.kind != "context").count()
                        })
                        .sum::<usize>();
                    let height = 26. + (lines + 1) as f32 * (130. / 7.);
                    let x = (f32::from(bounds.center().x) - 160.).max(8.);
                    let y = (f32::from(bounds.top()) - height - 8.).max(8.);
                    window.set_tooltip(gpui::AnyTooltip {
                        view: cx
                            .new(|_| HookTooltip {
                                hooks: tooltip_hooks.clone(),
                                theme,
                            })
                            .into(),
                        mouse_position: gpui::point(px(x - 1.), px(y - 1.)),
                        check_visible_and_update: std::rc::Rc::new({
                            let focus = keyboard_focus.clone();
                            let home = tooltip_home.clone();
                            let id = tooltip_id.clone();
                            move |_, window, cx| {
                                let home = home.read(cx);
                                !home.dismissed_hook_tooltips.contains(&id)
                                    && ((home.expanded_commands.contains(&id)
                                        && focus.as_ref().is_some_and(|f| f.is_focused(window)))
                                        || bounds.contains(&window.mouse_position()))
                            }
                        }),
                    });
                },
                |_, (), _, _| {},
            )
            .absolute()
            .inset_0(),
        )
}

#[derive(IntoElement)]
pub(super) struct HookPromptBubble {
    pub prompt: AgentScopedHookPrompt,
    pub home: Entity<HomeView>,
    pub theme: Theme,
}

impl RenderOnce for HookPromptBubble {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let text = self
            .prompt
            .prompt
            .fragments
            .iter()
            .map(|f| f.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let id = format!(
            "hook-prompt-{:?}-{:?}-{:?}",
            self.prompt.thread_id, self.prompt.turn_id, self.prompt.prompt.id
        );
        let expanded = self.home.read(cx).expanded_commands.contains(&id);
        let expand_focus = hook_control_focus(&self.home, &format!("{id}-expand"), cx);
        let copy_focus = hook_control_focus(&self.home, &format!("{id}-copy"), cx);
        let settings_focus = hook_control_focus(&self.home, &format!("{id}-settings"), cx);
        let expand_click_focus = expand_focus.clone();
        let copy_click_focus = copy_focus.clone();
        let settings_click_focus = settings_focus.clone();
        let long = text
            .lines()
            .map(|line| {
                (super::messages::user_message_text_width(line, window) / 483.2)
                    .ceil()
                    .max(1.) as usize
            })
            .sum::<usize>()
            >= 20;
        let visible = text.clone();
        let width = super::messages::user_message_text_width(&visible, window) + 33.;
        let document = hook_prompt_document(&visible);
        let theme = self.theme;
        let copy = text.clone();
        let keyboard_copy = copy.clone();
        let action_copy = copy.clone();
        let home = self.home.clone();
        let settings_home = self.home.clone();
        let keyboard_settings_home = self.home.clone();
        let action_settings_home = self.home.clone();
        let action_expand_home = self.home.clone();
        let action_expand_id = id.clone();
        div()
            .id(SharedString::from(id.clone()))
            .role(Role::Group)
            .aria_label("钩子反馈")
            .w_full()
            .flex()
            .flex_col()
            .items_end()
            .gap(px(4.))
            .group("hook-prompt")
            .when(!text.trim().is_empty(), |v| {
                v.child(
                    div()
                        .w(px(width.min(515.2)))
                        .max_w_full()
                        .min_w(px(0.))
                        .px(px(16.))
                        .py(px(10.))
                        .rounded(px(22.))
                        .bg(theme.user_message_surface)
                        .text_color(theme.user_message_text)
                        .text_size(px(14.))
                        .line_height(px(22.75))
                        .child(
                            div()
                                .when(long && !expanded, |v| v.max_h(px(399.)).overflow_hidden())
                                .child(render_selectable_markdown_document(&document, theme, &id)),
                        )
                        .when(long && !expanded, |v| {
                            v.child(div().line_height(px(21.)).child("…"))
                        })
                        .when(long, |v| {
                            v.child(
                                div()
                                    .id(SharedString::from(format!("{id}-expand")))
                                    .track_focus(&expand_focus)
                                    .tab_stop(true)
                                    .key_context("HookControl")
                                    .on_action(move |_: &ActivateHookControl, _, cx| {
                                        toggle_prompt(&action_expand_home, &action_expand_id, cx);
                                        cx.stop_propagation();
                                    })
                                    .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                                        expand_focus.focus(window, cx)
                                    })
                                    .role(Role::Button)
                                    .aria_expanded(expanded)
                                    .aria_label(if expanded {
                                        "显示更少"
                                    } else {
                                        "显示更多"
                                    })
                                    .mt(px(10.))
                                    .flex()
                                    .items_center()
                                    .gap(px(4.))
                                    .line_height(px(21.))
                                    .cursor_pointer()
                                    .focus_visible(|v| v.bg(theme.sidebar_hover))
                                    .text_color(theme.text_tertiary)
                                    .child(if expanded {
                                        "显示更少"
                                    } else {
                                        "显示更多"
                                    })
                                    .child(
                                        icon(
                                            if expanded {
                                                "settings-chevron-up"
                                            } else {
                                                "chevron-down"
                                            },
                                            theme.text_tertiary.into(),
                                        )
                                        .size(px(14.)),
                                    )
                                    .on_click({
                                        let id = id.clone();
                                        let home = home.clone();
                                        move |_, window, cx| {
                                            expand_click_focus.focus(window, cx);
                                            toggle_prompt(&home, &id, cx)
                                        }
                                    })
                                    .on_key_down({
                                        let id = id.clone();
                                        move |e, _, cx| {
                                            if matches!(e.keystroke.key.as_str(), "enter" | "space")
                                            {
                                                toggle_prompt(&home, &id, cx);
                                                cx.stop_propagation();
                                            }
                                        }
                                    }),
                            )
                        }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .mr(px(4.))
                        .child(
                            div()
                                .id(SharedString::from(format!("{id}-copy")))
                                .track_focus(&copy_focus)
                                .tab_stop(true)
                                .key_context("HookControl")
                                .on_action(move |_: &ActivateHookControl, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        action_copy.clone(),
                                    ));
                                    cx.stop_propagation();
                                })
                                .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                                    copy_focus.focus(window, cx)
                                })
                                .role(Role::Button)
                                .aria_label("复制钩子反馈")
                                .size(px(26.))
                                .rounded(px(10.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .opacity(0.)
                                .group_hover("hook-prompt", |v| v.opacity(1.))
                                .focus_visible(|v| v.opacity(1.).bg(theme.sidebar_hover))
                                .on_click(move |_, window, cx| {
                                    copy_click_focus.focus(window, cx);
                                    cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))
                                })
                                .on_key_down(move |e, _, cx| {
                                    if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            keyboard_copy.clone(),
                                        ));
                                        cx.stop_propagation();
                                    }
                                })
                                .child(
                                    icon("message-copy", theme.text_tertiary.into()).size(px(18.)),
                                ),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("{id}-settings")))
                                .track_focus(&settings_focus)
                                .tab_stop(true)
                                .key_context("HookControl")
                                .on_action(move |_: &ActivateHookControl, _, cx| {
                                    action_settings_home
                                        .update(cx, |_, cx| cx.emit(OpenHookSettings));
                                    cx.stop_propagation();
                                })
                                .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                                    settings_focus.focus(window, cx)
                                })
                                .role(Role::Link)
                                .aria_label("钩子反馈，打开钩子设置")
                                .px(px(4.))
                                .py(px(2.))
                                .rounded(px(6.))
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .text_size(px(13.))
                                .text_color(theme.text_tertiary)
                                .cursor_pointer()
                                .hover(|v| v.text_color(theme.text))
                                .focus_visible(|v| v.bg(theme.sidebar_hover))
                                .on_click(move |_, window, cx| {
                                    settings_click_focus.focus(window, cx);
                                    settings_home.update(cx, |_, cx| cx.emit(OpenHookSettings))
                                })
                                .on_key_down(move |e, _, cx| {
                                    if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                                        keyboard_settings_home
                                            .update(cx, |_, cx| cx.emit(OpenHookSettings));
                                        cx.stop_propagation();
                                    }
                                })
                                .child(icon("hook", theme.text_tertiary.into()).size(px(14.)))
                                .child("钩子反馈"),
                        ),
                )
            })
    }
}

fn toggle_prompt(home: &Entity<HomeView>, id: &str, cx: &mut App) {
    home.update(cx, |home, cx| {
        if !home.expanded_commands.remove(id) {
            home.expanded_commands.insert(id.to_owned());
        }
        home.conversation_cache_dirty = true;
        cx.notify();
    });
}

fn toggle_hook_tooltip(home: &Entity<HomeView>, id: &str, window: &mut Window, cx: &mut App) {
    let focus = window.focused(cx);
    home.update(cx, |home, _| {
        home.hook_tooltip_focus = focus;
        home.dismissed_hook_tooltips.remove(id);
    });
    toggle_prompt(home, id, cx);
}

fn hook_control_focus(home: &Entity<HomeView>, id: &str, cx: &mut App) -> gpui::FocusHandle {
    home.update(cx, |home, cx| {
        home.hook_control_focus
            .entry(id.to_owned())
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone()
    })
}

impl HomeView {
    pub fn dismiss_hook_tooltips(&mut self, cx: &mut Context<Self>) -> bool {
        let keys = self
            .expanded_commands
            .iter()
            .filter(|id| id.starts_with("hook-statistics-"))
            .cloned()
            .chain(
                self.hook_hover_started
                    .iter()
                    .filter(|(_, time)| time.elapsed() >= std::time::Duration::from_millis(700))
                    .map(|(id, _)| id.clone()),
            )
            .collect::<Vec<_>>();
        if keys.is_empty() {
            return false;
        }
        self.dismissed_hook_tooltips.extend(keys);
        self.expanded_commands
            .retain(|id| !id.starts_with("hook-statistics-"));
        self.conversation_cache_dirty = true;
        cx.notify();
        true
    }
}

fn hook_prompt_document(text: &str) -> MarkdownDocument {
    fn inlines(values: &mut [MarkdownInline]) {
        for value in values {
            match value {
                MarkdownInline::SoftBreak => *value = MarkdownInline::HardBreak,
                MarkdownInline::Strong(children)
                | MarkdownInline::Emphasis(children)
                | MarkdownInline::Strikethrough(children)
                | MarkdownInline::Link {
                    content: children, ..
                } => inlines(children),
                _ => {}
            }
        }
    }
    fn blocks(values: &mut [MarkdownBlock]) {
        for value in values {
            match value {
                MarkdownBlock::Paragraph(content) | MarkdownBlock::Heading { content, .. } => {
                    inlines(content)
                }
                MarkdownBlock::List { items, .. } => {
                    for item in items {
                        blocks(&mut item.blocks);
                    }
                }
                MarkdownBlock::BlockQuote(children) => blocks(children),
                MarkdownBlock::Table { header, rows, .. } => {
                    for cell in header.iter_mut().chain(rows.iter_mut().flatten()) {
                        inlines(&mut cell.content);
                    }
                }
                _ => {}
            }
        }
    }
    let mut document = parse_markdown(text);
    blocks(&mut document.blocks);
    document
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hook_prompt_preserves_soft_lines_without_rewriting_code_or_inline_styles() {
        let doc = hook_prompt_document("first\n**second**\n\n```text\na\nb\n```\n");
        let MarkdownBlock::Paragraph(parts) = &doc.blocks[0] else {
            panic!()
        };
        assert_eq!(
            parts.as_slice(),
            [
                MarkdownInline::Text("first".into()),
                MarkdownInline::HardBreak,
                MarkdownInline::Strong(vec![MarkdownInline::Text("second".into())])
            ]
        );
        let MarkdownBlock::CodeBlock { code, .. } = &doc.blocks[1] else {
            panic!()
        };
        assert_eq!(code, "a\nb\n");
    }
    #[test]
    fn hook_only_history_has_no_fabricated_human_message_or_empty_activity() {
        use crate::agent::{AgentHookPrompt, AgentHookPromptFragment, AgentScopedHookPrompt};
        for text in ["hook input", " \n"] {
            let activities = vec![ConversationActivity::HookPrompt(AgentScopedHookPrompt {
                thread_id: "thread".into(),
                turn_id: "turn".into(),
                prompt: AgentHookPrompt {
                    id: "id".into(),
                    completed: None,
                    fragments: vec![AgentHookPromptFragment {
                        hook_run_id: "run".into(),
                        text: text.into(),
                    }],
                },
            })];
            let rows = super::super::timeline::conversation_list_rows(
                vec![],
                super::super::context::CurrentTurnRows {
                    phase: crate::conversation::ConversationPhase::Complete,
                    user_message: String::new(),
                    user_images: vec![],
                    user_message_time: String::new(),
                    assistant_message: String::new(),
                    assistant_message_time: None,
                    conversation_activity: &activities,
                    resumed_turn: None,
                },
                &Default::default(),
            );
            assert_eq!(rows.len(), usize::from(!text.trim().is_empty()));
            assert!(rows.iter().all(|row| matches!(
                row,
                super::super::timeline::ConversationListRow::Activity {
                    unit: super::super::timeline::ActivityStreamUnit::Standalone(
                        ConversationActivity::HookPrompt(_)
                    ),
                    ..
                }
            )));
        }
    }

    #[test]
    fn resumed_hook_input_is_not_folded_into_work_or_replaced_by_an_empty_disclosure() {
        use crate::agent::{AgentHookPrompt, AgentHookPromptFragment, AgentScopedHookPrompt};
        let activities = vec![
            ConversationActivity::HookPrompt(AgentScopedHookPrompt {
                thread_id: "thread".into(),
                turn_id: "turn".into(),
                prompt: AgentHookPrompt {
                    id: "hook-input".into(),
                    completed: None,
                    fragments: vec![AgentHookPromptFragment {
                        hook_run_id: "run".into(),
                        text: "injected text".into(),
                    }],
                },
            }),
            ConversationActivity::AssistantMessage {
                item_id: "answer".into(),
                text: "final".into(),
            },
        ];
        let turn = crate::conversation::ResumedTurnPresentation {
            id: "turn".into(),
            duration_ms: None,
            final_message_ids: vec!["answer".into()],
        };
        let mut rows = Vec::new();
        super::super::timeline::append_turn_activity_rows(
            &mut rows,
            &activities,
            false,
            crate::conversation::ConversationPhase::Complete,
            Some(&turn),
            &Default::default(),
        );
        assert_eq!(rows.len(), 2);
        assert!(matches!(
            &rows[0],
            super::super::timeline::ConversationListRow::Activity {
                unit: super::super::timeline::ActivityStreamUnit::Standalone(
                    ConversationActivity::HookPrompt(_)
                ),
                ..
            }
        ));
    }
}
