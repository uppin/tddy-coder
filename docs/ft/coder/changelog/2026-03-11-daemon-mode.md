# 2026-03-11 — Daemon Mode

- **--daemon flag**: tddy-coder runs as a headless gRPC server for systemd deployment. Process serves multiple sessions sequentially; stateless between sessions (reads changeset.yaml from disk).
- **Session lifecycle**: StartSession creates a new session per prompt. GetSession and ListSessions RPCs query session status from disk. Session states: Pending, Active, WaitingForInput, Completed, Failed.
- **Git worktrees**: Each session gets a worktree in `.worktrees/` (repo root). Worktree path and branch persisted in changeset.yaml. Agent working directory switches to worktree for post-plan steps.
- **Branch/worktree elicitation**: Agent suggests branch and worktree names in plan output; client confirms via WorktreeElicitation. Two-phase flow: PlanApproval then ConfirmWorktree.
- **Commit & push**: Final workflow step instructs agent to commit and push to remote branch. Branch name from changeset context.
- **Packages**: tddy-core (worktree.rs, changeset extensions, ElicitationEvent::WorktreeConfirmation, worktree_dir override, commit/push in tdd_hooks), tddy-grpc (DaemonService, StartSession/ConfirmWorktree flow, proto extensions), tddy-coder (run_daemon, --daemon flag).
