//! Conversation presentation and interaction for the conversation view.

use std::rc::Rc;

use gpui::{
    BoxShadow, Div, Entity, FocusHandle, FollowMode, FontWeight, IntoElement, KeyDownEvent,
    ListAlignment, ListState, Role, ScrollDelta, ScrollHandle, ScrollWheelEvent, SharedString,
    Window, div, list, prelude::*, px, rgba,
};

use super::{
    CONVERSATION_BOTTOM_EPSILON, CONVERSATION_CONTENT_MAX_WIDTH, CONVERSATION_LIST_OVERDRAW,
    CONVERSATION_TOP_INSET, HomeView, OpenDiffReview,
    activity::{activity_stream, render_activity_stream_unit},
    animation::thinking_shimmer,
    context::ConversationRenderContext,
    messages::{current_response_footer, current_user_message, message_action},
    timeline::{ActivityStreamUnit, ConversationListRow, conversation_status},
};
use crate::{
    components::{
        file_change::DiffReviewPresentation, icons::icon, markdown::render_assistant_markdown,
    },
    conversation::{ConversationActivity, ConversationPhase, ConversationTranscriptTurn},
    theme::Theme,
};

pub(super) fn subagent_conversation(
    render: ConversationRenderContext,
    transcript: Vec<ConversationTranscriptTurn>,
    phase: ConversationPhase,
    assistant_message: String,
    conversation_activity: Vec<ConversationActivity>,
    conversation_scroll: ScrollHandle,
) -> Div {
    let home_entity = render.home_entity.clone();
    let theme = render.theme;
    let thinking_shimmer_progress = render.thinking_shimmer_progress;
    let response_feedback = render.response_feedback;
    let has_historical_content = transcript
        .iter()
        .any(|turn| !turn.assistant_message.is_empty() || !turn.activities.is_empty());
    let has_current_content = !assistant_message.is_empty() || !conversation_activity.is_empty();
    let has_active_reasoning = conversation_activity.iter().any(|activity| {
        matches!(activity, ConversationActivity::Reasoning(reasoning) if reasoning.is_active())
    });
    let show_thinking_tail = conversation_status(phase).is_some() && !has_active_reasoning;
    let has_visible_current = has_current_content || show_thinking_tail;
    let complete = matches!(
        phase,
        ConversationPhase::Complete | ConversationPhase::Failed
    );
    let copied_assistant_message = assistant_message.clone();

    let mut messages = div()
        .w_full()
        // Live Electron child view: the 16px toolbar gutter plus stable
        // scrollbar gutters place content 31px from each panel edge.
        .px(px(31.0))
        .pt(px(32.0))
        .pb(px(32.0))
        .flex()
        .flex_col()
        .gap(px(12.0));

    for (index, turn) in transcript.into_iter().enumerate() {
        if turn.assistant_message.is_empty() && turn.activities.is_empty() {
            continue;
        }
        let answer = if turn.activities.is_empty() {
            render_assistant_markdown(
                &turn.assistant_message,
                theme,
                &format!("subagent-historical-assistant-{index}"),
            )
            .into_any_element()
        } else {
            activity_stream(
                &render,
                turn.activities,
                conversation_status(turn.phase).is_some(),
            )
            .into_any_element()
        };
        messages = messages.child(
            div()
                .id(("subagent-transcript-turn", index))
                .w_full()
                .text_size(px(14.0))
                .line_height(px(22.0))
                .text_color(theme.text)
                .child(answer),
        );
    }

    if has_visible_current {
        let answer = if conversation_activity.is_empty() {
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(16.0))
                .when(!assistant_message.is_empty(), |stream| {
                    stream.child(render_assistant_markdown(
                        &assistant_message,
                        theme,
                        "subagent-current-assistant",
                    ))
                })
                .when(show_thinking_tail, |stream| {
                    stream.child(thinking_shimmer(theme, thinking_shimmer_progress))
                })
        } else {
            activity_stream(&render, conversation_activity, show_thinking_tail)
        };
        messages = messages.child(
            div()
                .id("subagent-current-turn")
                .w_full()
                .text_size(px(14.0))
                .line_height(px(22.0))
                .text_color(theme.text)
                .child(answer)
                .when(complete && !copied_assistant_message.is_empty(), |turn| {
                    turn.child(
                        div()
                            .mt(px(6.0))
                            .h(px(20.0))
                            .flex()
                            .items_center()
                            .gap(px(2.0))
                            .child(message_action(
                                "message-copy",
                                "subagent-response-copy",
                                0,
                                false,
                                copied_assistant_message.clone(),
                                home_entity.clone(),
                                theme,
                            ))
                            .child(message_action(
                                "message-thumb-up",
                                "subagent-response-thumb-up",
                                1,
                                response_feedback == 1,
                                copied_assistant_message.clone(),
                                home_entity.clone(),
                                theme,
                            ))
                            .child(message_action(
                                "message-thumb-down",
                                "subagent-response-thumb-down",
                                2,
                                response_feedback == -1,
                                copied_assistant_message.clone(),
                                home_entity.clone(),
                                theme,
                            )),
                    )
                }),
        );
    }

    div().size_full().relative().child(
        div()
            .id("subagent-conversation")
            .size_full()
            .relative()
            .when(!has_historical_content && !has_visible_current, |root| {
                root.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .id("subagent-loading")
                                .role(Role::ProgressIndicator)
                                .aria_label("正在载入子智能体")
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .text_size(px(13.0))
                                .line_height(px(20.0))
                                .text_color(theme.text_tertiary)
                                .child(icon("subagent-activity", theme.text.into()).size(px(16.0)))
                                .child("正在载入子智能体…"),
                        ),
                )
            })
            .when(has_historical_content || has_visible_current, |root| {
                root.child(
                    div()
                        .id("subagent-conversation-scroll")
                        .absolute()
                        .inset_0()
                        .overflow_y_scroll()
                        .restrict_scroll_to_axis()
                        .scrollbar_width(px(0.0))
                        .track_scroll(&conversation_scroll)
                        .child(messages),
                )
            }),
    )
}

