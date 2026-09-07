mod loaders;
mod preferences;

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use async_channel::{Receiver, Sender};

use crate::agent::{
    AgentBackend, AgentCapabilities, AgentCapability, AgentConnectionEvent, AgentThreadStatusState,
    CreateProject, FilterValue, Project, ProjectChange, ProjectId, ThreadActivity, ThreadHistory,
    ThreadId, ThreadListRequest, ThreadMetadataUpdate, ThreadSearchResult, ThreadSectionId,
    ThreadSortKey, ThreadSummary, UpdateProject, WorkspaceError, WorkspaceResult,
};
use loaders::{
    load_all_projects, load_all_search_results, load_all_sections, load_all_threads,
    load_all_turns, receive,
};
pub use preferences::UiPreferences;
use preferences::{PreferenceStore, default_preferences_path};

// `Pinned` is the app-server's canonical built-in section name. The sidebar
// localizes the heading independently; sending the localized label would
// create a second, incompatible server section.
const PINNED_SECTION_NAME: &str = "Pinned";

#[derive(Clone, Debug, Default)]
struct ThreadNotificationOverlay {
    deleted: bool,
    archived: Option<bool>,
    name: Option<Option<String>>,
    project_id: Option<Option<ProjectId>>,
    activity: Option<ThreadActivity>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceLoading {
    pub projects: bool,
    pub recent: bool,
    pub archived: bool,
    pub pinned: bool,
    pub search: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorkspaceOperation {
    CreateProject(String),
    UpdateProject(ProjectId),
    DeleteProject(ProjectId),
    MoveProject(ProjectId),
    RenameThread(ThreadId),
    ArchiveThread(ThreadId),
    UnarchiveThread(ThreadId),
    DeleteThread(ThreadId),
    MoveThread(ThreadId),
    PinThread(ThreadId),
}

impl WorkspaceOperation {
    pub fn thread_id(&self) -> Option<&str> {
        match self {
            Self::RenameThread(id)
            | Self::ArchiveThread(id)
            | Self::UnarchiveThread(id)
            | Self::DeleteThread(id)
            | Self::MoveThread(id)
            | Self::PinThread(id) => Some(id),
            _ => None,
        }
    }

    pub fn project_id(&self) -> Option<&str> {
        match self {
            Self::UpdateProject(id) | Self::DeleteProject(id) | Self::MoveProject(id) => Some(id),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceSnapshot {
    pub capabilities: AgentCapabilities,
    pub projects: Vec<Project>,
    pub recent_threads: Vec<ThreadSummary>,
    pub archived_threads: Vec<ThreadSummary>,
    pub pinned_threads: Vec<ThreadSummary>,
    pub search_query: String,
    pub search_results: Vec<ThreadSearchResult>,
    pub loading: WorkspaceLoading,
    pub error: Option<String>,
    pub preference_error: Option<String>,
    pub pending: BTreeSet<WorkspaceOperation>,
    pub preferences: UiPreferences,
}

impl WorkspaceSnapshot {
    fn new(capabilities: AgentCapabilities, preferences: UiPreferences) -> Self {
        Self {
            capabilities,
            projects: Vec::new(),
            recent_threads: Vec::new(),
            archived_threads: Vec::new(),
            pinned_threads: Vec::new(),
            search_query: String::new(),
            search_results: Vec::new(),
            loading: WorkspaceLoading::default(),
            error: None,
            preference_error: None,
            pending: BTreeSet::new(),
            preferences,
        }
    }

    pub fn thread(&self, thread_id: &str) -> Option<&ThreadSummary> {
        self.pinned_threads
            .iter()
            .chain(&self.recent_threads)
            .chain(&self.archived_threads)
            .find(|thread| thread.thread_id == thread_id)
    }

    pub fn is_pending_thread(&self, thread_id: &str) -> bool {
        self.pending
            .iter()
            .any(|operation| operation.thread_id() == Some(thread_id))
    }

    pub fn is_pending_project(&self, project_id: &str) -> bool {
        self.pending
            .iter()
            .any(|operation| operation.project_id() == Some(project_id))
    }
}

pub struct WorkspaceStore {
    backend: Arc<dyn AgentBackend>,
    snapshot: Mutex<WorkspaceSnapshot>,
    subscribers: Mutex<Vec<Sender<WorkspaceSnapshot>>>,
    preferences: PreferenceStore,
    search_generation: AtomicU64,
    projects_generation: AtomicU64,
    recent_generation: AtomicU64,
    archived_generation: AtomicU64,
    pinned_generation: AtomicU64,
    pin_section_lock: Mutex<()>,
    preference_save_lock: Mutex<()>,
    thread_notification_overlays: Mutex<HashMap<ThreadId, ThreadNotificationOverlay>>,
    deleted_project_ids: Mutex<HashSet<ProjectId>>,
}

impl WorkspaceStore {
    pub fn new(backend: Arc<dyn AgentBackend>) -> Arc<Self> {
        Self::with_preferences_path(backend, default_preferences_path())
    }

    pub fn with_preferences_path(
        backend: Arc<dyn AgentBackend>,
        preferences_path: PathBuf,
    ) -> Arc<Self> {
        let preference_store = PreferenceStore::new(preferences_path);
        let (preferences, preference_error) = match preference_store.load() {
            Ok(preferences) => (preferences, None),
            Err(error) => (UiPreferences::current(), Some(error)),
        };
        let mut snapshot = WorkspaceSnapshot::new(backend.capabilities(), preferences);
        snapshot.preference_error = preference_error;
        let store = Arc::new(Self {
            backend,
            snapshot: Mutex::new(snapshot),
            subscribers: Mutex::new(Vec::new()),
            preferences: preference_store,
            search_generation: AtomicU64::new(0),
            projects_generation: AtomicU64::new(0),
            recent_generation: AtomicU64::new(0),
            archived_generation: AtomicU64::new(0),
            pinned_generation: AtomicU64::new(0),
            pin_section_lock: Mutex::new(()),
            preference_save_lock: Mutex::new(()),
            thread_notification_overlays: Mutex::new(HashMap::new()),
            deleted_project_ids: Mutex::new(HashSet::new()),
        });
        Self::listen_for_backend_events(&store);
        store
    }

    pub fn snapshot(&self) -> WorkspaceSnapshot {
        self.snapshot
            .lock()
            .map(|snapshot| snapshot.clone())
            .unwrap_or_else(|_| {
                let mut snapshot =
                    WorkspaceSnapshot::new(self.backend.capabilities(), UiPreferences::current());
                snapshot.error = Some("WorkspaceStore 状态锁已损坏".to_owned());
                snapshot
            })
    }

    pub fn subscribe(&self) -> Receiver<WorkspaceSnapshot> {
        let (sender, receiver) = async_channel::unbounded();
        let _ = sender.send_blocking(self.snapshot());
        if let Ok(mut subscribers) = self.subscribers.lock() {
            subscribers.push(sender);
        }
        receiver
    }

    fn publish(&self) {
        let snapshot = self.snapshot();
        if let Ok(mut subscribers) = self.subscribers.lock() {
            subscribers.retain(|subscriber| subscriber.send_blocking(snapshot.clone()).is_ok());
        }
    }

    fn update(&self, update: impl FnOnce(&mut WorkspaceSnapshot)) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            update(&mut snapshot);
        }
        self.publish();
    }

    fn listen_for_backend_events(store: &Arc<Self>) {
        let events = store.backend.subscribe_connection_events();
        let weak = Arc::downgrade(store);
        std::thread::spawn(move || {
            while let Ok(event) = events.recv_blocking() {
                let Some(store) = weak.upgrade() else {
                    break;
                };
                store.apply_backend_event(event);
            }
        });
    }

    fn update_thread_overlay(
        &self,
        thread_id: &str,
        update: impl FnOnce(&mut ThreadNotificationOverlay),
    ) {
        if let Ok(mut overlays) = self.thread_notification_overlays.lock() {
            update(overlays.entry(thread_id.to_owned()).or_default());
        }
    }

    fn thread_overlays(&self) -> HashMap<ThreadId, ThreadNotificationOverlay> {
        self.thread_notification_overlays
            .lock()
            .map(|overlays| overlays.clone())
            .unwrap_or_default()
    }

    fn deleted_projects(&self) -> HashSet<ProjectId> {
        self.deleted_project_ids
            .lock()
            .map(|projects| projects.clone())
            .unwrap_or_default()
    }

    fn apply_backend_event(self: &Arc<Self>, event: AgentConnectionEvent) {
        match event {
            AgentConnectionEvent::ProjectChanged { project_id, change } => match change {
                ProjectChange::Deleted => {
                    if let Ok(mut deleted) = self.deleted_project_ids.lock() {
                        deleted.insert(project_id.clone());
                    }
                    self.update(|snapshot| {
                        snapshot
                            .projects
                            .retain(|project| project.project_id != project_id);
                        for thread in snapshot
                            .recent_threads
                            .iter_mut()
                            .chain(snapshot.archived_threads.iter_mut())
                            .chain(snapshot.pinned_threads.iter_mut())
                        {
                            if thread.project_id.as_deref() == Some(project_id.as_str()) {
                                thread.project_id = None;
                            }
                        }
                    });
                }
                ProjectChange::Created | ProjectChange::Updated => {
                    if let Ok(mut deleted) = self.deleted_project_ids.lock() {
                        deleted.remove(&project_id);
                    }
                    self.refresh_projects();
                }
            },
            AgentConnectionEvent::ThreadArchived { thread_id } => {
                self.update_thread_overlay(&thread_id, |overlay| overlay.archived = Some(true));
                self.update(|snapshot| {
                    snapshot
                        .recent_threads
                        .retain(|thread| thread.thread_id != thread_id);
                    snapshot
                        .pinned_threads
                        .retain(|thread| thread.thread_id != thread_id);
                });
                self.refresh_archived();
            }
            AgentConnectionEvent::ThreadUnarchived { thread_id } => {
                self.update_thread_overlay(&thread_id, |overlay| overlay.archived = Some(false));
                self.update(|snapshot| {
                    snapshot
                        .archived_threads
                        .retain(|thread| thread.thread_id != thread_id);
                });
                self.refresh_recent_and_pinned();
            }
            AgentConnectionEvent::ThreadDeleted { thread_id } => {
                self.update_thread_overlay(&thread_id, |overlay| overlay.deleted = true);
                self.update(|snapshot| remove_thread(snapshot, &thread_id));
            }
            AgentConnectionEvent::ThreadNameUpdated { thread_id, name } => {
                self.update_thread_overlay(&thread_id, |overlay| overlay.name = Some(name.clone()));
                self.update(|snapshot| {
                    visit_thread_mut(snapshot, &thread_id, |thread| {
                        thread.title = name
                            .clone()
                            .filter(|name| !name.trim().is_empty())
                            .unwrap_or_else(|| fallback_thread_title(&thread.preview));
                    });
                });
            }
            AgentConnectionEvent::ThreadClosed { thread_id } => {
                self.update_thread_overlay(&thread_id, |overlay| {
                    overlay.activity = Some(ThreadActivity::Closed)
                });
                self.update(|snapshot| {
                    visit_thread_mut(snapshot, &thread_id, |thread| {
                        thread.activity = ThreadActivity::Closed;
                    });
                });
            }
            AgentConnectionEvent::ThreadProjectUpdated {
                thread_id,
                project_id,
            } => {
                self.update_thread_overlay(&thread_id, |overlay| {
                    overlay.project_id = Some(project_id.clone())
                });
                self.update(|snapshot| {
                    visit_thread_mut(snapshot, &thread_id, |thread| {
                        thread.project_id = project_id.clone();
                    });
                });
            }
            AgentConnectionEvent::ThreadStatusChanged(status) => {
                let activity = activity_from_connection_status(&status.state);
                self.update_thread_overlay(&status.thread_id, |overlay| {
                    overlay.activity = Some(activity.clone())
                });
                self.update(|snapshot| {
                    visit_thread_mut(snapshot, &status.thread_id, |thread| {
                        thread.activity = activity.clone();
                    });
                });
            }
            AgentConnectionEvent::Warning { .. }
            | AgentConnectionEvent::ConfigWarning(_)
            | AgentConnectionEvent::McpServerStartupStatusUpdated(_)
            | AgentConnectionEvent::ThreadSettingsUpdated { .. }
            | AgentConnectionEvent::AccountRateLimitsUpdated(_) => {}
        }
    }

    pub fn refresh_all(self: &Arc<Self>) {
        let projects_generation = self.projects_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let recent_generation = self.recent_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let archived_generation = self.archived_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let pinned_generation = self.pinned_generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.update(|snapshot| {
            snapshot.loading.projects = true;
            snapshot.loading.recent = true;
            snapshot.loading.archived = true;
            snapshot.loading.pinned = true;
            snapshot.error = None;
        });
        let capabilities = self.snapshot().capabilities;
        let supports_projects = capabilities.supports(AgentCapability::ProjectList);
        let supports_threads = capabilities.supports(AgentCapability::ThreadList);
        let supports_sections = capabilities.supports(AgentCapability::ThreadSectionList);

        // These collections are independent app-server RPC families. Keep them
        // in separate workers so the connection can pipeline the requests and
        // route their out-of-order responses, instead of turning startup into a
        // project -> recent -> archived -> pinned waterfall.
        let store = Arc::clone(self);
        std::thread::spawn(move || {
            let mut projects = if supports_projects {
                load_all_projects(store.backend.as_ref())
            } else {
                Ok(Vec::new())
            };
            let deleted_projects = store.deleted_projects();
            if let Ok(projects) = &mut projects {
                projects.retain(|project| !deleted_projects.contains(&project.project_id));
            }
            if store.projects_generation.load(Ordering::Acquire) != projects_generation {
                return;
            }
            store.update(|snapshot| {
                snapshot.loading.projects = false;
                match projects {
                    Ok(projects) => snapshot.projects = projects,
                    Err(error) => append_error(&mut snapshot.error, error.user_message("加载项目")),
                }
            });
        });

        let store = Arc::clone(self);
        std::thread::spawn(move || {
            let mut recent = if supports_threads {
                load_all_threads(store.backend.as_ref(), ThreadListRequest::default())
            } else {
                Ok(Vec::new())
            };
            if let Ok(threads) = &mut recent {
                apply_thread_overlays(
                    threads,
                    &store.thread_overlays(),
                    ThreadCollectionKind::Recent,
                );
            }
            if store.recent_generation.load(Ordering::Acquire) != recent_generation {
                return;
            }
            store.update(|snapshot| {
                snapshot.loading.recent = false;
                match recent {
                    Ok(recent) => snapshot.recent_threads = recent,
                    Err(error) => {
                        append_error(&mut snapshot.error, error.user_message("加载最近聊天"))
                    }
                }
            });
        });

        let store = Arc::clone(self);
        std::thread::spawn(move || {
            let mut archived = if supports_threads {
                load_all_threads(
                    store.backend.as_ref(),
                    ThreadListRequest {
                        archived: true,
                        ..ThreadListRequest::default()
                    },
                )
            } else {
                Ok(Vec::new())
            };
            if let Ok(threads) = &mut archived {
                apply_thread_overlays(
                    threads,
                    &store.thread_overlays(),
                    ThreadCollectionKind::Archived,
                );
            }
            if store.archived_generation.load(Ordering::Acquire) != archived_generation {
                return;
            }
            store.update(|snapshot| {
                snapshot.loading.archived = false;
                match archived {
                    Ok(archived) => snapshot.archived_threads = archived,
                    Err(error) => {
                        append_error(&mut snapshot.error, error.user_message("加载已归档聊天"))
                    }
                }
            });
        });

        let store = Arc::clone(self);
        std::thread::spawn(move || {
            let sections = if supports_sections {
                load_all_sections(store.backend.as_ref())
            } else {
                Ok(Vec::new())
            };
            let previous_pinned_section_id = store.snapshot().preferences.pinned_section_id.clone();
            let pinned_section = sections.as_ref().ok().and_then(|sections| {
                let preferred = store
                    .snapshot()
                    .preferences
                    .pinned_section_id
                    .and_then(|id| sections.iter().find(|section| section.section_id == id));
                preferred
                    .or_else(|| {
                        sections
                            .iter()
                            .find(|section| section.name == PINNED_SECTION_NAME)
                    })
                    .cloned()
            });
            let preference_changed = previous_pinned_section_id
                != pinned_section
                    .as_ref()
                    .map(|section| section.section_id.clone());
            let mut pinned = match &pinned_section {
                Some(section) if supports_threads => load_all_threads(
                    store.backend.as_ref(),
                    ThreadListRequest {
                        section: FilterValue::Value(section.section_id.clone()),
                        sort_key: ThreadSortKey::SectionPosition,
                        ..ThreadListRequest::default()
                    },
                ),
                None => Ok(Vec::new()),
                Some(_) => Ok(Vec::new()),
            };
            if let Ok(threads) = &mut pinned {
                apply_thread_overlays(
                    threads,
                    &store.thread_overlays(),
                    ThreadCollectionKind::Pinned,
                );
            }
            if store.pinned_generation.load(Ordering::Acquire) != pinned_generation {
                return;
            }
            store.update(|snapshot| {
                match pinned {
                    Ok(pinned) => snapshot.pinned_threads = pinned,
                    Err(error) => {
                        append_error(&mut snapshot.error, error.user_message("加载置顶聊天"))
                    }
                }
                if let Some(section) = pinned_section {
                    snapshot.preferences.pinned_section_id = Some(section.section_id);
                } else if sections.is_ok() {
                    snapshot.preferences.pinned_section_id = None;
                }
                snapshot.loading.pinned = false;
                if let Err(error) = sections {
                    append_error(&mut snapshot.error, error.user_message("加载会话分区"));
                }
            });
            if preference_changed {
                store.save_preferences();
            }
        });
    }

    pub fn retry(self: &Arc<Self>) {
        self.refresh_all();
    }

    fn refresh_projects(self: &Arc<Self>) {
        let generation = self.projects_generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.update(|snapshot| {
            snapshot.loading.projects = true;
            snapshot.error = None;
        });
        let store = Arc::clone(self);
        std::thread::spawn(move || {
            let mut result = load_all_projects(store.backend.as_ref());
            let deleted_projects = store.deleted_projects();
            if let Ok(projects) = &mut result {
                projects.retain(|project| !deleted_projects.contains(&project.project_id));
            }
            if store.projects_generation.load(Ordering::Acquire) != generation {
                return;
            }
            store.update(|snapshot| {
                snapshot.loading.projects = false;
                match result {
                    Ok(projects) => snapshot.projects = projects,
                    Err(error) => snapshot.error = Some(error.user_message("刷新项目")),
                }
            });
        });
    }

    fn refresh_archived(self: &Arc<Self>) {
        let generation = self.archived_generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.update(|snapshot| snapshot.loading.archived = true);
        let store = Arc::clone(self);
        std::thread::spawn(move || {
            let mut result = load_all_threads(
                store.backend.as_ref(),
                ThreadListRequest {
                    archived: true,
                    ..ThreadListRequest::default()
                },
            );
            if let Ok(threads) = &mut result {
                apply_thread_overlays(
                    threads,
                    &store.thread_overlays(),
                    ThreadCollectionKind::Archived,
                );
            }
            if store.archived_generation.load(Ordering::Acquire) != generation {
                return;
            }
            store.update(|snapshot| {
                snapshot.loading.archived = false;
                match result {
                    Ok(threads) => snapshot.archived_threads = threads,
                    Err(error) => snapshot.error = Some(error.user_message("刷新已归档聊天")),
                }
            });
        });
    }

    fn refresh_recent_and_pinned(self: &Arc<Self>) {
        let recent_generation = self.recent_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let pinned_generation = self.pinned_generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.update(|snapshot| {
            snapshot.loading.recent = true;
            snapshot.loading.pinned = true;
            snapshot.error = None;
        });
        let store = Arc::clone(self);
        std::thread::spawn(move || {
            let mut recent = load_all_threads(store.backend.as_ref(), ThreadListRequest::default());
            let pinned_id = store.snapshot().preferences.pinned_section_id;
            let mut pinned = match pinned_id {
                Some(section_id) => load_all_threads(
                    store.backend.as_ref(),
                    ThreadListRequest {
                        section: FilterValue::Value(section_id),
                        sort_key: ThreadSortKey::SectionPosition,
                        ..ThreadListRequest::default()
                    },
                ),
                None => Ok(Vec::new()),
            };
            let overlays = store.thread_overlays();
            if let Ok(threads) = &mut recent {
                apply_thread_overlays(threads, &overlays, ThreadCollectionKind::Recent);
            }
            if let Ok(threads) = &mut pinned {
                apply_thread_overlays(threads, &overlays, ThreadCollectionKind::Pinned);
            }
            let recent_current =
                store.recent_generation.load(Ordering::Acquire) == recent_generation;
            let pinned_current =
                store.pinned_generation.load(Ordering::Acquire) == pinned_generation;
            if !recent_current && !pinned_current {
                return;
            }
            store.update(|snapshot| {
                let mut errors = Vec::new();
                if recent_current {
                    snapshot.loading.recent = false;
                    match recent {
                        Ok(recent) => snapshot.recent_threads = recent,
                        Err(error) => errors.push(error.user_message("刷新最近聊天")),
                    }
                }
                if pinned_current {
                    snapshot.loading.pinned = false;
                    match pinned {
                        Ok(pinned) => snapshot.pinned_threads = pinned,
                        Err(error) => errors.push(error.user_message("刷新置顶聊天")),
                    }
                }
                if !errors.is_empty() {
                    snapshot.error = Some(errors.join("\n"));
                }
            });
        });
    }

    pub fn search(self: &Arc<Self>, query: String) {
        let generation = self.search_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let trimmed = query.trim().to_owned();
        self.update(|snapshot| {
            snapshot.search_query = query;
            snapshot.search_results.clear();
            snapshot.loading.search = !trimmed.is_empty();
            snapshot.error = None;
        });
        if trimmed.is_empty() {
            return;
        }
        let store = Arc::clone(self);
        std::thread::spawn(move || {
            let mut result = load_all_search_results(store.backend.as_ref(), trimmed);
            if store.search_generation.load(Ordering::Acquire) != generation {
                return;
            }
            if let Ok(results) = &mut result {
                apply_search_overlays(results, &store.thread_overlays());
            }
            store.update(|snapshot| {
                snapshot.loading.search = false;
                match result {
                    Ok(results) => snapshot.search_results = results,
                    Err(error) => snapshot.error = Some(error.user_message("搜索聊天")),
                }
            });
        });
    }

    pub fn create_project(self: &Arc<Self>, root: PathBuf) {
        let name = root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("项目")
            .to_owned();
        let operation = WorkspaceOperation::CreateProject(root.display().to_string());
        self.begin(operation.clone());
        let receiver = self.backend.create_project(CreateProject {
            name,
            roots: vec![root],
        });
        let store = Arc::clone(self);
        std::thread::spawn(move || match receive(receiver, "创建项目") {
            Ok(project) => {
                if let Ok(mut deleted) = store.deleted_project_ids.lock() {
                    deleted.remove(&project.project_id);
                }
                store.update(|snapshot| upsert_project(&mut snapshot.projects, project));
                store.finish(&operation, None);
            }
            Err(error) => store.finish(&operation, Some(error.user_message("创建项目"))),
        });
    }

    pub fn update_project(self: &Arc<Self>, project_id: ProjectId, update: UpdateProject) {
        let operation = WorkspaceOperation::UpdateProject(project_id.clone());
        self.begin(operation.clone());
        let receiver = self.backend.update_project(project_id, update);
        let store = Arc::clone(self);
        std::thread::spawn(move || match receive(receiver, "更新项目") {
            Ok(project) => {
                if !store.deleted_projects().contains(&project.project_id) {
                    store.update(|snapshot| upsert_project(&mut snapshot.projects, project));
                }
                store.finish(&operation, None);
            }
            Err(error) => store.finish(&operation, Some(error.user_message("更新项目"))),
        });
    }

    pub fn delete_project(self: &Arc<Self>, project_id: ProjectId) {
        let operation = WorkspaceOperation::DeleteProject(project_id.clone());
        self.begin(operation.clone());
        let receiver = self.backend.delete_project(project_id);
        let store = Arc::clone(self);
        std::thread::spawn(move || match receive(receiver, "删除项目") {
            Ok(()) => {
                if let WorkspaceOperation::DeleteProject(project_id) = &operation
                    && let Ok(mut deleted) = store.deleted_project_ids.lock()
                {
                    deleted.insert(project_id.clone());
                }
                store.finish(&operation, None);
                store.refresh_all();
            }
            Err(error) => store.finish(&operation, Some(error.user_message("删除项目"))),
        });
    }

    pub fn move_project(
        self: &Arc<Self>,
        project_id: ProjectId,
        before_project_id: Option<ProjectId>,
    ) {
        let operation = WorkspaceOperation::MoveProject(project_id.clone());
        self.begin(operation.clone());
        let receiver = self.backend.move_project(project_id, before_project_id);
        let store = Arc::clone(self);
        std::thread::spawn(move || match receive(receiver, "移动项目") {
            Ok(()) => {
                store.finish(&operation, None);
                store.refresh_projects();
            }
            Err(error) => store.finish(&operation, Some(error.user_message("移动项目"))),
        });
    }

    pub fn rename_thread(self: &Arc<Self>, thread_id: ThreadId, name: String) {
        let operation = WorkspaceOperation::RenameThread(thread_id.clone());
        self.begin(operation.clone());
        let receiver = self
            .backend
            .set_thread_name(thread_id.clone(), name.clone());
        let store = Arc::clone(self);
        std::thread::spawn(move || match receive(receiver, "重命名会话") {
            Ok(()) => {
                store.update_thread_overlay(&thread_id, |overlay| {
                    overlay.name = Some(Some(name.clone()))
                });
                store.update(|snapshot| {
                    visit_thread_mut(snapshot, &thread_id, |thread| {
                        thread.title = if name.trim().is_empty() {
                            fallback_thread_title(&thread.preview)
                        } else {
                            name.clone()
                        };
                    });
                });
                store.finish(&operation, None);
            }
            Err(error) => store.finish(&operation, Some(error.user_message("重命名聊天"))),
        });
    }

    pub fn archive_thread(self: &Arc<Self>, thread_id: ThreadId) {
        let operation = WorkspaceOperation::ArchiveThread(thread_id.clone());
        self.begin(operation.clone());
        let receiver = self.backend.archive_thread(thread_id.clone());
        let store = Arc::clone(self);
        std::thread::spawn(move || match receive(receiver, "归档会话") {
            Ok(()) => {
                store.update_thread_overlay(&thread_id, |overlay| overlay.archived = Some(true));
                store.update(|snapshot| {
                    snapshot
                        .recent_threads
                        .retain(|thread| thread.thread_id != thread_id);
                    snapshot
                        .pinned_threads
                        .retain(|thread| thread.thread_id != thread_id);
                });
                store.finish(&operation, None);
                store.refresh_archived();
            }
            Err(error) => store.finish(&operation, Some(error.user_message("归档聊天"))),
        });
    }

    pub fn unarchive_thread(self: &Arc<Self>, thread_id: ThreadId) {
        let operation = WorkspaceOperation::UnarchiveThread(thread_id.clone());
        self.begin(operation.clone());
        let receiver = self.backend.unarchive_thread(thread_id);
        let store = Arc::clone(self);
        std::thread::spawn(move || match receive(receiver, "取消归档") {
            Ok(mut thread) => {
                store.update_thread_overlay(&thread.thread_id, |overlay| {
                    overlay.archived = Some(false)
                });
                let visible = apply_thread_overlay(
                    &mut thread,
                    &store.thread_overlays(),
                    ThreadCollectionKind::Recent,
                );
                store.update(|snapshot| {
                    snapshot
                        .archived_threads
                        .retain(|candidate| candidate.thread_id != thread.thread_id);
                    if visible {
                        upsert_thread(&mut snapshot.recent_threads, thread);
                    }
                });
                store.finish(&operation, None);
                store.refresh_recent_and_pinned();
            }
            Err(error) => store.finish(&operation, Some(error.user_message("取消归档"))),
        });
    }

    pub fn delete_thread(self: &Arc<Self>, thread_id: ThreadId) {
        let operation = WorkspaceOperation::DeleteThread(thread_id.clone());
        self.begin(operation.clone());
        let receiver = self.backend.delete_thread(thread_id.clone());
        let store = Arc::clone(self);
        std::thread::spawn(move || match receive(receiver, "删除会话") {
            Ok(()) => {
                store.update_thread_overlay(&thread_id, |overlay| overlay.deleted = true);
                store.update(|snapshot| remove_thread(snapshot, &thread_id));
                store.finish(&operation, None);
            }
            Err(error) => store.finish(&operation, Some(error.user_message("删除聊天"))),
        });
    }

    pub fn move_thread_to_project(
        self: &Arc<Self>,
        thread_id: ThreadId,
        project_id: Option<ProjectId>,
    ) {
        let operation = WorkspaceOperation::MoveThread(thread_id.clone());
        self.begin(operation.clone());
        let receiver = self.backend.update_thread_metadata(
            thread_id,
            ThreadMetadataUpdate {
                project: match project_id {
                    Some(project_id) => crate::agent::AgentOptionalField::Value(project_id),
                    None => crate::agent::AgentOptionalField::Null,
                },
            },
        );
        let store = Arc::clone(self);
        std::thread::spawn(move || match receive(receiver, "移动会话") {
            Ok(mut thread) => {
                store.update_thread_overlay(&thread.thread_id, |overlay| {
                    overlay.project_id = Some(thread.project_id.clone())
                });
                let visible = apply_thread_overlay(
                    &mut thread,
                    &store.thread_overlays(),
                    ThreadCollectionKind::Recent,
                );
                store.update(|snapshot| {
                    if visible {
                        upsert_thread_everywhere(snapshot, thread);
                    } else {
                        remove_thread(snapshot, &thread.thread_id);
                    }
                });
                store.finish(&operation, None);
            }
            Err(error) => store.finish(&operation, Some(error.user_message("移动聊天"))),
        });
    }

    pub fn set_thread_pinned(self: &Arc<Self>, thread_id: ThreadId, pinned: bool) {
        let operation = WorkspaceOperation::PinThread(thread_id.clone());
        self.begin(operation.clone());
        let store = Arc::clone(self);
        std::thread::spawn(move || {
            let mut attempted_section_id = None;
            let result = (|| {
                let section_id = if pinned {
                    Some(store.ensure_pinned_section()?)
                } else {
                    None
                };
                attempted_section_id = section_id.clone();
                receive(
                    store
                        .backend
                        .move_thread_to_section(thread_id, section_id, None),
                    if pinned {
                        "置顶会话"
                    } else {
                        "取消置顶"
                    },
                )
            })();
            match result {
                Ok(()) => {
                    store.finish(&operation, None);
                    store.refresh_recent_and_pinned();
                }
                Err(error) => {
                    if pinned && attempted_section_id.is_some() {
                        store.update(|snapshot| {
                            if snapshot.preferences.pinned_section_id == attempted_section_id {
                                snapshot.preferences.pinned_section_id = None;
                            }
                        });
                        store.save_preferences();
                    }
                    store.finish(&operation, Some(error.user_message("更新置顶状态")));
                }
            }
        });
    }

    fn ensure_pinned_section(&self) -> WorkspaceResult<ThreadSectionId> {
        let _guard = self
            .pin_section_lock
            .lock()
            .map_err(|_| WorkspaceError::backend("pinned section 锁已损坏"))?;
        if let Some(section_id) = self.snapshot().preferences.pinned_section_id {
            return Ok(section_id);
        }
        let sections = load_all_sections(self.backend.as_ref())?;
        let section = match sections
            .into_iter()
            .find(|section| section.name == PINNED_SECTION_NAME)
        {
            Some(section) => section,
            None => receive(
                self.backend
                    .create_thread_section(PINNED_SECTION_NAME.to_owned(), None),
                "创建置顶分区",
            )?,
        };
        self.update(|snapshot| {
            snapshot.preferences.pinned_section_id = Some(section.section_id.clone());
        });
        self.save_preferences();
        Ok(section.section_id)
    }

    fn begin(&self, operation: WorkspaceOperation) {
        self.update(|snapshot| {
            snapshot.pending.insert(operation);
            snapshot.error = None;
        });
    }

    fn finish(&self, operation: &WorkspaceOperation, error: Option<String>) {
        self.update(|snapshot| {
            snapshot.pending.remove(operation);
            snapshot.error = error;
        });
    }

    pub fn set_project_collapsed(&self, project_id: ProjectId, collapsed: bool) {
        self.update(|snapshot| {
            if collapsed {
                snapshot
                    .preferences
                    .collapsed_project_ids
                    .insert(project_id);
            } else {
                snapshot
                    .preferences
                    .collapsed_project_ids
                    .remove(&project_id);
            }
        });
        self.save_preferences();
    }

    pub fn set_section_collapsed(&self, section: &'static str, collapsed: bool) {
        self.update(|snapshot| match section {
            "pinned" => snapshot.preferences.pinned_collapsed = collapsed,
            "projects" => snapshot.preferences.projects_collapsed = collapsed,
            "recent" => snapshot.preferences.recent_collapsed = collapsed,
            _ => {}
        });
        self.save_preferences();
    }

    fn save_preferences(&self) {
        let _save_guard = match self.preference_save_lock.lock() {
            Ok(guard) => guard,
            Err(_) => {
                self.update(|snapshot| {
                    snapshot.preference_error = Some("UI 偏好写入锁已损坏".to_owned())
                });
                return;
            }
        };
        // Capture after serializing writers, so a delayed older save cannot
        // overwrite a preference update that completed later.
        let preferences = self.snapshot().preferences;
        let result = self.preferences.save(&preferences);
        self.update(|snapshot| snapshot.preference_error = result.err());
    }

    pub fn load_history(
        self: &Arc<Self>,
        thread_id: ThreadId,
    ) -> Receiver<WorkspaceResult<ThreadHistory>> {
        let (sender, receiver) = async_channel::bounded(1);
        let store = Arc::clone(self);
        std::thread::spawn(move || {
            let result = (|| {
                let mut thread = receive(store.backend.read_thread(thread_id.clone()), "读取会话")?;
                if !apply_thread_overlay(
                    &mut thread,
                    &store.thread_overlays(),
                    ThreadCollectionKind::History,
                ) {
                    return Err(WorkspaceError::backend("会话已不可用"));
                }
                let turns = load_all_turns(store.backend.as_ref(), thread_id)?;
                Ok(ThreadHistory {
                    thread,
                    turns: turns.data,
                    next_turn_cursor: turns.next_cursor,
                    backwards_turn_cursor: turns.backwards_cursor,
                })
            })();
            let _ = sender.send_blocking(result);
        });
        receiver
    }
}

fn append_error(target: &mut Option<String>, message: String) {
    match target {
        Some(existing) => {
            existing.push('\n');
            existing.push_str(&message);
        }
        None => *target = Some(message),
    }
}

fn fallback_thread_title(preview: &str) -> String {
    preview
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| "新对话".to_owned())
}

fn normalize_workspace_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // Absolute app-server paths cannot traverse above their root.
                // For a relative test path, preserve leading `..` components.
                if !normalized.pop() && !path.is_absolute() {
                    normalized.push(component.as_os_str());
                }
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

/// Resolve the project used by the sidebar without mutating app-server owned
/// metadata. An explicit project assignment always wins. Legacy/unassigned
/// threads fall back to the deepest component-aware project root containing
/// their normalized cwd.
pub fn project_id_for_thread(thread: &ThreadSummary, projects: &[Project]) -> Option<ProjectId> {
    if let Some(project_id) = &thread.project_id {
        return Some(project_id.clone());
    }

    let cwd = normalize_workspace_path(&thread.cwd);
    let mut best: Option<(usize, &Project)> = None;
    for project in projects {
        for root in &project.roots {
            let root = normalize_workspace_path(root);
            if cwd.starts_with(&root) {
                let depth = root.components().count();
                if best.is_none_or(|(best_depth, _)| depth > best_depth) {
                    best = Some((depth, project));
                }
            }
        }
    }
    best.map(|(_, project)| project.project_id.clone())
}

fn activity_from_connection_status(status: &AgentThreadStatusState) -> ThreadActivity {
    match status {
        AgentThreadStatusState::NotLoaded => ThreadActivity::NotLoaded,
        AgentThreadStatusState::Idle => ThreadActivity::Idle,
        AgentThreadStatusState::SystemError => ThreadActivity::SystemError,
        AgentThreadStatusState::Active { active_flags } => ThreadActivity::Active {
            flags: active_flags.clone(),
        },
    }
}

#[derive(Clone, Copy)]
enum ThreadCollectionKind {
    Recent,
    Archived,
    Pinned,
    History,
}

fn apply_thread_overlay(
    thread: &mut ThreadSummary,
    overlays: &HashMap<ThreadId, ThreadNotificationOverlay>,
    collection: ThreadCollectionKind,
) -> bool {
    let Some(overlay) = overlays.get(&thread.thread_id) else {
        return true;
    };
    if overlay.deleted
        || matches!(
            (collection, overlay.archived),
            (ThreadCollectionKind::Archived, Some(false))
                | (
                    ThreadCollectionKind::Recent | ThreadCollectionKind::Pinned,
                    Some(true)
                )
        )
    {
        return false;
    }
    if let Some(name) = &overlay.name {
        thread.title = name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| fallback_thread_title(&thread.preview));
    }
    if let Some(project_id) = &overlay.project_id {
        thread.project_id = project_id.clone();
    }
    if let Some(activity) = &overlay.activity {
        thread.activity = activity.clone();
    }
    true
}

fn apply_thread_overlays(
    threads: &mut Vec<ThreadSummary>,
    overlays: &HashMap<ThreadId, ThreadNotificationOverlay>,
    collection: ThreadCollectionKind,
) {
    threads.retain_mut(|thread| apply_thread_overlay(thread, overlays, collection));
}

fn apply_search_overlays(
    results: &mut Vec<ThreadSearchResult>,
    overlays: &HashMap<ThreadId, ThreadNotificationOverlay>,
) {
    results.retain_mut(|result| {
        apply_thread_overlay(&mut result.thread, overlays, ThreadCollectionKind::Recent)
    });
}

fn visit_thread_mut(
    snapshot: &mut WorkspaceSnapshot,
    thread_id: &str,
    mut visit: impl FnMut(&mut ThreadSummary),
) {
    for thread in snapshot
        .recent_threads
        .iter_mut()
        .chain(snapshot.archived_threads.iter_mut())
        .chain(snapshot.pinned_threads.iter_mut())
    {
        if thread.thread_id == thread_id {
            visit(thread);
        }
    }
    for result in &mut snapshot.search_results {
        if result.thread.thread_id == thread_id {
            visit(&mut result.thread);
        }
    }
}

fn remove_thread(snapshot: &mut WorkspaceSnapshot, thread_id: &str) {
    snapshot
        .recent_threads
        .retain(|thread| thread.thread_id != thread_id);
    snapshot
        .archived_threads
        .retain(|thread| thread.thread_id != thread_id);
    snapshot
        .pinned_threads
        .retain(|thread| thread.thread_id != thread_id);
    snapshot
        .search_results
        .retain(|result| result.thread.thread_id != thread_id);
}

fn upsert_project(projects: &mut Vec<Project>, project: Project) {
    if let Some(existing) = projects
        .iter_mut()
        .find(|existing| existing.project_id == project.project_id)
    {
        *existing = project;
    } else {
        projects.push(project);
    }
    projects.sort_by_key(|project| project.position);
}

fn upsert_thread(threads: &mut Vec<ThreadSummary>, thread: ThreadSummary) {
    if let Some(existing) = threads
        .iter_mut()
        .find(|existing| existing.thread_id == thread.thread_id)
    {
        *existing = thread;
    } else {
        threads.push(thread);
    }
    threads.sort_by_key(|thread| std::cmp::Reverse(thread.recency_at.unwrap_or(thread.updated_at)));
}

fn upsert_thread_everywhere(snapshot: &mut WorkspaceSnapshot, thread: ThreadSummary) {
    let id = thread.thread_id.clone();
    visit_thread_mut(snapshot, &id, |existing| *existing = thread.clone());
    if snapshot.thread(&id).is_none() {
        upsert_thread(&mut snapshot.recent_threads, thread);
    }
}

#[cfg(test)]
mod tests;
