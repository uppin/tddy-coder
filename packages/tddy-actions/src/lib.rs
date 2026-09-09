//! Unified action spec and runtime for tddy tools.
//!
//! Every tool (Claude CLI, Bash, tddy-coder, build actions, session actions) is described
//! by an [`ActionSpec`] and executed via [`ActionRuntime`], producing a [`tddy_task::TaskHandle`].

pub mod catalog;
pub mod convert;
pub mod error;
pub mod pipeline;
pub mod process_runtime;
pub mod result_kind;
pub mod spec;

pub use catalog::ActionCatalog;
pub use convert::{
    action_spec_from_session_manifest, build_action_fields_to_spec, BuildActionFields,
    SessionManifestFields,
};
pub use error::ActionError;
pub use pipeline::PipelineRuntime;
pub use process_runtime::ProcessRuntime;
pub use result_kind::apply_result_kind;
pub use spec::{
    ActionInput, ActionOutput, ActionSpec, ChannelMode, OutputKind, PipelineSpec, PipelineStage,
    SandboxRequest, SessionActionExtras,
};

/// The `actions.ActionService` entry the daemon's wiring layer registers.
///
/// `#unbundle` node 3 moved this service out of `tddy-daemon` and into the crate that already owns
/// the session-action surface this crate already implements. The daemon's whole contract with a subsystem is a
/// [`tddy_rpc::ServiceEntry`], so registration becomes a call to the owner rather than a
/// daemon-internal type.
pub fn build_action_service_entry() -> tddy_rpc::ServiceEntry {
    // TODO(sandbox-spawn-services): implement
    unimplemented!("build_action_service_entry")
}

#[cfg(test)]
mod unbundle_service_entry_tests {
    #[test]
    fn names_the_service_the_wiring_layer_registers() {
        assert_eq!(
            super::build_action_service_entry().name,
            "actions.ActionService"
        );
    }
}
