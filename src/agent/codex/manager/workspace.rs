//! Workspace operations over the shared connection.

use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context as _, Result, bail};
use async_channel::Receiver;
use serde_json::{Value, json};

use super::{
    super::workspace_protocol::{
        object_field, parse_history_item, parse_history_turn, parse_page, parse_project,
        parse_thread_section, parse_thread_summary, response_result, sort_direction, string_field,
        thread_list_params, thread_sort_key,
    },
    CodexAppServerManager,
    connection::Connection,
};
use crate::agent::{
    AgentOptionalField, CreateProject, HistoryItemDetail, Page, PageRequest, Project, ProjectId,
    ThreadHistoryItemEntry, ThreadId, ThreadListRequest, ThreadMetadataUpdate, ThreadSearchResult,
    ThreadSection, ThreadSectionAppearance, ThreadSectionId, ThreadSummary, ThreadTurn,
    UpdateProject, WorkspaceError, WorkspaceResult,
};

pub(super) fn validate_workspace_response<T>(
    connection: &Connection,
    method: &str,
    parsed: Result<T>,
) -> Result<T> {
    match parsed {
        Ok(value) => Ok(value),
        Err(error) => {
            let message = format!("无法解析 {method} 响应；Codex 0.153.0 schema 不匹配：{error:#}");
            connection.fail_protocol(message.clone());
            bail!(message)
        }
    }
}