pub(super) fn conversation(
    render: ConversationRenderContext,
    rows: Rc<Vec<ConversationListRow>>,
    conversation_list: ListState,
    bottom_inset: f32,
) -> impl IntoElement {
    let home_entity = render.home_entity.clone();
    let theme = render.theme;
    let thinking_shimmer_progress = render.thinking_shimmer_progress;
    let response_feedback = render.response_feedback;
    let user_message_actions_visible_for_capture = render.user_message_actions_visible_for_capture;
    sync_list_item_count(&conversation_list, rows.len());
    let conversation_rows = list(conversation_list, move |index, window, _cx| {
        let Some(row) = rows.get(index).cloned() else {
            return div().into_any_element();
        };
        let is_markdown = matches!(
            &row,
            ConversationListRow::AssistantMarkdown { .. }
                | ConversationListRow::Activity {
                    unit: ActivityStreamUnit::Standalone(
                        ConversationActivity::AssistantMessage { .. }
                    ),
                    ..
                }
        );
        let (row, top_gap, bottom_gap) = match row {
            ConversationListRow::FileSummary(review) => (
                resumed_file_summary_card(review, home_entity.clone(), theme, _cx)
                    .into_any_element(),
                0.0,
                44.0,
            ),
            ConversationListRow::ResumedWork {
                id,
                label,
                expanded,
            } => (
                resumed_work_header(
                    home_entity.clone(),
                    home_entity
                        .read(_cx)
                        .resumed_turn_focus
                        .get(&id)
                        .expect("resumed header focus")
                        .clone(),
                    id,
                    label,
                    expanded,
                    theme,
                )
                .into_any_element(),
                0.0,
                16.0,
            ),
            ConversationListRow::HistoricalUser {
                turn_index,
                message,
                images,
                time,
            } => (
                div()
                    .id(("transcript-turn-user", turn_index))
                    .w_full()
                    .child(current_user_message(
                        super::messages::UserMessageContent {
                            text: message,
                            images,
                            time: time.unwrap_or_default(),
                        },
                        false,
                        theme,
                        window,
                        home_entity.clone(),
                        home_entity.read(_cx).content_width,
                    ))
                    .into_any_element(),
                0.0,
                16.0,
            ),
            ConversationListRow::CurrentUser {
                message,
                images,
                time,
            } => (
                current_user_message(
                    super::messages::UserMessageContent {
                        text: message,
                        images,
                        time,
                    },
                    user_message_actions_visible_for_capture,
                    theme,
                    window,
                    home_entity.clone(),
                    home_entity.read(_cx).content_width,
                )
                .into_any_element(),
                0.0,
                16.0,
            ),
            ConversationListRow::AssistantMarkdown { id, text } => (
                render_assistant_markdown(&text, theme, &id).into_any_element(),
                0.0,
                16.0,
            ),
            ConversationListRow::Activity {
                unit,
                show_thinking_tail,
            } => (
                render_activity_stream_unit(&render, index, unit, show_thinking_tail),
                0.0,
                16.0,
            ),
            ConversationListRow::Thinking => (
                thinking_shimmer(theme, thinking_shimmer_progress).into_any_element(),
                0.0,
                16.0,
            ),
            ConversationListRow::CurrentResponseFooter {
                id,
                message,
                completed_at,
            } => (
                div()
                    .id(SharedString::from(format!("response-footer-{id}")))
                    .child(current_response_footer(
                        &id,
                        message,
                        completed_at,
                        response_feedback,
                        home_entity.clone(),
                        theme,
                        _cx,
                    ))
                    .into_any_element(),
                0.0,
                3.0,
            ),
        };
        // The response toolbar owns its 3px top inset. A normal activity gap
        // here double-counts spacing and shifts the entire bottom-anchored
        // answer upward by 16px in a resumed thread.
        let bottom_gap = if matches!(
            rows.get(index + 1),
            Some(ConversationListRow::CurrentResponseFooter { .. })
        ) {
            0.0
        } else {
            bottom_gap
        };
        div()
            .w_full()
            .flex()
            .justify_center()
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .when(!is_markdown, |element| {
                        element.max_w(px(CONVERSATION_CONTENT_MAX_WIDTH))
                    })
                    .pt(px(top_gap))
                    .pb(px(bottom_gap))
                    .text_size(px(14.0))
                    .line_height(px(22.0))
                    .text_color(theme.text)
                    .child(row),
            )
            .into_any_element()
    })
    .size_full()
    .pt(px(CONVERSATION_TOP_INSET))
    .pb(px(bottom_inset));

    div()
        .id("conversation-scroll")
        .debug_selector(|| "conversation-scroll".to_owned())
        .absolute()
        .inset_0()
        // `ListState` deliberately lays out an overdraw band above and below
        // the viewport. Unlike the previous `overflow_y_scroll` container,
        // the virtual list does not establish its own paint mask. Keep that
        // overdraw inside the window without shortening the scroll area at
        // the Composer's top edge.
        .overflow_hidden()
        .child(conversation_rows)
}

