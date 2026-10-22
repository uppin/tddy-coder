//! Agent session launch, split out of `tddy-session-lifecycle` by `#carve` 21/21.
pub mod chat;
pub mod child_spawn_handler;
pub mod claude_cli_spawn;
pub mod claude_cli_spawn_steps;
pub mod cli_branch_starts;
pub mod cli_route_launch;
mod cli_session_worktree;
mod clone_backend;
mod clone_planner;
pub mod clone_ssh_env;
pub mod conversation_spawn;
pub mod conversation_spawn_handler;
pub mod conversation_worktree_op;
pub mod cursor_cli_spawn;
pub mod family_proto_bridge;
pub mod harness_profile_request;
pub mod harness_profile_start;
pub mod hooks_and_urls;
pub mod host_session_socket;
pub mod inherited_host_sockets;
pub mod jail_env_builders;
pub mod jail_launch_steps;
pub mod jail_relaunch;
pub mod jail_session_files;
pub mod jail_worktree;
pub mod launch_ports;
pub mod managed_launch;
mod opencode_cli_spawn;
pub mod placement;
mod provider_account_lookup;
pub mod relaunch_jail_dirs;
pub mod relaunch_jail_steps;
pub mod resume;
pub mod session_acting_identity;
pub mod session_coordinate_handlers;
pub mod session_git;
pub mod session_provider_route;
pub mod session_worktree_observer;
pub mod stack_child_spawn;
pub mod stack_parent;
pub mod stack_seed_validation;
pub mod start_keepalive;
pub mod start_request_checks;
pub mod svc_ensure_project_available_for_start;
pub mod svc_index_workspace_worktree;
pub mod svc_pr_status_for_caller;
pub mod svc_relaunch_sandboxed_runner;
pub mod svc_resume_claude_cli_session;
pub mod svc_resume_sandboxed_claude_cli_session;
pub mod svc_resume_sandboxed_cli_harness_session;
pub mod svc_resume_session;
pub mod svc_signal_delete_session;
pub mod svc_start_claude_cli_session;
pub mod svc_start_sandboxed_claude_cli_session;
pub mod svc_start_sandboxed_cli_harness_session;
pub mod svc_start_sandboxed_cursor_cli_session;
pub mod svc_start_session_core;
pub mod tool_session_route;
pub mod tool_session_spawn;
pub mod tool_spawn_plan;
pub mod workspace_branch_start;
pub mod worktree_source;

#[cfg(test)]
mod cli_route_launch_tests;
#[cfg(test)]
mod conversation_spawn_wiring_tests;
#[cfg(test)]
mod default_route_tests;
mod facilitation;
#[cfg(test)]
mod harness_profile_request_tests;
#[cfg(test)]
mod harness_profile_start_tests;
#[cfg(test)]
mod host_session_socket_tests;
#[cfg(test)]
mod provider_account_lookup_tests;
#[cfg(test)]
mod sandboxed_cli_harness_tests;
#[cfg(test)]
mod session_acting_identity_tests;
#[cfg(test)]
mod session_provider_route_tests;
#[cfg(test)]
mod start_request_checks_unit_tests;
#[cfg(test)]
mod tool_session_route_tests;

pub use start_request_checks::start_forward_deadline;
