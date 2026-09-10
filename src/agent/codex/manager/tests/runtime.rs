use super::*;
use crate::agent::{AgentHookStatus, AgentLocalClosure, AgentRuntimeState};

fn hook(thread: &str, turn: Option<&str>, completed: bool) -> Value {
    json!({"method":if completed {"hook/completed"}else{"hook/started"},"params":{"threadId":thread,"turnId":turn,"run":{
        "id":"same-run","displayOrder":0,"eventName":"sessionStart","executionMode":"sync","handlerType":"command","scope":"thread","sourcePath":"/tmp/hooks.json","source":"project",
        "status":if completed {"failed"}else{"running"},"entries":[{"kind":"context","text":"internal input"}],"startedAt":10,"completedAt":null,"durationMs":null,"statusMessage":null
    }}})
}

fn auth(thread: &str, turn: &str, completed: bool) -> Value {
    json!({"method":if completed {"modelProvider/authRecoveryCompleted"}else{"modelProvider/authRecoveryStarted"},"params":{"threadId":thread,"turnId":turn,"provider":"openai","message":if completed {"Credentials refreshed; retry pending"}else{"Refreshing credentials"}}})
}

#[test]
fn runtime_notifications_before_start_and_after_completion_do_not_steal_or_end_turns() {
    let (manager, spawner) = manager_with_fake();
    let observations = manager.subscribe_connection_events();
    let (a, ha) = manager.run_prompt(request("a", Some("a"))).into_parts();
    let (b, hb) = manager.run_prompt(request("b", Some("b"))).into_parts();
    let mut endpoint = spawner.next_endpoint();
    let initialize = endpoint.recv();
    let opt_out = initialize["params"]["capabilities"]["optOutNotificationMethods"]
        .as_array()
        .unwrap();
    assert_eq!(opt_out.len(), 7);
    assert!(opt_out.contains(&json!("thread/goal/cleared")));
    assert!(
        !opt_out.contains(&json!("item/started")) && !opt_out.contains(&json!("item/completed"))
    );
    endpoint.send(json!({"method":"deprecationNotice","params":{"summary":"Deprecated setting","details":"Use its replacement"}}));
    endpoint.respond(&initialize, json!({"userAgent":"fake"}));
    assert_eq!(endpoint.recv()["method"], "initialized");
    let mut starts = HashMap::new();
    while starts.len() < 2 {
        let msg = endpoint.recv();
        match msg["method"].as_str().unwrap() {
            "thread/resume" => {
                endpoint.send(json!({"method":"thread/goal/cleared","params":{"threadId":msg["params"]["threadId"]}}));
                endpoint.respond(&msg, json!({"thread":{"id":msg["params"]["threadId"]}}));
            }
            "turn/start" => {
                starts.insert(msg["params"]["threadId"].as_str().unwrap().to_owned(), msg);
            }
            other => panic!("{other}"),
        }
    }
    // A turnless hook and observations from an earlier turn cannot claim start.
    endpoint.send(hook("a", None, false));
    endpoint.send(hook("b", Some("old-b"), true));
    endpoint.send(auth("a", "old-a", true));
    endpoint.send(auth("a", "old-a", false));
    endpoint.respond(&starts["a"], json!({"turn":{"id":"ta"}}));
    endpoint.respond(&starts["b"], json!({"turn":{"id":"tb"}}));
    endpoint.send(auth("a", "ta", false));
    endpoint.send(auth("b", "tb", false));
    endpoint.send(auth("a", "ta", true));
    let item = json!({"type":"hookPrompt","id":"prompt","fragments":[{"hookRunId":"same-run","text":"first"},{"hookRunId":"second","text":"second"}]});
    endpoint
        .send(json!({"method":"item/started","params":{"threadId":"a","turnId":"ta","item":item}}));
    complete(&endpoint, "a", "ta", "interrupted");
    endpoint.send(
        json!({"method":"item/completed","params":{"threadId":"a","turnId":"ta","item":item}}),
    );
    endpoint
        .send(json!({"method":"item/started","params":{"threadId":"a","turnId":"ta","item":item}}));
    endpoint.send(hook("a", None, true));
    endpoint.send(hook("a", None, false));
    endpoint.send(json!({"method":"item/agentMessage/delta","params":{"threadId":"b","turnId":"tb","itemId":"answer","delta":"B is still running"}}));
    complete(&endpoint, "b", "tb", "completed");
    let events_a = collect_terminal(&a);
    let events_b = collect_terminal(&b);
    assert_eq!(events_a.last(), Some(&AgentEvent::Interrupted));
    assert_eq!(events_b.last(), Some(&AgentEvent::Completed));
    assert!(events_b.contains(&AgentEvent::TextDelta("B is still running".into())));
    let mut states = [
        crate::conversation::ConversationState {
            thread_id: Some("a".into()),
            ..Default::default()
        },
        crate::conversation::ConversationState {
            thread_id: Some("b".into()),
            ..Default::default()
        },
    ];
    while let Ok(event) = observations.try_recv() {
        for s in &mut states {
            s.apply_connection_event(event.clone());
        }
    }
    assert!(states[0].runtime.hooks.iter().all(|h| h.thread_id == "a"));
    assert!(states[1].runtime.hooks.iter().all(|h| h.thread_id == "b"));
    assert_eq!(states[0].runtime.hooks.len(), 1);
    assert_eq!(states[0].runtime.hooks[0].status, AgentHookStatus::Failed);
    assert_eq!(
        states[0].runtime.hook_prompts[0].prompt.completed,
        Some(true)
    );
    assert_eq!(states[0].runtime.hook_prompts[0].prompt.fragments.len(), 2);
    assert!(
        states
            .iter()
            .all(|s| s.phase == crate::conversation::ConversationPhase::Empty)
    );
    assert!(
        !states[0]
            .runtime
            .auth_recoveries
            .iter()
            .any(|r| r.is_waiting())
    );
    assert_eq!(
        states[1].runtime.auth_recoveries[0].closed_locally,
        Some(AgentLocalClosure::TurnCompleted)
    );
    assert!(endpoint.process.is_alive());
    drop(ha);
    drop(hb);
    manager.shutdown();
}

