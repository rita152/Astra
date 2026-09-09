use super::{
    approvals::parse_command_approval_request,
    dispatch::process_turn_message,
    requests::{handle_server_request_resolved, respond_to_server_request_on_session},
    session::{CodexTurnSession, TurnOutcome, cleanup_pending_server_requests},
};
use crate::agent::{
    AgentCommandApprovalChoice as CommandChoice, AgentCommandApprovalKind, AgentEvent,
    AgentFileApprovalChoice as FileChoice, AgentNetworkPolicyAction, AgentNetworkPolicyAmendment,
    AgentOptionalField, AgentServerRequestFailureKind, AgentServerRequestId,
    AgentServerRequestKind,
};
use serde_json::{Value, json};
use std::{
    io::Write,
    sync::{Arc, Mutex},
};

fn command(id: Value) -> Value {
    json!({"id":id,"method":"item/commandExecution/requestApproval","params":{
        "threadId":"approval-thread","turnId":"approval-turn","itemId":"shared-item","startedAtMs":17
    }})
}
fn file(id: Value) -> Value {
    let mut message = command(id);
    message["method"] = json!("item/fileChange/requestApproval");
    message
}
fn session() -> Arc<CodexTurnSession<Vec<u8>>> {
    let session = Arc::new(CodexTurnSession::new(Vec::new(), None));
    session
        .activate_turn("approval-thread".into(), "approval-turn".into())
        .unwrap();
    session
}
fn output(session: &CodexTurnSession<Vec<u8>>) -> Vec<Value> {
    let bytes = std::mem::take(session.writer.lock().unwrap().as_mut().unwrap());
    String::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn resolved(id: Value) -> Value {
    json!({"method":"serverRequest/resolved","params":{"threadId":"approval-thread","requestId":id}})
}

#[test]
fn command_schema_defaults_nulls_and_stdin_are_decodable_without_losing_callback_identity() {
    let missing = command(json!(7));
    let (_, request, raw, _) = parse_command_approval_request(&missing).unwrap();
    assert_eq!(raw, missing["params"]);
    assert_eq!(request.kind, AgentCommandApprovalKind::Command);
    assert_eq!(request.environment_id, None);
    assert!(request.available_decisions.contains(&CommandChoice::Accept));
    assert_eq!(
        request.additional_permissions,
        AgentOptionalField::Unspecified
    );
    let mut nullable = missing;
    for key in [
        "environmentId",
        "approvalId",
        "command",
        "cwd",
        "reason",
        "networkApprovalContext",
        "commandActions",
        "availableDecisions",
        "additionalPermissions",
        "proposedExecpolicyAmendment",
        "proposedNetworkPolicyAmendments",
    ] {
        nullable["params"][key] = Value::Null;
    }
    let (_, request, raw, _) = parse_command_approval_request(&nullable).unwrap();
    assert_eq!(raw, nullable["params"]);
    assert!(
        request
            .available_decisions
            .contains(&CommandChoice::Decline)
    );
    assert_eq!(request.additional_permissions, AgentOptionalField::Null);
    nullable["params"]["kind"] = json!("writeStdin");
    nullable["params"]["approvalId"] = json!("opaque-callback-not-the-rpc-id");
    nullable["params"]["command"] = json!("echo 🦀\n");
    let (_, request, _, _) = parse_command_approval_request(&nullable).unwrap();
    assert_eq!(request.kind, AgentCommandApprovalKind::WriteStdin);
    assert_eq!(
        request.approval_id.as_deref(),
        Some("opaque-callback-not-the-rpc-id")
    );
    assert_eq!(request.request_id, AgentServerRequestId::Number(7));
    assert_eq!(request.command, "echo 🦀\n");
}

#[test]
fn command_schema_rejects_unknown_kinds_permissions_protocols_and_decisions() {
    for (field, value) in [
        ("kind", json!(null)),
        ("kind", json!("future")),
        ("startedAtMs", json!(1.5)),
        ("approvalId", json!(false)),
        ("environmentId", json!({})),
        ("networkApprovalContext", json!({"host":"example.com"})),
        (
            "networkApprovalContext",
            json!({"host":"example.com","protocol":"ftp"}),
        ),
        ("additionalPermissions", json!({"futurePermission":true})),
        ("availableDecisions", json!(["future"])),
        (
            "availableDecisions",
            json!([{"acceptWithExecpolicyAmendment":{"execpolicy_amendment":[1]}}]),
        ),
        (
            "proposedNetworkPolicyAmendments",
            json!([{"host":"example.com","action":"future"}]),
        ),
    ] {
        let mut message = command(json!(4));
        message["params"][field] = value;
        assert!(
            parse_command_approval_request(&message).is_err(),
            "accepted invalid field {field}"
        );
    }
    let mut message = command(json!(4));
    message["params"]["availableDecisions"] = json!([]);
    assert!(
        parse_command_approval_request(&message)
            .unwrap()
            .1
            .available_decisions
            .is_empty()
    );
}

#[test]
fn advertised_command_decisions_echo_exact_policies_and_reject_unadvertised_grants() {
    let decisions = vec![
        json!("acceptForSession"),
        json!({"acceptWithExecpolicyAmendment":{"execpolicy_amendment":["git","status"]}}),
        json!({"applyNetworkPolicyAmendment":{"network_policy_amendment":{"host":"example.com:443","action":"allow"}}}),
        json!({"applyNetworkPolicyAmendment":{"network_policy_amendment":{"host":"example.com:8443","action":"deny"}}}),
        json!("cancel"),
    ];
    for (index, decision) in decisions.into_iter().enumerate() {
        let session = session();
        let (tx, rx) = async_channel::unbounded();
        let mut message = command(json!(index));
        message["params"]["availableDecisions"] = json!([decision]);
        message["params"]["additionalPermissions"] = json!({"network":{"enabled":true},"fileSystem":{"entries":[{"path":{"type":"path","path":"/tmp/approval-output"},"access":"write"}]}});
        respond_to_server_request_on_session(&session, &message, &tx).unwrap();
        let AgentEvent::CommandApprovalRequested { request, responder } = rx.try_recv().unwrap()
        else {
            panic!("missing command")
        };
        assert!(matches!(
            request.additional_permissions,
            AgentOptionalField::Value(_)
        ));
        assert!(responder.respond(CommandChoice::Accept).is_err());
        assert!(
            responder
                .respond(CommandChoice::ApplyNetworkPolicyAmendment(
                    AgentNetworkPolicyAmendment {
                        host: "other.example".into(),
                        action: AgentNetworkPolicyAction::Allow
                    }
                ))
                .is_err()
        );
        responder
            .respond(request.available_decisions[0].clone())
            .unwrap();
        assert_eq!(
            output(&session),
            vec![json!({"id":index,"result":{"decision":decision}})]
        );
    }
}

#[test]
fn callbacks_sharing_an_item_keep_string_and_numeric_ids_isolated_until_resolved() {
    let session = session();
    let (tx, rx) = async_channel::unbounded();
    for (id, callback) in [(json!(7), "callback-a"), (json!("7"), "callback-b")] {
        let mut message = command(id);
        message["params"]["approvalId"] = json!(callback);
        respond_to_server_request_on_session(&session, &message, &tx).unwrap();
    }
    let AgentEvent::CommandApprovalRequested {
        responder: first,
        request: a,
    } = rx.try_recv().unwrap()
    else {
        panic!()
    };
    let AgentEvent::CommandApprovalRequested {
        responder: second,
        request: b,
    } = rx.try_recv().unwrap()
    else {
        panic!()
    };
    assert_eq!(a.item_id, b.item_id);
    assert_ne!(a.approval_id, b.approval_id);
    first.respond(CommandChoice::Accept).unwrap();
    let mut wrong = resolved(json!(7));
    wrong["params"]["threadId"] = json!("other-thread");
    assert!(handle_server_request_resolved(&session, &wrong, &tx).is_err());
    assert_eq!(session.pending_server_request_snapshot().len(), 2);
    handle_server_request_resolved(&session, &resolved(json!(7)), &tx).unwrap();
    handle_server_request_resolved(&session, &resolved(json!(7)), &tx).unwrap();
    assert!(first.respond(CommandChoice::Decline).is_err());
    assert!(respond_to_server_request_on_session(&session, &command(json!(7)), &tx).is_err());
    second.respond(CommandChoice::Decline).unwrap();
    assert_eq!(
        output(&session),
        vec![
            json!({"id":7,"result":{"decision":"accept"}}),
            json!({"id":"7","result":{"decision":"decline"}})
        ]
    );
    assert_eq!(
        rx.try_recv().unwrap(),
        AgentEvent::ServerRequestResolved {
            request: crate::agent::AgentServerRequestMetadata {
                request_id: AgentServerRequestId::Number(7),
                thread_id: "approval-thread".into(),
                turn_id: "approval-turn".into(),
                item_id: "shared-item".into(),
                kind: AgentServerRequestKind::CommandApproval
            }
        }
    );
    assert!(rx.try_recv().is_err());
}

#[test]
fn file_approval_flows_through_turn_dispatch_and_preserves_patch_until_resolution() {
    let session = session();
    let (tx, rx) = async_channel::unbounded();
    let mut streamed = false;
    let patch = "@@ -1 +1 @@\n-旧文本\n+新文本\n\\ No newline at end of file\n";
    let item = json!({"id":"shared-item","type":"fileChange","status":"inProgress","changes":[{"path":"/tmp/approval.rs","kind":{"type":"update","move_path":"/tmp/renamed.rs"},"diff":patch}]});
    let started = json!({"method":"item/started","params":{"threadId":"approval-thread","turnId":"approval-turn","startedAtMs":17,"item":item}});
    for message in [&started, &file(json!("file-7"))] {
        assert!(
            process_turn_message(
                &session,
                message,
                "approval-thread",
                "approval-turn",
                &tx,
                &mut streamed
            )
            .unwrap()
            .is_none()
        );
    }
    let AgentEvent::FileChangeUpdated(change) = rx.try_recv().unwrap() else {
        panic!("missing original diff")
    };
    assert_eq!(change.changes[0].diff, patch);
    let AgentEvent::FileApprovalRequested { request, responder } = rx.try_recv().unwrap() else {
        panic!("missing file approval")
    };
    assert_eq!(request.item_id, change.id);
    assert!(
        session
            .respond_to_command_approval(&request.request_id, CommandChoice::Accept)
            .is_err()
    );
    responder.respond(FileChoice::AcceptForSession).unwrap();
    assert_eq!(session.pending_server_request_snapshot().len(), 1);
    assert!(session.pending_server_request_snapshot()[0].2);
    assert!(responder.respond(FileChoice::Decline).is_err());
    assert_eq!(
        output(&session),
        vec![json!({"id":"file-7","result":{"decision":"acceptForSession"}})]
    );
    process_turn_message(
        &session,
        &resolved(json!("file-7")),
        "approval-thread",
        "approval-turn",
        &tx,
        &mut streamed,
    )
    .unwrap();
    assert!(session.pending_server_request_snapshot().is_empty());
    assert!(
        matches!(rx.try_recv().unwrap(),AgentEvent::ServerRequestResolved{request} if request.kind==AgentServerRequestKind::FileApproval)
    );
}

#[test]
fn file_choices_and_terminal_cleanup_do_not_alias_decline_with_cancel() {
    for (choice, expected) in [
        (FileChoice::Accept, "accept"),
        (FileChoice::AcceptForSession, "acceptForSession"),
        (FileChoice::Decline, "decline"),
        (FileChoice::Cancel, "cancel"),
    ] {
        let session = session();
        let (tx, rx) = async_channel::unbounded();
        respond_to_server_request_on_session(&session, &file(json!(8)), &tx).unwrap();
        let AgentEvent::FileApprovalRequested { responder, .. } = rx.try_recv().unwrap() else {
            panic!()
        };
        responder.respond(choice).unwrap();
        assert_eq!(output(&session)[0]["result"]["decision"], expected);
        session.mark_terminal();
        cleanup_pending_server_requests(&session, &Ok(TurnOutcome::Interrupted), &tx).unwrap();
        assert!(matches!(
            rx.try_recv().unwrap(),
            AgentEvent::ServerRequestFailed {
                kind: AgentServerRequestFailureKind::Cancelled,
                ..
            }
        ));
        assert!(session.pending_server_request_snapshot().is_empty());
        assert!(responder.respond(FileChoice::Accept).is_err());
    }
}

struct PartialFailure(Arc<Mutex<Vec<u8>>>);
impl Write for PartialFailure {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let mut written = self.0.lock().unwrap();
        let count = bytes.len().min(3usize.saturating_sub(written.len()));
        if count == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "partial approval write",
            ));
        }
        written.extend_from_slice(&bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[test]
fn partial_file_response_failure_is_visible_and_cannot_be_retried_or_reused() {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let session = Arc::new(CodexTurnSession::new(PartialFailure(bytes.clone()), None));
    let (tx, rx) = async_channel::unbounded();
    respond_to_server_request_on_session(&session, &file(json!("partial")), &tx).unwrap();
    let AgentEvent::FileApprovalRequested { responder, .. } = rx.try_recv().unwrap() else {
        panic!()
    };
    assert!(
        responder
            .respond(FileChoice::Accept)
            .unwrap_err()
            .contains("JSON-RPC response")
    );
    let first = bytes.lock().unwrap().clone();
    assert!(!first.is_empty());
    assert!(
        responder
            .respond(FileChoice::Accept)
            .unwrap_err()
            .contains("拒绝重复")
    );
    assert_eq!(*bytes.lock().unwrap(), first);
    session.mark_terminal();
    cleanup_pending_server_requests(&session, &Err(anyhow::anyhow!("transport closed")), &tx)
        .unwrap();
    assert!(matches!(
        rx.try_recv().unwrap(),
        AgentEvent::ServerRequestFailed {
            kind: AgentServerRequestFailureKind::Failed,
            ..
        }
    ));
}
