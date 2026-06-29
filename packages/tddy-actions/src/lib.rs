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
