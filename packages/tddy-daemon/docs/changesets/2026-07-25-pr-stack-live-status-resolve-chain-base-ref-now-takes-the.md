# 2026-07-25 — pr-stack-live-status: `resolve_chain_base_ref` now takes the new branch name and, for a pr-stack orchestrator parent, bases the child off its node's effective base via `Stack::base_ref_for_spawn` (propagating the ordering guard as `failed_precondition`) instead of the default branch

**Type:** Feature

fixing both spawn paths (web Start-session + agent spawn-child) that previously ignored the DAG. New `get_pr_status` (owner/repo from the session's git remote → `get_pr_by_head`) and `repoint_planned_pr` (→ `repoint_planned_pr_node`) RPC handlers, network/git in `spawn_blocking`, token-less/no-PR → `exists=false`. `SessionEntry.branch` populated from `Changeset.branch` in `ListSessions` enrichment. Feature [pr-stack-live-status.md](../../../../docs/ft/coder/pr-stack-live-status.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-daemon)