impl CodexAppServerManager {
    pub(super) fn workspace_call<T, F>(&self, operation: F) -> Receiver<WorkspaceResult<T>>
    where
        T: Send + 'static,
        F: FnOnce(CodexAppServerManager) -> Result<T> + Send + 'static,
    {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result =
                operation(manager).map_err(|error| WorkspaceError::backend(format!("{error:#}")));
            let _ = sender.send_blocking(result);
        });
        receiver
    }
    pub(in crate::agent::codex) fn list_projects(
        &self,
        page: PageRequest,
    ) -> Receiver<WorkspaceResult<Page<Project>>> {
        self.workspace_call(move |manager| manager.list_projects_blocking(page))
    }
    pub(super) fn list_projects_blocking(&self, page: PageRequest) -> Result<Page<Project>> {
        let connection = self.inner.ensure_connection()?;
        let response = connection.request(
            "project/list",
            json!({
                "cursor": page.cursor,
                "limit": page.limit,
                "sortKey": "position",
                "sortDirection": "asc"
            }),
        )?;
        validate_workspace_response(
            &connection,
            "project/list",
            parse_page(&response, "project/list", parse_project),
        )
    }
    pub(in crate::agent::codex) fn create_project(
        &self,
        project: CreateProject,
    ) -> Receiver<WorkspaceResult<Project>> {
        self.workspace_call(move |manager| manager.create_project_blocking(project))
    }
    pub(super) fn create_project_blocking(&self, project: CreateProject) -> Result<Project> {
        static NEXT_IDEMPOTENCY_KEY: AtomicU64 = AtomicU64::new(1);
        let connection = self.inner.ensure_connection()?;
        let roots = project
            .roots
            .into_iter()
            .map(|path| json!({ "path": path }))
            .collect::<Vec<_>>();
        let response = connection.request(
            "project/create",
            json!({
                "idempotencyKey": format!(
                    "gpui-{}-{}",
                    std::process::id(),
                    NEXT_IDEMPOTENCY_KEY.fetch_add(1, Ordering::Relaxed)
                ),
                "name": project.name,
                "roots": roots
            }),
        )?;
        validate_workspace_response(
            &connection,
            "project/create",
            (|| {
                parse_project(object_field(
                    response_result(&response, "project/create")?,
                    "project",
                    "project/create result",
                )?)
            })(),
        )
    }
    pub(in crate::agent::codex) fn update_project(
        &self,
        project_id: ProjectId,
        update: UpdateProject,
    ) -> Receiver<WorkspaceResult<Project>> {
        self.workspace_call(move |manager| manager.update_project_blocking(project_id, update))
    }
    pub(super) fn update_project_blocking(
        &self,
        project_id: ProjectId,
        update: UpdateProject,
    ) -> Result<Project> {
        let connection = self.inner.ensure_connection()?;
        let mut params = serde_json::Map::new();
        params.insert("projectId".into(), json!(project_id));
        if let Some(name) = update.name {
            params.insert("name".into(), json!(name));
        }
        if let Some(roots) = update.roots {
            params.insert(
                "roots".into(),
                Value::Array(
                    roots
                        .into_iter()
                        .map(|path| json!({ "path": path }))
                        .collect(),
                ),
            );
        }
        let response = connection.request("project/update", Value::Object(params))?;
        validate_workspace_response(
            &connection,
            "project/update",
            (|| {
                parse_project(object_field(
                    response_result(&response, "project/update")?,
                    "project",
                    "project/update result",
                )?)
            })(),
        )
    }
    pub(in crate::agent::codex) fn delete_project(
        &self,
        project_id: ProjectId,
    ) -> Receiver<WorkspaceResult<()>> {
        self.workspace_call(move |manager| {
            manager.empty_workspace_request("project/delete", json!({ "projectId": project_id }))
        })
    }
    pub(in crate::agent::codex) fn move_project(
        &self,
        project_id: ProjectId,
        before_project_id: Option<ProjectId>,
    ) -> Receiver<WorkspaceResult<()>> {
        self.workspace_call(move |manager| {
            manager.empty_workspace_request(
                "project/move",
                json!({ "projectId": project_id, "beforeProjectId": before_project_id }),
            )
        })
    }
    pub(super) fn empty_workspace_request(&self, method: &str, params: Value) -> Result<()> {
        let connection = self.inner.ensure_connection()?;
        let response = connection.request(method, params)?;
        validate_workspace_response(
            &connection,
            method,
            (|| {
                response_result(&response, method)?
                    .as_object()
                    .with_context(|| format!("{method} result 必须是对象"))?;
                Ok(())
            })(),
        )
    }
    pub(in crate::agent::codex) fn list_threads(
        &self,
        request: ThreadListRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSummary>>> {
        self.workspace_call(move |manager| manager.list_threads_blocking(request))
    }
    pub(super) fn list_threads_blocking(
        &self,
        request: ThreadListRequest,
    ) -> Result<Page<ThreadSummary>> {
        let connection = self.inner.ensure_connection()?;
        let response = connection.request("thread/list", thread_list_params(&request))?;
        validate_workspace_response(
            &connection,
            "thread/list",
            parse_page(&response, "thread/list", parse_thread_summary),
        )
    }
    pub(in crate::agent::codex) fn search_threads(
        &self,
        request: ThreadListRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSearchResult>>> {
        self.workspace_call(move |manager| manager.search_threads_blocking(request))
    }
    pub(super) fn search_threads_blocking(
        &self,
        request: ThreadListRequest,
    ) -> Result<Page<ThreadSearchResult>> {
        let search_term = request
            .search_term
            .as_deref()
            .filter(|term| !term.trim().is_empty())
            .context("thread search 需要非空搜索词")?;
        let connection = self.inner.ensure_connection()?;
        let response = connection.request(
            "thread/search",
            json!({
                "searchTerm": search_term,
                "archived": request.archived,
                "cursor": request.page.cursor,
                "limit": request.page.limit,
                "sortKey": thread_sort_key(request.sort_key),
                "sortDirection": sort_direction(request.sort_direction)
            }),
        )?;
        validate_workspace_response(
            &connection,
            "thread/search",
            parse_page(&response, "thread/search", |entry| {
                Ok(ThreadSearchResult {
                    thread: parse_thread_summary(object_field(
                        entry,
                        "thread",
                        "thread/search entry",
                    )?)?,
                    snippet: string_field(entry, "snippet", "thread/search entry")?,
                })
            }),
        )
    }
    pub(in crate::agent::codex) fn read_thread(
        &self,
        thread_id: ThreadId,
    ) -> Receiver<WorkspaceResult<ThreadSummary>> {
        self.workspace_call(move |manager| manager.read_thread_blocking(thread_id))
    }
    pub(super) fn read_thread_blocking(&self, thread_id: ThreadId) -> Result<ThreadSummary> {
        let connection = self.inner.ensure_connection()?;
        let response = connection.request(
            "thread/read",
            json!({ "threadId": thread_id, "includeTurns": false }),
        )?;
        validate_workspace_response(
            &connection,
            "thread/read",
            (|| {
                parse_thread_summary(object_field(
                    response_result(&response, "thread/read")?,
                    "thread",
                    "thread/read result",
                )?)
            })(),
        )
    }
    pub(in crate::agent::codex) fn list_thread_turns(
        &self,
        thread_id: ThreadId,
        page: PageRequest,
        detail: HistoryItemDetail,
    ) -> Receiver<WorkspaceResult<Page<ThreadTurn>>> {
        self.workspace_call(move |manager| {
            manager.list_thread_turns_blocking(thread_id, page, detail)
        })
    }
    pub(super) fn list_thread_turns_blocking(
        &self,
        thread_id: ThreadId,
        page: PageRequest,
        detail: HistoryItemDetail,
    ) -> Result<Page<ThreadTurn>> {
        let connection = self.inner.ensure_connection()?;
        let response = connection.request(
            "thread/turns/list",
            json!({
                "threadId": thread_id,
                "cursor": page.cursor,
                "limit": page.limit,
                "sortDirection": "asc",
                "itemsView": match detail {
                    HistoryItemDetail::NotLoaded => "notLoaded",
                    HistoryItemDetail::Summary => "summary",
                    HistoryItemDetail::Full => "full",
                }
            }),
        )?;
        validate_workspace_response(
            &connection,
            "thread/turns/list",
            parse_page(&response, "thread/turns/list", parse_history_turn),
        )
    }
    pub(in crate::agent::codex) fn list_thread_items(
        &self,
        thread_id: ThreadId,
        turn_id: Option<String>,
        page: PageRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadHistoryItemEntry>>> {
        self.workspace_call(move |manager| {
            manager.list_thread_items_blocking(thread_id, turn_id, page)
        })
    }
    pub(super) fn list_thread_items_blocking(
        &self,
        thread_id: ThreadId,
        turn_id: Option<String>,
        page: PageRequest,
    ) -> Result<Page<ThreadHistoryItemEntry>> {
        let connection = self.inner.ensure_connection()?;
        let response = connection.request(
            "thread/items/list",
            json!({
                "threadId": thread_id,
                "turnId": turn_id,
                "cursor": page.cursor,
                "limit": page.limit,
                "sortDirection": "asc"
            }),
        )?;
        validate_workspace_response(
            &connection,
            "thread/items/list",
            parse_page(&response, "thread/items/list", |entry| {
                Ok(ThreadHistoryItemEntry {
                    turn_id: string_field(entry, "turnId", "thread/items/list entry")?,
                    item: parse_history_item(object_field(
                        entry,
                        "item",
                        "thread/items/list entry",
                    )?)?,
                })
            }),
        )
    }
    pub(in crate::agent::codex) fn set_thread_name(
        &self,
        thread_id: ThreadId,
        name: String,
    ) -> Receiver<WorkspaceResult<()>> {
        self.workspace_call(move |manager| {
            manager.empty_workspace_request(
                "thread/name/set",
                json!({ "threadId": thread_id, "name": name }),
            )
        })
    }
    pub(in crate::agent::codex) fn archive_thread(
        &self,
        thread_id: ThreadId,
    ) -> Receiver<WorkspaceResult<()>> {
        self.workspace_call(move |manager| {
            manager.empty_workspace_request("thread/archive", json!({ "threadId": thread_id }))
        })
    }
    pub(in crate::agent::codex) fn unarchive_thread(
        &self,
        thread_id: ThreadId,
    ) -> Receiver<WorkspaceResult<ThreadSummary>> {
        self.workspace_call(move |manager| {
            let connection = manager.inner.ensure_connection()?;
            let response =
                connection.request("thread/unarchive", json!({ "threadId": thread_id }))?;
            validate_workspace_response(
                &connection,
                "thread/unarchive",
                (|| {
                    parse_thread_summary(object_field(
                        response_result(&response, "thread/unarchive")?,
                        "thread",
                        "thread/unarchive result",
                    )?)
                })(),
            )
        })
    }
    pub(in crate::agent::codex) fn delete_thread(
        &self,
        thread_id: ThreadId,
    ) -> Receiver<WorkspaceResult<()>> {
        self.workspace_call(move |manager| {
            manager.empty_workspace_request("thread/delete", json!({ "threadId": thread_id }))
        })
    }
    pub(in crate::agent::codex) fn update_thread_metadata(
        &self,
        thread_id: ThreadId,
        update: ThreadMetadataUpdate,
    ) -> Receiver<WorkspaceResult<ThreadSummary>> {
        self.workspace_call(move |manager| {
            let connection = manager.inner.ensure_connection()?;
            let mut params = serde_json::Map::new();
            params.insert("threadId".into(), json!(thread_id));
            match update.project {
                AgentOptionalField::Unspecified => {}
                AgentOptionalField::Null => {
                    // Codex 0.153.0 uses an empty string as the explicit
                    // project-unassignment sentinel; null only represents an
                    // omitted optional field in the generated JSON schema.
                    params.insert("projectId".into(), Value::String(String::new()));
                }
                AgentOptionalField::Value(project_id) => {
                    params.insert("projectId".into(), Value::String(project_id));
                }
            }
            let response = connection.request("thread/metadata/update", Value::Object(params))?;
            validate_workspace_response(
                &connection,
                "thread/metadata/update",
                (|| {
                    parse_thread_summary(object_field(
                        response_result(&response, "thread/metadata/update")?,
                        "thread",
                        "thread/metadata/update result",
                    )?)
                })(),
            )
        })
    }
    pub(in crate::agent::codex) fn list_thread_sections(
        &self,
        page: PageRequest,
    ) -> Receiver<WorkspaceResult<Page<ThreadSection>>> {
        self.workspace_call(move |manager| {
            let connection = manager.inner.ensure_connection()?;
            let response = connection.request(
                "threadSection/list",
                json!({ "cursor": page.cursor, "limit": page.limit }),
            )?;
            validate_workspace_response(
                &connection,
                "threadSection/list",
                parse_page(&response, "threadSection/list", parse_thread_section),
            )
        })
    }
    pub(in crate::agent::codex) fn create_thread_section(
        &self,
        name: String,
        appearance: Option<ThreadSectionAppearance>,
    ) -> Receiver<WorkspaceResult<ThreadSection>> {
        self.workspace_call(move |manager| {
            let connection = manager.inner.ensure_connection()?;
            let mut params = serde_json::Map::new();
            params.insert("name".into(), Value::String(name));
            if let Some(appearance) = appearance {
                params.insert(
                    "appearance".into(),
                    json!({ "icon": appearance.icon, "color": appearance.color }),
                );
            }
            let response = connection.request("threadSection/create", Value::Object(params))?;
            validate_workspace_response(
                &connection,
                "threadSection/create",
                (|| {
                    parse_thread_section(object_field(
                        response_result(&response, "threadSection/create")?,
                        "section",
                        "threadSection/create result",
                    )?)
                })(),
            )
        })
    }
    pub(in crate::agent::codex) fn move_thread_to_section(
        &self,
        thread_id: ThreadId,
        section_id: Option<ThreadSectionId>,
        before_thread_id: Option<ThreadId>,
    ) -> Receiver<WorkspaceResult<()>> {
        self.workspace_call(move |manager| {
            manager.empty_workspace_request(
                "thread/section/move",
                json!({
                    "threadId": thread_id,
                    "sectionId": section_id,
                    "beforeThreadId": before_thread_id
                }),
            )
        })
    }
}
