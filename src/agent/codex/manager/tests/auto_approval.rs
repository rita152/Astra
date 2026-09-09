use super::*;
use crate::agent::AgentAutoApprovalReviewStatus;

fn review(thread: &str, turn: &str, id: &str, status: &str) -> Value {
    let completed = status != "inProgress";
    let mut params = json!({"threadId":thread,"turnId":turn,"reviewId":id,"targetItemId":null,"startedAtMs":100,"action":{"type":"networkAccess","host":"example.com","port":443,"protocol":"https","target":"example.com:443"},"review":{"status":status,"rationale":"Public read","riskLevel":"low","userAuthorization":"high"}});
    if completed {
        params["completedAtMs"] = json!(200);
        params["decisionSource"] = json!("agent");
    }
    json!({"method":if completed {"item/autoApprovalReview/completed"}else{"item/autoApprovalReview/started"},"params":params})
}

#[test]
fn auto_approval_early_interleaved_reviews_keep_two_turn_streams_independent() {
    let (manager, spawner) = manager_with_fake();
    let connection_events = manager.subscribe_connection_events();
    let (a, handle_a) = manager.run_prompt(request("a", Some("a"))).into_parts();
    let (b, handle_b) = manager.run_prompt(request("b", Some("b"))).into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    let mut requests = HashMap::new();
    while requests.len() < 2 {
        let message = endpoint.recv();
        match message["method"].as_str().unwrap() {
            "thread/resume" => endpoint.respond(
                &message,
                json!({"thread":{"id":message["params"]["threadId"]}}),
            ),
            "turn/start" => {
                requests.insert(
                    message["params"]["threadId"].as_str().unwrap().to_owned(),
                    message,
                );
            }
            other => panic!("unexpected {other}"),
        }
    }
    endpoint.send(review("a", "ta", "same", "inProgress"));
    endpoint.send(review("b", "tb", "same", "denied"));
    endpoint.send(json!({"method":"autoApprovalReview/strictReviewRequired","params":{"threadId":"a","turnId":"ta","startedAtMs":125}}));
    endpoint.send(
        json!({"method":"guardianWarning","params":{"threadId":"b","message":"Review warning"}}),
    );
    // A completion and its trailing review can both precede the turn/start reply.
    complete(&endpoint, "b", "tb", "interrupted");
    endpoint.send(review("b", "tb", "late", "aborted"));
    endpoint.respond(&requests["a"], json!({"turn":{"id":"ta"}}));
    endpoint.respond(&requests["b"], json!({"turn":{"id":"tb"}}));
    endpoint.send(review("a", "ta", "same", "approved"));
    endpoint.send(json!({"method":"item/agentMessage/delta","params":{"threadId":"a","turnId":"ta","itemId":"answer","delta":"Still working"}}));
    complete(&endpoint, "a", "ta", "completed");
    let a = collect_terminal(&a);
    let b = collect_terminal(&b);
    for (thread, events) in [("a", &a), ("b", &b)] {
        assert!(
            events
                .iter()
                .filter_map(|e| if let AgentEvent::AutoApprovalReviewUpdated(r) = e {
                    Some(r)
                } else {
                    None
                })
                .all(|r| r.key.thread_id == thread)
        );
    }
    assert!(a.contains(&AgentEvent::TextDelta("Still working".into())));
    assert_eq!(a.last(), Some(&AgentEvent::Completed));
    assert!(!a.iter().chain(&b).any(|e| matches!(
        e,
        AgentEvent::AutoApprovalReviewUpdated(_) | AgentEvent::StrictReviewRequired(_)
    )));
    assert_eq!(b.last(), Some(&AgentEvent::Interrupted));
    let observations = std::iter::from_fn(|| connection_events.try_recv().ok()).collect::<Vec<_>>();
    assert!(
        observations
            .iter()
            .any(|e| matches!(e,AgentConnectionEvent::GuardianWarning(w) if w.thread_id=="b"))
    );
    assert!(
        observations
            .iter()
            .any(|e| matches!(e,AgentConnectionEvent::StrictReviewRequired(r) if r.thread_id=="a"))
    );
    // Unbound review observations must wait for the actual turn/start identity,
    // not bind an arbitrary early turn ID to the pending request.
    for (thread, events) in [("a", a), ("b", b)] {
        let mut state = crate::conversation::ConversationState {
            thread_id: Some(thread.to_owned()),
            ..Default::default()
        };
        state.begin_prompt(thread);
        for event in &observations {
            state.apply_connection_event(event.clone());
        }
        state.apply_agent_event_batch(events);
        let reviews = state
            .activities
            .iter()
            .filter_map(|a| {
                if let crate::conversation::ConversationActivity::AutoApprovalReview(r) = a {
                    Some(r)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert!(reviews.iter().all(|r| r.review.key.thread_id == thread));
        assert_eq!(reviews.len(), if thread == "a" { 1 } else { 2 });
        assert!(state.approval_responders.is_empty());
    }
    assert!(endpoint.process.is_alive());
    drop(handle_a);
    drop(handle_b);
    manager.shutdown();
}

#[test]
fn auto_approval_late_notifications_cannot_bind_or_finish_the_next_turn() {
    let (manager, spawner) = manager_with_fake();
    let notifications = manager.subscribe_connection_events();
    let (first, handle) = manager.run_prompt(request("first", Some("a"))).into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "a", "old");
    endpoint.send(review("a", "old", "r", "inProgress"));
    complete(&endpoint, "a", "old", "interrupted");
    assert_eq!(
        collect_terminal(&first).last(),
        Some(&AgentEvent::Interrupted)
    );
    while notifications.try_recv().is_ok() {}
    let (second, second_handle) = manager
        .run_prompt(request("second", Some("a")))
        .into_parts();
    let next = endpoint.recv();
    assert_eq!(next["method"], "turn/start");
    endpoint.send(review("a", "unknown-prior-turn", "foreign", "approved"));
    endpoint.send(review("a", "old", "r", "approved"));
    endpoint.send(review("a", "old", "r", "inProgress"));
    let _foreign = wait_value(&notifications);
    let late = wait_value(&notifications);
    assert!(
        matches!(late,AgentConnectionEvent::AutoApprovalReviewUpdated(r) if r.key.turn_id=="old"&&r.status==AgentAutoApprovalReviewStatus::Approved)
    );
    endpoint.respond(&next, json!({"turn":{"id":"new"}}));
    endpoint.send(review("a", "new", "r", "inProgress"));
    complete(&endpoint, "a", "new", "completed");
    let events = collect_terminal(&second);
    assert!(
        events
            .iter()
            .filter_map(|e| if let AgentEvent::AutoApprovalReviewUpdated(r) = e {
                Some(r)
            } else {
                None
            })
            .all(|r| r.key.turn_id == "new")
    );
    assert_eq!(events.last(), Some(&AgentEvent::Completed));
    assert!(endpoint.process.is_alive());
    let replay = manager.subscribe_connection_events();
    assert!(
        std::iter::from_fn(|| replay.try_recv().ok()).any(|event|matches!(event,AgentConnectionEvent::AutoApprovalReviewUpdated(r) if r.key.turn_id=="old"&&r.status==AgentAutoApprovalReviewStatus::Approved))
    );
    drop(handle);
    drop(second_handle);
    manager.shutdown();
}

#[test]
fn auto_approval_unowned_notifications_do_not_poison_an_active_thread() {
    let (manager, spawner) = manager_with_fake();
    let (events, handle) = manager.run_prompt(request("a", Some("a"))).into_parts();
    let mut endpoint = spawner.next_endpoint();
    handshake(&mut endpoint);
    start_known_turn(&mut endpoint, "a", "turn");
    endpoint.send(review("unloaded", "foreign", "r", "approved"));
    endpoint.send(review("a", "unknown-old-turn", "r", "timedOut"));
    complete(&endpoint, "a", "turn", "completed");
    let events = collect_terminal(&events);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AgentEvent::AutoApprovalReviewUpdated(_)))
    );
    assert_eq!(events.last(), Some(&AgentEvent::Completed));
    assert!(endpoint.process.is_alive());
    drop(handle);
    manager.shutdown();
}

