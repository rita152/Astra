use anyhow::{Context as _, Result, anyhow};
use serde_json::{Value, json};

use super::{
    object_field, parse_history_item, parse_history_turn, parse_page, parse_project,
    parse_thread_section, parse_thread_summary, string_field,
};
use crate::agent::{ThreadHistoryItem, ThreadHistoryItemEntry, ThreadSearchResult};

const METHODS: [&str; 6] = [
    "project/list",
    "thread/list",
    "thread/search",
    "thread/turns/list",
    "thread/items/list",
    "threadSection/list",
];

fn assert_envelope_error(method: &str, response: &Value, expected: &str) {
    let result = parse_page::<()>(response, method, |_| {
        panic!("the entry decoder must not run for this envelope")
    });
    let Err(error) = result else {
        panic!("invalid page must fail");
    };
    assert_eq!(format!("{error:#}"), expected);
}

#[test]
fn preserves_entry_order_duplicates_and_both_cursors() {
    let response = json!({
        "result": {
            "data": [3, 1, 3],
            "nextCursor": "next/+=",
            "backwardsCursor": "previous",
            "unknownField": true
        }
    });
    for method in METHODS {
        let mut visited = Vec::new();
        let page = parse_page(&response, method, |entry| {
            let number = entry.as_i64().context("entry must be an integer")?;
            visited.push(number);
            Ok(number * 10)
        })
        .unwrap();
        assert_eq!(visited, vec![3, 1, 3]);
        assert_eq!(page.data, vec![30, 10, 30]);
        assert_eq!(page.next_cursor.as_deref(), Some("next/+="));
        assert_eq!(page.backwards_cursor.as_deref(), Some("previous"));
    }
}

#[test]
fn accepts_empty_pages_with_missing_null_or_string_cursors() {
    let cursors = [
        None,
        Some(Value::Null),
        Some(json!("")),
        Some(json!("游标/+=?")),
    ];
    for method in METHODS {
        for next in &cursors {
            for backwards in &cursors {
                let mut response = json!({"result": {"data": []}});
                let result = response["result"].as_object_mut().unwrap();
                if let Some(cursor) = next {
                    result.insert("nextCursor".into(), cursor.clone());
                }
                if let Some(cursor) = backwards {
                    result.insert("backwardsCursor".into(), cursor.clone());
                }
                let page = parse_page::<()>(&response, method, |_| {
                    panic!("empty pages must not invoke the entry decoder")
                })
                .unwrap();
                assert!(page.data.is_empty());
                assert_eq!(
                    page.next_cursor.as_deref(),
                    next.as_ref().and_then(Value::as_str),
                );
                assert_eq!(
                    page.backwards_cursor.as_deref(),
                    backwards.as_ref().and_then(Value::as_str),
                );
            }
        }
    }
}

#[test]
fn reports_missing_result_with_the_original_method_context() {
    for method in METHODS {
        for response in [
            Value::Null,
            json!([]),
            json!({}),
            json!({"error": {"code": -1}}),
        ] {
            assert_envelope_error(method, &response, &format!("{method} 响应缺少 result"));
        }
    }
}

#[test]
fn reports_missing_data_for_non_object_or_incomplete_results() {
    for method in METHODS {
        for result in [
            Value::Null,
            json!(false),
            json!(1),
            json!("result"),
            json!([]),
            json!({}),
        ] {
            assert_envelope_error(
                method,
                &json!({"result": result}),
                &format!("{method} result 缺少字段 `data`"),
            );
        }
    }
}

#[test]
fn rejects_non_array_data_before_decoding_entries() {
    for method in METHODS {
        for data in [Value::Null, json!(false), json!(1), json!("data"), json!({})] {
            assert_envelope_error(
                method,
                &json!({"result": {"data": data, "nextCursor": false}}),
                &format!("{method} result.data 必须是数组"),
            );
        }
    }
}

#[test]
fn rejects_invalid_cursor_types_without_coercing_them() {
    for method in METHODS {
        for field in ["nextCursor", "backwardsCursor"] {
            for cursor in [json!(false), json!(17), json!(1.5), json!([]), json!({})] {
                let mut response = json!({"result": {"data": []}});
                response["result"][field] = cursor;
                assert_envelope_error(
                    method,
                    &response,
                    &format!("{method} result.{field} 必须是字符串或 null"),
                );
            }
        }
    }
}

#[test]
fn preserves_entry_error_chains_and_stops_at_the_first_failed_entry() {
    let response = json!({"result": {"data": [1, 2, 3], "nextCursor": false}});
    for method in METHODS {
        let mut visited = Vec::new();
        let result = parse_page(&response, method, |entry| -> Result<i64> {
            let number = entry.as_i64().context("entry must be an integer")?;
            visited.push(number);
            if number == 2 {
                return Err(anyhow!("bad entry")).context("entry decoder");
            }
            Ok(number)
        });
        assert_eq!(visited, vec![1, 2]);
        let Err(error) = result else {
            panic!("entry decoding must fail");
        };
        assert_eq!(format!("{error:#}"), "entry decoder: bad entry");
    }
}

