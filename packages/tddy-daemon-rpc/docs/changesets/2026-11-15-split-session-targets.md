# 2026-11-15 — Records carry the codebase half; worktree tools on a split agent are redirected

**Type:** Fix

`target_records()` fills `TargetRecord.codebase` from each row's
`codebase_daemon_instance_id` / `codebase_session_id`. In the project-home forward
(`served_on_target_host`), a worktree tool named with a split agent half's id and
no host is forwarded to its codebase half on the codebase host with
`retarget_args`, exactly as an explicit host is forwarded; the stack git tools
build their targets with the optional worktree and the codebase half.

Cross-package record: [docs/dev/changesets/2026-11-15-split-session-targets.md](../../../../docs/dev/changesets/2026-11-15-split-session-targets.md).
