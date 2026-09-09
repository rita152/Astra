//! Requests behavior and presentation for the prompt composer.

use gpui::{Context, KeyDownEvent};

use super::{ComposerView, ConversationChanged};
use crate::{
    agent::{
        AgentCommandApprovalChoice, AgentFileApprovalChoice, AgentPermissionsApprovalChoice,
        AgentUserInputAnswer, AgentUserInputResponse,
    },
    components::{
        approval::{
            ApprovalCardEvent, ApprovalCardStatus, ApprovalDecision, ApprovalKeyboardFocus,
            ApprovalMenuItem, ApprovalScope, ApprovalVisualState,
        },
        file_change::{
            FileApprovalDecision, FileApprovalEvent, FileApprovalKeyboardFocus,
            FileApprovalMenuItem, FileApprovalStatus, FileApprovalVisualState,
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
    pub(crate) fn request_cycle(&self) -> u64 {
        self.conversation.cycle
    }
    pub(crate) fn has_visible_request(&self) -> bool {
        self.conversation
            .activities
            .iter()
            .any(|activity| match activity {
                ConversationActivity::Approval(model) => model.should_render(),
                ConversationActivity::FileApproval(model) => model.should_render(),
                ConversationActivity::PermissionsApproval(model) => model.should_render(),
                ConversationActivity::UserInput(model) => model.should_render(),
                _ => false,
            })
    }
    pub(crate) fn set_approval_preview_lines(
        &mut self,
        request_id: &str,
        lines: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(ConversationActivity::Approval(model)) = self.conversation.activities.iter_mut().find(|activity| matches!(activity, ConversationActivity::Approval(model) if model.request_id==request_id)) else { return; };
        if model.preview_line_count != lines {
            model.preview_line_count = lines;
            cx.emit(ConversationChanged);
            cx.notify();
        }
    }

    pub(crate) fn file_approval_review(
        &self,
        request_id: &str,
        file_index: usize,
    ) -> Option<crate::components::file_change::DiffReviewPresentation> {
        let model = self
            .conversation
            .activities
            .iter()
            .find_map(|activity| match activity {
                ConversationActivity::FileApproval(model)
                    if model.request_id == request_id && model.should_render() =>
                {
                    Some(model)
                }
                _ => None,
            })?;
        model.files.get(file_index)?;
        let context = self.conversation.server_request_contexts.get(request_id)?;
        let entry = self
            .conversation
            .file_changes
            .get(&context.item_id)?
            .changes
            .get(file_index)?;
        Some(
            crate::components::file_change::DiffReviewPresentation::from_file_change_entries(
                format!("approval-review-{request_id}-{file_index}"),
                "待审批",
                std::slice::from_ref(entry),
                Some(&self.conversation.cwd),
            ),
        )
    }
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
        if event == ApprovalCardEvent::StopTurn {
            if matches!(&self.conversation.activities[index],ConversationActivity::Approval(model) if model.status==ApprovalCardStatus::Failed)
            {
                self.stop_generation(cx);
            }
            return;
        }
        if matches!(
            &self.conversation.activities[index],
            ConversationActivity::Approval(model) if !model.is_interactive() && !(model.status==ApprovalCardStatus::Failed && matches!(event,ApprovalCardEvent::TogglePreview|ApprovalCardEvent::OpenNetworkDestination))
        ) {
            return;
        }

        match event {
            ApprovalCardEvent::StopTurn => return,
            ApprovalCardEvent::OpenNetworkDestination => {
                if let ConversationActivity::Approval(model) = &self.conversation.activities[index]
                    && let crate::components::approval::ApprovalRequestPresentation::Network {
                        destination,
                        ..
                    } = &model.request
                {
                    cx.open_url(destination);
                }
            }
            ApprovalCardEvent::TogglePreview => {
                if let ConversationActivity::Approval(model) =
                    &mut self.conversation.activities[index]
                {
                    model.preview_expanded = !model.preview_expanded;
                }
            }
            ApprovalCardEvent::Decision(decision) => {
                let choice = match decision {
                    ApprovalDecision::ServerChoice(index) => {
                        let Some(choice) = self
                            .conversation
                            .command_approval_requests
                            .get(request_id)
                            .and_then(|request| request.available_decisions.get(index))
                            .cloned()
                        else {
                            return;
                        };
                        choice
                    }
                    ApprovalDecision::AllowOnce => AgentCommandApprovalChoice::Accept,
                    ApprovalDecision::Decline => AgentCommandApprovalChoice::Decline,
                    ApprovalDecision::Cancel => AgentCommandApprovalChoice::Cancel,
                    ApprovalDecision::AllowScoped(ApprovalScope::SimilarCommands) => {
                        let Some(choice) = self
                            .conversation
                            .command_approval_requests
                            .get(request_id)
                            .and_then(|request| {
                                request.available_decisions.iter().find(|choice| {
                                    matches!(
                                        choice,
                                        AgentCommandApprovalChoice::AcceptWithExecpolicyAmendment(
                                            _
                                        )
                                    )
                                })
                            })
                            .cloned()
                        else {
                            return;
                        };
                        choice
                    }
                    ApprovalDecision::AllowScoped(ApprovalScope::Session) => {
                        AgentCommandApprovalChoice::AcceptForSession
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
                        if let ConversationActivity::Approval(model) =
                            &mut self.conversation.activities[index]
                        {
                            model.status = ApprovalCardStatus::Failed;
                            model.failure_message =
                                Some("无法写入审批响应，请停止此轮次后重试".to_owned());
                        }
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
                            if let ConversationActivity::Approval(model) =
                                &mut self.conversation.activities[index]
                            {
                                model.status = ApprovalCardStatus::Failed;
                                model.failure_message = Some("命令审批响应连接不存在".to_owned());
                            }
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
                                | ApprovalKeyboardFocus::MenuChoice(_)
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
                if let Some(ApprovalMenuItem::Choice(index)) = focused {
                    model.keyboard_focus = Some(ApprovalKeyboardFocus::MenuChoice(index));
                }
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
                    Some(ApprovalKeyboardFocus::MenuChoice(index)) => {
                        model.visual_state = ApprovalVisualState::SplitMenu {
                            focused: Some(ApprovalMenuItem::Choice(index)),
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
        let Some(activity) = self
            .conversation
            .activities
            .iter()
            .find(|activity| activity.shows_request())
        else {
            return false;
        };
        let key = event.keystroke.key.as_str();
        let shift = event.keystroke.modifiers.shift;
        match activity {
            ConversationActivity::Approval(model) => {
                let Some(event) = model.keyboard_event(key, shift) else {
                    return false;
                };
                let id = model.request_id.clone();
                self.handle_approval_card_event(&id, event, cx);
            }
            ConversationActivity::FileApproval(model) => {
                let Some(event) = model.keyboard_event(key, shift) else {
                    return false;
                };
                let id = model.request_id.clone();
                self.handle_file_approval_event(&id, event, cx);
            }
            ConversationActivity::PermissionsApproval(model) => {
                let Some(event) = model.keyboard_event(key, shift) else {
                    return false;
                };
                let id = model.request_id.clone();
                self.handle_permissions_approval_event(&id, event, cx);
            }
            _ => return false,
        }
        true
    }
    pub fn handle_file_approval_event(
        &mut self,
        request_id: &str,
        event: FileApprovalEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .conversation
            .activities
            .iter()
            .position(|activity| matches!(activity, ConversationActivity::FileApproval(model) if model.request_id == request_id))
        else {
            return;
        };
        let ConversationActivity::FileApproval(model) = &self.conversation.activities[index] else {
            unreachable!()
        };
        if event == FileApprovalEvent::StopTurn {
            if model.status == FileApprovalStatus::Failed {
                self.stop_generation(cx);
            }
            return;
        }
        if !model.is_interactive() {
            return;
        }
        if !model.changes_ready && matches!(event, FileApprovalEvent::ToggleMenu) {
            return;
        }

        if let FileApprovalEvent::Decision(decision) = event {
            if matches!(
                decision,
                FileApprovalDecision::AllowOnce | FileApprovalDecision::AllowAllEdits
            ) && !model.changes_ready
            {
                return;
            }
            let choice = match decision {
                FileApprovalDecision::AllowOnce => AgentFileApprovalChoice::Accept,
                FileApprovalDecision::AllowAllEdits => AgentFileApprovalChoice::AcceptForSession,
                FileApprovalDecision::Decline => AgentFileApprovalChoice::Decline,
                FileApprovalDecision::Cancel => AgentFileApprovalChoice::Cancel,
            };
            let response = self
                .conversation
                .file_approval_responders
                .get(request_id)
                .map(|responder| responder.respond(choice));
            let live_request = self
                .conversation
                .server_request_contexts
                .contains_key(request_id);
            let error = match response {
                Some(Ok(())) => None,
                Some(Err(error)) => Some(error),
                None if live_request => Some("文件审批 responder 不存在".to_owned()),
                None => None,
            };
            let ConversationActivity::FileApproval(model) =
                &mut self.conversation.activities[index]
            else {
                unreachable!()
            };
            model.status = if error.is_some() {
                FileApprovalStatus::Failed
            } else {
                FileApprovalStatus::Submitting
            };
            if let Some(error) = error {
                model.failure_message = Some("无法写入文件审批响应，请停止此轮次后重试".to_owned());
                self.conversation
                    .activities
                    .push(ConversationActivity::ProtocolError {
                        message: "无法回复文件审批".to_owned(),
                        details: Some(error),
                        will_retry: false,
                    });
            }
            cx.emit(ConversationChanged);
            cx.notify();
            return;
        }

        let ConversationActivity::FileApproval(model) = &mut self.conversation.activities[index]
        else {
            unreachable!()
        };

        match event {
            FileApprovalEvent::Decision(_) => unreachable!(),
            FileApprovalEvent::StopTurn => return,
            FileApprovalEvent::ReviewFile(_) => return,
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
                if let Some(item) = focused {
                    model.keyboard_focus = Some(match item {
                        FileApprovalMenuItem::AllowOnce => FileApprovalKeyboardFocus::MenuAllowOnce,
                        FileApprovalMenuItem::AllowAllEdits => {
                            FileApprovalKeyboardFocus::MenuAllowAllEdits
                        }
                    });
                }
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
        if !matches!(
            self.conversation
                .activities
                .iter()
                .find(|activity| activity.shows_request()),
            Some(ConversationActivity::UserInput(_))
        ) {
            return false;
        }
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
