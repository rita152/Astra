//! Application-facing backend contract, capabilities, errors, and run ownership.

use std::{collections::BTreeSet, fmt, path::PathBuf, sync::Arc};

use async_channel::Receiver;

use super::{
    catalog::{
        AgentModelCatalog, AgentPermissionMode, AgentPermissionProfile, AgentThreadSettings,
    },
    events::{AgentConnectionEvent, AgentEvent},
    thread::{
        CreateProject, HistoryItemDetail, Page, PageRequest, Project, ProjectId,
        ThreadHistoryItemEntry, ThreadId, ThreadListRequest, ThreadMetadataUpdate,
        ThreadSearchResult, ThreadSection, ThreadSectionAppearance, ThreadSectionId, ThreadSummary,
        ThreadTurn, UpdateProject,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AgentCapability {
    ProjectList,
    ProjectCreate,
    ProjectUpdate,
    ProjectDelete,
    ProjectMove,
    ThreadList,
    ThreadSearch,
    ThreadRead,
    ThreadTurnsList,
    ThreadItemsList,
    ThreadRename,
    ThreadArchive,
    ThreadUnarchive,
    ThreadDelete,
    ThreadMetadataUpdate,
    ThreadSectionList,
    ThreadSectionCreate,
    ThreadSectionMove,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentCapabilities {
    supported: BTreeSet<AgentCapability>,
}

impl AgentCapabilities {
    pub fn new(capabilities: impl IntoIterator<Item = AgentCapability>) -> Self {
        Self {
            supported: capabilities.into_iter().collect(),
        }
    }

    pub fn supports(&self, capability: AgentCapability) -> bool {
        self.supported.contains(&capability)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unsupported {
    pub capability: AgentCapability,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceError {
    Unsupported(Unsupported),
    Backend(String),
}

impl WorkspaceError {
    pub fn backend(message: impl Into<String>) -> Self {
        Self::Backend(message.into())
    }

    /// Produces an agent-neutral message suitable for product UI. Concrete
    /// protocol and transport details remain available in the error value for
    /// diagnostics, but must not cross into views.
    pub fn user_message(&self, action: &str) -> String {
        match self {
            Self::Unsupported(_) => format!("当前 coding agent 不支持{action}"),
            Self::Backend(_) => format!("{action}失败，请重试"),
        }
    }

    fn unsupported(capability: AgentCapability) -> Self {
        Self::Unsupported(Unsupported {
            capability,
            message: format!("当前 coding agent 不支持 {capability:?}"),
        })
    }
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(unsupported) => formatter.write_str(&unsupported.message),
            Self::Backend(message) => formatter.write_str(message),
        }
    }
}

pub type WorkspaceResult<T> = Result<T, WorkspaceError>;

/// Agent-neutral input consumed by every coding-agent adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRequest {
    pub prompt: String,
    pub cwd: PathBuf,
    pub project_id: Option<ProjectId>,
    pub thread_id: Option<String>,
    pub model: String,
    pub effort: String,
    pub service_tier: Option<String>,
    pub permission_mode: AgentPermissionMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentInterruptOutcome {
    Requested,
    AlreadyRequested,
    AlreadyFinished,
}

pub(crate) trait AgentInterruptControl: Send + Sync {
    fn request_interrupt(&self) -> Result<AgentInterruptOutcome, String>;
    fn abandon(&self);
}

pub struct AgentInterruptHandle {
    control: Arc<dyn AgentInterruptControl>,
}

impl AgentInterruptHandle {
    pub(crate) fn new(control: Arc<dyn AgentInterruptControl>) -> Self {
        Self { control }
    }

    pub fn interrupt(&self) -> Result<AgentInterruptOutcome, String> {
        self.control.request_interrupt()
    }
}

impl Drop for AgentInterruptHandle {
    fn drop(&mut self) {
        self.control.abandon();
    }
}

pub struct AgentRun {
    events: Receiver<AgentEvent>,
    interrupt: Option<AgentInterruptHandle>,
}

impl AgentRun {
    pub(crate) fn new(
        events: Receiver<AgentEvent>,
        interrupt: Option<AgentInterruptHandle>,
    ) -> Self {
        Self { events, interrupt }
    }

    pub fn into_parts(self) -> (Receiver<AgentEvent>, Option<AgentInterruptHandle>) {
        (self.events, self.interrupt)
    }
}

/// Boundary between the application and a concrete coding-agent protocol.
pub trait AgentBackend: Send + Sync {
    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities::default()
    }

    fn subscribe_connection_events(&self) -> Receiver<AgentConnectionEvent>;
    #[cfg_attr(test, allow(dead_code))]
    fn load_model_catalog(&self) -> Receiver<Result<AgentModelCatalog, String>>;
    #[allow(dead_code)]
    fn load_permission_profiles(
        &self,
        cwd: PathBuf,
    ) -> Receiver<Result<Vec<AgentPermissionProfile>, String>>;
    fn update_thread_permissions(
        &self,
        thread_id: String,
        cwd: PathBuf,
        mode: AgentPermissionMode,
    ) -> Receiver<Result<AgentThreadSettings, String>>;

    fn list_projects(&self, _page: PageRequest) -> Receiver<WorkspaceResult<Page<Project>>> {
        unsupported_receiver(AgentCapability::ProjectList)
    }

    fn create_project(&self, _project: CreateProject) -> Receiver<WorkspaceResult<Project>> {
        unsupported_receiver(AgentCapability::ProjectCreate)
    }

    fn update_project(
        &self,
        _project_id: ProjectId,
        _update: UpdateProject,
    ) -> Receiver<WorkspaceResult<Project>> {
        unsupported_receiver(AgentCapability::ProjectUpdate)
    }

    fn delete_project(&self, _project_id: ProjectId) -> Receiver<WorkspaceResult<()>> {
        unsupported_receiver(AgentCapability::ProjectDelete)
    }

    fn move_project(
        &self,
        _project_id: ProjectId,
        _before_project_id: Option<ProjectId>,
    ) -> Receiver<WorkspaceResult<()>> {
        unsupported_receiver(AgentCapability::ProjectMove)
    }

    fn list_threads(
        &self,
        _request: ThreadListRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSummary>>> {
        unsupported_receiver(AgentCapability::ThreadList)
    }

    fn search_threads(
        &self,
        _request: ThreadListRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSearchResult>>> {
        unsupported_receiver(AgentCapability::ThreadSearch)
    }

    fn read_thread(&self, _thread_id: ThreadId) -> Receiver<WorkspaceResult<ThreadSummary>> {
        unsupported_receiver(AgentCapability::ThreadRead)
    }

    fn list_thread_turns(
        &self,
        _thread_id: ThreadId,
        _page: PageRequest,
        _detail: HistoryItemDetail,
    ) -> Receiver<WorkspaceResult<Page<ThreadTurn>>> {
        unsupported_receiver(AgentCapability::ThreadTurnsList)
    }

    fn list_thread_items(
        &self,
        _thread_id: ThreadId,
        _turn_id: Option<String>,
        _page: PageRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadHistoryItemEntry>>> {
        unsupported_receiver(AgentCapability::ThreadItemsList)
    }

    fn set_thread_name(
        &self,
        _thread_id: ThreadId,
        _name: String,
    ) -> Receiver<WorkspaceResult<()>> {
        unsupported_receiver(AgentCapability::ThreadRename)
    }

    fn archive_thread(&self, _thread_id: ThreadId) -> Receiver<WorkspaceResult<()>> {
        unsupported_receiver(AgentCapability::ThreadArchive)
    }

    fn unarchive_thread(&self, _thread_id: ThreadId) -> Receiver<WorkspaceResult<ThreadSummary>> {
        unsupported_receiver(AgentCapability::ThreadUnarchive)
    }

    fn delete_thread(&self, _thread_id: ThreadId) -> Receiver<WorkspaceResult<()>> {
        unsupported_receiver(AgentCapability::ThreadDelete)
    }

    fn update_thread_metadata(
        &self,
        _thread_id: ThreadId,
        _update: ThreadMetadataUpdate,
    ) -> Receiver<WorkspaceResult<ThreadSummary>> {
        unsupported_receiver(AgentCapability::ThreadMetadataUpdate)
    }

    fn list_thread_sections(
        &self,
        _page: PageRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSection>>> {
        unsupported_receiver(AgentCapability::ThreadSectionList)
    }

    fn create_thread_section(
        &self,
        _name: String,
        _appearance: Option<ThreadSectionAppearance>,
    ) -> Receiver<WorkspaceResult<ThreadSection>> {
        unsupported_receiver(AgentCapability::ThreadSectionCreate)
    }

    fn move_thread_to_section(
        &self,
        _thread_id: ThreadId,
        _section_id: Option<ThreadSectionId>,
        _before_thread_id: Option<ThreadId>,
    ) -> Receiver<WorkspaceResult<()>> {
        unsupported_receiver(AgentCapability::ThreadSectionMove)
    }

    fn run_prompt(&self, request: AgentRequest) -> AgentRun;
}

fn unsupported_receiver<T: Send + 'static>(
    capability: AgentCapability,
) -> Receiver<WorkspaceResult<T>> {
    let (sender, receiver) = async_channel::bounded(1);
    let _ = sender.send_blocking(Err(WorkspaceError::unsupported(capability)));
    receiver
}
