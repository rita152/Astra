use super::*;
use serde_json::json;

fn notification(action: Value, status: &str, completed: bool) -> Value {
    let mut params = json!({"threadId":"thread-a","turnId":"turn-a","reviewId":"review-a","targetItemId":null,"startedAtMs":42,"action":action,"review":{"status":status,"rationale":null,"riskLevel":null,"userAuthorization":null}});
    if completed {
        params["completedAtMs"] = json!(84);
        params["decisionSource"] = json!("agent");
    }
    json!({"method":if completed { REVIEW_METHODS[1] } else { REVIEW_METHODS[0] },"params":params})
}

fn actions() -> Vec<Value> {
    vec![
        json!({"type":"command","command":"pwd","cwd":"/tmp","source":"shell"}),
        json!({"type":"execve","program":"/bin/ls","argv":["ls","a b"],"cwd":"/tmp","source":"unifiedExec"}),
        json!({"type":"writeStdin","approvalId":"child","processId":"process","stdin":"answer\n","cwd":"legacy/relative"}),
        json!({"type":"applyPatch","files":["/tmp/a","/tmp/b"],"cwd":"/tmp"}),
        json!({"type":"networkAccess","host":"example.com","port":65535,"protocol":"socks5Udp","target":"example.com:65535"}),
        json!({"type":"mcpToolCall","server":"s","toolName":"t","toolTitle":null,"connectorId":null,"connectorName":null}),
        json!({"type":"requestPermissions","reason":"read test fixture","permissions":{"network":{"enabled":false},"fileSystem":{"read":null,"write":["/tmp/a"],"globScanMaxDepth":2,"entries":[{"access":"deny","path":{"type":"glob_pattern","pattern":"**/*.key"}},{"access":"read","path":{"type":"special","value":{"kind":"project_roots","subpath":null}}}]}}}),
    ]
}

#[test]
fn all_schema_actions_statuses_nullable_targets_and_sparse_fields_decode() {
    for action in actions() {
        for status in ["inProgress", "approved", "denied", "timedOut", "aborted"] {
            // The schema shares its enum between both methods; do not invent a narrower enum.
            for completed in [false, true] {
                let message = notification(action.clone(), status, completed);
                super::super::methods::ensure_server_method_is_defined(&message).unwrap();
                let review = parse_review(&message).unwrap();
                assert_eq!(review.key.thread_id, "thread-a");
                assert_eq!(review.key.turn_id, "turn-a");
                assert_eq!(review.key.review_id, "review-a");
                assert_eq!(review.target_item_id, None);
                assert_eq!(review.completed_at_ms, completed.then_some(84));
                let mut sparse = message;
                sparse["params"]
                    .as_object_mut()
                    .unwrap()
                    .remove("targetItemId");
                sparse["params"]["review"] = json!({"status":status});
                assert_eq!(parse_review(&sparse).unwrap(), review);
            }
        }
    }
}

#[test]
fn review_preserves_detail_and_separate_child_identity() {
    let mut message = notification(actions().remove(2), "denied", true);
    message["params"]["targetItemId"] = json!("parent-command");
    message["params"]["review"] = json!({"status":"denied","rationale":"Needs authorization","riskLevel":"critical","userAuthorization":"unknown"});
    let review = parse_review(&message).unwrap();
    assert!(
        matches!(review.action, Action::WriteStdin { approval_id, process_id, stdin, .. } if approval_id=="child" && process_id=="process" && stdin=="answer\n")
    );
    assert_eq!(review.rationale.as_deref(), Some("Needs authorization"));
    assert_eq!(review.risk_level.as_deref(), Some("critical"));
    assert_eq!(review.user_authorization.as_deref(), Some("unknown"));
    assert_eq!(review.target_item_id.as_deref(), Some("parent-command"));
    assert_eq!(review.started_at_ms, 42);
    assert_eq!(review.completed_at_ms, Some(84));
}

#[test]
fn review_schema_rejects_bad_shapes_and_unknown_enums() {
    let base = notification(actions().remove(0), "approved", true);
    for field in [
        "threadId",
        "turnId",
        "reviewId",
        "startedAtMs",
        "action",
        "review",
        "completedAtMs",
        "decisionSource",
    ] {
        let mut invalid = base.clone();
        invalid["params"].as_object_mut().unwrap().remove(field);
        assert!(parse_review(&invalid).is_err(), "{field}");
    }
    for (pointer, value) in [
        ("/params/targetItemId", json!(1)),
        ("/params/review/status", json!("done")),
        ("/params/review/riskLevel", json!("none")),
        ("/params/review/userAuthorization", json!("critical")),
        ("/params/review/rationale", json!([])),
        ("/params/decisionSource", json!("user")),
        ("/params/action/source", json!("other")),
        ("/params/action/cwd", json!("relative")),
        ("/params/startedAtMs", json!(0.5)),
    ] {
        let mut invalid = base.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        assert!(parse_review(&invalid).is_err(), "{pointer}");
    }
    let mut network = notification(actions().remove(4), "inProgress", false);
    for port in [-1, 65536] {
        network["params"]["action"]["port"] = json!(port);
        assert!(parse_review(&network).is_err());
    }
    let mut permissions = notification(actions().remove(6), "inProgress", false);
    permissions["params"]["action"]["permissions"]["unknown"] = json!(true);
    assert!(parse_review(&permissions).is_err());
}

#[test]
fn strict_review_and_guardian_warning_keep_their_schema_scopes() {
    let strict =
        json!({"method":REVIEW_METHODS[2],"params":{"threadId":"a","turnId":"b","startedAtMs":12}});
    assert_eq!(parse_strict_review(&strict).unwrap().turn_id, "b");
    assert!(!super::super::methods::is_integrated_server_request_method(
        REVIEW_METHODS[2]
    ));
    let warning = json!({"method":"guardianWarning","params":{"threadId":"a","message":"Review unavailable"}});
    assert_eq!(
        parse_guardian_warning(&warning).unwrap().message,
        "Review unavailable"
    );
    assert!(parse_guardian_warning(&json!({"params":{"message":"missing thread"}})).is_err());
    assert!(
        parse_strict_review(&json!({"params":{"threadId":"a","turnId":"b","startedAtMs":null}}))
            .is_err()
    );
}
