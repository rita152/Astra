use super::*;
use crate::{
    agent::{
        AgentEvent, AgentFileApprovalChoice, AgentFileApprovalControl, AgentFileApprovalHandle,
        AgentFileApprovalRequest, AgentFileChange, AgentFileChangeEntry, AgentFileChangeKind,
        AgentFileChangeStatus, AgentServerRequestId, AgentServerRequestKind,
        AgentServerRequestMetadata,
    },
    components::file_change::{FileApprovalDecision, FileApprovalEvent, FileApprovalStatus},
    conversation::ConversationActivity,
};
use gpui::TestApp;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct FileReplies {
    choices: Mutex<Vec<AgentFileApprovalChoice>>,
    fail: bool,
}
impl AgentFileApprovalControl for FileReplies {
    fn respond(
        &self,
        _: &AgentServerRequestId,
        choice: AgentFileApprovalChoice,
    ) -> Result<(), String> {
        self.choices.lock().unwrap().push(choice);
        if self.fail {
            Err("模拟写入失败".into())
        } else {
            Ok(())
        }
    }
}
fn request(id: &str, thread: &str) -> AgentFileApprovalRequest {
    AgentFileApprovalRequest {
        request_id: AgentServerRequestId::String(id.into()),
        thread_id: thread.into(),
        turn_id: "approval-turn".into(),
        item_id: "approval-item".into(),
        started_at_ms: 12,
        reason: None,
        grant_root: Some("/tmp/approval-root".into()),
    }
}
fn change(diff: &str) -> AgentFileChange {
    AgentFileChange {
        id: "approval-item".into(),
        status: AgentFileChangeStatus::InProgress,
        changes: vec![
            AgentFileChangeEntry {
                path: "/tmp/approval-root/a.rs".into(),
                kind: AgentFileChangeKind::Update { move_path: None },
                diff: diff.into(),
            },
            AgentFileChangeEntry {
                path: "/tmp/approval-root/b.rs".into(),
                kind: AgentFileChangeKind::Add,
                diff: "+SECOND_ORIGINAL\n".into(),
            },
        ],
    }
}

#[test]
fn file_approval_waits_for_its_original_diff_and_replies_once_until_server_resolution() {
    let mut app = TestApp::new();
    let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
    let control = Arc::new(FileReplies::default());
    let request = request("file-ui", "thread-a");
    let key = request.request_id.ui_key();
    let original = "@@ -1 +1 @@\n-ORIGINAL_BEFORE\n+ORIGINAL_AFTER\n";
    app.update_entity(&composer,|composer,cx| {
        composer.conversation.begin_prompt("修改测试文件");composer.conversation.thread_id=Some("thread-a".into());
        composer.apply_agent_event_batch(vec![AgentEvent::FileApprovalRequested {request:request.clone(),responder:AgentFileApprovalHandle::new(request.request_id.clone(),control.clone())}]);
        composer.handle_file_approval_event(&key,FileApprovalEvent::Decision(FileApprovalDecision::AllowOnce),cx);
        assert!(control.choices.lock().unwrap().is_empty(),"must not approve an unseen patch");
        composer.apply_agent_event_batch(vec![AgentEvent::FileChangeUpdated(change(original))]);
        let review=composer.file_approval_review(&key,0).unwrap();
        assert_eq!(review.files.len(),1);
        assert!(review.raw_diff.as_deref().unwrap().contains(original));
        let second=composer.file_approval_review(&key,1).unwrap();
        assert_eq!(second.files.len(),1);
        assert!(second.raw_diff.as_deref().unwrap().contains("SECOND_ORIGINAL"));
        assert!(!second.raw_diff.as_deref().unwrap().contains("ORIGINAL_BEFORE"));
        composer.apply_agent_event_batch(vec![AgentEvent::TurnDiffUpdated {diff:"diff --git a/unrelated b/unrelated\n--- a/unrelated\n+++ b/unrelated\n@@ -1 +1 @@\n-OTHER\n+OTHER_NEW\n".into()}]);
        assert_eq!(composer.file_approval_review(&key,0).unwrap().raw_diff,review.raw_diff);
        let model=composer.conversation.activities.iter().find_map(|a|if let ConversationActivity::FileApproval(m)=a {Some(m)} else {None}).unwrap();
        assert_eq!(model.files[0].path,"/tmp/approval-root/a.rs");
        composer.handle_file_approval_event(&key,FileApprovalEvent::Decision(FileApprovalDecision::AllowAllEdits),cx);
        composer.handle_file_approval_event(&key,FileApprovalEvent::Decision(FileApprovalDecision::Decline),cx);
        assert_eq!(*control.choices.lock().unwrap(),vec![AgentFileApprovalChoice::AcceptForSession]);
        assert!(composer.conversation.file_approval_responders.contains_key(&key));
        assert!(composer.conversation.server_request_contexts.contains_key(&key));
        assert!(composer.conversation.activities.iter().any(|a|matches!(a,ConversationActivity::FileApproval(m) if m.status==FileApprovalStatus::Submitting)));
        composer.apply_agent_event_batch(vec![AgentEvent::ServerRequestResolved {request:AgentServerRequestMetadata {request_id:request.request_id.clone(),thread_id:request.thread_id.clone(),turn_id:request.turn_id.clone(),item_id:request.item_id.clone(),kind:AgentServerRequestKind::FileApproval}}]);
        assert!(!composer.conversation.file_approval_responders.contains_key(&key));assert!(!composer.conversation.server_request_contexts.contains_key(&key));
    });
}