#[test]
fn decodes_entries_before_cursors_and_validates_next_cursor_first() {
    let response = json!({
        "result": {"data": [1, 2, 3], "nextCursor": false, "backwardsCursor": false}
    });
    for method in METHODS {
        let mut visited = Vec::new();
        let result = parse_page(&response, method, |entry| {
            visited.push(entry.clone());
            Ok(())
        });
        assert_eq!(visited, vec![json!(1), json!(2), json!(3)]);
        let Err(error) = result else {
            panic!("invalid cursor must fail");
        };
        assert_eq!(
            format!("{error:#}"),
            format!("{method} result.nextCursor 必须是字符串或 null"),
        );
    }
}

fn thread_value() -> Value {
    json!({
        "id": "thread-1",
        "preview": "hello",
        "cwd": "/work",
        "projectId": null,
        "createdAt": 1,
        "updatedAt": 2,
        "status": {"type": "idle"}
    })
}

#[test]
fn decodes_project_pages_with_the_existing_entry_parser() {
    let response = json!({"result": {"data": [{
        "id": "project-1", "name": "Project", "roots": [{"path": "/work"}],
        "createdAt": 1, "updatedAt": 2, "position": 3
    }]}});
    let page = parse_page(&response, "project/list", parse_project).unwrap();
    assert_eq!(page.data.len(), 1);
    assert_eq!(page.data[0].project_id, "project-1");
    assert_eq!(page.data[0].roots[0].to_str(), Some("/work"));
}

#[test]
fn decodes_thread_pages_with_the_existing_entry_parser() {
    let response = json!({"result": {"data": [thread_value()]}});
    let page = parse_page(&response, "thread/list", parse_thread_summary).unwrap();
    assert_eq!(page.data.len(), 1);
    assert_eq!(page.data[0].thread_id, "thread-1");
    assert_eq!(page.data[0].title, "hello");
}

#[test]
fn decodes_search_pages_with_the_nested_thread_and_snippet() {
    let response = json!({"result": {"data": [{"thread": thread_value(), "snippet": "match"}]}});
    let page = parse_page(&response, "thread/search", |entry| {
        Ok(ThreadSearchResult {
            thread: parse_thread_summary(object_field(entry, "thread", "thread/search entry")?)?,
            snippet: string_field(entry, "snippet", "thread/search entry")?,
        })
    })
    .unwrap();
    assert_eq!(page.data.len(), 1);
    assert_eq!(page.data[0].thread.thread_id, "thread-1");
    assert_eq!(page.data[0].snippet, "match");
}

#[test]
fn decodes_turn_pages_with_the_existing_entry_parser() {
    let response = json!({"result": {"data": [{
        "id": "turn-1", "status": "completed", "items": []
    }]}});
    let page = parse_page(&response, "thread/turns/list", parse_history_turn).unwrap();
    assert_eq!(page.data.len(), 1);
    assert_eq!(page.data[0].turn_id, "turn-1");
    assert!(page.data[0].items.is_empty());
}

#[test]
fn decodes_item_pages_with_the_enclosing_turn_id() {
    let response = json!({"result": {"data": [{
        "turnId": "turn-1", "item": {"id": "item-1", "type": "agentMessage", "text": "hello"}
    }]}});
    let page = parse_page(&response, "thread/items/list", |entry| {
        Ok(ThreadHistoryItemEntry {
            turn_id: string_field(entry, "turnId", "thread/items/list entry")?,
            item: parse_history_item(object_field(entry, "item", "thread/items/list entry")?)?,
        })
    })
    .unwrap();
    assert_eq!(page.data.len(), 1);
    assert_eq!(page.data[0].turn_id, "turn-1");
    assert!(matches!(
        &page.data[0].item,
        ThreadHistoryItem::AssistantMessage { item_id, text, .. }
            if item_id == "item-1" && text == "hello"
    ));
}

#[test]
fn decodes_section_pages_with_the_existing_entry_parser() {
    let response = json!({"result": {"data": [{
        "id": "section-1", "name": "Section", "appearance": {"icon": "star", "color": null}
    }]}});
    let page = parse_page(&response, "threadSection/list", parse_thread_section).unwrap();
    assert_eq!(page.data.len(), 1);
    assert_eq!(page.data[0].section_id, "section-1");
    assert_eq!(
        page.data[0].appearance.as_ref().unwrap().icon.as_deref(),
        Some("star"),
    );
}
