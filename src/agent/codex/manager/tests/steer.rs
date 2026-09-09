use super::*;
use crate::agent::{AgentInputFile, AgentPromptContext, AgentSteerRequest, AgentTurnIdentity};

fn ready(receiver: &async_channel::Receiver<AgentEvent>) -> AgentTurnIdentity {
    loop {
        if let AgentEvent::TurnReady(id) = wait_value(receiver) {
            return id;
        }
    }
}
fn steer(target: &AgentTurnIdentity, id: &str) -> AgentSteerRequest {
    AgentSteerRequest {
        target: target.clone(),
        client_message_id: id.into(),
        prompt: format!("input {id}"),
        context: Default::default(),
    }
}
fn user(endpoint: &FakeEndpoint, target: &AgentTurnIdentity, id: &str) {
    for method in ["item/started", "item/completed", "item/completed"] {
        endpoint.send(json!({"method":method,"params":{"threadId":target.thread_id,"turnId":target.turn_id,
            "item":{"type":"userMessage","id":format!("item-{id}"),"clientId":id,"content":[{"type":"text","text":format!("input {id}")} ]}}}));
    }
}
#[test]
fn steer_parameters_share_input_encoding_and_exclude_turn_overrides() {
    let target = AgentTurnIdentity {
        generation: 1,
        thread_id: "t".into(),
        turn_id: "r".into(),
    };
    let mut request = steer(&target, "client-1");
    request.context = AgentPromptContext {
        plan_mode: Some(true),
        files: vec![
            AgentInputFile {
                path: "/tmp/file.rs".into(),
                image: false,
            },
            AgentInputFile {
                path: "/tmp/image.png".into(),
                image: true,
            },
        ],
    };
    let params = super::super::steer::build_steer_params(&request).unwrap();
    let mut start = super::request(&request.prompt, Some("t"));
    start.context = request.context.clone();
    let start = super::super::turn::build_turn_start_params(&start, "t", false).unwrap();
    assert_eq!(params["input"], start["input"]);
    assert_eq!(params.as_object().unwrap().len(), 4);
    assert_eq!(params["clientUserMessageId"], "client-1");
    assert_eq!(
        params["input"][1],
        json!({"type":"localImage","path":"/tmp/image.png"})
    );
    for field in [
        "model",
        "effort",
        "serviceTier",
        "cwd",
        "sandboxPolicy",
        "collaborationMode",
    ] {
        assert!(params.get(field).is_none(), "{field}");
    }
}

#[test]
fn steer_early_duplicate_events_out_of_order_and_late_responses_do_not_restart_turn() {
    let (manager, spawner) = manager_with_fake();
    let (events, _handle) = manager
        .run_prompt(request("initial", Some("t")))
        .into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "t", "r");
    let target = ready(&events);
    let first = manager.steer_turn(steer(&target, "one"));
    let a = endpoint.recv();
    assert_eq!(a["method"], "turn/steer");
    let second = manager.steer_turn(steer(&target, "two"));
    let b = endpoint.recv();
    assert_ne!(a["id"], b["id"]);
    user(&endpoint, &target, "one");
    user(&endpoint, &target, "two");
    endpoint.respond(&b, json!({"turnId":"r"}));
    assert!(wait_value(&second).is_ok());
    endpoint.respond(&b, json!({"turnId":"r"}));
    endpoint.send(json!({"method":"item/agentMessage/delta","params":{"threadId":"t","turnId":"r","itemId":"answer","delta":"continued"}}));
    complete(&endpoint, "t", "r", "completed");
    let received = collect_terminal(&events);
    assert!(
        received
            .iter()
            .any(|e| matches!(e,AgentEvent::TextDelta(t) if t == "continued"))
    );
    endpoint.respond(&a, json!({"turnId":"r"}));
    assert!(wait_value(&first).is_ok());
    assert!(wait_value(&manager.steer_turn(steer(&target, "late"))).is_err());
    assert_eq!(
        endpoint
            .methods()
            .iter()
            .filter(|m| **m == "turn/start")
            .count(),
        1
    );
    assert_eq!(spawner.spawn_count.load(Ordering::Acquire), 1);
    manager.shutdown();
}

