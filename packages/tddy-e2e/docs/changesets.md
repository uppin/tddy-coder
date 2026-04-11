# Changesets Applied

Wrapped changeset history for tddy-e2e.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-03-28** [Feature] gRPC Virtual TUI idle semantics — `grpc_terminal_rpc` asserts frozen elapsed and ~1 Hz idle dot cadence in clarification wait; `grpc_reconnect_acceptance` threshold aligned with smaller idle frames; `pty_full_workflow` stream-order assertions. (tddy-e2e)
- **2026-03-23** [Feature] Install script E2E — `install_contract` static checks and `tests/install_script.rs` functional tests (temp tree, `INSTALL_NO_SYSTEMCTL=1`, idempotent config, unit generation). See [docs/ft/daemon/systemd-install.md](../../../docs/ft/daemon/systemd-install.md). (tddy-e2e)
- **2026-03-22** [Feature] Web-dev script contract — `web_dev_contract` module (`verify_*`, substring detectors) and `tests/web_dev_script.rs` for `bash -n` and daemon-only content checks; granular tests delegate to `verify_*`. (tddy-e2e)
- **2026-03-22** [Fix] gRPC terminal reconnect — `UserIntent::SelectHighlightChanged` syncs Select highlight to presenter for `connect_view` snapshots; `grpc_reconnect_second_stream_receives_full_tui_render` re-enabled. (tddy-core, tddy-tui, tddy-service, tddy-e2e)
- **2026-03-21** [Docs] LiveKit / gRPC terminal RPC E2E knowledge consolidated into [livekit-terminal-rpc-e2e.md](../../../docs/dev/guides/livekit-terminal-rpc-e2e.md); removed WIP `docs/dev/1-WIP/livekit-terminal-rpc-e2e-knowledge.md`. (tddy-e2e)
- **2026-03-14** [Feature] Per-Connection Virtual TUI — spawn_presenter_with_view_connection_factory (LiveKit), spawn_presenter_with_terminal_service (gRPC). virtual_tui_sessions.rs: two_grpc_clients_get_independent_terminal_streams. terminal_service_livekit.rs: two_livekit_clients_get_independent_terminal_streams. (tddy-e2e)
- **2026-03-14** [Feature] E2E Git Repo Setup — temp_dir_with_git_repo helper creates temp dir with git init, commit, origin/master. spawn_presenter_with_grpc, spawn_presenter_with_grpc_and_tui, spawn_presenter_with_livekit_and_tui use it. Fixes clarification_flow_submit_answer_select_workflow_completes (worktree creation requires git repo). (tddy-e2e)
- **2026-03-14** [Feature] Workflow Restart on Completion E2E — pty_full_workflow asserts "Type your feature description" on completion; grpc_full_workflow asserts ModeChanged(FeatureInput) after WorkflowComplete. (tddy-e2e)
- **2026-03-14** [Feature] LiveKit Token Generation E2E — server_connects_via_token_generator test (livekit feature). (tddy-e2e)
- **2026-03-09** [Feature] TUI E2E Testing & Clarification Question Fix — New package. gRPC-driven tests: spawn_presenter_with_grpc, connect_grpc. tests/grpc_clarification.rs (CLARIFY flow), grpc_full_workflow.rs (SKIP_QUESTIONS flow). PTY test: pty_clarification.rs with termwright (#[ignore] by default). Validates clarification question rendering and workflow completion. (tddy-e2e)
