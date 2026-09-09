//! AgentBackend adapter delegating to the application-owned manager.

use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use async_channel::Receiver;

use super::manager::CodexAppServerManager;
use crate::agent::{
    AgentBackend, AgentConnectionEvent, AgentModelCatalog, AgentPermissionMode,
    AgentPermissionProfile, AgentRequest, AgentRun, AgentThreadSettings, CreateProject,
    HistoryItemDetail, Page, PageRequest, Project, ProjectId, SideConversationRequest,
    ThreadHistoryItemEntry, ThreadId, ThreadListRequest, ThreadMetadataUpdate, ThreadSearchResult,
    ThreadSection, ThreadSectionAppearance, ThreadSectionId, ThreadSummary, ThreadTurn,
    UpdateProject, WorkspaceResult,
};

/// Codex CLI adapter. JSON-RPC details intentionally stay inside this module.
#[derive(Clone)]
pub struct CodexAppServerBackend {
    pub(super) manager: Arc<CodexAppServerManager>,
}

impl CodexAppServerBackend {
    pub fn new() -> Self {
        Self {
            manager: Arc::new(CodexAppServerManager::new()),
        }
    }

    pub fn with_manager(manager: Arc<CodexAppServerManager>) -> Self {
        Self { manager }
    }
}

impl Default for CodexAppServerBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentBackend for CodexAppServerBackend {
    fn capabilities(&self) -> crate::agent::AgentCapabilities {
        self.manager.capabilities()
    }

    fn subscribe_connection_events(&self) -> Receiver<AgentConnectionEvent> {
        self.manager.subscribe_connection_events()
    }

    fn open_side_conversation(
        &self,
        request: SideConversationRequest,
    ) -> Receiver<WorkspaceResult<ThreadId>> {
        self.manager.open_side_conversation(request)
    }

    fn close_side_conversation(&self, thread_id: ThreadId) -> Receiver<WorkspaceResult<()>> {
        self.manager.close_side_conversation(thread_id)
    }

    fn load_model_catalog(&self) -> Receiver<Result<AgentModelCatalog, String>> {
        self.manager.load_model_catalog()
    }

    fn load_permission_profiles(
        &self,
        cwd: PathBuf,
    ) -> Receiver<Result<Vec<AgentPermissionProfile>, String>> {
        self.manager.load_permission_profiles(cwd)
    }

    fn update_thread_permissions(
        &self,
        thread_id: String,
        cwd: PathBuf,
        mode: AgentPermissionMode,
    ) -> Receiver<Result<AgentThreadSettings, String>> {
        self.manager.update_thread_permissions(thread_id, cwd, mode)
    }

    fn list_projects(&self, page: PageRequest) -> Receiver<WorkspaceResult<Page<Project>>> {
        self.manager.list_projects(page)
    }

    fn create_project(&self, project: CreateProject) -> Receiver<WorkspaceResult<Project>> {
        self.manager.create_project(project)
    }

    fn update_project(
        &self,
        project_id: ProjectId,
        update: UpdateProject,
    ) -> Receiver<WorkspaceResult<Project>> {
        self.manager.update_project(project_id, update)
    }

    fn delete_project(&self, project_id: ProjectId) -> Receiver<WorkspaceResult<()>> {
        self.manager.delete_project(project_id)
    }

    fn move_project(
        &self,
        project_id: ProjectId,
        before_project_id: Option<ProjectId>,
    ) -> Receiver<WorkspaceResult<()>> {
        self.manager.move_project(project_id, before_project_id)
    }

    fn list_threads(
        &self,
        request: ThreadListRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSummary>>> {
        self.manager.list_threads(request)
    }

    fn search_threads(
        &self,
        request: ThreadListRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSearchResult>>> {
        self.manager.search_threads(request)
    }

    fn read_thread(&self, thread_id: ThreadId) -> Receiver<WorkspaceResult<ThreadSummary>> {
        self.manager.read_thread(thread_id)
    }

    fn list_thread_turns(
        &self,
        thread_id: ThreadId,
        page: PageRequest,
        detail: HistoryItemDetail,
    ) -> Receiver<WorkspaceResult<Page<ThreadTurn>>> {
        self.manager.list_thread_turns(thread_id, page, detail)
    }

    fn list_thread_items(
        &self,
        thread_id: ThreadId,
        turn_id: Option<String>,
        page: PageRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadHistoryItemEntry>>> {
        self.manager.list_thread_items(thread_id, turn_id, page)
    }

    fn set_thread_name(&self, thread_id: ThreadId, name: String) -> Receiver<WorkspaceResult<()>> {
        self.manager.set_thread_name(thread_id, name)
    }

    fn archive_thread(&self, thread_id: ThreadId) -> Receiver<WorkspaceResult<()>> {
        self.manager.archive_thread(thread_id)
    }

    fn unarchive_thread(&self, thread_id: ThreadId) -> Receiver<WorkspaceResult<ThreadSummary>> {
        self.manager.unarchive_thread(thread_id)
    }

    fn delete_thread(&self, thread_id: ThreadId) -> Receiver<WorkspaceResult<()>> {
        self.manager.delete_thread(thread_id)
    }

    fn update_thread_metadata(
        &self,
        thread_id: ThreadId,
        update: ThreadMetadataUpdate,
    ) -> Receiver<WorkspaceResult<ThreadSummary>> {
        self.manager.update_thread_metadata(thread_id, update)
    }

    fn list_thread_sections(
        &self,
        page: PageRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSection>>> {
        self.manager.list_thread_sections(page)
    }

    fn create_thread_section(
        &self,
        name: String,
        appearance: Option<ThreadSectionAppearance>,
    ) -> Receiver<WorkspaceResult<ThreadSection>> {
        self.manager.create_thread_section(name, appearance)
    }

    fn move_thread_to_section(
        &self,
        thread_id: ThreadId,
        section_id: Option<ThreadSectionId>,
        before_thread_id: Option<ThreadId>,
    ) -> Receiver<WorkspaceResult<()>> {
        self.manager
            .move_thread_to_section(thread_id, section_id, before_thread_id)
    }

    fn steer_turn(&self, request: crate::agent::AgentSteerRequest) -> Receiver<Result<(), String>> {
        self.manager.steer_turn(request)
    }

    fn run_prompt(&self, request: AgentRequest) -> AgentRun {
        self.manager.run_prompt(request)
    }
}