#[test]
fn steer_response_validation_and_rpc_failure_are_submission_scoped() {
    let (manager, spawner) = manager_with_fake();
    let (a_events, _a_handle) = manager.run_prompt(request("a", Some("a"))).into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "a", "ra");
    let a_target = ready(&a_events);
    let (b_events, _b_handle) = manager.run_prompt(request("b", Some("b"))).into_parts();
    start_known_turn(&mut endpoint, "b", "rb");
    let b_target = ready(&b_events);
    for result in [json!({}), json!({"turnId":3}), json!({"turnId":"other"})] {
        let response = manager.steer_turn(steer(&a_target, "bad"));
        let rpc = endpoint.recv();
        endpoint.respond(&rpc, result);
        assert!(wait_value(&response).is_err());
    }
    let a = manager.steer_turn(steer(&a_target, "failed"));
    let arpc = endpoint.recv();
    let b = manager.steer_turn(steer(&b_target, "good"));
    let brpc = endpoint.recv();
    endpoint.respond(&brpc, json!({"turnId":"rb"}));
    endpoint.send(json!({"id":arpc["id"],"error":{"code":-32600,"message":"turn ended"}}));
    assert!(wait_value(&a).is_err());
    assert!(wait_value(&b).is_ok());
    complete(&endpoint, "a", "ra", "completed");
    complete(&endpoint, "b", "rb", "completed");
    assert!(
        !collect_terminal(&a_events)
            .iter()
            .any(|e| matches!(e, AgentEvent::Failed(_)))
    );
    assert!(
        !collect_terminal(&b_events)
            .iter()
            .any(|e| matches!(e, AgentEvent::Failed(_)))
    );
    manager.shutdown();
}

#[test]
fn steer_interrupt_and_connection_generation_are_not_rebound() {
    let (manager, spawner) = manager_with_fake();
    let (events, handle) = manager.run_prompt(request("a", Some("t"))).into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "t", "r");
    let target = ready(&events);
    let response = manager.steer_turn(steer(&target, "before-stop"));
    let rpc = endpoint.recv();
    handle.as_ref().unwrap().interrupt().unwrap();
    let stop = endpoint.recv();
    assert_eq!(stop["method"], "turn/interrupt");
    assert!(wait_value(&manager.steer_turn(steer(&target, "after-stop"))).is_err());
    endpoint.respond(&stop, json!({}));
    complete(&endpoint, "t", "r", "interrupted");
    assert_eq!(
        collect_terminal(&events).last(),
        Some(&AgentEvent::Interrupted)
    );
    endpoint.respond(&rpc, json!({"turnId":"r"}));
    assert!(wait_value(&response).is_ok());
    endpoint.close_stdout();
    wait_for_process(&endpoint.process);
    let (events2, _handle2) = manager.run_prompt(request("new", Some("t"))).into_parts();
    let mut endpoint2 = spawner.next_endpoint();
    handshake(&mut endpoint2);
    start_known_turn(&mut endpoint2, "t", "r");
    let new_target = ready(&events2);
    assert_ne!(target.generation, new_target.generation);
    assert!(wait_value(&manager.steer_turn(steer(&target, "stale"))).is_err());
    let pending = manager.steer_turn(steer(&new_target, "disconnect"));
    assert_eq!(endpoint2.recv()["method"], "turn/steer");
    endpoint2.close_stdout();
    assert!(wait_value(&pending).is_err());
    assert!(matches!(
        collect_terminal(&events2).last(),
        Some(AgentEvent::Failed(_))
    ));
    manager.shutdown();
}

#[test]
fn steer_late_notifications_cannot_bind_to_a_new_start_or_fail_another_turn() {
    let (manager, spawner) = manager_with_fake();
    let (events, _handle) = manager
        .run_prompt(request("initial", Some("t")))
        .into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "t", "old");
    let target = ready(&events);
    complete(&endpoint, "t", "old", "completed");
    collect_terminal(&events);
    let (next_events, _next_handle) = manager.run_prompt(request("next", Some("t"))).into_parts();
    let rpc = endpoint.recv();
    assert_eq!(rpc["method"], "turn/start");
    user(&endpoint, &target, "late-user");
    complete(&endpoint, "t", "old", "completed");
    endpoint.send(json!({"method":"turn/started","params":{"threadId":"t","turn":{"id":"old","status":"inProgress","items":[]}}}));
    endpoint.respond(&rpc, json!({"turn":{"id":"next"}}));
    let next = ready(&next_events);
    assert_eq!(next.turn_id, "next");
    complete(&endpoint, "t", "next", "completed");
    let received = collect_terminal(&next_events);
    assert!(
        !received
            .iter()
            .any(|e| matches!(e, AgentEvent::UserMessage { .. } | AgentEvent::Failed(_)))
    );
    manager.shutdown();
}

