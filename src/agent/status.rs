//! Connection, thread, and account status snapshots.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentConfigWarning {
    pub summary: String,
    pub details: Option<String>,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub column: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentMcpServerStartupState {
    Starting,
    Ready,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentMcpServerStartupFailureReason {
    ReauthenticationRequired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentMcpServerStartupStatus {
    pub thread_id: Option<String>,
    pub name: String,
    pub state: AgentMcpServerStartupState,
    pub error: Option<String>,
    pub failure_reason: Option<AgentMcpServerStartupFailureReason>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentThreadActiveFlag {
    WaitingOnApproval,
    WaitingOnUserInput,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentThreadStatusState {
    NotLoaded,
    Idle,
    SystemError,
    Active {
        active_flags: Vec<AgentThreadActiveFlag>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadStatus {
    pub thread_id: String,
    pub state: AgentThreadStatusState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTokenUsageBreakdown {
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub cache_write_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentThreadTokenUsage {
    pub thread_id: String,
    pub turn_id: String,
    pub total: AgentTokenUsageBreakdown,
    pub last: AgentTokenUsageBreakdown,
    pub model_context_window: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRateLimitWindow {
    pub used_percent: i32,
    pub window_duration_mins: Option<i64>,
    pub resets_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentCreditsSnapshot {
    pub has_credits: bool,
    pub unlimited: bool,
    pub balance: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentSpendControlLimit {
    pub limit: String,
    pub used: String,
    pub remaining_percent: i32,
    pub resets_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentAccountRateLimits {
    pub limit_id: Option<String>,
    pub limit_name: Option<String>,
    pub primary: Option<AgentRateLimitWindow>,
    pub secondary: Option<AgentRateLimitWindow>,
    pub credits: Option<AgentCreditsSnapshot>,
    pub individual_limit: Option<AgentSpendControlLimit>,
    pub spend_control_reached: Option<bool>,
    pub plan_type: Option<String>,
    pub rate_limit_reached_type: Option<String>,
}
