//! Blocking backend reads used by workspace worker threads.

use std::collections::HashSet;

use async_channel::Receiver;

use crate::agent::{
    AgentBackend, HistoryItemDetail, Page, PageRequest, Project, ThreadId, ThreadListRequest,
    ThreadSearchResult, ThreadSection, ThreadSummary, ThreadTurn, WorkspaceError, WorkspaceResult,
};

const PAGE_SIZE: u32 = 100;

pub(super) fn receive<T>(
    receiver: Receiver<WorkspaceResult<T>>,
    label: &str,
) -> WorkspaceResult<T> {
    receiver
        .recv_blocking()
        .map_err(|_| WorkspaceError::backend(format!("{label}响应通道提前关闭")))?
}

pub(super) fn load_all_projects(backend: &dyn AgentBackend) -> WorkspaceResult<Vec<Project>> {
    load_all_pages(|cursor| {
        backend.list_projects(PageRequest {
            cursor,
            limit: PAGE_SIZE,
        })
    })
}

pub(super) fn load_all_sections(backend: &dyn AgentBackend) -> WorkspaceResult<Vec<ThreadSection>> {
    load_all_pages(|cursor| {
        backend.list_thread_sections(PageRequest {
            cursor,
            limit: PAGE_SIZE,
        })
    })
}

pub(super) fn load_all_threads(
    backend: &dyn AgentBackend,
    request: ThreadListRequest,
) -> WorkspaceResult<Vec<ThreadSummary>> {
    load_all_pages(|cursor| {
        let mut request = request.clone();
        request.page.cursor = cursor;
        request.page.limit = PAGE_SIZE;
        backend.list_threads(request)
    })
}

pub(super) fn load_all_search_results(
    backend: &dyn AgentBackend,
    search_term: String,
) -> WorkspaceResult<Vec<ThreadSearchResult>> {
    load_all_pages(|cursor| {
        backend.search_threads(ThreadListRequest {
            page: PageRequest {
                cursor,
                limit: PAGE_SIZE,
            },
            search_term: Some(search_term.clone()),
            ..ThreadListRequest::default()
        })
    })
}

pub(super) fn load_all_turns(
    backend: &dyn AgentBackend,
    thread_id: ThreadId,
) -> WorkspaceResult<Page<ThreadTurn>> {
    collect_pages("会话历史", |cursor| {
        let mut page = receive(
            backend.list_thread_turns(
                thread_id.clone(),
                PageRequest {
                    cursor,
                    limit: PAGE_SIZE,
                },
                HistoryItemDetail::Full,
            ),
            "读取会话历史",
        )?;
        for turn in &mut page.data {
            if turn.items_view != HistoryItemDetail::Full {
                let turn_id = turn.turn_id.clone();
                let entries = load_all_pages(|cursor| {
                    backend.list_thread_items(
                        thread_id.clone(),
                        Some(turn_id.clone()),
                        PageRequest {
                            cursor,
                            limit: PAGE_SIZE,
                        },
                    )
                })?;
                turn.items = entries
                    .into_iter()
                    .filter(|entry| entry.turn_id == turn.turn_id)
                    .map(|entry| entry.item)
                    .collect();
                turn.items_view = HistoryItemDetail::Full;
            }
        }
        Ok(page)
    })
}

fn load_all_pages<T>(
    mut request: impl FnMut(Option<String>) -> Receiver<WorkspaceResult<Page<T>>>,
) -> WorkspaceResult<Vec<T>> {
    collect_pages("workspace ", |cursor| {
        receive(request(cursor), "加载 workspace 分页")
    })
    .map(|page| page.data)
}

/// Exhaust forward pages while retaining the first available backwards cursor.
/// Reject any repeated cursor, including cycles longer than a single page.
fn collect_pages<T>(
    label: &str,
    mut request: impl FnMut(Option<String>) -> WorkspaceResult<Page<T>>,
) -> WorkspaceResult<Page<T>> {
    let mut cursor = None;
    let mut seen = HashSet::new();
    let mut values = Vec::new();
    let mut backwards_cursor = None;
    loop {
        let page = request(cursor)?;
        backwards_cursor = backwards_cursor.or(page.backwards_cursor);
        values.extend(page.data);
        let Some(next) = page.next_cursor else {
            return Ok(Page {
                data: values,
                next_cursor: None,
                backwards_cursor,
            });
        };
        if !seen.insert(next.clone()) {
            return Err(WorkspaceError::backend(format!(
                "{label}返回了重复分页 cursor `{next}`"
            )));
        }
        cursor = Some(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_empty_and_nonempty_pages_and_keeps_first_backwards_cursor() {
        let mut cursors = Vec::new();
        let page = collect_pages("fixture", |cursor| {
            cursors.push(cursor.clone());
            Ok(match cursor.as_deref() {
                None => Page {
                    data: vec![1, 2],
                    next_cursor: Some("second".into()),
                    backwards_cursor: None,
                },
                Some("second") => Page {
                    data: Vec::new(),
                    next_cursor: Some("third".into()),
                    backwards_cursor: Some("first-back".into()),
                },
                Some("third") => Page {
                    data: vec![3],
                    next_cursor: None,
                    backwards_cursor: Some("later-back".into()),
                },
                other => panic!("unexpected cursor: {other:?}"),
            })
        })
        .unwrap();

        assert_eq!(cursors, [None, Some("second".into()), Some("third".into())]);
        assert_eq!(page.data, [1, 2, 3]);
        assert_eq!(page.next_cursor, None);
        assert_eq!(page.backwards_cursor.as_deref(), Some("first-back"));
    }

    #[test]
    fn rejects_immediate_and_multistep_cursor_cycles() {
        for sequence in [vec!["a", "a"], vec!["a", "b", "a"]] {
            let mut responses = sequence.into_iter();
            let result = collect_pages("fixture", |_| {
                Ok(Page::<()> {
                    data: Vec::new(),
                    next_cursor: Some(responses.next().expect("cycle must terminate").into()),
                    backwards_cursor: None,
                })
            });
            assert_eq!(
                result,
                Err(WorkspaceError::backend("fixture返回了重复分页 cursor `a`"))
            );
        }
    }

    #[test]
    fn a_later_error_does_not_return_partial_results() {
        let expected = WorkspaceError::backend("backend unavailable");
        let result = collect_pages("fixture", |cursor| match cursor {
            None => Ok(Page {
                data: vec![1],
                next_cursor: Some("second".into()),
                backwards_cursor: None,
            }),
            Some(_) => Err(expected.clone()),
        });
        assert_eq!(result, Err(expected));
    }

    #[test]
    fn closed_response_channels_fail_with_operation_context() {
        let (sender, receiver) = async_channel::bounded::<WorkspaceResult<()>>(1);
        drop(sender);
        assert_eq!(
            receive(receiver, "读取会话"),
            Err(WorkspaceError::backend("读取会话响应通道提前关闭"))
        );
    }
}
