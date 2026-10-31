//! The hermetic-action port (`#agent-tools 4/12`).
//!
//! PRD: `docs/ft/build/1-WIP/PRD-2026-10-31-build-tools-are-tool-defs.md`.
//!
//! An action with `sandbox: true` — or every action of a `--hermetic` run — is not spawned by the
//! executor itself. It is handed to an [`ActionSandbox`], which runs it in a jail where:
//!
//! - the action's inputs are read-only;
//! - its declared outputs ([`ActionSpec::outputs`]) are the only writable paths;
//! - its tool closure is the only set of executables beyond the jail's baseline;
//! - its network is the lowering of that closure's `network:` sections.
//!
//! It is a port so this crate stays free of sandbox crates: the implementation is
//! `tddy_daemon_sandbox::build_action_sandbox::DaemonActionSandbox`, wired by the binaries that
//! run builds.

use futures::future::BoxFuture;
use tddy_actions::tool::ResolvedTool;
use tddy_actions::ActionSpec;

use crate::error::BuildError;
use crate::executor::ActionOutcome;

/// Runs one build action hermetically.
pub trait ActionSandbox: Send + Sync {
    /// Run `spec` confined to its inputs, its outputs and `tools` — the resolved closure of the
    /// action's `tool_dep_ids`, deps before dependents. An action the jail stops (an undeclared
    /// write, an undeclared exec) is an `Ok` outcome with a non-zero exit and the refusal in its
    /// `stderr`; `Err` is reserved for a jail that could not be started at all.
    fn run<'a>(
        &'a self,
        spec: &'a ActionSpec,
        tools: &'a [ResolvedTool],
    ) -> BoxFuture<'a, Result<ActionOutcome, BuildError>>;
}
