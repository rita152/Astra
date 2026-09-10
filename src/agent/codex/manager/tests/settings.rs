use super::*;
use crate::agent::{AgentThreadPermissionUpdate, AgentThreadSettings};

fn update(thread: &str, operation: u64, profile: &str) -> AgentThreadPermissionUpdate {
    AgentThreadPermissionUpdate {
        thread_id: thread.into(),
        cwd: "/tmp/project".into(),
        mode: AgentPermissionMode::Profile(profile.into()),
        expected_generation: Some(1),
        operation_id: operation,
    }
}
fn notification(thread: &str, profile: &str) -> Value {
    json!({"method":"thread/settings/updated","params":{"threadId":thread,"threadSettings":{
        "model":"gpt-test","effort":"medium","serviceTier":null,"cwd":"/tmp/project",
        "approvalPolicy":"on-request","approvalsReviewer":"user","sandboxPolicy":{"type":"workspaceWrite"},
        "activePermissionProfile":{"id":profile,"extends":":workspace"}
    }}})
}
fn next_update(endpoint: &mut FakeEndpoint) -> Value {
    loop {
        let request = endpoint.recv();
        match request["method"].as_str().unwrap() {
        "thread/resume" => endpoint.respond(&request,json!({"thread":{"id":request["params"]["threadId"]}})),
        "permissionProfile/list" => endpoint.respond(&request,json!({"data":[{"id":"org-a","allowed":true,"extends":":workspace"},{"id":"org-b","allowed":true,"extends":":workspace"},{"id":":workspace","allowed":true}]})),
        "thread/settings/update" => return request,
        other => panic!("unexpected request {other}"),
    }
    }
}
fn assert_empty<T>(receiver: &async_channel::Receiver<T>) {
    std::thread::sleep(Duration::from_millis(20));
    assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
}

#[test]
fn permissions_need_both_rpc_and_matching_notification_in_either_order() {
    let (manager, spawner) = manager_with_fake();
    let result = manager.update_thread_permissions(update("main", 11, "org-a"));
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let request = next_update(&mut endpoint);
    endpoint.send(notification("side", "org-a"));
    endpoint.send(notification("main", "org-b"));
    assert_empty(&result);
    endpoint.send(notification("main", "org-a"));
    assert_empty(&result);
    endpoint.respond(&request, json!({}));
    let confirmed = wait_value(&result).unwrap();
    assert_eq!(
        (
            confirmed.thread_id.as_str(),
            confirmed.operation_id,
            confirmed.generation
        ),
        ("main", 11, 1)
    );
    let next = manager.update_thread_permissions(update("main", 12, "org-b"));
    let request = next_update(&mut endpoint);
    endpoint.respond(&request, json!({}));
    assert_empty(&next);
    endpoint.send(notification("main", "org-a"));
    assert_empty(&next);
    endpoint.send(notification("main", "org-b"));
    assert_eq!(wait_value(&next).unwrap().operation_id, 12);
    manager.shutdown();
}

#[test]
fn queued_changes_preserve_order_and_two_threads_do_not_block_each_other() {
    let (manager, spawner) = manager_with_fake();
    let first = manager.update_thread_permissions(update("main", 1, "org-a"));
    let second = manager.update_thread_permissions(update("main", 2, "org-b"));
    let side = manager.update_thread_permissions(update("side", 3, "org-a"));
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let a = next_update(&mut endpoint);
    let b = next_update(&mut endpoint);
    let (main_request, side_request) = if a["params"]["threadId"] == "main" {
        (a, b)
    } else {
        (b, a)
    };
    assert_eq!(main_request["params"]["permissions"], "org-a");
    endpoint.respond(&side_request, json!({}));
    endpoint.send(notification("side", "org-a"));
    assert_eq!(wait_value(&side).unwrap().operation_id, 3);
    assert_empty(&first);
    assert_empty(&second);
    endpoint.send(notification("main", "org-a"));
    endpoint.respond(&main_request, json!({}));
    wait_value(&first).unwrap();
    let second_request = next_update(&mut endpoint);
    assert_eq!(second_request["params"]["permissions"], "org-b");
    endpoint.send(notification("main", "org-a"));
    endpoint.respond(&second_request, json!({}));
    assert_empty(&second);
    endpoint.send(notification("main", "org-b"));
    assert_eq!(wait_value(&second).unwrap().operation_id, 2);
    manager.shutdown();
}

