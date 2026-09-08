use super::*;
use crate::agent::SideConversationRequest;

fn side_request() -> SideConversationRequest {
    SideConversationRequest {
        parent_thread_id: "parent".into(),
        cwd: "/tmp/project".into(),
        model: Some("gpt-test".into()),
        effort: Some("medium".into()),
        service_tier: None,
    }
}

fn accept_fork(endpoint: &mut FakeEndpoint, id: &str) {
    let fork = endpoint.recv();
    assert_eq!(fork["method"], "thread/fork");
    assert_eq!(fork["params"]["threadId"], "parent");
    assert_eq!(fork["params"]["ephemeral"], true);
    assert_eq!(fork["params"]["excludeTurns"], true);
    assert_eq!(fork["params"]["config"]["model_reasoning_effort"], "medium");
    assert!(
        fork["params"]["developerInstructions"]
            .as_str()
            .unwrap()
            .contains("reference")
    );
    let mut thread = workspace_thread(id, None);
    thread["ephemeral"] = json!(true);
    endpoint.send(json!({ "method": "thread/started", "params": { "thread": thread } }));
    endpoint.respond(&fork, json!({ "thread": thread }));
}

fn accept_boundary(endpoint: &mut FakeEndpoint, id: &str) {
    let boundary = endpoint.recv();
    assert_eq!(boundary["method"], "thread/inject_items");
    assert_eq!(boundary["params"]["threadId"], id);
    assert_eq!(boundary["params"]["items"][0]["role"], "user");
    assert!(
        boundary["params"]["items"][0]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("reference")
    );
    endpoint.respond(&boundary, json!({}));
}

#[test]
fn side_conversation_forks_once_and_streams_independently_of_active_parent() {
    let (manager, spawner) = manager_with_fake();
    let (parent_events, parent_control) = manager
        .run_prompt(request("parent work", Some("parent")))
        .into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "parent", "parent-turn");
    let opened = manager.open_side_conversation(side_request());
    accept_fork(&mut endpoint, "side");
    accept_boundary(&mut endpoint, "side");
    assert_eq!(wait_value(&opened).unwrap(), "side");
    assert!(
        endpoint.from_client.try_recv().is_err(),
        "opening a side chat must not start a turn"
    );
    for index in 0..2 {
        let (events, control) = manager
            .run_prompt(request("side question", Some("side")))
            .into_parts();
        let turn = endpoint.recv();
        assert_eq!(
            turn["method"], "turn/start",
            "an ephemeral fork must not resume from disk"
        );
        assert_eq!(turn["params"]["threadId"], "side");
        let turn_id = format!("side-turn-{index}");
        endpoint.respond(&turn, json!({"turn":{"id":turn_id}}));
        complete(&endpoint, "side", &turn_id, "completed");
        let events = collect_terminal(&events);
        assert!(matches!(events.last(), Some(AgentEvent::Completed)));
        drop(control);
    }
    complete(&endpoint, "parent", "parent-turn", "completed");
    assert!(matches!(
        collect_terminal(&parent_events).last(),
        Some(AgentEvent::Completed)
    ));
    drop(parent_control);
    assert_eq!(
        endpoint
            .methods()
            .iter()
            .filter(|method| **method == "thread/fork")
            .count(),
        1
    );
    assert_eq!(spawner.spawn_count.load(Ordering::Acquire), 1);
    manager.shutdown();
}

#[test]
fn side_conversation_close_interrupts_only_its_own_turn_and_unsubscribes() {
    let (manager, spawner) = manager_with_fake();
    let opened = manager.open_side_conversation(side_request());
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    accept_fork(&mut endpoint, "side");
    accept_boundary(&mut endpoint, "side");
    wait_value(&opened).unwrap();
    let (events, control) = manager
        .run_prompt(request("question", Some("side")))
        .into_parts();
    start_known_turn(&mut endpoint, "side", "side-turn");
    let closed = manager.close_side_conversation("side".into());
    let mut methods = HashSet::new();
    for _ in 0..2 {
        let message = endpoint.recv();
        assert_eq!(message["params"]["threadId"], "side");
        match message["method"].as_str().unwrap() {
            "turn/interrupt" => {
                assert_eq!(message["params"]["turnId"], "side-turn");
                endpoint.respond(&message, json!({}));
            }
            "thread/unsubscribe" => endpoint.respond(&message, json!({"status":"unsubscribed"})),
            method => panic!("unexpected close method {method}"),
        }
        methods.insert(message["method"].as_str().unwrap().to_owned());
    }
    wait_value(&closed).unwrap();
    complete(&endpoint, "side", "side-turn", "interrupted");
    assert!(matches!(
        collect_terminal(&events).last(),
        Some(AgentEvent::Interrupted)
    ));
    drop(control);
    assert_eq!(methods.len(), 2);
    let (events, control) = manager
        .run_prompt(request("stale", Some("side")))
        .into_parts();
    assert!(
        matches!(collect_terminal(&events).last(), Some(AgentEvent::Failed(error)) if error.contains("连接已结束"))
    );
    assert!(endpoint.from_client.try_recv().is_err());
    drop(control);
    manager.shutdown();
}

