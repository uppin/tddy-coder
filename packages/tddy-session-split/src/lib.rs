//! Split and sandboxed-codebase sessions, split out of `tddy-session-lifecycle` by `#carve` 21/21.
pub mod agent_argv;
pub mod agent_credentials;
pub mod attached_initial_prompt;
pub mod service_util;
pub mod split_claude_cli_start;
pub mod split_ports;
pub mod split_session;
pub mod split_start;
pub mod svc_paired_codebase_teardown;
pub mod svc_provision_workspace_tool_sandbox;
pub mod svc_resolve_tddy_tools_path;
pub mod svc_resume_split_wiring;
pub mod svc_spawn_split_agent;
pub mod svc_split_context_from_codebase_host;
pub mod svc_start_sandboxed_codebase_session;
pub mod workspace_session;