#[test]
fn rpc_failure_after_notification_does_not_publish_success_and_queue_recovers() {
    let (manager, spawner) = manager_with_fake();
    let events = manager.subscribe_connection_events();
    let first = manager.update_thread_permissions(update("main", 1, "org-a"));
    let next = manager.update_thread_permissions(update("main", 2, "org-b"));
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let request = next_update(&mut endpoint);
    endpoint.send(notification("main", "org-a"));
    endpoint
        .send(json!({"id":request["id"],"error":{"code":-32602,"message":"managed rejection"}}));
    assert!(
        wait_value(&first)
            .unwrap_err()
            .contains("managed rejection")
    );
    assert!(
        std::iter::from_fn(|| events.try_recv().ok()).all(|event| matches!(
            event,
            AgentConnectionEvent::Runtime(crate::agent::AgentRuntimeEvent {
                observation: crate::agent::AgentRuntimeObservation::GenerationStarted,
                ..
            })
        ))
    );
    let request = next_update(&mut endpoint);
    endpoint.respond(&request, json!({}));
    endpoint.send(notification("main", "org-b"));
    wait_value(&next).unwrap();
    endpoint.send(notification("main", "org-b"));
    let events: Vec<_> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AgentConnectionEvent::ThreadSettingsUpdated { .. }))
            .count(),
        1
    );
    manager.shutdown();
}

#[test]
fn disallowed_profile_never_reaches_settings_update() {
    let (manager, spawner) = manager_with_fake();
    let result = manager.update_thread_permissions(update("main", 1, "org-a"));
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let resume = endpoint.recv();
    endpoint.respond(&resume, json!({"thread":{"id":"main"}}));
    let profiles = endpoint.recv();
    assert_eq!(profiles["method"], "permissionProfile/list");
    endpoint.respond(
        &profiles,
        json!({"data":[{"id":"org-a","allowed":false,"extends":":workspace"}]}),
    );
    assert!(wait_value(&result).unwrap_err().contains("不允许"));
    assert!(endpoint.from_client.try_recv().is_err());
    manager.shutdown();
}

#[test]
fn disconnected_generation_cannot_confirm_or_resume_temporary_thread() {
    let (manager, spawner) = manager_with_fake();
    let pending = manager.update_thread_permissions(update("main", 1, "org-a"));
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let _ = next_update(&mut endpoint);
    manager.inner.fail_generation(1, "test disconnect".into());
    assert!(wait_value(&pending).is_err());
    let stale = manager.update_thread_permissions(update("main", 2, "org-b"));
    let mut rebuilt = spawner.next_endpoint();
    handshake(&mut rebuilt);
    assert!(wait_value(&stale).unwrap_err().contains("连接已变化"));
    assert!(rebuilt.from_client.try_recv().is_err());
    manager.shutdown();
}

#[test]
fn settings_match_preserves_granular_policy_and_reviewer_alias() {
    let granular =
        json!({"granular":{"rules":true,"sandbox_approval":false,"mcp_elicitations":true}});
    let settings = AgentThreadSettings {
        model: "m".into(),
        effort: None,
        service_tier: None,
        cwd: "/work".into(),
        permissions: Some(crate::agent::AgentEffectivePermissions {
            approval_policy: granular.clone(),
            approvals_reviewer: "auto_review".into(),
            sandbox_policy: None,
            active_permission_profile: None,
        }),
    };
    assert!(super::super::settings::settings_match(
        &json!({"approvalPolicy":granular,"approvalsReviewer":"guardian_subagent"}),
        &settings
    ));
    assert!(!super::super::settings::settings_match(
        &json!({"approvalPolicy":"never"}),
        &settings
    ));
}

