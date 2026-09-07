use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use super::*;
use crate::agent::{
    AgentCapability, AgentEvent, AgentModelCatalog, AgentPermissionMode, AgentPermissionProfile,
    AgentRequest, AgentRun, AgentThreadSettings, CommandExecutionStatus, HistoryItemDetail,
    HistoryTurnStatus, Page, PageRequest, ThreadHistoryItem, ThreadHistoryItemEntry, ThreadSection,
    ThreadTurn,
};

fn response<T: Send + 'static>(value: T) -> Receiver<T> {
    let (sender, receiver) = async_channel::bounded(1);
    let _ = sender.send_blocking(value);
    receiver
}

fn project(id: &str, position: i64) -> Project {
    Project {
        project_id: id.to_owned(),
        name: format!("Project {id}"),
        roots: vec![PathBuf::from(format!("/tmp/{id}"))],
        created_at: 1,
        updated_at: 2,
        recency_at: Some(3),
        position,
    }
}

fn thread(id: &str, project_id: Option<&str>) -> ThreadSummary {
    ThreadSummary {
        thread_id: id.to_owned(),
        title: format!("Thread {id}"),
        preview: format!("Preview {id}"),
        cwd: PathBuf::from("/tmp/workspace"),
        project_id: project_id.map(str::to_owned),
        section: None,
        created_at: 1,
        updated_at: 2,
        recency_at: Some(3),
        activity: ThreadActivity::Idle,
    }
}

struct FakeWorkspaceBackend {
    events_tx: Sender<AgentConnectionEvent>,
    events_rx: Receiver<AgentConnectionEvent>,
    calls: Mutex<Vec<String>>,
}

struct DelayedThreadListBackend {
    events_tx: Sender<AgentConnectionEvent>,
    events_rx: Receiver<AgentConnectionEvent>,
    recent_sender: Mutex<Option<Sender<WorkspaceResult<Page<ThreadSummary>>>>>,
    recent_receiver: Mutex<Option<Receiver<WorkspaceResult<Page<ThreadSummary>>>>>,
    recent_requested: AtomicU64,
}

impl DelayedThreadListBackend {
    fn new() -> Arc<Self> {
        let (events_tx, events_rx) = async_channel::unbounded();
        let (recent_sender, recent_receiver) = async_channel::bounded(1);
        Arc::new(Self {
            events_tx,
            events_rx,
            recent_sender: Mutex::new(Some(recent_sender)),
            recent_receiver: Mutex::new(Some(recent_receiver)),
            recent_requested: AtomicU64::new(0),
        })
    }

    fn release_recent(&self, page: Page<ThreadSummary>) {
        self.recent_sender
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .send_blocking(Ok(page))
            .unwrap();
    }
}

impl AgentBackend for DelayedThreadListBackend {
    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities::new([AgentCapability::ThreadList])
    }

    fn subscribe_connection_events(&self) -> Receiver<AgentConnectionEvent> {
        self.events_rx.clone()
    }

    fn load_model_catalog(&self) -> Receiver<Result<AgentModelCatalog, String>> {
        response(Err("not used".to_owned()))
    }

    fn load_permission_profiles(
        &self,
        _cwd: PathBuf,
    ) -> Receiver<Result<Vec<AgentPermissionProfile>, String>> {
        response(Err("not used".to_owned()))
    }

    fn update_thread_permissions(
        &self,
        _thread_id: String,
        _cwd: PathBuf,
        _mode: AgentPermissionMode,
    ) -> Receiver<Result<AgentThreadSettings, String>> {
        response(Err("not used".to_owned()))
    }

    fn list_threads(
        &self,
        request: ThreadListRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSummary>>> {
        if request.archived {
            return response(Ok(Page::single(Vec::new())));
        }
        self.recent_requested.fetch_add(1, Ordering::Release);
        self.recent_receiver.lock().unwrap().take().unwrap()
    }

    fn run_prompt(&self, _request: AgentRequest) -> AgentRun {
        let (sender, receiver) = async_channel::unbounded::<AgentEvent>();
        drop(sender);
        AgentRun::new(receiver, None)
    }
}

