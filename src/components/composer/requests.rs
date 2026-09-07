//! Requests behavior and presentation for the prompt composer.

use gpui::{Context, KeyDownEvent};

use super::{ComposerView, ConversationChanged};
use crate::{
    agent::{
        AgentCommandApprovalChoice, AgentPermissionsApprovalChoice, AgentUserInputAnswer,
        AgentUserInputResponse,
    },
    components::{
        approval::{
            ApprovalCardEvent, ApprovalCardStatus, ApprovalDecision, ApprovalKeyboardFocus,
            ApprovalMenuItem, ApprovalScope, ApprovalVisualState,
        },
        file_change::{
            FileApprovalEvent, FileApprovalKeyboardFocus, FileApprovalMenuItem, FileApprovalStatus,
            FileApprovalVisualState,
        },
        permissions_approval::{
            PermissionApprovalDecision, PermissionApprovalEvent, PermissionApprovalHover,
            PermissionApprovalKeyboardFocus, PermissionApprovalMenuItem, PermissionApprovalStatus,
            PermissionApprovalVisualState,
        },
        user_input_request::{
            UserInputKeyboardOutcome, UserInputRequestEvent, UserInputRequestStatus,
        },
    },
    conversation::ConversationActivity,
};

impl ComposerView {
    pub fn handle_approval_card_event(
        &mut self,
        request_id: &str,
        event: ApprovalCardEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.conversation.activities.iter().position(|activity| {
            matches!(
                activity,
                ConversationActivity::Approval(model) if model.request_id == request_id
            )
        }) else {
            return;
        };
        if matches!(
            &self.conversation.activities[index],
            ConversationActivity::Approval(model) if !model.should_render()
        ) {
            return;
        }

        match event {
            ApprovalCardEvent::Decision(decision) => {
                let choice = match decision {
                    ApprovalDecision::AllowOnce => AgentCommandApprovalChoice::Accept,
                    ApprovalDecision::Decline => AgentCommandApprovalChoice::Decline,
                    ApprovalDecision::AllowScoped(ApprovalScope::SimilarCommands) => {
                        AgentCommandApprovalChoice::AcceptWithExecpolicyAmendment
                    }
                    ApprovalDecision::AllowScoped(_) => return,
                };
                let response = self
                    .conversation
                    .approval_responders
                    .get(request_id)
                    .map(|responder| responder.respond(choice));
                match response {
                    Some(Ok(())) => {
                        // Keep the activity until serverRequest/resolved so the
                        // server remains authoritative, while immediately
                        // unmounting the card and blocking duplicate clicks.
                        if let ConversationActivity::Approval(model) =
                            &mut self.conversation.activities[index]
                        {
                            model.status = ApprovalCardStatus::Submitting;
                        }
                    }
                    Some(Err(error)) => {
                        self.conversation
                            .activities
                            .push(ConversationActivity::ProtocolError {
                                message: "无法回复命令审批".to_owned(),
                                details: Some(error),
                                will_retry: false,
                            });
                    }
                    None => {
                        if self
                            .conversation
                            .server_request_contexts
                            .contains_key(request_id)
                        {
                            self.conversation.activities.push(
                                ConversationActivity::ProtocolError {
                                    message: "无法回复命令审批".to_owned(),
                                    details: Some("命令审批 responder 不存在".to_owned()),
                                    will_retry: false,
                                },
                            );
                        } else if let ConversationActivity::Approval(model) =
                            &mut self.conversation.activities[index]
                        {
                            model.status = ApprovalCardStatus::Submitting;
                        }
                    }
                }
            }
            ApprovalCardEvent::ToggleMenu => {
                let ConversationActivity::Approval(model) =
                    &mut self.conversation.activities[index]
                else {
                    return;
                };
                model.visual_state = if model.visual_state.menu_open() {
                    if matches!(
                        model.keyboard_focus,
                        Some(
                            ApprovalKeyboardFocus::MenuAllowOnce
                                | ApprovalKeyboardFocus::MenuScoped(_)
                        )
                    ) {
                        model.keyboard_focus = Some(ApprovalKeyboardFocus::MenuToggle);
                    }
                    ApprovalVisualState::Default
                } else {
                    ApprovalVisualState::SplitMenu { focused: None }
                };
            }
            ApprovalCardEvent::MenuFocusChanged(focused) => {
                let ConversationActivity::Approval(model) =
                    &mut self.conversation.activities[index]
                else {
                    return;
                };
                model.visual_state = ApprovalVisualState::SplitMenu { focused };
            }
            ApprovalCardEvent::KeyboardFocusChanged(focused) => {
                let ConversationActivity::Approval(model) =
                    &mut self.conversation.activities[index]
                else {
                    return;
                };
                model.keyboard_focus = focused;
                match focused {
                    Some(ApprovalKeyboardFocus::MenuAllowOnce) => {
                        model.visual_state = ApprovalVisualState::SplitMenu {
                            focused: Some(ApprovalMenuItem::AllowOnce),
                        };
                    }
                    Some(ApprovalKeyboardFocus::MenuScoped(scope)) => {
                        model.visual_state = ApprovalVisualState::SplitMenu {
                            focused: Some(ApprovalMenuItem::Scoped(scope)),
                        };
                    }
                    _ => {}
                }
            }
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }
    pub fn handle_permissions_approval_event(
        &mut self,
        request_id: &str,
        event: PermissionApprovalEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.conversation.activities.iter().position(|activity| {
            matches!(
                activity,
                ConversationActivity::PermissionsApproval(model)
                    if model.request_id == request_id
            )
        }) else {
            return;
        };
        if !matches!(
            &self.conversation.activities[index],
            ConversationActivity::PermissionsApproval(model) if model.is_interactive()
        ) {
            return;
        }

        match event {
            PermissionApprovalEvent::Decision(decision) => {
                let choice = match decision {
                    PermissionApprovalDecision::AllowOnce => {
                        AgentPermissionsApprovalChoice::AllowOnce
                    }
                    PermissionApprovalDecision::AllowForConversation => {
                        AgentPermissionsApprovalChoice::AllowForSession
                    }
                    PermissionApprovalDecision::Decline => AgentPermissionsApprovalChoice::Decline,
                };
                let response = self
                    .conversation
                    .permissions_approval_responders
                    .get(request_id)
                    .map(|responder| responder.respond(choice));
                match response {
                    Some(Ok(())) => {
                        let ConversationActivity::PermissionsApproval(model) =
                            &mut self.conversation.activities[index]
                        else {
                            unreachable!("activity kind was checked above")
                        };
                        model.status = if decision == PermissionApprovalDecision::Decline {
                            PermissionApprovalStatus::Declined
                        } else {
                            PermissionApprovalStatus::Approved
                        };
                    }
                    Some(Err(error)) => {
                        let ConversationActivity::PermissionsApproval(model) =
                            &mut self.conversation.activities[index]
                        else {
                            unreachable!("activity kind was checked above")
                        };
                        model.status = PermissionApprovalStatus::Failed;
                        model.failure_message = Some("无法写入权限审批响应".to_owned());
                        self.conversation
                            .activities
                            .push(ConversationActivity::ProtocolError {
                                message: "无法回复权限审批".to_owned(),
                                details: Some(error),
                                will_retry: false,
                            });
                    }
                    None => {
                        let ConversationActivity::PermissionsApproval(model) =
                            &mut self.conversation.activities[index]
                        else {
                            unreachable!("activity kind was checked above")
                        };
                        if self
                            .conversation
                            .server_request_contexts
                            .contains_key(request_id)
                        {
                            model.status = PermissionApprovalStatus::Failed;
                            model.failure_message = Some("权限审批 responder 不存在".to_owned());
                            self.conversation.activities.push(
                                ConversationActivity::ProtocolError {
                                    message: "无法回复权限审批".to_owned(),
                                    details: Some("权限审批 responder 不存在".to_owned()),
                                    will_retry: false,
                                },
                            );
                        } else {
                            model.status = if decision == PermissionApprovalDecision::Decline {
                                PermissionApprovalStatus::Declined
                            } else {
                                PermissionApprovalStatus::Approved
                            };
                        }
                    }
                }
            }
            PermissionApprovalEvent::ToggleMenu => {
                let ConversationActivity::PermissionsApproval(model) =
                    &mut self.conversation.activities[index]
                else {
                    unreachable!("activity kind was checked above")
                };
                model.visual_state = if model.visual_state.menu_open() {
                    if matches!(
                        model.keyboard_focus,
                        Some(
                            PermissionApprovalKeyboardFocus::MenuAllowOnce
                                | PermissionApprovalKeyboardFocus::MenuAllowForConversation
                        )
                    ) {
                        model.keyboard_focus = Some(PermissionApprovalKeyboardFocus::MenuToggle);
                    }
                    PermissionApprovalVisualState::Default
                } else {
                    PermissionApprovalVisualState::Menu { focused: None }
                };
            }
            PermissionApprovalEvent::HoverChanged(hovered) => {
                let ConversationActivity::PermissionsApproval(model) =
                    &mut self.conversation.activities[index]
                else {
                    unreachable!("activity kind was checked above")
                };
                if !model.visual_state.menu_open() {
                    model.visual_state = match hovered {
                        Some(PermissionApprovalHover::Allow) => {
                            PermissionApprovalVisualState::AllowHovered
                        }
                        Some(PermissionApprovalHover::Decline) => {
                            PermissionApprovalVisualState::DeclineHovered
                        }
                        None => PermissionApprovalVisualState::Default,
                    };
                }
            }
            PermissionApprovalEvent::MenuFocusChanged(focused) => {
                let ConversationActivity::PermissionsApproval(model) =
                    &mut self.conversation.activities[index]
                else {
                    unreachable!("activity kind was checked above")
                };
                model.visual_state = PermissionApprovalVisualState::Menu { focused };
            }
            PermissionApprovalEvent::KeyboardFocusChanged(focused) => {
                let ConversationActivity::PermissionsApproval(model) =
                    &mut self.conversation.activities[index]
                else {
                    unreachable!("activity kind was checked above")
                };
                model.keyboard_focus = focused;
                match focused {
                    Some(PermissionApprovalKeyboardFocus::MenuAllowOnce) => {
                        model.visual_state = PermissionApprovalVisualState::Menu {
                            focused: Some(PermissionApprovalMenuItem::AllowOnce),
                        };
                    }
                    Some(PermissionApprovalKeyboardFocus::MenuAllowForConversation) => {
                        model.visual_state = PermissionApprovalVisualState::Menu {
                            focused: Some(PermissionApprovalMenuItem::AllowForConversation),
                        };
                    }
                    _ => {}
                }
            }
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }
    pub fn handle_approval_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        if let Some((request_id, card_event)) =
            self.conversation.activities.iter().find_map(|activity| {
                let ConversationActivity::Approval(model) = activity else {
                    return None;
                };
                model.should_render().then(|| {
                    model
                        .keyboard_event(
                            event.keystroke.key.as_str(),
                            event.keystroke.modifiers.shift,
                        )
                        .map(|card_event| (model.request_id.clone(), card_event))
                })?
            })
        {
            self.handle_approval_card_event(&request_id, card_event, cx);
            return true;
        }

        if let Some((request_id, card_event)) =
            self.conversation.activities.iter().find_map(|activity| {
                let ConversationActivity::PermissionsApproval(model) = activity else {
                    return None;
                };
                model.should_render().then(|| {
                    model
                        .keyboard_event(
                            event.keystroke.key.as_str(),
                            event.keystroke.modifiers.shift,
                        )
                        .map(|card_event| (model.request_id.clone(), card_event))
                })?
            })
        {
            self.handle_permissions_approval_event(&request_id, card_event, cx);
            return true;
        }

        let Some((request_id, card_event)) =
            self.conversation.activities.iter().find_map(|activity| {
                let ConversationActivity::FileApproval(model) = activity else {
                    return None;
                };
                model.should_render().then(|| {
                    model
                        .keyboard_event(
                            event.keystroke.key.as_str(),
                            event.keystroke.modifiers.shift,
                        )
                        .map(|card_event| (model.request_id.clone(), card_event))
                })?
            })
        else {
            return false;
        };
        self.handle_file_approval_event(&request_id, card_event, cx);
        true
    }
    pub fn handle_file_approval_event(
        &mut self,
        request_id: &str,
        event: FileApprovalEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(model) = self
            .conversation
            .activities
            .iter_mut()
            .find_map(|activity| {
                let ConversationActivity::FileApproval(model) = activity else {
                    return None;
                };
                (model.request_id == request_id).then_some(model)
            })
        else {
            return;
        };

        match event {
            FileApprovalEvent::Decision(_) => model.status = FileApprovalStatus::Resolved,
            FileApprovalEvent::ToggleMenu => {
                model.visual_state = if model.visual_state.menu_open() {
                    if matches!(
                        model.keyboard_focus,
                        Some(
                            FileApprovalKeyboardFocus::MenuAllowOnce
                                | FileApprovalKeyboardFocus::MenuAllowAllEdits
                        )
                    ) {
                        model.keyboard_focus = Some(FileApprovalKeyboardFocus::MenuToggle);
                    }
                    FileApprovalVisualState::Default
                } else {
                    FileApprovalVisualState::SplitMenu { focused: None }
                };
            }
            FileApprovalEvent::MenuFocusChanged(focused) => {
                model.visual_state = FileApprovalVisualState::SplitMenu { focused };
            }
            FileApprovalEvent::KeyboardFocusChanged(focused) => {
                model.keyboard_focus = focused;
                match focused {
                    Some(FileApprovalKeyboardFocus::MenuAllowOnce) => {
                        model.visual_state = FileApprovalVisualState::SplitMenu {
                            focused: Some(FileApprovalMenuItem::AllowOnce),
                        };
                    }
                    Some(FileApprovalKeyboardFocus::MenuAllowAllEdits) => {
                        model.visual_state = FileApprovalVisualState::SplitMenu {
                            focused: Some(FileApprovalMenuItem::AllowAllEdits),
                        };
                    }
                    _ => {}
                }
            }
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }
    pub fn handle_user_input_request_event(
        &mut self,
        request_id: &str,
        event: UserInputRequestEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.conversation.activities.iter().position(|activity| {
            matches!(activity, ConversationActivity::UserInput(model) if model.request_id == request_id)
        }) else {
            return;
        };
        if !matches!(
            &self.conversation.activities[index],
            ConversationActivity::UserInput(model) if model.is_interactive()
        ) {
            return;
        }

        let mut submit = false;
        let mut dismiss = false;
        let mut mismatch = None;
        let input_configuration = {
            let ConversationActivity::UserInput(model) = &mut self.conversation.activities[index]
            else {
                unreachable!("activity kind was checked above")
            };
            let current_question_id = model.current_question().map(|question| question.id.clone());
            match event {
                UserInputRequestEvent::SelectOption {
                    question_id,
                    option_index,
                    label,
                } => {
                    if current_question_id.as_deref() != Some(question_id.as_str()) {
                        mismatch = Some(format!(
                            "选择事件 question id `{question_id}` 与当前 question id {:?} 不一致",
                            current_question_id
                        ));
                    } else {
                        model.save_selected_option(option_index, label);
                        submit = !model.is_multi_question() || !model.next_question();
                    }
                }
                UserInputRequestEvent::BeginOtherAnswer { question_id, .. } => {
                    if current_question_id.as_deref() != Some(question_id.as_str()) {
                        mismatch = Some(format!(
                            "Other 事件 question id `{question_id}` 与当前 question id {:?} 不一致",
                            current_question_id
                        ));
                    } else {
                        model.visual_state.active_option_index = None;
                        model.focus_other_answer();
                    }
                }
                UserInputRequestEvent::SubmitOtherAnswer {
                    question_id,
                    answer,
                } => {
                    if current_question_id.as_deref() != Some(question_id.as_str()) {
                        mismatch = Some(format!(
                            "Other 提交 question id `{question_id}` 与当前 question id {:?} 不一致",
                            current_question_id
                        ));
                    } else {
                        model.save_other_answer(answer);
                        submit = !model.is_multi_question() || !model.next_question();
                    }
                }
                UserInputRequestEvent::Skip => {
                    model.skip_current_question();
                    submit = !model.is_multi_question() || !model.next_question();
                }
                UserInputRequestEvent::PreviousQuestion => {
                    model.persist_current_answer();
                    model.previous_question();
                }
                UserInputRequestEvent::NextQuestion => {
                    model.persist_current_answer();
                    submit = !model.next_question();
                }
                UserInputRequestEvent::Dismiss => {
                    submit = true;
                    dismiss = true;
                }
                UserInputRequestEvent::ActiveOptionChanged(index) => {
                    model.visual_state.active_option_index = index.or(model.selected_option_index);
                }
            }

            model.current_question().map(|question| {
                (
                    question.other_placeholder.clone(),
                    question.is_secret,
                    model.other_answer.clone(),
                )
            })
        };
        if let Some(details) = mismatch {
            self.conversation
                .activities
                .push(ConversationActivity::ProtocolError {
                    message: "用户输入请求事件标识不一致".to_owned(),
                    details: Some(details),
                    will_retry: false,
                });
            cx.emit(ConversationChanged);
            cx.notify();
            return;
        }
        if submit {
            let answers = if dismiss {
                Vec::new()
            } else {
                let ConversationActivity::UserInput(model) = &self.conversation.activities[index]
                else {
                    unreachable!("activity kind was checked above")
                };
                model
                    .response_answers()
                    .into_iter()
                    .map(|(question_id, answers)| AgentUserInputAnswer {
                        question_id,
                        answers,
                    })
                    .collect()
            };
            let response = self
                .conversation
                .user_input_responders
                .get(request_id)
                .map(|responder| responder.respond(AgentUserInputResponse { answers }));
            match response {
                Some(Ok(())) => {
                    let ConversationActivity::UserInput(model) =
                        &mut self.conversation.activities[index]
                    else {
                        unreachable!("activity kind was checked above")
                    };
                    model.status = UserInputRequestStatus::Submitting;
                }
                Some(Err(error)) => {
                    let ConversationActivity::UserInput(model) =
                        &mut self.conversation.activities[index]
                    else {
                        unreachable!("activity kind was checked above")
                    };
                    model.status = UserInputRequestStatus::Failed;
                    model.failure_message = Some("无法写入用户输入响应".to_owned());
                    self.conversation
                        .activities
                        .push(ConversationActivity::ProtocolError {
                            message: "无法回复用户输入请求".to_owned(),
                            details: Some(error),
                            will_retry: false,
                        });
                }
                None => {
                    let ConversationActivity::UserInput(model) =
                        &mut self.conversation.activities[index]
                    else {
                        unreachable!("activity kind was checked above")
                    };
                    if self
                        .conversation
                        .server_request_contexts
                        .contains_key(request_id)
                    {
                        model.status = UserInputRequestStatus::Failed;
                        model.failure_message = Some("用户输入 responder 不存在".to_owned());
                        self.conversation
                            .activities
                            .push(ConversationActivity::ProtocolError {
                                message: "无法回复用户输入请求".to_owned(),
                                details: Some("用户输入 responder 不存在".to_owned()),
                                will_retry: false,
                            });
                    } else {
                        model.status = UserInputRequestStatus::Submitting;
                    }
                }
            }
        }
        if let Some((placeholder, secret, answer)) = input_configuration {
            self.user_input_other_input.update(cx, |input, cx| {
                input.configure_inline_other(placeholder, secret, cx);
                input.set_text_silently(answer, cx);
            });
        }
        cx.emit(ConversationChanged);
        cx.notify();
    }
    pub fn handle_user_input_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let modifiers = event.keystroke.modifiers;
        let outcome = self
            .conversation
            .activities
            .iter_mut()
            .find_map(|activity| {
                let ConversationActivity::UserInput(model) = activity else {
                    return None;
                };
                if !model.should_render() {
                    return None;
                }
                let request_id = model.request_id.clone();
                model
                    .keyboard_event(
                        event.keystroke.key.as_str(),
                        event.keystroke.key_char.as_deref(),
                        modifiers.shift,
                        modifiers.platform,
                        modifiers.control,
                    )
                    .map(|outcome| (request_id, outcome))
            });
        let Some((request_id, outcome)) = outcome else {
            return false;
        };

        match outcome {
            UserInputKeyboardOutcome::Handled => {
                cx.emit(ConversationChanged);
                cx.notify();
            }
            UserInputKeyboardOutcome::PreviousQuestion => self.handle_user_input_request_event(
                &request_id,
                UserInputRequestEvent::PreviousQuestion,
                cx,
            ),
            UserInputKeyboardOutcome::NextQuestion => self.handle_user_input_request_event(
                &request_id,
                UserInputRequestEvent::NextQuestion,
                cx,
            ),
            UserInputKeyboardOutcome::SubmitOption {
                question_id,
                option_index,
                label,
            } => self.handle_user_input_request_event(
                &request_id,
                UserInputRequestEvent::SelectOption {
                    question_id,
                    option_index,
                    label,
                },
                cx,
            ),
            UserInputKeyboardOutcome::SubmitOther {
                question_id,
                answer,
            } => self.handle_user_input_request_event(
                &request_id,
                UserInputRequestEvent::SubmitOtherAnswer {
                    question_id,
                    answer,
                },
                cx,
            ),
            UserInputKeyboardOutcome::Skip => {
                self.handle_user_input_request_event(&request_id, UserInputRequestEvent::Skip, cx)
            }
            UserInputKeyboardOutcome::Dismiss => self.handle_user_input_request_event(
                &request_id,
                UserInputRequestEvent::Dismiss,
                cx,
            ),
        }
        true
    }
}
