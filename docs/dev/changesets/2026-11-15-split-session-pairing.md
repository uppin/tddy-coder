# 2026-11-15 — A split session shows its branch, its worktree and its orchestrator

**Type:** Fix

A split session is listed on the operator's host as its agent half, which holds
no checkout. Its start reported empty `branch` and `worktree`, its drawer row
showed no branch, and every CLI agent session lost the link to the orchestrator
that started it. The start ledger now follows a split agent to its codebase
half (`reported_branch_and_worktree` in `tddy-agent-launch`, behind a private
`ListedSessions` seam; the `StartedSessionDetails` trait is unchanged) and
reports that half's branch and worktree. The split start reads the codebase
branch back from the codebase host once its half exists and records it in the
agent half's first changeset before the agent is spawned
(`split_codebase_branch.rs` in `tddy-session-split`); a branch that cannot be
read fails the start and the codebase half is torn down. CLI enrichment in
`tddy-session-activity` reads `orchestrator_session_id` and `recipe` from the
changeset for every agent CLI type, and the drawer row of a split session in
`tddy-session-web` shows its recorded branch beside the codebase-host badge.
Stack `#split-alive` 4/4
([#908](https://github.com/uppin/tddy-coder/pull/908)); the product statements
are in [background-tasks.md § Session starts run as tasks](../../ft/daemon/background-tasks.md#session-starts-run-as-tasks),
[remote-managed-worktree.md § Session start](../../ft/daemon/remote-managed-worktree.md#session-start),
[project-concept.md § Project-home tools](../../ft/daemon/project-concept.md#project-home-tools)
and [session-drawer.md](../../ft/web/session-drawer.md).

This closes the backlog entry
`2026-08-29-a-cursor-cli-session-s-orchestrator-link-never-reaches-sessionentry`
(a cursor-cli session's orchestrator link never reaches `SessionEntry`), deleted
at this wrap, and fixes 6–7 of `2026-11-15-split-session-agent-not-running`
(deleted at the #905 wrap). What remains of that entry is recorded in
`docs/dev/todo/2026-11-15-split-agent-exit-cause-unknown.md`, updated at this
wrap to mark fixes 6–7 done in #908: why the agent exits (fix 4) is still
unknown, and `spawn_split_agent` still awaits decomposition in a follow-up
branch after the stack lands.

Validation at this wrap: scoped `./test -p tddy-agent-launch -p
tddy-session-split -p tddy-session-activity` green, and the cross-host daemon
acceptance `a_split_agent_half_lists_its_codebase_branch` green against the
LiveKit testkit. File-length gate over the PR range: no touched non-test source
file is at or over 500 production lines. Code issues re-measured:
`complexity-svc-spawn-split-agent-spawn-split-agent` (`spawn_split_agent`)
grew 152 → 157 lines by brace count in this PR — the changeset write moved
later in the same function, after the codebase half exists — so its record
stays open, regressed, with the new row; its decomposition stays the follow-up
recorded in the backlog entry above. The other open records in the touched
packages name files this PR did not touch and stay open as written.