#[test]
fn steer_interrupt_rpc_error_does_not_replace_the_real_turn_terminal() {
    let (manager, spawner) = manager_with_fake();
    let (events, handle) = manager
        .run_prompt(request("initial", Some("t")))
        .into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "t", "r");
    ready(&events);
    handle.as_ref().unwrap().interrupt().unwrap();
    let rpc = endpoint.recv();
    endpoint.send(json!({"id":rpc["id"],"error":{"code":-32600,"message":"interrupt rejected"}}));
    loop {
        if matches!(wait_value(&events), AgentEvent::Warning { .. }) {
            break;
        }
    }
    complete(&endpoint, "t", "r", "completed");
    assert_eq!(
        collect_terminal(&events).last(),
        Some(&AgentEvent::Completed)
    );
    manager.shutdown();
}

#[test]
fn steer_fast_submissions_keep_wire_order_without_serializing_responses() {
    let (manager, spawner) = manager_with_fake();
    let (events, _handle) = manager
        .run_prompt(request("initial", Some("t")))
        .into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "t", "r");
    let target = ready(&events);
    let receipts = (0..12)
        .map(|i| manager.steer_turn(steer(&target, &format!("fast-{i}"))))
        .collect::<Vec<_>>();
    let rpcs = (0..12)
        .map(|i| {
            let rpc = endpoint.recv();
            assert_eq!(rpc["params"]["clientUserMessageId"], format!("fast-{i}"));
            rpc
        })
        .collect::<Vec<_>>();
    for rpc in rpcs.iter().rev() {
        endpoint.respond(rpc, json!({"turnId":"r"}));
    }
    for receipt in receipts {
        assert!(wait_value(&receipt).is_ok());
    }
    complete(&endpoint, "t", "r", "completed");
    assert_eq!(
        collect_terminal(&events).last(),
        Some(&AgentEvent::Completed)
    );
    manager.shutdown();
}

#[test]
fn steer_attachment_history_keeps_files_and_persisted_images_in_order() {
    let target = AgentTurnIdentity {
        generation: 1,
        thread_id: "t".into(),
        turn_id: "r".into(),
    };
    let mut request = steer(&target, "attachment-client");
    request.context.files = vec![
        AgentInputFile {
            path: "/tmp/context.txt".into(),
            image: false,
        },
        AgentInputFile {
            path: "/tmp/photo.png".into(),
            image: true,
        },
    ];
    let input = super::super::steer::build_steer_params(&request).unwrap()["input"].clone();
    let mut stored = input.clone();
    stored[1] = json!({"type":"image","url":"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="});
    let item = parse_history_item(
        &json!({"type":"userMessage","id":"item","clientId":"attachment-client","content":stored}),
    )
    .unwrap();
    let ThreadHistoryItem::UserMessage {
        images,
        client_message_id,
        text,
        ..
    } = item
    else {
        panic!()
    };
    assert_eq!(client_message_id.as_deref(), Some("attachment-client"));
    assert_eq!(text, request.prompt);
    assert_eq!(images.len(), 2);
    assert!(
        matches!(&images[0],crate::agent::UserMessageAttachment::File(path) if path==&PathBuf::from("/tmp/context.txt"))
    );
    assert!(
        matches!(&images[1],crate::agent::UserMessageAttachment::Local(path) if path.is_file())
    );
}