#[test]
fn custom_defaults_use_server_resolution_and_close_probe_without_a_turn() {
    let (manager, spawner) = manager_with_fake();
    let mut request = update("main", 1, "unused");
    request.mode = AgentPermissionMode::Custom;
    let result = manager.update_thread_permissions(request);
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let resume = endpoint.recv();
    endpoint.respond(&resume, json!({"thread":{"id":"main"}}));
    let probe = endpoint.recv();
    assert_eq!(probe["method"], "thread/start");
    assert_eq!(probe["params"]["ephemeral"], true);
    endpoint.respond(&probe,json!({"thread":{"id":"probe","ephemeral":true},"approvalPolicy":"on-request","approvalsReviewer":"user","activePermissionProfile":{"id":"org-a","extends":":workspace"},"sandbox":{"type":"workspaceWrite"}}));
    let close = endpoint.recv();
    assert_eq!(close["method"], "thread/unsubscribe");
    assert_eq!(close["params"]["threadId"], "probe");
    endpoint.send(json!({"method":"thread/started","params":{"thread":{"id":"probe"}}}));
    endpoint.respond(&close, json!({"status":"unsubscribed"}));
    endpoint.send(json!({"method":"thread/closed","params":{"threadId":"probe"}}));
    endpoint.send(json!({"method":"thread/started","params":{"thread":{"id":"probe"}}}));
    let change = next_update(&mut endpoint);
    assert_eq!(change["params"]["permissions"], "org-a");
    endpoint.respond(&change, json!({}));
    endpoint.send(notification("main", "org-a"));
    wait_value(&result).unwrap();
    assert!(
        !endpoint
            .received
            .iter()
            .any(|request| request["method"] == "turn/start")
    );
    manager.shutdown();
}

#[test]
fn loading_existing_permissions_reuses_lifecycle_fields_and_cache() {
    let (manager, spawner) = manager_with_fake();
    let loaded = manager.load_thread_settings("main".into(), 1);
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let resume = endpoint.recv();
    assert_eq!(resume["method"], "thread/resume");
    endpoint.respond(&resume,json!({"thread":{"id":"main"},"model":"gpt-test","reasoningEffort":null,"serviceTier":null,"cwd":"/tmp/project","approvalPolicy":"on-request","approvalsReviewer":"user","sandbox":{"type":"readOnly"},"activePermissionProfile":{"id":":read-only","extends":null}}));
    let snapshot = wait_value(&loaded).unwrap();
    assert_eq!(snapshot.generation, 1);
    assert_eq!(
        snapshot
            .settings
            .permissions
            .unwrap()
            .active_permission_profile
            .unwrap()
            .id,
        ":read-only"
    );
    wait_value(&manager.load_thread_settings("main".into(), 1)).unwrap();
    assert!(endpoint.from_client.try_recv().is_err());
    manager.shutdown();
}

#[test]
fn duplicate_settings_rpc_response_does_not_break_the_next_operation() {
    let (manager, spawner) = manager_with_fake();
    let first = manager.update_thread_permissions(update("main", 1, "org-a"));
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let request = next_update(&mut endpoint);
    endpoint.respond(&request, json!({}));
    endpoint.send(notification("main", "org-a"));
    wait_value(&first).unwrap();
    endpoint.respond(&request, json!({}));
    let second = manager.update_thread_permissions(update("main", 2, "org-b"));
    let request = next_update(&mut endpoint);
    endpoint.respond(&request, json!({}));
    endpoint.send(notification("main", "org-b"));
    assert_eq!(wait_value(&second).unwrap().operation_id, 2);
    assert_eq!(spawner.spawn_count.load(Ordering::Acquire), 1);
    manager.shutdown();
}

