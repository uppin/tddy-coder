# Changesets Applied

Wrapped changeset history for tddy-e2e.

- **2026-03-14** [Feature] E2E Git Repo Setup — temp_dir_with_git_repo helper creates temp dir with git init, commit, origin/master. spawn_presenter_with_grpc, spawn_presenter_with_grpc_and_tui, spawn_presenter_with_livekit_and_tui use it. Fixes clarification_flow_submit_answer_select_workflow_completes (worktree creation requires git repo). (tddy-e2e)
- **2026-03-14** [Feature] LiveKit Token Generation E2E — server_connects_via_token_generator test (livekit feature). (tddy-e2e)
- **2026-03-09** [Feature] TUI E2E Testing & Clarification Question Fix — New package. gRPC-driven tests: spawn_presenter_with_grpc, connect_grpc. tests/grpc_clarification.rs (CLARIFY flow), grpc_full_workflow.rs (SKIP_QUESTIONS flow). PTY test: pty_clarification.rs with termwright (#[ignore] by default). Validates clarification question rendering and workflow completion. (tddy-e2e)
