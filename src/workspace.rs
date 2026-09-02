use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use async_channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};

use crate::agent::{
    AgentBackend, AgentCapabilities, AgentCapability, AgentConnectionEvent, AgentThreadStatusState,
    CreateProject, FilterValue, HistoryItemDetail, Page, PageRequest, Project, ProjectChange,
    ProjectId, ThreadActivity, ThreadHistory, ThreadId, ThreadListRequest, ThreadMetadataUpdate,
    ThreadSearchResult, ThreadSection, ThreadSectionId, ThreadSortKey, ThreadSummary, ThreadTurn,
    UpdateProject, WorkspaceError, WorkspaceResult,
};

const PREFERENCES_VERSION: u32 = 1;
const PAGE_SIZE: u32 = 100;
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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiPreferences {
    #[serde(default = "preferences_version")]
    pub version: u32,
    #[serde(default)]
    pub pinned_section_id: Option<ThreadSectionId>,
    #[serde(default)]
    pub collapsed_project_ids: BTreeSet<ProjectId>,
    #[serde(default)]
    pub pinned_collapsed: bool,
    #[serde(default)]
    pub projects_collapsed: bool,
    #[serde(default)]
    pub recent_collapsed: bool,
}

fn preferences_version() -> u32 {
    PREFERENCES_VERSION
}

impl UiPreferences {
    fn current() -> Self {
        Self {
            version: PREFERENCES_VERSION,
            ..Self::default()
        }
    }
}

#[derive(Debug)]
struct PreferenceStore {
    path: PathBuf,
    write_serial: AtomicU64,
}

impl PreferenceStore {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            write_serial: AtomicU64::new(1),
        }
    }

    fn load(&self) -> Result<UiPreferences, String> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(UiPreferences::current());
            }
            Err(error) => return Err(format!("无法读取 UI 偏好：{error}")),
        };
        let preferences: UiPreferences =
            serde_json::from_slice(&bytes).map_err(|error| format!("UI 偏好格式无效：{error}"))?;
        if preferences.version != PREFERENCES_VERSION {
            return Ok(UiPreferences::current());
        }
        Ok(preferences)
    }

    fn save(&self, preferences: &UiPreferences) -> Result<(), String> {
        let Some(parent) = self.path.parent() else {
            return Err("UI 偏好路径缺少父目录".to_owned());
        };
        fs::create_dir_all(parent).map_err(|error| format!("无法创建 UI 偏好目录：{error}"))?;
        let serial = self.write_serial.fetch_add(1, Ordering::Relaxed);
        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("ui-preferences.json");
        let temporary = parent.join(format!(
            ".{file_name}.{}.{}.tmp",
            std::process::id(),
            serial
        ));
        let write_result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)
                .map_err(|error| format!("无法创建 UI 偏好临时文件：{error}"))?;
            let bytes = serde_json::to_vec_pretty(preferences)
                .map_err(|error| format!("无法序列化 UI 偏好：{error}"))?;
            file.write_all(&bytes)
                .map_err(|error| format!("无法写入 UI 偏好：{error}"))?;
            file.write_all(b"\n")
                .map_err(|error| format!("无法完成 UI 偏好写入：{error}"))?;
            file.sync_all()
                .map_err(|error| format!("无法同步 UI 偏好：{error}"))?;
            fs::rename(&temporary, &self.path)
                .map_err(|error| format!("无法原子替换 UI 偏好：{error}"))?;
            if let Ok(directory) = File::open(parent) {
                let _ = directory.sync_all();
            }
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        write_result
    }
}