pub(super) fn resumed_work_header(
    home: Entity<HomeView>,
    focus: FocusHandle,
    id: String,
    label: String,
    expanded: bool,
    theme: Theme,
) -> Div {
    let click_home = home.clone();
    let click_id = id.clone();
    let mut muted = theme.text;
    muted.a = 0.6;
    div()
        .w_full()
        .pb(px(4.0))
        .border_b_1()
        .border_color(theme.border)
        .flex()
        .flex_col()
        .items_start()
        .child(
            div()
                .id(SharedString::from(format!("resumed-work-{id}")))
                .flex()
                .items_center()
                .gap(px(4.0))
                .h(px(23.0))
                .max_w_full()
                .rounded(px(6.0))
                .focusable()
                .track_focus(&focus)
                .tab_stop(true)
                .role(Role::Button)
                .aria_label(format!(
                    "{label}，{}工作过程",
                    if expanded { "折叠" } else { "展开" }
                ))
                .aria_expanded(expanded)
                .cursor_pointer()
                .text_size(px(14.0))
                .line_height(px(21.0))
                .text_color(muted)
                .hover(|style| style.text_color(theme.text))
                .focus_visible(|style| {
                    style.shadow(vec![
                        BoxShadow::new(px(0.0), px(0.0), theme.accent.into())
                            .spread_radius(px(2.0))
                            .inset(),
                    ])
                })
                .on_click(move |_, window, cx| {
                    window.focus(&focus, cx);
                    cx.stop_propagation();
                    click_home.update(cx, |home, cx| home.toggle_resumed_turn(&click_id, cx));
                })
                .on_key_down(|event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "tab" {
                        if event.keystroke.modifiers.shift {
                            window.focus_prev(cx);
                        } else {
                            window.focus_next(cx);
                        }
                        cx.stop_propagation();
                    }
                })
                .child(label)
                .child(
                    icon(
                        if expanded {
                            "chevron-down"
                        } else {
                            "settings-chevron-right"
                        },
                        muted.into(),
                    )
                    .size(px(12.0)),
                ),
        )
}

