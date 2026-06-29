//! Product-specific sandbox recipes and plan assembly.
//!
//! `tddy-sandbox` stays generic; this crate composes reads/copies/policy/env overlays for
//! known workloads (Claude CLI, shell, …). Orchestrators (`tddy-daemon`, `tddy-actions`) build
//! a [`RunnerPlanRequest`] or [`ProcessPlanRequest`] and call [`build_runner_plan`] /
//! [`build_process_plan`] — the sandbox crate never sees action types.

pub mod claude_cli;
pub mod plan;

pub use claude_cli::{
    append_claude_mcp_args, build_claude_allowlist, claude_credentials_copies,
    claude_interactive_policy, claude_runner_env_overlay, claude_scratch_mcp_dir,
    process_claude_exec_reads, write_claude_mcp_config,
};
pub use plan::{
    build_process_plan, build_runner_plan, detect_recipe_from_argv, recipe_from_name,
    shell_interactive_policy, ProcessPlanRequest, RunnerPlanRequest, SandboxRecipe,
};