fn default_preferences_path() -> PathBuf {
    if let Some(path) = std::env::var_os("GPUI_UI_PREFERENCES_PATH") {
        return PathBuf::from(path);
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join("Library/Application Support/GPUI")
            .join("ui-preferences.json");
    }
    if let Some(config) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(config)
            .join("gpui")
            .join("ui-preferences.json");
    }
    PathBuf::from(".gpui-ui-preferences.json")
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
        let store = Arc::clone(self);
        std::thread::spawn(move || {
            let capabilities = store.snapshot().capabilities;
            let sections = if capabilities.supports(AgentCapability::ThreadSectionList) {
                load_all_sections(store.backend.as_ref())
            } else {
                Ok(Vec::new())
            };
            let mut projects = if capabilities.supports(AgentCapability::ProjectList) {
                load_all_projects(store.backend.as_ref())
            } else {
                Ok(Vec::new())
            };
            let (mut recent, mut archived) = if capabilities.supports(AgentCapability::ThreadList) {
                (
                    load_all_threads(store.backend.as_ref(), ThreadListRequest::default()),
                    load_all_threads(
                        store.backend.as_ref(),
                        ThreadListRequest {
                            archived: true,
                            ..ThreadListRequest::default()
                        },
                    ),
                )
            } else {
                (Ok(Vec::new()), Ok(Vec::new()))
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
                Some(section) if capabilities.supports(AgentCapability::ThreadList) => {
                    load_all_threads(
                        store.backend.as_ref(),
                        ThreadListRequest {
                            section: FilterValue::Value(section.section_id.clone()),
                            sort_key: ThreadSortKey::SectionPosition,
                            ..ThreadListRequest::default()
                        },
                    )
                }
                None => Ok(Vec::new()),
                Some(_) => Ok(Vec::new()),
            };
            let overlays = store.thread_overlays();
            let deleted_projects = store.deleted_projects();
            if let Ok(projects) = &mut projects {
                projects.retain(|project| !deleted_projects.contains(&project.project_id));
            }
            if let Ok(threads) = &mut recent {
                apply_thread_overlays(threads, &overlays, ThreadCollectionKind::Recent);
            }
            if let Ok(threads) = &mut archived {
                apply_thread_overlays(threads, &overlays, ThreadCollectionKind::Archived);
            }
            if let Ok(threads) = &mut pinned {
                apply_thread_overlays(threads, &overlays, ThreadCollectionKind::Pinned);
            }
            let projects_current =
                store.projects_generation.load(Ordering::Acquire) == projects_generation;
            let recent_current =
                store.recent_generation.load(Ordering::Acquire) == recent_generation;
            let archived_current =
                store.archived_generation.load(Ordering::Acquire) == archived_generation;
            let pinned_current =
                store.pinned_generation.load(Ordering::Acquire) == pinned_generation;
            let mut errors = Vec::new();
            if projects_current && let Err(error) = &projects {
                errors.push(error.user_message("加载项目"));
            }
            if recent_current && let Err(error) = &recent {
                errors.push(error.user_message("加载最近聊天"));
            }
            if archived_current && let Err(error) = &archived {
                errors.push(error.user_message("加载已归档聊天"));
            }
            if pinned_current {
                if let Err(error) = &sections {
                    errors.push(error.user_message("加载会话分区"));
                }
                if let Err(error) = &pinned {
                    errors.push(error.user_message("加载置顶聊天"));
                }
            }
            store.update(|snapshot| {
                if projects_current {
                    if let Ok(projects) = projects {
                        snapshot.projects = projects;
                    }
                    snapshot.loading.projects = false;
                }
                if recent_current {
                    if let Ok(recent) = recent {
                        snapshot.recent_threads = recent;
                    }
                    snapshot.loading.recent = false;
                }
                if archived_current {
                    if let Ok(archived) = archived {
                        snapshot.archived_threads = archived;
                    }
                    snapshot.loading.archived = false;
                }
                if pinned_current {
                    if let Ok(pinned) = pinned {
                        snapshot.pinned_threads = pinned;
                    }
                    if let Some(section) = pinned_section {
                        snapshot.preferences.pinned_section_id = Some(section.section_id);
                    } else if sections.is_ok() {
                        snapshot.preferences.pinned_section_id = None;
                    }
                    snapshot.loading.pinned = false;
                }
                if !errors.is_empty() {
                    snapshot.error = Some(errors.join("\n"));
                }
            });
            if pinned_current && preference_changed {
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
                let (turns, next_turn_cursor, backwards_turn_cursor) =
                    load_all_turns(store.backend.as_ref(), thread_id)?;
                Ok(ThreadHistory {
                    thread,
                    turns,
                    next_turn_cursor,
                    backwards_turn_cursor,
                })
            })();
            let _ = sender.send_blocking(result);
        });
        receiver
    }
}

fn receive<T>(receiver: Receiver<WorkspaceResult<T>>, label: &str) -> WorkspaceResult<T> {
    receiver
        .recv_blocking()
        .map_err(|_| WorkspaceError::backend(format!("{label}响应通道提前关闭")))?
}