#[test]
fn auto_approval_hub_replay_keeps_completed_details_and_deduplicates_updates() {
    let mut hub = super::super::events::ConnectionEventHub::default();
    let parse = |message| super::super::super::auto_approval::parse_review(&message).unwrap();
    let first = parse(review("a", "turn", "r", "inProgress"));
    let receiver = hub.subscribe();
    hub.publish(AgentConnectionEvent::AutoApprovalReviewUpdated(Box::new(
        first.clone(),
    )));
    let mut done = parse(review("a", "turn", "r", "approved"));
    done.rationale = None;
    done.risk_level = None;
    done.user_authorization = None;
    hub.publish(AgentConnectionEvent::AutoApprovalReviewUpdated(Box::new(
        done.clone(),
    )));
    hub.publish(AgentConnectionEvent::AutoApprovalReviewUpdated(Box::new(
        done,
    )));
    hub.publish(AgentConnectionEvent::AutoApprovalReviewUpdated(Box::new(
        first,
    )));
    let mut stale = parse(review("a", "turn", "r", "denied"));
    stale.completed_at_ms = Some(150);
    hub.publish(AgentConnectionEvent::AutoApprovalReviewUpdated(Box::new(
        stale,
    )));
    assert_eq!(std::iter::from_fn(|| receiver.try_recv().ok()).count(), 2);
    let replay = hub.subscribe();
    let AgentConnectionEvent::AutoApprovalReviewUpdated(value) = wait_value(&replay) else {
        panic!()
    };
    assert_eq!(value.status, AgentAutoApprovalReviewStatus::Approved);
    assert_eq!(value.rationale.as_deref(), Some("Public read"));
    assert_eq!(value.risk_level.as_deref(), Some("low"));
    assert_eq!(value.completed_at_ms, Some(200));
}
