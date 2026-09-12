//! `tddy-daemon` endpoint — wiring, configuration, and transport only.

pub mod config;
pub mod daemon_config_service;
pub mod daemon_settings;
pub mod local_socket_server;
pub mod relay_idle;
pub mod runtime;
pub mod server;
pub mod startup;
pub mod tddy_user_config;
pub mod user_sessions_path;

/// Legacy paths for integration suites (`tddy_daemon::connection_service`, …).
pub use tddy_session_lifecycle::{
    action_service, agent_list_mapping, auth, build_pr_stack_entry, build_session_entry,
    claude_cli_session, cli_session_manager, connection_service, local_token_tonic_adapter,
    cursor_cli_spawn, github_pr_credentials, oauth_loopback_tunnel, pr_stack_rpc, PrStackHandler,
    PrStackServiceImpl, presenter_intent_client, pty_runtime, session_admission_service,
    session_agent_clone, session_deletion, session_list_enrichment, session_notification_subscribers,
    session_notifications, session_reader, session_toolcall, split_session, task_service,
    terminal_session_adapter, test_util, workspace_session, worktree_tonic_adapter, CliSessionManager,
    SessionError, SessionHandler, SessionServiceImpl, task_registry,
};
pub use tddy_session_lifecycle::{
    active_elicitation, base_sync_cache, branch_intent, branch_owner, common_room_supervisor,
    context_files, context_sync, elicitation, host_desktop_targets, host_documents, host_keypair,
    host_messages, host_private_key, host_prompt_stream, host_prompts, host_registry,
    host_session_service, host_stats, host_tooling, host_tonic_adapter, livekit_peer_discovery,
    livekit_rooms_stream, livekit_service, multi_host, project_provision, project_storage,
    remote_desktop_probe, remote_git_service, session_agent_inference, session_agent_roster,
    session_agent_status, session_attachment_staging, session_attachments, session_context_docs,
    session_file_upload, session_room, session_uploads, session_workflow_files, ssh_agent,
    ssh_agent_add, stack_doc_attachments, telegram_github_link, telegram_tracked_session,
    telegram_bot, telegram_multi_select_shortcuts, telegram_notifier, telegram_session_control,
    telegram_session_subscriber, tool_call_log,
    tool_engine, worktree_files, worktrees,
};
