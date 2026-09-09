//! Landing presentation and interaction for the conversation view.

use gpui::{Div, Entity, div, prelude::*, px};

use super::{
    COMPOSER_BOTTOM_INSET, CONVERSATION_BOTTOM_INSET,
    context::{ConversationRenderContext, MainConversationSnapshot},
    conversation::conversation,
    requests::{
        command_approval_card, file_approval_card, permissions_approval_card,
        user_input_request_card,
    },
};
use crate::{
    components::{
        composer::{COMPOSER_CORNER_RADIUS, ComposerView},
        icons::icon,
        prompt_input::PromptInput,
    },
    conversation::{ConversationActivity, ConversationPhase},
};

pub(super) fn home(
    render: ConversationRenderContext,
    composer: Entity<ComposerView>,
    user_input_other: Entity<PromptInput>,
    snapshot: MainConversationSnapshot,
    first_suggestion: impl IntoElement,
    second_suggestion: impl IntoElement,
) -> Div {
    let home_entity = render.home_entity.clone();
    let theme = render.theme;
    let MainConversationSnapshot {
        side_chat,
        composer_height,
        rows: conversation_rows,
        phase,
        activities: conversation_activity,
        list: conversation_list,
    } = snapshot;
    let pending_command_approval = conversation_activity.iter().find_map(|activity| {
        let ConversationActivity::Approval(model) = activity else {
            return None;
        };
        model.should_render().then(|| model.clone())
    });
    let pending_user_input = conversation_activity.iter().find_map(|activity| {
        let ConversationActivity::UserInput(model) = activity else {
            return None;
        };
        model.should_render().then(|| model.clone())
    });
    let pending_file_approval = conversation_activity.iter().find_map(|activity| {
        let ConversationActivity::FileApproval(model) = activity else {
            return None;
        };
        model.should_render().then(|| model.clone())
    });
    let pending_permissions_approval = conversation_activity.iter().find_map(|activity| {
        let ConversationActivity::PermissionsApproval(model) = activity else {
            return None;
        };
        model.should_render().then(|| model.clone())
    });
    let turn_plan = conversation_activity
        .iter()
        .rev()
        .find_map(|activity| match activity {
            ConversationActivity::TurnPlan(plan) if !plan.steps.is_empty() => Some(plan.clone()),
            _ => None,
        });
    let blocking_request_pending = pending_command_approval.is_some()
        || pending_user_input.is_some()
        || pending_file_approval.is_some()
        || pending_permissions_approval.is_some();

    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .relative()
        .when(phase == ConversationPhase::Empty, |root| {
            root.child(
                div()
                    .absolute()
                    // These are component boundaries, not a viewport-specific
                    // heading coordinate. GPUI centers the group in between them.
                    .top(px(if side_chat { 78.0 } else { 46.0 }))
                    .bottom(px(if side_chat {
                        composer_height + 60.0
                    } else {
                        CONVERSATION_BOTTOM_INSET
                    }))
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(768.0))
                            .px(px(24.0))
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(12.0))
                            .child(
                                icon(
                                    if side_chat { "side-chat" } else { "home-mark" },
                                    if side_chat {
                                        theme.text_secondary
                                    } else {
                                        theme.home_mark
                                    }
                                    .into(),
                                )
                                .size(px(if side_chat { 32.0 } else { 56.0 }))
                                .relative()
                                .top(px(-2.0)),
                            )
                            .child(
                                div()
                                    .text_size(px(if side_chat { 16.0 } else { 28.0 }))
                                    .line_height(px(if side_chat { 24.0 } else { 33.6 }))
                                    .font_weight(gpui::FontWeight::NORMAL)
                                    .text_color(theme.text)
                                    .child(if side_chat {
                                        "侧边聊天"
                                    } else {
                                        "你想让我们在 coda 中构建什么？"
                                    }),
                            )
                            .when(side_chat, |group| {
                                group.child(
                                    div()
                                        .mt(px(-4.0))
                                        .text_size(px(13.0))
                                        .line_height(px(18.5714))
                                        .text_color(theme.text_secondary)
                                        .text_center()
                                        .child("侧边聊天是临时聊天，关闭应用后会消失。"),
                                )
                            }),
                    ),
            )
        })
        .when(phase != ConversationPhase::Empty, |root| {
            root.child(conversation(
                render.clone(),
                conversation_rows,
                conversation_list,
                if side_chat {
                    composer_height + 55.0
                } else {
                    CONVERSATION_BOTTOM_INSET
                },
            ))
        })
        .when(phase != ConversationPhase::Empty, |root| {
            root.child(
                div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .w_full()
                    // The upper corner cutouts remain open so rows pass behind
                    // the floating Composer. Its lower corners and the window
                    // inset are outside the conversation's visible region.
                    .h(px(COMPOSER_BOTTOM_INSET + COMPOSER_CORNER_RADIUS))
                    .bg(theme.surface),
            )
        })
        .when_some(pending_command_approval, |root, model| {
            let card = command_approval_card(home_entity.clone(), model, theme);
            root.when_some(card, |root, card| {
                root.child(
                    div()
                        .id("command-approval-overlay")
                        .debug_selector(|| "command-approval-overlay".to_owned())
                        .absolute()
                        .bottom(px(16.0))
                        .w_full()
                        .max_w(px(736.0))
                        .child(card),
                )
            })
        })
        .when_some(pending_user_input, |root, model| {
            let card = user_input_request_card(
                home_entity.clone(),
                model,
                theme,
                user_input_other.clone(),
            );
            root.when_some(card, |root, card| {
                root.child(
                    div()
                        .absolute()
                        .bottom(px(16.0))
                        .w_full()
                        .max_w(px(736.0))
                        .child(div().relative().left(px(2.671_875)).w_full().child(card)),
                )
            })
        })
        .when_some(pending_file_approval, |root, model| {
            let card = file_approval_card(home_entity.clone(), model, theme);
            root.when_some(card, |root, card| {
                root.child(
                    div()
                        .absolute()
                        .bottom(px(16.0))
                        .w_full()
                        .max_w(px(736.0))
                        .child(
                            // CDP 39-43 rasterize the left edge one pixel before
                            // GPUI at the same fractional CSS coordinate.
                            div().relative().left(px(1.671_875)).w_full().child(card),
                        ),
                )
            })
        })
        .when_some(pending_permissions_approval, |root, model| {
            let card = permissions_approval_card(home_entity.clone(), model, theme);
            root.when_some(card, |root, card| {
                root.child(
                    div()
                        .absolute()
                        .bottom(px(16.0))
                        .w_full()
                        .max_w(px(736.0))
                        .child(div().relative().left(px(0.671_875)).w_full().child(card)),
                )
            })
        })
        .child(
            div()
                .id("composer-overlay")
                .debug_selector(|| "composer-overlay".to_owned())
                .absolute()
                .bottom(px(COMPOSER_BOTTOM_INSET))
                .w_full()
                .max_w(px(748.0))
                // Match the reference composition at every window size: the
                // composer sits 6px inside its responsive container, while
                // its utility strip adds its own 14px inset.
                .px(px(6.0))
                .flex()
                .flex_col()
                .justify_end()
                .gap(px(8.0))
                .when(
                    phase == ConversationPhase::Empty && !side_chat,
                    |container| {
                        container.child(
                            div()
                                .min_h(px(80.0))
                                .px(px(19.0))
                                .flex()
                                .flex_col()
                                .justify_end()
                                .child(first_suggestion)
                                .child(second_suggestion),
                        )
                    },
                )
                .when_some(
                    turn_plan.filter(|_| {
                        !blocking_request_pending
                            && matches!(
                                phase,
                                ConversationPhase::Starting
                                    | ConversationPhase::Thinking
                                    | ConversationPhase::Streaming
                                    | ConversationPhase::Stopping
                            )
                    }),
                    |container, plan| {
                        let expanded = render
                            .disclosures
                            .expanded_commands
                            .contains(&format!("turn-plan-{}", plan.turn_id));
                        container.child(div().w_full().flex().justify_center().child(
                            super::progress::turn_plan_control(
                                home_entity.clone(),
                                plan,
                                expanded,
                                theme,
                            ),
                        ))
                    },
                )
                .when(!blocking_request_pending, |container| {
                    container.child(composer)
                }),
        )
}