#[test]
fn runtime_application_snapshot_and_disconnect_rebuild_are_generation_scoped() {
    let (manager, spawner) = manager_with_fake();
    let catalog = manager.load_model_catalog();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    endpoint.send(
        json!({"method":"deprecationNotice","params":{"summary":"Migration","details":null}}),
    );
    endpoint.send(hook("idle", None, false));
    endpoint.send(auth("idle", "no-active-turn", false));
    let model = endpoint.recv();
    endpoint.respond(&model, model_page());
    assert!(catalog.recv_blocking().unwrap().is_ok());
    let mut before = AgentRuntimeState::default();
    let mut notices = 0;
    let events = manager.subscribe_connection_events();
    while let Ok(e) = events.try_recv() {
        match e {
            AgentConnectionEvent::Runtime(e) => {
                before.apply(e).unwrap();
            }
            AgentConnectionEvent::DeprecationNotice(_) => notices += 1,
            _ => {}
        }
    }
    assert_eq!(notices, 1);
    assert!(before.hooks[0].is_waiting());
    manager
        .inner
        .fail_generation(1, "fixture disconnected".into());
    let mut replay = AgentRuntimeState::default();
    let events = manager.subscribe_connection_events();
    while let Ok(e) = events.try_recv() {
        if let AgentConnectionEvent::Runtime(e) = e {
            replay.apply(e).unwrap();
        }
    }
    assert_eq!(
        replay.hooks[0].closed_locally,
        Some(AgentLocalClosure::Disconnected)
    );
    assert_eq!(replay.hooks[0].status, AgentHookStatus::Running);
    let catalog = manager.load_model_catalog();
    let mut fresh = spawner.next_endpoint();
    handshake(&mut fresh);
    let model = fresh.recv();
    fresh.respond(&model, model_page());
    assert!(catalog.recv_blocking().unwrap().is_ok());
    let events = manager.subscribe_connection_events();
    let mut state = AgentRuntimeState::default();
    while let Ok(e) = events.try_recv() {
        if let AgentConnectionEvent::Runtime(e) = e {
            state.apply(e).unwrap();
        }
    }
    assert_eq!(state.generation, 2);
    assert!(state.hooks.is_empty() && state.auth_recoveries.is_empty());
    manager.shutdown();
}

#[test]
fn runtime_notification_policy_never_swallows_server_requests() {
    for method in [
        "deprecationNotice",
        "hook/started",
        "modelProvider/authRecoveryCompleted",
        "thread/goal/cleared",
        "skills/changed",
    ] {
        let (manager, spawner) = manager_with_fake();
        let catalog = manager.load_model_catalog();
        let mut endpoint = spawner.next_endpoint();
        handshake(&mut endpoint);
        assert_eq!(endpoint.recv()["method"], "model/list");
        endpoint.send(json!({"id":"server-request","method":method,"params":{}}));
        let error = endpoint.recv();
        assert_eq!(error["id"], "server-request");
        assert_eq!(error["error"]["code"], -32601);
        assert!(wait_value(&catalog).is_err());
        wait_for_process(&endpoint.process);
        manager.shutdown();
    }
}
