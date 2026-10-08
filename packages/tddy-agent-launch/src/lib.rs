//! Agent session launch, split out of `tddy-session-lifecycle` by `#carve` 21/21.
pub mod chat;
pub mod child_spawn_handler;
pub mod claude_cli_spawn;
pub mod claude_cli_spawn_steps;
pub mod cli_branch_starts;
pub mod conversation_spawn;
pub mod conversation_spawn_handler;
pub mod conversation_worktree_op;
pub mod cursor_cli_spawn;
pub mod family_proto_bridge;
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
pub mod relaunch_jail_dirs;
pub mod relaunch_jail_steps;
pub mod resume;
pub mod session_acting_identity;
pub mod session_coordinate_handlers;
pub mod session_worktree_observer;
pub mod stack_child_spawn;
pub mod stack_parent;
pub mod stack_seed_validation;
pub mod start_request_checks;
pub mod svc_ensure_project_available_for_start;
pub mod svc_index_workspace_worktree;
pub mod svc_pr_status_for_caller;
pub mod svc_relaunch_sandboxed_runner;
pub mod svc_resume_claude_cli_session;
pub mod svc_resume_sandboxed_claude_cli_session;
pub mod svc_resume_session;
pub mod svc_signal_delete_session;
pub mod svc_start_claude_cli_session;
pub mod svc_start_sandboxed_claude_cli_session;
pub mod svc_start_sandboxed_cursor_cli_session;
pub mod svc_start_session_core;
pub mod tool_session_spawn;
pub mod tool_spawn_plan;
pub mod workspace_branch_start;
pub mod worktree_source;

#[cfg(test)]
mod conversation_spawn_wiring_tests;
#[cfg(test)]
mod host_session_socket_tests;
#[cfg(test)]
mod session_acting_identity_tests;