impl FakeWorkspaceBackend {
    fn new() -> Arc<Self> {
        let (events_tx, events_rx) = async_channel::unbounded();
        Arc::new(Self {
            events_tx,
            events_rx,
            calls: Mutex::new(Vec::new()),
        })
    }

    fn record(&self, call: impl Into<String>) {
        self.calls.lock().unwrap().push(call.into());
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl AgentBackend for FakeWorkspaceBackend {
    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities::new([
            AgentCapability::ProjectList,
            AgentCapability::ProjectCreate,
            AgentCapability::ProjectUpdate,
            AgentCapability::ProjectDelete,
            AgentCapability::ProjectMove,
            AgentCapability::ThreadList,
            AgentCapability::ThreadSearch,
            AgentCapability::ThreadRead,
            AgentCapability::ThreadTurnsList,
            AgentCapability::ThreadItemsList,
            AgentCapability::ThreadRename,
            AgentCapability::ThreadArchive,
            AgentCapability::ThreadUnarchive,
            AgentCapability::ThreadDelete,
            AgentCapability::ThreadMetadataUpdate,
            AgentCapability::ThreadSectionList,
            AgentCapability::ThreadSectionCreate,
            AgentCapability::ThreadSectionMove,
        ])
    }

    fn subscribe_connection_events(&self) -> Receiver<AgentConnectionEvent> {
        self.events_rx.clone()
    }

    fn load_model_catalog(&self) -> Receiver<Result<AgentModelCatalog, String>> {
        response(Err("not used".to_owned()))
    }

    fn load_permission_profiles(
        &self,
        _cwd: PathBuf,
    ) -> Receiver<Result<Vec<AgentPermissionProfile>, String>> {
        response(Err("not used".to_owned()))
    }

    fn update_thread_permissions(
        &self,
        _thread_id: String,
        _cwd: PathBuf,
        _mode: AgentPermissionMode,
    ) -> Receiver<Result<AgentThreadSettings, String>> {
        response(Err("not used".to_owned()))
    }

    fn list_projects(&self, page: PageRequest) -> Receiver<WorkspaceResult<Page<Project>>> {
        self.record(format!("project/list:{:?}", page.cursor));
        response(Ok(match page.cursor.as_deref() {
            None => Page {
                data: vec![project("project-a", 0)],
                next_cursor: Some("projects-next".to_owned()),
                backwards_cursor: None,
            },
            Some("projects-next") => Page::single(vec![project("project-b", 1)]),
            Some(other) => panic!("unexpected project cursor {other}"),
        }))
    }

    fn create_project(&self, request: CreateProject) -> Receiver<WorkspaceResult<Project>> {
        self.record(format!("project/create:{}", request.name));
        response(Ok(project("created-project", 2)))
    }

    fn update_project(
        &self,
        project_id: ProjectId,
        update: UpdateProject,
    ) -> Receiver<WorkspaceResult<Project>> {
        self.record(format!("project/update:{project_id}"));
        let mut value = project(&project_id, 0);
        if let Some(name) = update.name {
            value.name = name;
        }
        response(Ok(value))
    }

    fn delete_project(&self, project_id: ProjectId) -> Receiver<WorkspaceResult<()>> {
        self.record(format!("project/delete:{project_id}"));
        response(Ok(()))
    }

    fn move_project(
        &self,
        project_id: ProjectId,
        before_project_id: Option<ProjectId>,
    ) -> Receiver<WorkspaceResult<()>> {
        self.record(format!("project/move:{project_id}:{before_project_id:?}"));
        response(Ok(()))
    }

    fn list_threads(
        &self,
        request: ThreadListRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSummary>>> {
        self.record(format!(
            "thread/list:{:?}:{:?}:{}",
            request.page.cursor, request.section, request.archived
        ));
        let page = if request.archived {
            Page::single(vec![thread("archived-thread", None)])
        } else if matches!(request.section, FilterValue::Value(_)) {
            Page::single(vec![thread("pinned-thread", Some("project-a"))])
        } else {
            match request.page.cursor.as_deref() {
                None => Page {
                    data: vec![thread("thread-a", Some("project-a"))],
                    next_cursor: Some("threads-next".to_owned()),
                    backwards_cursor: None,
                },
                Some("threads-next") => Page::single(vec![thread("thread-b", None)]),
                Some(other) => panic!("unexpected thread cursor {other}"),
            }
        };
        response(Ok(page))
    }

    fn search_threads(
        &self,
        request: ThreadListRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSearchResult>>> {
        let term = request.search_term.unwrap_or_default();
        self.record(format!("thread/search:{term}"));
        response(Ok(Page::single(vec![ThreadSearchResult {
            thread: thread("search-thread", None),
            snippet: format!("matched {term}"),
        }])))
    }

    fn read_thread(&self, thread_id: ThreadId) -> Receiver<WorkspaceResult<ThreadSummary>> {
        self.record(format!("thread/read:{thread_id}"));
        response(Ok(thread(&thread_id, Some("project-a"))))
    }

    fn list_thread_turns(
        &self,
        thread_id: ThreadId,
        page: PageRequest,
        detail: HistoryItemDetail,
    ) -> Receiver<WorkspaceResult<Page<ThreadTurn>>> {
        self.record(format!(
            "thread/turns/list:{thread_id}:{:?}:{detail:?}",
            page.cursor
        ));
        let make_turn = |id: &str| {
            let full = id == "turn-a";
            ThreadTurn {
                turn_id: id.to_owned(),
                status: HistoryTurnStatus::Completed,
                items_view: if full {
                    HistoryItemDetail::Full
                } else {
                    HistoryItemDetail::Summary
                },
                items: if full {
                    vec![
                        ThreadHistoryItem::UserMessage {
                            images: Vec::new(),
                            item_id: format!("{id}-user"),
                            text: format!("question {id}"),
                        },
                        ThreadHistoryItem::AssistantMessage {
                            item_id: format!("{id}-assistant"),
                            text: format!("answer {id}"),
                            phase: None,
                        },
                    ]
                } else {
                    Vec::new()
                },
                started_at: Some(1),
                completed_at: Some(2),
                duration_ms: Some(1),
                error: None,
            }
        };
        response(Ok(match page.cursor.as_deref() {
            None => Page {
                data: vec![make_turn("turn-a")],
                next_cursor: Some("turns-next".to_owned()),
                backwards_cursor: Some("turns-back".to_owned()),
            },
            Some("turns-next") => Page::single(vec![make_turn("turn-b")]),
            Some(other) => panic!("unexpected turn cursor {other}"),
        }))
    }

    fn list_thread_items(
        &self,
        thread_id: ThreadId,
        turn_id: Option<String>,
        _page: PageRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadHistoryItemEntry>>> {
        self.record(format!("thread/items/list:{thread_id}:{turn_id:?}"));
        response(Ok(Page::single(vec![ThreadHistoryItemEntry {
            turn_id: turn_id.unwrap_or_else(|| "turn-a".to_owned()),
            item: ThreadHistoryItem::Command {
                item_id: "command-a".to_owned(),
                command: "pwd".to_owned(),
                output: "/tmp/workspace".to_owned(),
                status: CommandExecutionStatus::Completed,
                actions: Vec::new(),
                cwd: None,
                exit_code: None,
            },
        }])))
    }

    fn set_thread_name(&self, thread_id: ThreadId, name: String) -> Receiver<WorkspaceResult<()>> {
        self.record(format!("thread/name/set:{thread_id}:{name}"));
        let _ = self
            .events_tx
            .send_blocking(AgentConnectionEvent::ThreadNameUpdated {
                thread_id,
                name: Some(name),
            });
        response(Ok(()))
    }

    fn archive_thread(&self, thread_id: ThreadId) -> Receiver<WorkspaceResult<()>> {
        self.record(format!("thread/archive:{thread_id}"));
        response(Ok(()))
    }

    fn unarchive_thread(&self, thread_id: ThreadId) -> Receiver<WorkspaceResult<ThreadSummary>> {
        self.record(format!("thread/unarchive:{thread_id}"));
        response(Ok(thread(&thread_id, None)))
    }

    fn delete_thread(&self, thread_id: ThreadId) -> Receiver<WorkspaceResult<()>> {
        self.record(format!("thread/delete:{thread_id}"));
        response(Ok(()))
    }

    fn update_thread_metadata(
        &self,
        thread_id: ThreadId,
        update: ThreadMetadataUpdate,
    ) -> Receiver<WorkspaceResult<ThreadSummary>> {
        self.record(format!("thread/metadata/update:{thread_id}:{update:?}"));
        response(Ok(thread(&thread_id, Some("project-b"))))
    }

    fn list_thread_sections(
        &self,
        _page: PageRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSection>>> {
        self.record("threadSection/list");
        response(Ok(Page::single(vec![ThreadSection {
            section_id: "pinned-section".to_owned(),
            name: PINNED_SECTION_NAME.to_owned(),
            appearance: None,
        }])))
    }

    fn create_thread_section(
        &self,
        name: String,
        _appearance: Option<crate::agent::ThreadSectionAppearance>,
    ) -> Receiver<WorkspaceResult<ThreadSection>> {
        self.record(format!("threadSection/create:{name}"));
        response(Ok(ThreadSection {
            section_id: "created-pinned-section".to_owned(),
            name,
            appearance: None,
        }))
    }

    fn move_thread_to_section(
        &self,
        thread_id: ThreadId,
        section_id: Option<ThreadSectionId>,
        before_thread_id: Option<ThreadId>,
    ) -> Receiver<WorkspaceResult<()>> {
        self.record(format!(
            "thread/section/move:{thread_id}:{section_id:?}:{before_thread_id:?}"
        ));
        response(Ok(()))
    }

    fn run_prompt(&self, _request: AgentRequest) -> AgentRun {
        let (sender, receiver) = async_channel::unbounded::<AgentEvent>();
        drop(sender);
        AgentRun::new(receiver, None)
    }
}

fn test_preferences_path(label: &str) -> PathBuf {
    static SERIAL: AtomicU64 = AtomicU64::new(1);
    std::env::temp_dir()
        .join(format!(
            "gpui-workspace-{label}-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ))
        .join("preferences.json")
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for store update"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn cwd_fallback_uses_normalized_deepest_component_root_and_never_overrides_project_id() {
    let mut parent = project("parent", 0);
    parent.roots = vec![PathBuf::from("/tmp/workspace")];
    let mut nested = project("nested", 1);
    nested.roots = vec![PathBuf::from("/tmp/workspace/./crates/../crates/app")];
    let mut sibling_prefix = project("sibling-prefix", 2);
    sibling_prefix.roots = vec![PathBuf::from("/tmp/work")];
    let projects = vec![parent, nested, sibling_prefix];

    let mut legacy = thread("legacy", None);
    legacy.cwd = PathBuf::from("/tmp/workspace/crates/app/../app/src");
    assert_eq!(
        project_id_for_thread(&legacy, &projects).as_deref(),
        Some("nested")
    );

    legacy.cwd = PathBuf::from("/tmp/workspace-other");
    assert_eq!(project_id_for_thread(&legacy, &projects), None);

    legacy.project_id = Some("server-owned".to_owned());
    assert_eq!(
        project_id_for_thread(&legacy, &projects).as_deref(),
        Some("server-owned")
    );
}

#[test]
fn fake_backend_refresh_accumulates_pages_and_uses_server_ids() {
    let backend = FakeWorkspaceBackend::new();
    let path = test_preferences_path("pagination");
    let store = WorkspaceStore::with_preferences_path(backend.clone(), path.clone());
    store.refresh_all();
    wait_until(|| {
        let snapshot = store.snapshot();
        !snapshot.loading.projects
            && !snapshot.loading.recent
            && !snapshot.loading.archived
            && !snapshot.loading.pinned
            && snapshot.projects.len() == 2
            && snapshot.recent_threads.len() == 2
    });
    let snapshot = store.snapshot();
    assert_eq!(
        snapshot
            .projects
            .iter()
            .map(|project| project.project_id.as_str())
            .collect::<Vec<_>>(),
        ["project-a", "project-b"]
    );
    assert_eq!(snapshot.archived_threads[0].thread_id, "archived-thread");
    assert_eq!(snapshot.pinned_threads[0].thread_id, "pinned-thread");

    store.move_thread_to_project("thread-a".to_owned(), Some("project-b".to_owned()));
    wait_until(|| {
        !store
            .snapshot()
            .pending
            .contains(&WorkspaceOperation::MoveThread("thread-a".to_owned()))
    });
    assert!(backend.calls().iter().any(|call| {
        call.starts_with("thread/metadata/update:thread-a:") && call.contains("project-b")
    }));
    if let Some(parent) = path.parent() {
        let _ = fs::remove_dir_all(parent);
    }
}

#[test]
fn creating_a_project_uses_the_selected_directory_name_and_updates_the_snapshot() {
    let backend = FakeWorkspaceBackend::new();
    let path = test_preferences_path("create-project");
    let store = WorkspaceStore::with_preferences_path(backend.clone(), path.clone());

    store.create_project(PathBuf::from("/tmp/new-project"));

    wait_until(|| {
        !store
            .snapshot()
            .pending
            .contains(&WorkspaceOperation::CreateProject(
                "/tmp/new-project".to_owned(),
            ))
    });
    assert_eq!(
        store
            .snapshot()
            .projects
            .iter()
            .map(|project| project.project_id.as_str())
            .collect::<Vec<_>>(),
        ["created-project"]
    );
    assert!(
        backend
            .calls()
            .iter()
            .any(|call| call == "project/create:new-project")
    );
    if let Some(parent) = path.parent() {
        let _ = fs::remove_dir_all(parent);
    }
}

#[test]
fn notifications_are_idempotent_and_may_arrive_before_operation_response() {
    let backend = FakeWorkspaceBackend::new();
    let path = test_preferences_path("notifications");
    let store = WorkspaceStore::with_preferences_path(backend.clone(), path.clone());
    store.refresh_all();
    wait_until(|| !store.snapshot().loading.recent);

    store.rename_thread("thread-a".to_owned(), "Renamed once".to_owned());
    wait_until(|| {
        store
            .snapshot()
            .thread("thread-a")
            .is_some_and(|thread| thread.title == "Renamed once")
    });
    backend
        .events_tx
        .send_blocking(AgentConnectionEvent::ThreadDeleted {
            thread_id: "thread-a".to_owned(),
        })
        .unwrap();
    backend
        .events_tx
        .send_blocking(AgentConnectionEvent::ThreadDeleted {
            thread_id: "thread-a".to_owned(),
        })
        .unwrap();
    wait_until(|| store.snapshot().thread("thread-a").is_none());
    assert!(store.snapshot().error.is_none());
    if let Some(parent) = path.parent() {
        let _ = fs::remove_dir_all(parent);
    }
}

#[test]
fn notifications_received_before_a_list_response_override_its_stale_snapshot() {
    let backend = DelayedThreadListBackend::new();
    let path = test_preferences_path("early-list-notification");
    let store = WorkspaceStore::with_preferences_path(backend.clone(), path.clone());
    store.refresh_all();
    wait_until(|| backend.recent_requested.load(Ordering::Acquire) == 1);
    wait_until(|| !store.snapshot().loading.archived);
    assert!(store.snapshot().loading.recent);

    for event in [
        AgentConnectionEvent::ThreadNameUpdated {
            thread_id: "thread-a".to_owned(),
            name: Some("Server rename won".to_owned()),
        },
        AgentConnectionEvent::ThreadProjectUpdated {
            thread_id: "thread-a".to_owned(),
            project_id: Some("project-new".to_owned()),
        },
        AgentConnectionEvent::ThreadClosed {
            thread_id: "thread-a".to_owned(),
        },
    ] {
        backend.events_tx.send_blocking(event).unwrap();
    }
    wait_until(|| {
        store
            .thread_overlays()
            .get("thread-a")
            .is_some_and(|overlay| {
                overlay.name.is_some()
                    && overlay.project_id.is_some()
                    && overlay.activity == Some(ThreadActivity::Closed)
            })
    });
    backend.release_recent(Page::single(vec![thread(
        "thread-a",
        Some("project-stale"),
    )]));
    wait_until(|| !store.snapshot().loading.recent);

    let snapshot = store.snapshot();
    let restored = snapshot.thread("thread-a").unwrap();
    assert_eq!(restored.title, "Server rename won");
    assert_eq!(restored.project_id.as_deref(), Some("project-new"));
    assert_eq!(restored.activity, ThreadActivity::Closed);
    if let Some(parent) = path.parent() {
        let _ = fs::remove_dir_all(parent);
    }
}

#[test]
fn pinning_uses_the_dedicated_section_and_persists_no_workspace_shadow() {
    let backend = FakeWorkspaceBackend::new();
    let path = test_preferences_path("pin");
    let store = WorkspaceStore::with_preferences_path(backend.clone(), path.clone());
    store.refresh_all();
    wait_until(|| !store.snapshot().loading.pinned);
    store.set_thread_pinned("thread-a".to_owned(), true);
    wait_until(|| {
        !store
            .snapshot()
            .pending
            .contains(&WorkspaceOperation::PinThread("thread-a".to_owned()))
    });
    wait_until(|| path.exists());
    assert!(
        backend
            .calls()
            .iter()
            .any(|call| { call == "thread/section/move:thread-a:Some(\"pinned-section\"):None" })
    );
    assert!(
        !backend
            .calls()
            .iter()
            .any(|call| call.starts_with("threadSection/create:"))
    );
    let persisted = fs::read_to_string(&path).unwrap();
    assert!(persisted.contains("pinned-section"));
    assert!(!persisted.contains("thread-a"));
    assert!(!persisted.contains("project-a"));
    if let Some(parent) = path.parent() {
        let _ = fs::remove_dir_all(parent);
    }
}

#[test]
fn history_loads_every_turn_page_and_items_endpoint_remains_agent_neutral() {
    let backend = FakeWorkspaceBackend::new();
    let path = test_preferences_path("history");
    let store = WorkspaceStore::with_preferences_path(backend.clone(), path.clone());
    backend
        .events_tx
        .send_blocking(AgentConnectionEvent::ThreadArchived {
            thread_id: "thread-a".to_owned(),
        })
        .unwrap();
    wait_until(|| {
        store
            .thread_overlays()
            .get("thread-a")
            .and_then(|overlay| overlay.archived)
            == Some(true)
    });
    let history = store
        .load_history("thread-a".to_owned())
        .recv_blocking()
        .unwrap()
        .unwrap();
    assert_eq!(history.thread.thread_id, "thread-a");
    assert_eq!(history.next_turn_cursor, None);
    assert_eq!(history.backwards_turn_cursor.as_deref(), Some("turns-back"));
    assert!(
        !backend
            .calls()
            .iter()
            .any(|call| { call == "thread/items/list:thread-a:Some(\"turn-a\")" }),
        "full turns must not be fetched again"
    );
    assert_eq!(
        history
            .turns
            .iter()
            .map(|turn| turn.turn_id.as_str())
            .collect::<Vec<_>>(),
        ["turn-a", "turn-b"]
    );
    assert!(matches!(
        history.turns[1].items.as_slice(),
        [ThreadHistoryItem::Command { command, .. }] if command == "pwd"
    ));
    let items = backend
        .list_thread_items(
            "thread-a".to_owned(),
            Some("turn-a".to_owned()),
            PageRequest::default(),
        )
        .recv_blocking()
        .unwrap()
        .unwrap();
    assert!(matches!(
        items.data[0].item,
        ThreadHistoryItem::Command { .. }
    ));
    assert!(
        backend
            .calls()
            .iter()
            .any(|call| { call == "thread/turns/list:thread-a:Some(\"turns-next\"):Full" })
    );
    assert!(
        backend
            .calls()
            .iter()
            .any(|call| { call == "thread/items/list:thread-a:Some(\"turn-b\")" })
    );
    if let Some(parent) = path.parent() {
        let _ = fs::remove_dir_all(parent);
    }
}
