//! Approval geometry and interactions measured in the owned ChatGPT instance.

use super::*;
use crate::{
    components::{file_editor::FileEditor, icons::icon},
    theme::{UI_BODY_FONT_WEIGHT, UI_MONOSPACE_FONT_FAMILY},
};
use gpui::{BoxShadow, Context, Entity, Render, Role, Window};

pub fn render_approval_card(
    model: &ApprovalCardViewModel,
    theme: Theme,
    preview: Option<Entity<FileEditor>>,
    callback: ApprovalCardCallback,
) -> Option<Stateful<Div>> {
    if !model.should_render() {
        return None;
    }
    let palette = ApprovalPalette::for_theme(theme);
    let interactive = model.is_interactive();
    let network = matches!(model.request, ApprovalRequestPresentation::Network { .. });
    let header = div()
        .min_h(px(APPROVAL_CARD_HEADER_HEIGHT))
        .px(px(16.0))
        .pt(px(16.0))
        .pb(px(12.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .h(px(20.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(13.0))
                .line_height(px(20.0))
                .font_weight(FontWeight::NORMAL)
                .text_color(palette.secondary)
                .child(
                    icon(
                        if network {
                            "approval-internet"
                        } else {
                            "panel-terminal"
                        },
                        palette.icon.into(),
                    )
                    .size(px(18.0)),
                )
                .child(model.request.title()),
        )
        .child(
            div()
                .id(approval_element_id("approval-question", &model.request_id))
                .role(Role::Alert)
                .aria_label(model.request.question())
                .max_h(px(160.0))
                .overflow_y_scroll()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(palette.question_text)
                .when(!network, |title| title.child(model.request.question()))
                .when(network, |header| {
                    let ApprovalRequestPresentation::Network {
                        destination,
                        reason,
                        ..
                    } = &model.request
                    else {
                        unreachable!()
                    };
                    let host = destination
                        .split_once("://")
                        .map(|(_, host)| host)
                        .unwrap_or(destination);
                    let open = callback.clone();
                    header
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap(px(4.0))
                                .child("允许 ChatGPT 与")
                                .child(
                                    div()
                                        .id(approval_element_id(
                                            "approval-destination",
                                            &model.request_id,
                                        ))
                                        .role(Role::Link)
                                        .aria_label(destination.clone())
                                        .flex()
                                        .items_center()
                                        .gap(px(4.0))
                                        .text_color(theme.markdown_file_link)
                                        .cursor_pointer()
                                        .on_click(move |_, window, cx| {
                                            open.emit(
                                                ApprovalCardEvent::OpenNetworkDestination,
                                                window,
                                                cx,
                                            )
                                        })
                                        .child(
                                            icon(
                                                "approval-link-globe",
                                                theme.markdown_file_link.into(),
                                            )
                                            .size(px(16.0)),
                                        )
                                        .child(destination.clone()),
                                )
                                .child("建立连接？"),
                        )
                        .child(
                            div()
                                .mt(px(2.0))
                                .text_size(px(13.0))
                                .line_height(px(19.5))
                                .font_weight(UI_BODY_FONT_WEIGHT)
                                .text_color(theme.text_tertiary)
                                .child(
                                    reason
                                        .as_deref()
                                        .map(str::trim)
                                        .filter(|value| !value.is_empty())
                                        .map(str::to_owned)
                                        .unwrap_or_else(|| {
                                            format!("{host} 不在当前网络允许列表中")
                                        }),
                                ),
                        )
                }),
        );

    let mut card = div()
        .min_h(px(model.geometry().card_height))
        .w_full()
        .rounded(px(APPROVAL_CARD_RADIUS))
        .border_1()
        .border_color(palette.button_border)
        .bg(palette.card)
        .overflow_hidden()
        .flex()
        .flex_col()
        .child(header);
    if let Some(text) = model.request.preview() {
        let has_expand = model.preview_line_count > 3;
        let content_height = model.preview_height() - if has_expand { 32.0 } else { 0.0 };
        let mut content = div()
            .h(px(content_height))
            .p(px(8.0))
            .min_w(px(0.0))
            .overflow_hidden();
        content = if let Some(preview) = preview {
            content.child(preview)
        } else {
            content.child(
                div()
                    .font_family(UI_MONOSPACE_FONT_FAMILY)
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_tertiary)
                    .child(text.to_owned()),
            )
        };
        let toggle = callback.clone();
        card = card.child(
            div().px(px(12.0)).child(
                div()
                    .h(px(model.preview_height()))
                    .rounded(px(10.0))
                    .bg(palette.preview)
                    .overflow_hidden()
                    .child(content)
                    .when(has_expand, |container| {
                        container.child(
                            div().h(px(32.0)).p(px(4.0)).flex().justify_end().child(
                                div()
                                    .id(approval_element_id(
                                        "approval-preview-toggle",
                                        &model.request_id,
                                    ))
                                    .role(Role::Button)
                                    .aria_label(if model.preview_expanded {
                                        "收起命令预览"
                                    } else {
                                        "展开命令预览"
                                    })
                                    .h(px(24.0))
                                    .px(px(8.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(999.0))
                                    .text_size(px(13.0))
                                    .line_height(px(18.0))
                                    .text_color(theme.text_tertiary)
                                    .cursor_pointer()
                                    .hover(|d| d.bg(palette.decline_hover))
                                    .on_click(move |_, w, cx| {
                                        toggle.emit(ApprovalCardEvent::TogglePreview, w, cx)
                                    })
                                    .child(if model.preview_expanded {
                                        "收起"
                                    } else {
                                        "展开"
                                    }),
                            ),
                        )
                    }),
            ),
        );
    }
    if !model.permission_details.is_empty() {
        let mut details = div()
            .id(approval_element_id(
                "approval-permissions",
                &model.request_id,
            ))
            .mx(px(16.0))
            .my(px(8.0))
            .max_h(px(120.0))
            .overflow_y_scroll()
            .text_size(px(13.0))
            .line_height(px(19.5))
            .text_color(palette.secondary);
        for permission in &model.permission_details {
            details = details.child(div().child(permission.clone()));
        }
        card = card.child(details);
    }
    if let Some(error) = &model.failure_message {
        card = card.child(
            div()
                .px(px(16.0))
                .pb(px(16.0))
                .id(approval_element_id("approval-error", &model.request_id))
                .role(Role::Alert)
                .aria_label(error.clone())
                .text_size(px(13.0))
                .line_height(px(20.0))
                .text_color(theme.warning)
                .child(error.clone()),
        );
    }
    if !interactive {
        let stop = callback.clone();
        let actions = div()
            .h(px(APPROVAL_CARD_ACTIONS_HEIGHT))
            .px(px(16.0))
            .pt(px(8.0))
            .pb(px(16.0))
            .flex()
            .justify_end()
            .child(
                div()
                    .id(approval_element_id("approval-stop", &model.request_id))
                    .role(Role::Button)
                    .aria_label("停止当前轮次")
                    .h(px(28.0))
                    .px(px(8.0))
                    .rounded_full()
                    .border_1()
                    .border_color(palette.button_border)
                    .text_color(palette.text)
                    .text_size(px(13.0))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .on_click(move |_, window, cx| {
                        stop.emit(ApprovalCardEvent::StopTurn, window, cx)
                    })
                    .child("停止当前轮次"),
            );
        return Some(
            div()
                .id(approval_element_id("approval-card", &model.request_id))
                .relative()
                .w_full()
                .child(card.child(actions)),
        );
    }
    let choices = model.choices();
    let menu_choices = model.menu_choices();
    let mut actions = div()
        .h(px(APPROVAL_CARD_ACTIONS_HEIGHT))
        .px(px(16.0))
        .pt(px(8.0))
        .pb(px(16.0))
        .flex()
        .items_center()
        .gap(px(8.0));
    for (i, choice) in choices
        .iter()
        .enumerate()
        .filter(|(_, c)| c.kind == ApprovalChoiceKind::NetworkAllow)
    {
        let mut choice = choice.clone();
        choice.label = "始终允许".to_owned();
        actions = actions.child(action_button(
            model,
            i,
            &choice,
            false,
            None,
            palette,
            callback.clone(),
        ));
    }
    actions = actions.child(div().flex_1());
    if let Some((i, mut choice)) = model.reject_choice() {
        choice.label = "拒绝".to_owned();
        actions = actions.child(action_button(
            model,
            i,
            &choice,
            false,
            Some("Esc"),
            palette,
            callback.clone(),
        ));
    }
    if let Some((i, choice)) = model.primary_choice() {
        let split = menu_choices.len() > 1;
        let primary = action_button(
            model,
            i,
            &choice,
            true,
            Some("⏎"),
            palette,
            callback.clone(),
        )
        .when(split, |b| {
            b.rounded_r(px(0.0)).pr(px(4.0)).border_r(px(0.0))
        });
        let menu_toggle = callback.clone();
        actions = actions.child(
            div()
                .flex()
                .h(px(APPROVAL_BUTTON_HEIGHT))
                .rounded(px(999.0))
                .overflow_hidden()
                .child(primary)
                .when(split, |group| {
                    group.child(
                        div()
                            .id(approval_element_id(
                                "approval-menu-toggle",
                                &model.request_id,
                            ))
                            .role(Role::Button)
                            .aria_label("审批选项")
                            .w(px(23.0))
                            .h(px(APPROVAL_BUTTON_HEIGHT))
                            .rounded_r(px(999.0))
                            .pl(px(2.0))
                            .pr(px(6.0))
                            .flex()
                            .items_center()
                            .bg(palette.approve)
                            .when(interactive, |d| {
                                d.cursor_pointer()
                                    .hover(|d| d.bg(palette.approve_hover))
                                    .on_click(move |_, w, cx| {
                                        menu_toggle.emit(ApprovalCardEvent::ToggleMenu, w, cx)
                                    })
                            })
                            .child(
                                icon("chevron-down", palette.approve_text.alpha(0.5).into())
                                    .size(px(14.0)),
                            ),
                    )
                }),
        );
    }
    card = card.child(actions);
    let mut result = div()
        .id(approval_element_id("approval-card", &model.request_id))
        .relative()
        .w_full()
        .child(card);
    if model.visual_state.menu_open() && interactive {
        let dismiss = callback.clone();
        result = result
            .on_mouse_down_out(move |_, w, cx| dismiss.emit(ApprovalCardEvent::ToggleMenu, w, cx));
        let mut menu = div()
            .id(approval_element_id("approval-menu", &model.request_id))
            .role(Role::Menu)
            .aria_label("审批选项")
            .absolute()
            .bottom(px(47.0))
            .right(px(17.75))
            .w(px(168.0))
            .p(px(4.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .rounded(px(16.0))
            .bg(palette.menu)
            .shadow(vec![
                BoxShadow::new(px(0.0), px(0.0), palette.menu_outline.into())
                    .spread_radius(px(0.5)),
                BoxShadow::new(px(0.0), px(8.0), rgba(0x0000001f).into()).blur_radius(px(16.0)),
            ]);
        for (i, choice) in menu_choices {
            let click = callback.clone();
            let hover = callback.clone();
            let decision = choice.decision;
            let focused = matches!(model.keyboard_focus,Some(ApprovalKeyboardFocus::MenuChoice(n)) if n==i)
                || matches!(model.visual_state,ApprovalVisualState::SplitMenu{focused:Some(ApprovalMenuItem::Choice(n))} if n==i);
            let mut row = div()
                .id(approval_element_id(
                    "approval-menu-choice",
                    &format!("{}-{i}", model.request_id),
                ))
                .role(Role::MenuItem)
                .aria_label(choice.label.clone())
                .h(px(APPROVAL_MENU_ROW_HEIGHT))
                .w_full()
                .px(px(8.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .rounded(px(12.0))
                .text_size(px(13.0))
                .line_height(px(18.5714))
                .font_weight(UI_BODY_FONT_WEIGHT)
                .text_color(palette.menu_text)
                .when(focused, |d| d.bg(palette.menu_focus))
                .cursor_pointer()
                .hover(|d| d.bg(palette.menu_focus))
                .on_hover(move |hovered, w, cx| {
                    hover.emit(
                        ApprovalCardEvent::MenuFocusChanged(
                            (*hovered).then_some(ApprovalMenuItem::Choice(i)),
                        ),
                        w,
                        cx,
                    )
                })
                .on_click(move |_, w, cx| click.emit(ApprovalCardEvent::Decision(decision), w, cx))
                .child(div().flex_1().min_w(px(0.0)).truncate().child(choice.label));
            if let Some(description) = choice.description {
                row = row
                    .tooltip(move |_, cx| {
                        cx.new(|_| ApprovalTooltip(description.clone(), palette))
                            .into()
                    })
                    .child(
                        icon("file-approval-info", palette.text.alpha(0.75).into()).size(px(16.0)),
                    );
            }
            menu = menu.child(row);
        }
        result = result.child(menu);
    }
    Some(result)
}

fn action_button(
    model: &ApprovalCardViewModel,
    index: usize,
    choice: &ApprovalChoicePresentation,
    primary: bool,
    shortcut: Option<&'static str>,
    palette: ApprovalPalette,
    callback: ApprovalCardCallback,
) -> Stateful<Div> {
    let enabled = model.is_interactive();
    let decision = choice.decision;
    let bg = if primary {
        palette.approve
    } else {
        palette.decline
    };
    let hover = if primary {
        palette.approve_hover
    } else {
        palette.decline_hover
    };
    let text = if primary {
        palette.approve_text
    } else {
        palette.decline_text
    };
    let mut button = div()
        .id(approval_element_id(
            "approval-choice",
            &format!("{}-{index}", model.request_id),
        ))
        .role(Role::Button)
        .aria_label(choice.label.clone())
        .h(px(APPROVAL_BUTTON_HEIGHT))
        .px(px(8.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .rounded(px(999.0))
        .border_1()
        .border_color(palette.button_border)
        .bg(bg)
        .text_size(px(13.0))
        .line_height(px(18.0))
        .font_weight(UI_BODY_FONT_WEIGHT)
        .text_color(text)
        .when(!enabled, |b| b.opacity(0.4))
        .when(enabled, |b| {
            b.cursor_pointer()
                .hover(move |b| b.bg(hover))
                .on_click(move |_, w, cx| {
                    callback.emit(ApprovalCardEvent::Decision(decision), w, cx)
                })
        })
        .child(choice.label.clone());
    if let Some(shortcut) = shortcut {
        button = button.child(keycap(shortcut, text));
    }
    if let Some(description) = choice.description.clone() {
        button = button.tooltip(move |_, cx| {
            cx.new(|_| ApprovalTooltip(description.clone(), palette))
                .into()
        });
    }
    button
}

struct ApprovalTooltip(String, ApprovalPalette);
impl Render for ApprovalTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w(px(400.0))
            .p(px(10.0))
            .rounded(px(12.0))
            .bg(self.1.menu)
            .text_color(self.1.text)
            .text_size(px(12.0))
            .line_height(px(18.0))
            .child(self.0.clone())
    }
}
