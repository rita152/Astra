//! Isolated compile/test entry for the capture-only permission card.
//!
//! The component is available to the deterministic screenshot harness, but
//! production app-server dispatch remains gated until every required renderer
//! state clears the per-state pixel threshold. This crate also compiles the
//! presentation model independently of that protocol layer.

#![allow(dead_code)]

#[path = "../src/components/icons.rs"]
mod icons_impl;
#[path = "../src/theme.rs"]
mod theme;

mod components {
    pub mod icons {
        pub use crate::icons_impl::*;
    }
}

#[path = "../src/components/permissions_approval.rs"]
mod permissions_approval;

#[test]
fn isolated_component_is_linked_without_protocol_integration() {
    let model =
        permissions_approval::PermissionApprovalPresentation::network("isolated-link-check", None);
    assert!(model.should_render());
}
