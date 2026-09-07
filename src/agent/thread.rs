//! Projects, thread collections, pagination, and restored history.

use std::path::PathBuf;

use serde_json::Value;

use super::{
    activity::{
        AgentCollaboration, AgentContextCompaction, AgentFileChange, AgentImageGeneration,
        AgentImageView, AgentMcpToolCall, CommandExecutionAction, CommandExecutionStatus,
    },
    requests::AgentOptionalField,
    status::AgentThreadActiveFlag,
};

/// Stable, agent-neutral identifier aliases used by the workspace UI.
pub type ProjectId = String;
pub type ThreadId = String;
pub type ThreadSectionId = String;
pub type PageCursor = String;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageRequest {
    pub cursor: Option<PageCursor>,
    pub limit: u32,
}

impl Default for PageRequest {
    fn default() -> Self {
        Self {
            cursor: None,
            limit: 50,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page<T> {
    pub data: Vec<T>,
    pub next_cursor: Option<PageCursor>,
    pub backwards_cursor: Option<PageCursor>,
}

impl<T> Page<T> {
    #[cfg(test)]
    pub fn single(data: Vec<T>) -> Self {
        Self {
            data,
            next_cursor: None,
            backwards_cursor: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub project_id: ProjectId,
    pub name: String,
    pub roots: Vec<PathBuf>,
    pub created_at: i64,
    pub updated_at: i64,
    pub recency_at: Option<i64>,
    pub position: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateProject {
    pub name: String,
    pub roots: Vec<PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateProject {
    pub name: Option<String>,
    pub roots: Option<Vec<PathBuf>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadSection {
    pub section_id: ThreadSectionId,
    pub name: String,
    pub appearance: Option<ThreadSectionAppearance>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadSectionAppearance {
    pub icon: Option<String>,
    pub color: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThreadActivity {
    NotLoaded,
    Idle,
    SystemError,
    Active { flags: Vec<AgentThreadActiveFlag> },
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadSummary {
    pub thread_id: ThreadId,
    pub title: String,
    pub preview: String,
    pub cwd: PathBuf,
    pub project_id: Option<ProjectId>,
    pub section: Option<ThreadSection>,
    pub created_at: i64,
    pub updated_at: i64,
    pub recency_at: Option<i64>,
    pub activity: ThreadActivity,
}

// The backend supports every protocol sort mode even though the current UI only
// constructs recency and section-position requests.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadSortKey {
    CreatedAt,
    UpdatedAt,
    RecencyAt,
    SectionPosition,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

// `None` is distinct from an omitted filter in the Codex app-server protocol.
#[allow(dead_code)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum FilterValue<T> {
    #[default]
    Any,
    None,
    Value(T),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadListRequest {
    pub page: PageRequest,
    pub archived: bool,
    pub project: FilterValue<ProjectId>,
    pub section: FilterValue<ThreadSectionId>,
    pub search_term: Option<String>,
    pub sort_key: ThreadSortKey,
    pub sort_direction: SortDirection,
}

impl Default for ThreadListRequest {
    fn default() -> Self {
        Self {
            page: PageRequest::default(),
            archived: false,
            project: FilterValue::Any,
            section: FilterValue::Any,
            search_term: None,
            sort_key: ThreadSortKey::RecencyAt,
            sort_direction: SortDirection::Descending,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadSearchResult {
    pub thread: ThreadSummary,
    pub snippet: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryItemDetail {
    NotLoaded,
    Summary,
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryTurnStatus {
    InProgress,
    Completed,
    Interrupted,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UserMessageImage {
    Local(PathBuf),
    Remote(String),
    Unavailable(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThreadHistoryItem {
    UserMessage {
        item_id: String,
        text: String,
        images: Vec<UserMessageImage>,
    },
    AssistantMessage {
        item_id: String,
        text: String,
        /// Distinguishes progress commentary from the final resumed answer.
        phase: Option<String>,
    },
    Reasoning {
        item_id: String,
        summary: Vec<String>,
        content: Vec<String>,
    },
    Command {
        item_id: String,
        command: String,
        output: String,
        status: CommandExecutionStatus,
        actions: Vec<CommandExecutionAction>,
        cwd: Option<String>,
        exit_code: Option<i64>,
    },
    FileChange(AgentFileChange),
    ImageView(AgentImageView),
    ImageGeneration(AgentImageGeneration),
    ContextCompaction(AgentContextCompaction),
    Collaboration(AgentCollaboration),
    McpToolCall(Box<AgentMcpToolCall>),
    WebSearch {
        item_id: String,
        query: String,
        action: Value,
        results: Value,
    },
    Unsupported {
        item_id: String,
        kind: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadTurn {
    pub turn_id: String,
    pub status: HistoryTurnStatus,
    pub items_view: HistoryItemDetail,
    pub items: Vec<ThreadHistoryItem>,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadHistoryItemEntry {
    pub turn_id: String,
    pub item: ThreadHistoryItem,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadHistory {
    pub thread: ThreadSummary,
    pub turns: Vec<ThreadTurn>,
    pub next_turn_cursor: Option<PageCursor>,
    pub backwards_turn_cursor: Option<PageCursor>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadMetadataUpdate {
    /// `Unspecified` keeps the server value, `Null` clears it, and `Value`
    /// assigns the thread to a project. Concrete adapters own the wire
    /// representation for the clear operation.
    pub project: AgentOptionalField<ProjectId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectChange {
    Created,
    Updated,
    Deleted,
}