#[test]
fn file_approval_failure_stays_visible_without_allowing_retry_and_terminal_clears_ownership() {
    let mut app = TestApp::new();
    let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
    let control = Arc::new(FileReplies {
        fail: true,
        ..Default::default()
    });
    let request = request("failure-ui", "thread-a");
    let key = request.request_id.ui_key();
    app.update_entity(&composer,|composer,cx| {
        composer.conversation.begin_prompt("修改测试文件");composer.conversation.thread_id=Some("thread-a".into());
        composer.apply_agent_event_batch(vec![AgentEvent::FileChangeUpdated(change("+ORIGINAL\n")),AgentEvent::FileApprovalRequested {request:request.clone(),responder:AgentFileApprovalHandle::new(request.request_id.clone(),control.clone())}]);
        for _ in 0..2 {composer.handle_file_approval_event(&key,FileApprovalEvent::Decision(FileApprovalDecision::AllowOnce),cx);}
        assert_eq!(control.choices.lock().unwrap().len(),1);
        assert!(composer.conversation.activities.iter().any(|a| matches!(a,ConversationActivity::FileApproval(m) if m.status==FileApprovalStatus::Failed && m.failure_message.is_some() && m.should_render())));
        assert!(composer.conversation.activities.iter().any(|a|matches!(a,ConversationActivity::ProtocolError {details:Some(error),..} if error.contains("模拟写入失败"))));
        composer.handle_file_approval_event(&key,FileApprovalEvent::StopTurn,cx);
        assert!(composer.conversation.server_request_contexts.is_empty());assert!(composer.conversation.file_approval_responders.is_empty());assert!(composer.conversation.file_changes.is_empty());
        assert!(!composer.conversation.activities.iter().any(|a|matches!(a,ConversationActivity::FileApproval(m) if m.should_render())));
        composer.handle_file_approval_event(&key,FileApprovalEvent::Decision(FileApprovalDecision::AllowOnce),cx);
        assert_eq!(control.choices.lock().unwrap().len(),1);
    });
}