#[test]
fn steer_with_progress_and_both_approval_paths_preserves_cleanup_and_isolation() {
    use crate::conversation::{ConversationActivity as A, ConversationPhase, ConversationState};
    let (manager, spawner) = manager_with_fake();
    let observations = manager.subscribe_connection_events();
    let (events, _handle) = manager
        .run_prompt(request("initial", Some("a")))
        .into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "a", "ra");
    let target = ready(&events);
    let mut state = ConversationState {
        thread_id: Some("a".into()),
        ..Default::default()
    };
    state.begin_prompt("initial");
    state.apply_agent_event_batch(vec![AgentEvent::TurnReady(target.clone())]);
    let (other, _other_handle) = manager.run_prompt(request("other", Some("b"))).into_parts();
    start_known_turn(&mut endpoint, "b", "rb");
    ready(&other);

    endpoint.send(json!({"method":"item/autoApprovalReview/started","params":{
        "threadId":"a","turnId":"ra","reviewId":"review","targetItemId":null,"startedAtMs":100,
        "action":{"type":"networkAccess","host":"example.com","port":443,"protocol":"https","target":"example.com:443"},
        "review":{"status":"inProgress"}
    }}));
    endpoint.send(
        json!({"method":"item/started","params":{"threadId":"a","turnId":"ra","startedAtMs":100,
        "item":{"type":"sleep","id":"sleep","durationMs":15000}}}),
    );
    endpoint.send(json!({"method":"item/completed","params":{"threadId":"a","turnId":"ra","completedAtMs":200,
        "item":{"type":"webSearch","id":"search","query":"Rust","action":{"type":"search"},"results":[]}}}));
    endpoint.send(json!({"method":"item/plan/delta","params":{"threadId":"a","turnId":"ra","itemId":"plan","delta":"keep this plan"}}));
    endpoint.send(json!({"id":"file","method":"item/fileChange/requestApproval","params":{
        "threadId":"a","turnId":"ra","itemId":"patch","startedAtMs":123,"reason":null,"grantRoot":null
    }}));
    let file = loop {
        let event = wait_value(&events);
        let handle = if let AgentEvent::FileApprovalRequested { responder, .. } = &event {
            Some(responder.clone())
        } else {
            None
        };
        state.apply_agent_event_batch(vec![event]);
        if let Some(handle) = handle {
            break handle;
        }
    };
    let submission = state.record_submission(
        crate::conversation::SubmissionDraft {
            text: "input appended".into(),
            context: Default::default(),
            comments: vec![],
        },
        "input appended".into(),
        false,
    );
    let ack = manager.steer_turn(steer(&target, &submission));
    let rpc = endpoint.recv();
    assert_eq!(rpc["method"], "turn/steer");
    user(&endpoint, &target, &submission);
    file.respond(crate::agent::AgentFileApprovalChoice::Accept)
        .unwrap();
    let response = endpoint.recv();
    assert_eq!(response["id"], "file");
    assert_eq!(response["result"]["decision"], "accept");
    endpoint.send(
        json!({"method":"serverRequest/resolved","params":{"threadId":"a","requestId":"file"}}),
    );
    endpoint.send(command_approval(json!(1001), "a", "ra"));
    let command = loop {
        let event = wait_value(&events);
        let handle = if let AgentEvent::CommandApprovalRequested { responder, .. } = &event {
            Some(responder.clone())
        } else {
            None
        };
        state.apply_agent_event_batch(vec![event]);
        if let Some(handle) = handle {
            break handle;
        }
    };
    for event in std::iter::from_fn(|| observations.try_recv().ok()) {
        state.apply_connection_event(event);
    }
    assert_eq!(state.turn_id.as_deref(), Some("ra"));
    assert!(
        state
            .activities
            .iter()
            .any(|a| matches!(a, A::AutoApprovalReview(_)))
    );
    assert!(
        state
            .activities
            .iter()
            .any(|a| matches!(a, A::Plan(p) if p.text == "keep this plan"))
    );
    assert!(
        state
            .activities
            .iter()
            .any(|a| matches!(a, A::UserMessage { .. }))
    );
    assert_eq!(state.file_approval_responders.len(), 0);
    assert_eq!(state.approval_responders.len(), 1);
    complete(&endpoint, "a", "ra", "interrupted");
    state.apply_agent_event_batch(collect_terminal(&events));
    assert_eq!(state.phase, ConversationPhase::Stopped);
    assert!(state.approval_responders.is_empty());
    assert!(command.respond(AgentCommandApprovalChoice::Accept).is_err());
    assert!(state.activities.iter().any(
        |a| matches!(a, A::Sleep(s) if s.status == crate::agent::AgentActivityStatus::Interrupted)
    ));
    assert!(state.activities.iter().any(
        |a| matches!(a, A::WebSearch(s) if s.status == crate::agent::AgentActivityStatus::Completed)
    ));
    assert!(
        state
            .activities
            .iter()
            .any(|a| matches!(a, A::AutoApprovalReview(r) if r.closed_locally))
    );
    endpoint.respond(&rpc, json!({"turnId":"ra"}));
    state.resolve_submission(&submission, wait_value(&ack));
    assert_eq!(state.phase, ConversationPhase::Stopped);
    endpoint.send(json!({"method":"item/agentMessage/delta","params":{"threadId":"b","turnId":"rb","itemId":"answer","delta":"other keeps running"}}));
    complete(&endpoint, "b", "rb", "completed");
    let remaining = collect_terminal(&other);
    assert!(remaining.contains(&AgentEvent::TextDelta("other keeps running".into())));
    assert_eq!(remaining.last(), Some(&AgentEvent::Completed));
    assert_eq!(spawner.spawn_count.load(Ordering::Acquire), 1);
    manager.shutdown();
}
