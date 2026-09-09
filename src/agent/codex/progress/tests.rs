use super::super::{process_turn_message, session::CodexTurnSession};
use super::*;
use serde_json::json;
use std::sync::Arc;

fn dispatch(message: Value) -> Result<Vec<AgentEvent>> {
    let session = Arc::new(CodexTurnSession::new(Vec::<u8>::new(), None));
    let (tx, rx) = async_channel::unbounded();
    assert!(
        process_turn_message(&session, &message, "thread-a", "turn-a", &tx, &mut false)?.is_none()
    );
    Ok(std::iter::from_fn(|| rx.try_recv().ok()).collect())
}
fn item_message(kind: &str, item: Value) -> Value {
    json!({"method":kind,"params":{"threadId":"thread-a","turnId":"turn-a","item":item,"startedAtMs":1000,"completedAtMs":2000}})
}
#[test]
fn progress_items_have_identical_live_and_history_payloads() {
    for value in [
        json!({"type":"plan","id":"same","text":"最终计划"}),
        json!({"type":"sleep","id":"same","durationMs":u64::MAX}),
        json!({"type":"webSearch","id":"same","query":"Rust","action":{"type":"search","queries":["a","b"],"extension":42},"results":[{"kind":"future","nested":{"x":[1,null]}}],"newMetadata":{"score":0.5}}),
    ] {
        let completed = dispatch(item_message("item/completed", value.clone()))
            .unwrap()
            .pop()
            .unwrap();
        let history = parse_progress_history(&value).unwrap();
        assert_eq!(
            completed,
            match history {
                ThreadHistoryItem::Plan(v) => AgentEvent::PlanUpdated(v),
                ThreadHistoryItem::WebSearch(v) => AgentEvent::WebSearchUpdated(v),
                ThreadHistoryItem::Sleep(v) => AgentEvent::SleepUpdated(v),
                _ => panic!(),
            }
        );
        let started = dispatch(item_message("item/started", value))
            .unwrap()
            .pop()
            .unwrap();
        let status = match started {
            AgentEvent::PlanUpdated(v) => v.status,
            AgentEvent::WebSearchUpdated(v) => v.status,
            AgentEvent::SleepUpdated(v) => v.status,
            _ => panic!(),
        };
        assert_eq!(status, AgentActivityStatus::InProgress);
    }
}
#[test]
fn all_search_action_variants_and_optional_fields_remain_lossless() {
    for action in [
        Value::Null,
        json!({"type":"search"}),
        json!({"type":"search","query":null,"queries":null}),
        json!({"type":"search","query":"one","queries":[]}),
        json!({"type":"openPage","url":null}),
        json!({"type":"openPage","url":"https://rust-lang.org"}),
        json!({"type":"findInPage","pattern":"Rust","url":"https://rust-lang.org","extra":[42]}),
        json!({"type":"other","future":{"data":true}}),
    ] {
        let value = json!({"type":"webSearch","id":"s","query":"","action":action,"results":null,"extension":"preserve"});
        let ThreadHistoryItem::WebSearch(v) = parse_progress_history(&value).unwrap() else {
            panic!()
        };
        assert_eq!(v.action, action);
        assert_eq!(v.extra["extension"], "preserve");
        assert!(v.results.is_null());
    }
    assert!(parse_progress_history(&json!({"type":"webSearch","id":"s","query":""})).is_ok());
}
#[test]
fn malformed_progress_items_fail_with_correlated_diagnostics() {
    for value in [
        json!({"type":"plan","id":"p"}),
        json!({"type":"plan","id":"p","text":null}),
        json!({"type":"sleep","id":"s","durationMs":-1}),
        json!({"type":"sleep","id":"s","durationMs":1.5}),
        json!({"type":"sleep","id":"s","durationMs":"1000"}),
        json!({"type":"webSearch","id":"s","query":null}),
        json!({"type":"webSearch","id":"s","query":"q","results":{}}),
        json!({"type":"webSearch","id":"s","query":"q","action":{"type":"future"}}),
        json!({"type":"webSearch","id":"s","query":"q","action":{"type":"search","queries":[1]}}),
        json!({"type":"webSearch","id":"s","query":"q","action":{"type":"openPage","url":1}}),
    ] {
        for method in ["item/started", "item/completed"] {
            let error = dispatch(item_message(method, value.clone()))
                .unwrap_err()
                .to_string();
            for needle in [
                method,
                "thread-a",
                "turn-a",
                value["type"].as_str().unwrap(),
            ] {
                assert!(error.contains(needle), "{error}");
            }
            assert!(parse_progress_history(&value).is_err());
        }
    }
}
#[test]
fn plan_steps_and_text_deltas_validate_scope_and_required_fields() {
    let delta = json!({"method":"item/plan/delta","params":{"threadId":"thread-a","turnId":"turn-a","itemId":"p","delta":"ab"}});
    assert_eq!(
        dispatch(delta.clone()).unwrap(),
        vec![AgentEvent::PlanDelta {
            item_id: "p".into(),
            delta: "ab".into()
        }]
    );
    for field in ["threadId", "turnId", "itemId", "delta"] {
        let mut bad = delta.clone();
        bad["params"].as_object_mut().unwrap().remove(field);
        assert!(dispatch(bad).is_err());
    }
    for field in ["threadId", "turnId"] {
        let mut other = delta.clone();
        other["params"][field] = json!("other");
        assert!(dispatch(other).is_err());
    }
    let value = json!({"method":"turn/plan/updated","params":{"threadId":"thread-a","turnId":"turn-a","explanation":null,"plan":[{"step":"a","status":"pending"},{"step":"b","status":"inProgress"},{"step":"c","status":"completed"}]}});
    let AgentEvent::TurnPlanUpdated(plan) = dispatch(value.clone()).unwrap().pop().unwrap() else {
        panic!()
    };
    assert_eq!(plan.steps.len(), 3);
    assert_eq!(plan.turn_id, "turn-a");
    for bad in [
        json!({"step":"x","status":"in_progress"}),
        json!({"step":1,"status":"pending"}),
        json!({"status":"pending"}),
    ] {
        let mut v = value.clone();
        v["params"]["plan"] = json!([bad]);
        assert!(dispatch(v).is_err());
    }
    let mut empty = value;
    empty["params"]["plan"] = json!([]);
    assert!(dispatch(empty).is_ok());
}