#[test]
fn side_conversation_failed_boundary_discards_fork_without_publishing_a_ready_id() {
    let (manager, spawner) = manager_with_fake();
    let opened = manager.open_side_conversation(side_request());
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    accept_fork(&mut endpoint, "side");
    let boundary = endpoint.recv();
    endpoint.send(json!({"id":boundary["id"],"error":{"code":-1,"message":"failed boundary"}}));
    let unsubscribe = endpoint.recv();
    assert_eq!(unsubscribe["method"], "thread/unsubscribe");
    endpoint.respond(&unsubscribe, json!({"status":"unsubscribed"}));
    assert!(wait_value(&opened).is_err());
    assert!(!endpoint.methods().contains(&"turn/start"));
    assert!(!endpoint.process.terminated.load(Ordering::Acquire));
    manager.shutdown();
}

#[test]
fn side_conversation_expired_connection_never_resumes_a_temporary_id() {
    let (manager, spawner) = manager_with_fake();
    let opened = manager.open_side_conversation(side_request());
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    accept_fork(&mut endpoint, "side");
    accept_boundary(&mut endpoint, "side");
    wait_value(&opened).unwrap();
    endpoint.close_stdout();
    wait_for_process(&endpoint.process);
    let (events, control) = manager
        .run_prompt(request("stale question", Some("side")))
        .into_parts();
    let mut next = spawner.next_endpoint();
    handshake(&mut next);
    assert!(
        matches!(collect_terminal(&events).last(), Some(AgentEvent::Failed(error)) if error.contains("连接已结束"))
    );
    assert!(!next.methods().contains(&"thread/resume"));
    assert!(next.from_client.try_recv().is_err());
    drop(control);
    manager.shutdown();
}

#[test]
fn side_conversation_rejects_parent_identity_and_mismatched_notifications() {
    for returned_id in ["parent", "wrong-side"] {
        let (manager, spawner) = manager_with_fake();
        let opened = manager.open_side_conversation(side_request());
        let mut endpoint = spawner.next_endpoint();
        handshake(&mut endpoint);
        let fork = endpoint.recv();
        let mut thread = workspace_thread("side", None);
        thread["ephemeral"] = json!(true);
        endpoint.send(json!({"method":"thread/started","params":{"thread":thread}}));
        thread["id"] = json!(returned_id);
        endpoint.respond(&fork, json!({"thread":thread}));
        assert!(wait_value(&opened).is_err());
        wait_for_process(&endpoint.process);
        assert!(!endpoint.methods().contains(&"thread/inject_items"));
        manager.shutdown();
    }
}

#[test]
fn side_conversation_cannot_close_an_unowned_parent_thread() {
    let (manager, spawner) = manager_with_fake();
    assert!(wait_value(&manager.close_side_conversation("parent".into())).is_err());
    assert_eq!(spawner.spawn_count.load(Ordering::Acquire), 0);
}

#[test]
fn side_conversation_file_context_and_planning_use_typed_turn_input() {
    let mut request = request("请查看附件", Some("side"));
    request.context.files = vec![
        crate::agent::AgentInputFile {
            path: "/tmp/带 空格.txt".into(),
            image: false,
        },
        crate::agent::AgentInputFile {
            path: "/tmp/example.png".into(),
            image: true,
        },
    ];
    request.context.plan_mode = Some(true);
    let params = super::super::turn::build_turn_start_params(&request, "side", false).unwrap();
    assert_eq!(params["threadId"], "side");
    assert!(
        params["input"][0]["text"]
            .as_str()
            .unwrap()
            .contains("/tmp/带 空格.txt")
    );
    assert_eq!(
        params["input"][1],
        json!({"type":"localImage","path":"/tmp/example.png"})
    );
    assert_eq!(params["collaborationMode"]["mode"], "plan");
    request.context.plan_mode = Some(false);
    let params = super::super::turn::build_turn_start_params(&request, "side", false).unwrap();
    assert_eq!(params["collaborationMode"]["mode"], "default");
}
