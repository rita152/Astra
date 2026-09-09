use super::*;

fn progress_message(method: &str, thread: &str, turn: &str, extra: Value) -> Value {
    let mut params =
        json!({"threadId":thread,"turnId":turn,"startedAtMs":1000,"completedAtMs":2000});
    params
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    json!({"method":method,"params":params})
}
#[test]
fn progress_early_late_and_interleaved_events_preserve_two_session_isolation() {
    let (manager, spawner) = manager_with_fake();
    let (events_a, interrupt_a) = manager
        .run_prompt(request("alpha", Some("thr_a")))
        .into_parts();
    let (events_b, interrupt_b) = manager
        .run_prompt(request("beta", Some("thr_b")))
        .into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let mut requests = HashMap::new();
    while requests.len() < 2 {
        let r = endpoint.recv();
        match r["method"].as_str().unwrap() {
            "thread/resume" => {
                endpoint.respond(&r, json!({"thread":{"id":r["params"]["threadId"]}}));
            }
            "turn/start" => {
                requests.insert(r["params"]["threadId"].as_str().unwrap().to_owned(), r);
            }
            m => panic!("unexpected {m}"),
        }
    }
    // Both owners emit progress before their turn/start response.
    for (thread, turn, text) in [("thr_a", "turn_a", "A"), ("thr_b", "turn_b", "B")] {
        endpoint.send(progress_message(
            "item/plan/delta",
            thread,
            turn,
            json!({"itemId":"same","delta":text}),
        ));
        endpoint.send(progress_message(
            "turn/plan/updated",
            thread,
            turn,
            json!({"plan":[{"step":text,"status":"inProgress"}]}),
        ));
        endpoint.send(progress_message(
            "item/started",
            thread,
            turn,
            json!({"item":{"type":"sleep","id":"wait","durationMs":15000}}),
        ));
    }
    endpoint.respond(&requests["thr_b"], json!({"turn":{"id":"turn_b"}}));
    endpoint.respond(&requests["thr_a"], json!({"turn":{"id":"turn_a"}}));
    let completed_plan = progress_message(
        "item/completed",
        "thr_a",
        "turn_a",
        json!({"item":{"type":"plan","id":"same","text":"final A"}}),
    );
    endpoint.send(completed_plan.clone());
    endpoint.send(completed_plan);
    complete(&endpoint, "thr_a", "turn_a", "interrupted");
    let a = collect_terminal(&events_a);
    assert_eq!(a.last(), Some(&AgentEvent::Interrupted));
    assert!(a.contains(&AgentEvent::PlanDelta {
        item_id: "same".into(),
        delta: "A".into()
    }));
    assert!(!a.contains(&AgentEvent::PlanDelta {
        item_id: "same".into(),
        delta: "B".into()
    }));
    // A new turn is starting on the same thread while B is still running.
    let (events_next, interrupt_next) = manager
        .run_prompt(request("next", Some("thr_a")))
        .into_parts();
    let next = endpoint.recv();
    assert_eq!(next["method"], "turn/start");
    endpoint.send(progress_message(
        "item/plan/delta",
        "thr_a",
        "turn_a",
        json!({"itemId":"same","delta":"stale"}),
    ));
    endpoint.send(progress_message(
        "item/completed",
        "thr_a",
        "turn_a",
        json!({"item":{"type":"sleep","id":"wait","durationMs":15000}}),
    ));
    complete(&endpoint, "thr_a", "turn_a", "interrupted");
    endpoint.send(progress_message(
        "item/plan/delta",
        "thr_a",
        "turn_next",
        json!({"itemId":"same","delta":"next"}),
    ));
    endpoint.respond(&next, json!({"turn":{"id":"turn_next"}}));
    endpoint.send(progress_message("item/completed","thr_b","turn_b",json!({"item":{"type":"webSearch","id":"search","query":"B","action":{"type":"other"},"results":[{"extension":true}]}})));
    complete(&endpoint, "thr_b", "turn_b", "completed");
    complete(&endpoint, "thr_a", "turn_next", "completed");
    let b = collect_terminal(&events_b);
    let n = collect_terminal(&events_next);
    assert!(b.contains(&AgentEvent::PlanDelta {
        item_id: "same".into(),
        delta: "B".into()
    }));
    assert_eq!(b.last(), Some(&AgentEvent::Completed));
    assert!(n.contains(&AgentEvent::PlanDelta {
        item_id: "same".into(),
        delta: "next".into()
    }));
    assert!(!n.contains(&AgentEvent::PlanDelta {
        item_id: "same".into(),
        delta: "stale".into()
    }));
    assert_eq!(n.last(), Some(&AgentEvent::Completed));
    assert!(endpoint.process.is_alive());
    drop((interrupt_a, interrupt_b, interrupt_next));
    manager.shutdown();
}

#[test]
fn progress_after_buffered_turn_completion_does_not_fail_the_generation() {
    let (manager, spawner) = manager_with_fake();
    let (events, interrupt) = manager
        .run_prompt(request("buffered", Some("thr_a")))
        .into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let resume = endpoint.recv();
    endpoint.respond(&resume, json!({"thread":{"id":"thr_a"}}));
    let start = endpoint.recv();
    assert_eq!(start["method"], "turn/start");
    endpoint.send(progress_message(
        "item/completed",
        "thr_a",
        "turn_a",
        json!({"item":{"type":"plan","id":"p","text":"final"}}),
    ));
    complete(&endpoint, "thr_a", "turn_a", "completed");
    endpoint.send(progress_message(
        "item/plan/delta",
        "thr_a",
        "turn_a",
        json!({"itemId":"p","delta":"late"}),
    ));
    complete(&endpoint, "thr_a", "turn_a", "completed");
    endpoint.respond(&start, json!({"turn":{"id":"turn_a"}}));
    let events = collect_terminal(&events);
    assert_eq!(events.last(), Some(&AgentEvent::Completed));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AgentEvent::PlanDelta { .. }))
    );
    assert!(endpoint.process.is_alive());
    drop(interrupt);
    manager.shutdown();
}
