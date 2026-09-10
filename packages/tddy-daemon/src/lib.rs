//! tddy-daemon library — shared by binary and tests.

pub mod action_service;
/// The per-chat active-elicitation lease, which now lives in `tddy-telegram`.
///
/// Re-exported under its own name so `crate::active_elicitation::X` — and
/// `tddy_daemon::active_elicitation::X` in the four acceptance suites that stay here — goes on
/// resolving unchanged. No caller moved with it.
pub use tddy_telegram::active_elicitation;
pub mod agent_list_mapping;
/// The three modules of `tddy-daemon-auth` this crate still names, out of the seven that moved
/// there.
///
/// Named one by one rather than globbed, for the same reason the worktree and host lines below
/// give — and the four that are absent are the point: `github_token_store`, `token_provider` and
/// `codex_oauth_participant_metadata` had exactly one caller each and it moved with them, while
/// `codex_oauth_relay`'s only caller was `tddy-integration-tests`, which now depends on the auth
/// crate directly. A re-export kept for nobody is an edge that reads as real.
pub use tddy_daemon_auth::{auth, github_pr_credentials, oauth_loopback_tunnel};
/// The eight git/worktree modules, which now live in `tddy-worktree-service`.
///
/// Named one by one rather than globbed: both new crates carry a `service` and a `stream`
/// module, and two globs would re-export each name twice under one path. Every module keeps its
/// own name in the crate it moved to, so `crate::worktrees::X` goes on resolving here and no
/// caller in this crate changed.
pub use tddy_worktree_service::{
    base_sync_cache, branch_intent, branch_owner, project_provision, project_storage,
    remote_git_service, worktree_files, worktrees,
};
pub mod claude_cli_session;
pub mod cli_session_manager;
pub use tddy_daemon_kernel::config;
pub mod connection_service;
pub mod connection_tonic_adapter;
/// The ten session-file-I/O modules, which now live in `tddy-session-files`.
///
/// Named one by one rather than globbed, for the reason the worktree, host and LiveKit facades
/// above give: every module keeps its own name in the crate it moved to, so `crate::context_files::X`
/// goes on resolving here and no caller in this crate changed.
///
/// The edge runs `tddy-daemon` → `tddy-session-files`, which is the direction the extraction needed:
/// none of the ten names `connection_service`, while `connection_service` and eight of its
/// submodules call *into* nine of them.
pub use tddy_session_files::{
    context_files, context_sync, host_documents, session_attachment_staging, session_attachments,
    session_context_docs, session_file_upload, session_uploads, session_workflow_files,
    stack_doc_attachments,
};
pub mod cursor_cli_spawn;
pub mod daemon_config_service;
pub mod daemon_settings;
/// Presenter-gate classification, which now lives in `tddy-telegram`.
///
/// `session_list_enrichment` and `session_notifications` are its callers and stay here; that edge
/// is `tddy-daemon` → `tddy-telegram`, the same direction as every other facade in this file.
pub use tddy_telegram::elicitation;
pub mod host_tonic_adapter;
/// The five LiveKit modules, which now live in `tddy-daemon-livekit`.
///
/// Named one by one for the reason the worktree and host facades above give: every module keeps
/// its own name in the crate it moved to, so `crate::session_room::X` goes on resolving here and
/// no caller in this crate changed.
pub use tddy_daemon_livekit::{
    common_room_supervisor, livekit_peer_discovery, livekit_rooms_stream, livekit_service,
    session_room,
};
/// The nine host modules and the four host-key ones, which now live in `tddy-host-service`.
///
/// Named one by one for the reason above; see `tddy_worktree_service`'s facade.
pub use tddy_host_service::{
    host_desktop_targets, host_keypair, host_messages, host_private_key, host_prompt_stream,
    host_prompts, host_registry, host_session_service, host_stats, host_tooling, multi_host,
    remote_desktop_probe, ssh_agent, ssh_agent_add,
};
pub mod local_socket_server;
pub mod presenter_intent_client;
pub mod pty_registry;
pub mod pty_runtime;
pub mod relay_idle;
pub mod runtime;
pub mod server;
pub mod session_admission_service;
pub mod session_agent_clone;
pub mod session_agent_inference;
pub mod session_agent_roster;
pub mod session_agent_status;
pub mod session_deletion;
pub mod session_list_enrichment;
pub mod session_notification_subscribers;
pub mod session_notifications;
pub mod session_reader;
pub mod session_toolcall;
pub mod split_session;
pub mod startup;
pub mod task_service;
pub mod tddy_user_config;
pub mod telegram_bot;
pub use tddy_telegram::telegram_github_link;
pub mod telegram_multi_select_shortcuts;
pub mod telegram_notifier;
pub mod telegram_session_control;
pub mod telegram_session_subscriber;
pub use tddy_telegram::telegram_tracked_session;
pub mod terminal_session_adapter;
pub mod tool_call_log;
pub mod user_sessions_path;
pub mod workspace_session;
pub mod worktree_tonic_adapter;

// Re-export the shared tool engine so legacy `crate::tool_engine::...` references inside the
// daemon keep resolving after the extraction into the `tddy-tool-engine` crate.
pub use tddy_tool_engine as tool_engine;

pub mod test_util;