fn load_all_projects(backend: &dyn AgentBackend) -> WorkspaceResult<Vec<Project>> {
    load_all_pages(|cursor| {
        backend.list_projects(PageRequest {
            cursor,
            limit: PAGE_SIZE,
        })
    })
}

fn load_all_sections(backend: &dyn AgentBackend) -> WorkspaceResult<Vec<ThreadSection>> {
    load_all_pages(|cursor| {
        backend.list_thread_sections(PageRequest {
            cursor,
            limit: PAGE_SIZE,
        })
    })
}

fn load_all_threads(
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

fn load_all_search_results(
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

fn load_all_turns(
    backend: &dyn AgentBackend,
    thread_id: ThreadId,
) -> WorkspaceResult<(Vec<ThreadTurn>, Option<String>, Option<String>)> {
    let mut cursor = None;
    let mut seen = HashSet::new();
    let mut turns = Vec::new();
    let mut backwards_cursor = None;
    loop {
        let mut page = receive(
            backend.list_thread_turns(
                thread_id.clone(),
                PageRequest {
                    cursor: cursor.clone(),
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
        backwards_cursor = backwards_cursor.or(page.backwards_cursor);
        turns.extend(page.data);
        let Some(next) = page.next_cursor else {
            return Ok((turns, None, backwards_cursor));
        };
        if !seen.insert(next.clone()) {
            return Err(WorkspaceError::backend(format!(
                "会话历史返回了重复分页 cursor `{next}`"
            )));
        }
        cursor = Some(next);
    }
}

fn load_all_pages<T>(
    mut request: impl FnMut(Option<String>) -> Receiver<WorkspaceResult<Page<T>>>,
) -> WorkspaceResult<Vec<T>> {
    let mut cursor = None;
    let mut seen = HashSet::new();
    let mut values = Vec::new();
    loop {
        let page = receive(request(cursor.clone()), "加载 workspace 分页")?;
        values.extend(page.data);
        let Some(next) = page.next_cursor else {
            return Ok(values);
        };
        if !seen.insert(next.clone()) {
            return Err(WorkspaceError::backend(format!(
                "workspace 返回了重复分页 cursor `{next}`"
            )));
        }
        cursor = Some(next);
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
mod tests {
    use super::*;
    use crate::agent::{
        AgentCapability, AgentEvent, AgentModelCatalog, AgentPermissionMode,
        AgentPermissionProfile, AgentRequest, AgentRun, AgentThreadSettings,
        CommandExecutionStatus, HistoryTurnStatus, ThreadHistoryItem, ThreadHistoryItemEntry,
    };
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
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
                                item_id: format!("{id}-user"),
                                text: format!("question {id}"),
                            },
                            ThreadHistoryItem::AssistantMessage {
                                item_id: format!("{id}-assistant"),
                                text: format!("answer {id}"),
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
                },
            }])))
        }

        fn set_thread_name(
            &self,
            thread_id: ThreadId,
            name: String,
        ) -> Receiver<WorkspaceResult<()>> {
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

        fn unarchive_thread(
            &self,
            thread_id: ThreadId,
        ) -> Receiver<WorkspaceResult<ThreadSummary>> {
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
            backend.calls().iter().any(|call| {
                call == "thread/section/move:thread-a:Some(\"pinned-section\"):None"
            })
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

    #[test]
    fn version_mismatch_does_not_reuse_old_preferences() {
        let directory = std::env::temp_dir().join(format!(
            "gpui-workspace-preferences-{}-{}",
            std::process::id(),
            PreferenceStore::new(PathBuf::new())
                .write_serial
                .fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("preferences.json");
        fs::write(&path, br#"{"version":999,"pinned_section_id":"stale"}"#).unwrap();
        let loaded = PreferenceStore::new(path.clone()).load().unwrap();
        assert_eq!(loaded, UiPreferences::current());
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn preferences_are_written_as_complete_versioned_json() {
        static SERIAL: AtomicU64 = AtomicU64::new(1);
        let directory = std::env::temp_dir().join(format!(
            "gpui-workspace-atomic-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let path = directory.join("preferences.json");
        let store = PreferenceStore::new(path.clone());
        let mut preferences = UiPreferences::current();
        preferences.pinned_section_id = Some("section-1".into());
        store.save(&preferences).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(bytes.ends_with(b"\n"));
        assert_eq!(
            serde_json::from_slice::<UiPreferences>(&bytes).unwrap(),
            preferences
        );
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}
