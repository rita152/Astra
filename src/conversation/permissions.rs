//! Identity of a pending permission edit, independent of the active GPUI view.
use crate::agent::{AgentPermissionMode, AgentThreadPermissionResult};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PermissionChange {
    pub operation_id: u64,
    pub thread_id: String,
    pub generation: Option<u64>,
    pub selection: AgentPermissionMode,
}

impl PermissionChange {
    pub fn confirms(&self, result: &AgentThreadPermissionResult) -> bool {
        self.operation_id == result.operation_id
            && self.thread_id == result.thread_id
            && self
                .generation
                .is_none_or(|generation| generation == result.generation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receipt_identity_includes_thread_operation_and_generation() {
        let pending = PermissionChange {
            operation_id: 7,
            thread_id: "main".into(),
            generation: Some(3),
            selection: AgentPermissionMode::Profile("org".into()),
        };
        let mut result = AgentThreadPermissionResult {
            operation_id: 7,
            thread_id: "main".into(),
            generation: 3,
            settings: crate::agent::AgentThreadSettings {
                model: "m".into(),
                effort: None,
                service_tier: None,
                cwd: "/work".into(),
                permissions: None,
            },
        };
        assert!(pending.confirms(&result));
        result.thread_id = "side".into();
        assert!(!pending.confirms(&result));
        result.thread_id = "main".into();
        result.operation_id = 6;
        assert!(!pending.confirms(&result));
        result.operation_id = 7;
        result.generation = 4;
        assert!(!pending.confirms(&result));
    }
}
