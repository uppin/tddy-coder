# 2026-10-25 — A workspace start admits its roster seed first

**Type:** Feature

`LaunchSessions::admitted_workspace_seed` (`workspace_branch_start.rs`) resolves the workspace seed and,
when the start carries no `split_agent`, admits each record with `workspace_roster_admits` — before the
session directory is written, so a refused start leaves nothing. A clone start (`agent_clone`) seeds
nothing. `seed_and_start_workspace_session` takes the admitted seed and reads its own spawn timeout.
`start_session_on_request` grew 384 → 392 lines (recorded in its complexity code issue). `#coagents` 6/8,
[#725](https://github.com/uppin/tddy-coder/pull/725); see [remote-codebase mode](../../../../docs/ft/daemon/remote-codebase-mode.md#coding-subagents-on-the-codebase-host).