#[test]
fn file_approvals_are_isolated_from_other_conversations_and_new_turns() {
    let mut app = TestApp::new();
    let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
    let control = Arc::new(FileReplies::default());
    app.update_entity(&composer, |composer, _| {
        composer.conversation.begin_prompt("当前会话");
        composer.conversation.thread_id = Some("thread-a".into());
        let foreign = request("same-id", "thread-b");
        composer.apply_agent_event_batch(vec![AgentEvent::FileApprovalRequested {
            responder: AgentFileApprovalHandle::new(foreign.request_id.clone(), control.clone()),
            request: foreign,
        }]);
        assert!(composer.conversation.file_approval_responders.is_empty());
        assert!(composer.conversation.server_request_contexts.is_empty());
        let own = request("same-id", "thread-a");
        composer.apply_agent_event_batch(vec![
            AgentEvent::FileChangeUpdated(change("+OLD_TURN\n")),
            AgentEvent::FileApprovalRequested {
                responder: AgentFileApprovalHandle::new(own.request_id.clone(), control.clone()),
                request: own,
            },
        ]);
        assert_eq!(composer.conversation.file_approval_responders.len(), 1);
        composer.conversation.begin_prompt("下一轮");
        assert!(composer.conversation.file_approval_responders.is_empty());
        assert!(composer.conversation.server_request_contexts.is_empty());
        assert!(composer.conversation.file_changes.is_empty());
    });
}

#[derive(Default)]
struct CommandReplies(Mutex<Vec<crate::agent::AgentCommandApprovalChoice>>);
impl crate::agent::AgentApprovalControl for CommandReplies {
    fn respond(
        &self,
        _: &AgentServerRequestId,
        choice: crate::agent::AgentCommandApprovalChoice,
    ) -> Result<(), String> {
        self.0.lock().unwrap().push(choice);
        Ok(())
    }
}

#[test]
fn keyboard_answers_only_the_first_visible_request_even_when_a_hidden_command_is_pending() {
    use crate::agent::{
        AgentApprovalHandle, AgentCommandApprovalChoice, AgentCommandApprovalKind,
        AgentCommandApprovalRequest, AgentOptionalField,
    };
    let mut app = TestApp::new();
    let composer = app.new_entity(|cx| ComposerView::new(ThemeMode::Dark, cx));
    let files = Arc::new(FileReplies {
        fail: true,
        ..Default::default()
    });
    let commands = Arc::new(CommandReplies::default());
    let file = request("first-file", "thread-a");
    let command = AgentCommandApprovalRequest {
        request_id: AgentServerRequestId::Number(7),
        thread_id: "thread-a".into(),
        turn_id: "approval-turn".into(),
        item_id: "command-item".into(),
        approval_id: None,
        kind: AgentCommandApprovalKind::Command,
        environment_id: None,
        started_at_ms: 17,
        cwd: None,
        command: "echo hidden".into(),
        reason: None,
        network: None,
        additional_permissions: AgentOptionalField::Unspecified,
        available_decisions: vec![
            AgentCommandApprovalChoice::Accept,
            AgentCommandApprovalChoice::Decline,
        ],
    };
    app.update_entity(&composer, |composer, cx| {
        composer.conversation.begin_prompt("并发审批");
        composer.conversation.thread_id = Some("thread-a".into());
        composer.apply_agent_event_batch(vec![
            AgentEvent::FileChangeUpdated(change("+VISIBLE_FILE\n")),
            AgentEvent::FileApprovalRequested {
                responder: AgentFileApprovalHandle::new(file.request_id.clone(), files.clone()),
                request: file,
            },
            AgentEvent::CommandApprovalRequested {
                responder: AgentApprovalHandle::new(command.request_id.clone(), commands.clone()),
                request: command,
            },
        ]);
        let key = gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse("enter").unwrap(),
            is_held: false,
            prefer_character_input: false,
        };
        assert!(composer.handle_approval_key(&key, cx));
        assert_eq!(
            *files.choices.lock().unwrap(),
            vec![AgentFileApprovalChoice::Accept]
        );
        assert!(commands.0.lock().unwrap().is_empty());
        assert!(!composer.handle_approval_key(&key, cx));
        assert!(
            commands.0.lock().unwrap().is_empty(),
            "failed visible card must not answer the command behind it"
        );
    });
}
