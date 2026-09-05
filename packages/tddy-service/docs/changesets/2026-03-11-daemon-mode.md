# 2026-03-11 — Daemon Mode

**Type:** Feature

Proto: GetSession, ListSessions, StartSession, ConfirmWorktree, SessionCreated, WorktreeElicitation. DaemonService with sessions_base and SharedBackend. Stream handler: StartSession flow (plan → PlanApproval → ApprovePlan → WorktreeElicitation → ConfirmWorktree → worktree creation → run_session). workflow_event_to_server_message, plan_approval_to_server_message. (tddy-grpc)
