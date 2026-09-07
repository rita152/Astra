//! Isolated compile/test entry for the protocol-neutral permission card.
//!
//! Production app-server dispatch adapts typed domain data into this model;
//! this crate additionally compiles the presentation layer independently of
//! the protocol adapter.

#![allow(dead_code)]

#[path = "../src/components/icons.rs"]
mod icons_impl;
#[path = "../src/theme.rs"]
mod theme;

#[path = "../src/components/callback.rs"]
mod callback_impl;

mod components {
    pub mod callback {
        pub use crate::callback_impl::*;
    }
    pub mod icons {
        pub use crate::icons_impl::*;
    }
}

#[path = "../src/components/permissions_approval.rs"]
mod permissions_approval;

#[test]
fn isolated_component_is_linked_without_protocol_types() {
    let model =
        permissions_approval::PermissionApprovalPresentation::network("isolated-link-check", None);
    assert!(model.should_render());
}