pub(super) fn resumed_file_summary_card(
    review: DiffReviewPresentation,
    home: Entity<HomeView>,
    theme: Theme,
    cx: &gpui::App,
) -> Div {
    let expanded = home
        .read(cx)
        .expanded_file_summaries
        .contains(&review.review_id);
    let open_home = home.clone();
    let open_review = review.clone();
    let review_home = home.clone();
    let review_button = review.clone();
    let added = if theme.surface.r > 0.5 {
        rgba(0x00a33aff)
    } else {
        rgba(0x40c977ff)
    };
    let removed = if theme.surface.r > 0.5 {
        rgba(0xe02e2aff)
    } else {
        rgba(0xfa423eff)
    };
    let counts = |additions, deletions| {
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .text_size(px(13.0))
            .line_height(px(19.5))
            .child(div().text_color(added).child(format!("+{additions}")))
            .child(div().text_color(removed).child(format!("-{deletions}")))
    };
    let card_surface = if theme.surface.r > 0.5 {
        rgba(0xffffffff)
    } else {
        rgba(0x232323ff)
    };
    let row_surface = if theme.surface.r > 0.5 {
        rgba(0xffffffff)
    } else {
        rgba(0x1c1c1cff)
    };
    let icon_surface = if theme.surface.r > 0.5 {
        rgba(0xf7f7f7ff)
    } else {
        rgba(0x151515ff)
    };
    let mut card = div()
        .w_full()
        .rounded(px(12.5))
        .overflow_hidden()
        .border_1()
        .border_color(theme.border)
        .bg(card_surface)
        .text_color(theme.text)
        .text_size(px(14.0))
        .line_height(px(21.0))
        .child(
            div()
                .id(SharedString::from(format!("{}-review", review.review_id)))
                .h(px(65.5))
                .px(px(12.0))
                .flex()
                .items_center()
                .gap(px(10.0))
                .role(Role::Button)
                .aria_label("审查已更改的文件")
                .focusable()
                .tab_stop(true)
                .cursor_pointer()
                .hover(move |s| s.bg(theme.sidebar_hover))
                .on_click(move |_, _, cx| {
                    open_home.update(cx, |_, cx| cx.emit(OpenDiffReview(open_review.clone())));
                })
                .child(
                    div()
                        .size(px(40.0))
                        .rounded(px(12.5))
                        .bg(icon_surface)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            icon("resumed-file-summary", theme.text_secondary.into())
                                .size(px(24.0)),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .child(format!("已编辑 {} 个文件", review.files.len())),
                        )
                        .child(counts(review.total_additions(), review.total_deletions())),
                )
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "{}-review-button",
                            review.review_id
                        )))
                        .h(px(28.0))
                        .px(px(8.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(theme.border)
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .role(Role::Button)
                        .aria_label("审核")
                        .on_click(move |_, _, cx| {
                            review_home
                                .update(cx, |_, cx| cx.emit(OpenDiffReview(review_button.clone())));
                            cx.stop_propagation();
                        })
                        .child("审核"),
                ),
        )
        .child(div().h(px(1.0)).bg(theme.border));
    for (index, file) in review
        .files
        .iter()
        .enumerate()
        .take(if expanded { usize::MAX } else { 3 })
    {
        let click_home = home.clone();
        let mut single = DiffReviewPresentation::new(
            format!("{}-{index}", review.review_id),
            "本轮更改",
            vec![file.clone()],
        );
        single.show_file_tree = false;
        card = card.child(
            div()
                .id(SharedString::from(format!(
                    "{}-file-{index}",
                    review.review_id
                )))
                .h(px(36.0))
                .bg(row_surface)
                .px(px(12.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .cursor_pointer()
                .role(Role::Button)
                .aria_label(format!("审核 {}", file.path))
                .hover(move |s| s.bg(theme.sidebar_hover))
                .on_click(move |_, _, cx| {
                    click_home.update(cx, |_, cx| cx.emit(OpenDiffReview(single.clone())))
                })
                .child(div().flex_1().min_w(px(0.0)).truncate().child(
                    gpui::StyledText::new(file.path.clone()).with_highlights(
                        file.path.rfind('/').map(|index| {
                            (
                                0..index + 1,
                                gpui::HighlightStyle {
                                    color: Some(theme.text_secondary.into()),
                                    ..Default::default()
                                },
                            )
                        }),
                    ),
                ))
                .child(counts(file.additions, file.deletions)),
        );
    }
    if review.files.len() > 3 {
        let id = review.review_id;
        card = card.child(
            div()
                .id(SharedString::from(format!("{id}-expand")))
                .h(px(36.0))
                .bg(row_surface)
                .px(px(12.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .role(Role::Button)
                .aria_label(if expanded {
                    "收起文件列表".to_owned()
                } else {
                    format!("再显示 {} 个文件", review.files.len() - 3)
                })
                .cursor_pointer()
                .hover(move |s| s.bg(theme.sidebar_hover))
                .on_click(move |_, _, cx| {
                    home.update(cx, |home, cx| {
                        if !home.expanded_file_summaries.remove(&id) {
                            home.expanded_file_summaries.insert(id.clone());
                        }
                        home.conversation_list.remeasure();
                        cx.notify();
                    })
                })
                .child(if expanded {
                    "收起文件列表".to_owned()
                } else {
                    format!("再显示 {} 个文件", review.files.len() - 3)
                })
                .child(
                    icon(
                        if expanded {
                            "settings-chevron-up"
                        } else {
                            "chevron-down"
                        },
                        theme.text.into(),
                    )
                    .size(px(12.0)),
                ),
        );
    }
    card
}

pub(super) fn scroll_should_follow_output(scroll_handle: &ScrollHandle) -> bool {
    let max_offset = f32::from(scroll_handle.max_offset().y).max(0.0);
    let offset = f32::from(scroll_handle.offset().y);
    max_offset <= CONVERSATION_BOTTOM_EPSILON || max_offset + offset <= CONVERSATION_BOTTOM_EPSILON
}

pub(super) fn sync_list_item_count(list: &ListState, item_count: usize) {
    let previous_count = list.item_count();
    if item_count > previous_count {
        list.splice(previous_count..previous_count, item_count - previous_count);
    } else if item_count < previous_count {
        list.splice(item_count..previous_count, 0);
    }
}

pub(super) fn conversation_list_state(item_count: usize) -> ListState {
    let state = ListState::new(
        item_count,
        ListAlignment::Top,
        px(CONVERSATION_LIST_OVERDRAW),
    );
    #[cfg(any(feature = "screenshot", test))]
    let state = state.measure_all();
    state.set_follow_mode(FollowMode::Tail);
    state
}

pub(super) fn nested_scroll_consumed(
    scroll_handle: &ScrollHandle,
    event: &ScrollWheelEvent,
    window: &Window,
) -> bool {
    let delta_y = match event.delta {
        ScrollDelta::Pixels(delta) => f32::from(delta.y),
        ScrollDelta::Lines(delta) => f32::from(window.line_height()) * delta.y,
    };
    if delta_y.abs() <= f32::EPSILON {
        return false;
    }

    let max_offset = f32::from(scroll_handle.max_offset().y).max(0.0);
    if max_offset <= CONVERSATION_BOTTOM_EPSILON {
        return false;
    }

    // GPUI registers the built-in scroller after custom wheel listeners, so
    // bubble dispatch runs the built-in listener first. Reconstruct the
    // pre-gesture position to decide whether this nested viewport owned the
    // gesture; at an edge the event keeps bubbling to the conversation.
    let previous_offset = (f32::from(scroll_handle.offset().y) - delta_y).clamp(-max_offset, 0.0);
    if delta_y < 0.0 {
        previous_offset > -max_offset + CONVERSATION_BOTTOM_EPSILON
    } else {
        previous_offset < -CONVERSATION_BOTTOM_EPSILON
    }
}