#[test]
fn permission_confirmation_and_runtime_events_share_connection_without_cross_talk() {
    use crate::agent::{AgentLocalClosure, AgentRuntimeObservation, AgentRuntimeState};
    let (manager, spawner) = manager_with_fake();
    let observations = manager.subscribe_connection_events();
    let (side_events, side_handle) = manager
        .run_prompt(request("side input", Some("side")))
        .into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let resume = endpoint.recv();
    endpoint.respond(&resume, json!({"thread":{"id":"side"}}));
    let start = endpoint.recv();
    assert_eq!(start["method"], "turn/start");
    endpoint.respond(&start, json!({"turn":{"id":"side-turn"}}));
    let pending = manager.update_thread_permissions(update("main", 1, "org-a"));
    let change = next_update(&mut endpoint);
    endpoint.send(super::runtime::hook("main", None, false));
    endpoint.send(super::runtime::hook("side", Some("side-turn"), false));
    endpoint.send(super::runtime::auth("side", "side-turn", false));
    endpoint.send(super::runtime::auth("side", "side-turn", true));
    endpoint.send(notification("main", "org-a"));
    assert_empty(&pending);
    endpoint.send(json!({"method":"item/agentMessage/delta","params":{"threadId":"side","turnId":"side-turn","itemId":"answer","delta":"Side remains active"}}));
    complete(&endpoint, "side", "side-turn", "completed");
    let side = collect_terminal(&side_events);
    assert!(side.contains(&AgentEvent::TextDelta("Side remains active".into())));
    assert_eq!(side.last(), Some(&AgentEvent::Completed));
    assert_empty(&pending);
    endpoint.respond(&change, json!({}));
    assert_eq!(wait_value(&pending).unwrap().operation_id, 1);
    let snapshot = manager.subscribe_connection_events();
    let mut runtime = AgentRuntimeState::default();
    let mut confirmed = Vec::new();
    while let Ok(event) = snapshot.try_recv() {
        match event {
            AgentConnectionEvent::Runtime(event) => {
                runtime.apply(event).unwrap();
            }
            AgentConnectionEvent::ThreadSettingsUpdated { thread_id, .. } => {
                confirmed.push(thread_id)
            }
            _ => {}
        }
    }
    assert_eq!(confirmed, ["main"]);
    assert!(
        runtime
            .hooks
            .iter()
            .find(|hook| hook.thread_id == "main")
            .unwrap()
            .is_waiting()
    );
    assert_eq!(
        runtime
            .hooks
            .iter()
            .find(|hook| hook.thread_id == "side")
            .unwrap()
            .closed_locally,
        Some(AgentLocalClosure::TurnCompleted)
    );
    assert!(runtime.auth_recoveries[0].completed_message.is_some());
    assert!(endpoint.process.is_alive());

    let pending = manager.update_thread_permissions(update("main", 2, "org-b"));
    let _ = next_update(&mut endpoint);
    manager
        .inner
        .fail_generation(1, "combined disconnect".into());
    assert!(wait_value(&pending).is_err());
    let snapshot = manager.subscribe_connection_events();
    let events: Vec<_> = std::iter::from_fn(|| snapshot.try_recv().ok()).collect();
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AgentConnectionEvent::ThreadSettingsUpdated { .. }))
    );
    assert!(events.iter().any(|event| matches!(
        event,
        AgentConnectionEvent::Runtime(crate::agent::AgentRuntimeEvent {
            observation: AgentRuntimeObservation::Disconnected,
            ..
        })
    )));
    let mut runtime = AgentRuntimeState::default();
    for event in std::iter::from_fn(|| observations.try_recv().ok()) {
        if let AgentConnectionEvent::Runtime(event) = event {
            runtime.apply(event).unwrap();
        }
    }
    assert!(runtime.hooks.iter().all(|hook| !hook.is_waiting()));
    drop(side_handle);
    manager.shutdown();
}
