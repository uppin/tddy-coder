# 2026-11-14 — Stack merge, repoint and rebase run by the daemon in each node's worktree

**Type:** Feature

`#orchestrators` 10/11, PR [#887](https://github.com/uppin/tddy-coder/pull/887). Feature:
[PR stacking § PR-management tools](../../ft/coder/pr-stacking.md#pr-management-tools),
[PR stacking § Merge and repoint](../../ft/coder/pr-stacking.md#merge-and-repoint),
[Project concept § Git tools](../../ft/daemon/project-concept.md#git-tools).

## What changed

- **`tddy-tool-engine`** — `stack_git_tools`: the six daemon git tools (`pr_merge`, `pr_close`,
  `pr_repoint`, `pr_set_parents`, `pr_resolve_conflicts`, `pr_spawn_child`), their
  `StackGitOp` / `StackGitToolPort`, and the orchestrator-only offering beside n7's plan family.
- **`tddy-daemon-rpc`** — `DaemonStackGitTools` serves the six on the owning host (forwarded one
  hop like the plan tools): git runs in the node's own worktree, a dirty worktree or a running
  node session is refused untouched, conflicts are probed with `git merge-tree --write-tree`,
  `delegate` messages or resumes the node's session, and `pr_spawn_child` starts the node through
  the daemon start path with the caller as stack parent. A merge keeps its `StackOpJournal`,
  beside the stack for an orchestrator-started operation.
- **`tddy-pr-stack`** — `git_ops::rebase_onto` rebases in the worktree that holds the branch,
  refusing a dirty one; `repo_root`'s own checkout is never switched.
- **`tddy-worktree-service`** — `PushIdentity` carries the project's remote name and the account
  token; a daemon-run push goes to that remote and answers an HTTPS credential request through a
  per-command credential helper with the token in its environment, never in argv or logs.
- **`tddy-tools` / `tddy-toolcall`** — the six `pr_*` methods and the `spawn-child` client relay
  mapping are removed; the audit of the advertised MCP surface pins 34 names (31 on the daemon
  path).
- **`tddy-workflow-recipes`** — the pr-stack prompt describes the daemon behaviour; the repoint
  bridge treats a branch with no worktree as the remote-only case it already skipped.

## Resolved backlog

- `2026-10-23-project-home-git-push-remote-and-token.md` — deleted. Project-home
  `PushSessionBranch` pushes to the project's remote with the account token, covered by
  `push_session_branch_pushes_to_the_projects_remote_with_the_account_token` (a project whose
  only remote is `upstream`, no `origin`).

## Left as recorded debt

- `2026-11-14-retire-stack-child-spawn-handler.md` — `StackChildSpawnHandler` and the
  `ChildSpawnHandler` plumbing stay for a follow-up PR over `tddy-agent-launch`, `tddy-toolcall`,
  `tddy-cli-sessions` and `tddy-session-lifecycle`, whose wiring tests also cover the operator
  Start-session dialog path. No agent can reach the relay since this PR.

## Code issues

- `packages/tddy-daemon-rpc/src/exec_tool/stack_git_tools.rs` is 999 production lines, created
  by this PR. Its split is **deferred to the stack's sweep node** (developer instruction
  2026-11-14: no local builds or tests in the wrap run, and the restructure engine needs a
  build/index) and recorded in `oversized-file-stack-git-tools.md`, with the seam the stopped
  `extract_module` plan proved (`check --deep` passed; no apply ran).
- `oversized-file-server.md` re-measured: 2,553 → 2,289 production lines — stack-shared with
  successor `#orchestrators` nodes, split deferred to the post-stack sweep.
- `oversized-file-lib.md` (`tddy-tool-engine`) re-measured: 892 → 893 production lines (+1 module
  declaration) — stack-shared, split deferred to the post-stack sweep.
- `oversized-file-pr-stack-ports.md` re-measured: 868 production lines, unchanged by this PR —
  stack-shared, split deferred to the post-stack sweep.
- The two `tddy-pr-stack` complexity records the changeset carried as ⚠ DURING
  (`repoint_planned_pr_node` 78 lines, `pull_base_into_node_branch` 157 lines) are unchanged:
  this PR touched neither function.
