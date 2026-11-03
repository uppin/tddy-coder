//! Product-specific sandbox recipes and plan assembly.
//!
//! `tddy-sandbox` stays generic; this crate composes reads/copies/policy/env overlays for generic
//! shapes of process (a shell, a runner hosting a PTY) and lowers a declared tool's jail sections
//! ([`tool_grants`]). It names no coding agent: what an agent needs of a jail is declared in its
//! tool file (`#agent-tools 10/12`). Orchestrators (`tddy-daemon`, `tddy-actions`) build a
//! [`RunnerPlanRequest`] or [`ProcessPlanRequest`] and call [`build_runner_plan`] /
//! [`build_process_plan`] — the sandbox crate never sees action types.

pub mod homes;
pub mod plan;
mod route_files;
pub mod tool_grants;

pub use plan::{
    build_process_plan, build_runner_plan, recipe_from_name, route_secrets, runner_pty_policy,
    shell_interactive_policy, ProcessPlanRequest, RunnerPlanRequest, SandboxRecipe,
};
pub use tool_grants::{
    lower_files, lower_home, lower_network, lower_policy, lower_secrets, lower_tools,
    BinaryLibraries, FileGrants, HomeGrant, HostBinaryLibraries, JailPaths, ToolGrants,
};
