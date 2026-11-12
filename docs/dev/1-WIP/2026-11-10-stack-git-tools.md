# Changeset: stack merge, repoint and rebase run by the daemon in the node's worktree

**Date**: 2026-11-10
**Status**: 🚧 In Progress — planning (commit 1)
**Type**: Feature
**Complexity**: 5 — git writes moved across a process boundary into another session's worktree, a credential channel for daemon-run pushes (askpass), crash-safety journal preserved, cross-host forwarding, more than one crate
**Stack**: `#orchestrators` 10/11 · branch `feature/orchestrators/stack-git-tools` · base `feature/orchestrators/stack-web-by-id` (#886)

## Initial Discovery

[2026-11-10-stack-git-tools-initial-discovery.md](./2026-11-10-stack-git-tools-initial-discovery.md)

## Prerequisites

### ✅ RESOLVED HERE — Project-home `PushSessionBranch` pushes to `origin` with the checkout's credentials — [`2026-10-23-project-home-git-push-remote-and-token.md`](../todo/2026-10-23-project-home-git-push-remote-and-token.md)

⛔ Blocking: every daemon-run push this node adds (`pr_set_parents`, `pr_repoint`) would go to the wrong
remote or fail without a credential. Closed as the entry says: the project's `remote_name` (default
`origin`) is carried on the push identity, and daemon-run pushes get the account token through
`tddy-askpass` (`GIT_ASKPASS` + the vault lookup a daemon-run clone uses). The entry's acceptance case
(`remote_name: upstream`, no `origin`) is added to `project_home_git_tools_acceptance.rs`, so the
existing `PushSessionBranch` is fixed too.

### ⚠ DURING — [`complexity-repoint-repoint-planned-pr-node.md`](../../../packages/tddy-pr-stack/docs/code-issues/complexity-repoint-repoint-planned-pr-node.md), [`complexity-pull-base-pull-base-into-node-branch.md`](../../../packages/tddy-pr-stack/docs/code-issues/complexity-pull-base-pull-base-into-node-branch.md)

`realign_node_to_effective_base` switches from `repo_root` to the node's worktree by reusing
`pull_base`'s `worktree_path_for_branch` lookup; neither function grows.

### ⚠ DURING — `server.rs` — [`oversized-file-server.md`](../../../packages/tddy-tools/docs/code-issues/oversized-file-server.md)

Six more `#[tool]` methods and the `spawn-child` relay leave the file; the record is re-measured at wrap.

### ⚠ DURING — open PR #876

Pass 2 waits for #876 (developer decision 2026-11-10).

## Affected Packages

- [tddy-pr-stack](../../../packages/tddy-pr-stack/README.md) — `git_ops::rebase_onto` in the node's worktree
- [tddy-tool-engine](../../../packages/tddy-tool-engine/README.md) — six tool defs in the stack-tools family
- [tddy-daemon-rpc](../../../packages/tddy-daemon-rpc/README.md) — port methods, journal, forwarding
- [tddy-worktree-service](../../../packages/tddy-worktree-service/) — push remote + askpass
- [tddy-workflow-recipes](../../../packages/tddy-workflow-recipes/README.md) — `execute_stack_merge` / `execute_stack_repoint` bridge reached from the daemon
- [tddy-tools](../../../packages/tddy-tools/README.md) — six tools + relay removed
- [tddy-agent-launch](../../../packages/tddy-agent-launch/) — `StackChildSpawnHandler` retired in favour of the daemon start path

## Related Feature Documentation

- PRD: [PRD-2026-11-10-stack-git-tools.md](../../ft/coder/1-WIP/PRD-2026-11-10-stack-git-tools.md)
- [PR stacking § Merge and repoint](../../ft/coder/pr-stacking.md#merge-and-repoint), [Project concept § Git tools](../../ft/daemon/project-concept.md#git-tools)

## Summary

`pr_merge`, `pr_close`, `pr_repoint`, `pr_set_parents`, `pr_resolve_conflicts`, `pr_spawn_child`
become daemon exec tools in n7's stack-tools family. Every git step runs on the daemon in the node's own
worktree; pushes go to the project's remote with the account's credential; `pr_resolve_conflicts
{ delegate: true }` messages (or resumes) the node's session to resolve the conflict.

## Background

`pr_set_parents` rebases in `TDDY_REPO_DIR` with `git checkout <branch>`, which fails when the branch
is checked out in the child's worktree; `pr_resolve_conflicts` trusts an agent-supplied worktree path. A
read-only orchestrator has neither a checkout nor write access. The developer decided (2026-11-10):
"daemon runs git; split by need".

## Responsibility

- Six tools in `stack_tools.rs` (names/schemas) and their `StackToolPort` methods.
- `git_ops::rebase_onto(worktree, …)` + node-worktree lookup; refusal for a dirty worktree or a running
  node session.
- `pr_resolve_conflicts { node_id, delegate? }` — merge try / abort in the node's worktree, mark
  `has-conflicts`, answer paths; `delegate` → `SendSessionMessage` (running) or `ResumeSession` with the
  message (stopped).
- `pr_spawn_child` through the daemon start path with `stack_parent`, caller as orchestrator.
- Push remote + askpass credential for daemon-run pushes (claimed todo).
- Removal of the six `tddy-tools` methods and the `spawn-child` toolcall relay.

## Boundaries

- Does **not** change the plan/read tools (n7), the store (n2) or the web (n9).
- Does **not** change `SendSessionMessage` / `ResumeSession` (n4); it calls them.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n7 `stack-plan-tools` (#884) | `stack_tools` family, `StackToolPort`, `StackCaller`, offered sets, forwarding, project-account GitHub credential | adds six tools to the same family and port | change the family's shape, caller kinds or the eleven tools |
| n4 `session-turn-control` (#881) | `SendSessionMessage`, `ResumeSession` (daemon port methods) | `delegate` calls them for the node's session | add another turn path |
| n2 `project-stack-store` (#879) | the store | via n7's port | touch the store |

## Draft PR contract

- Six tool defs added to `stack_tools.rs`; port method signatures (`TODO(stack-git-tools)`);
  `rebase_onto` worktree parameter; push identity's `remote_name` and askpass fields.
- Failing tests below.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no — needs n7's family and port (behaviour) and n4's message delivery
**Concurrent with:** n11
**Blocks:** none

    n1 → n7, n8    n2 → n7, n9    n3 → n8    n4 → n10    n7 → n10    n8 → n11

## Scope

- [ ] ✅ Push remote + askpass ([todo](../todo/2026-10-23-project-home-git-push-remote-and-token.md))
- [ ] `rebase_onto` in the node's worktree; dirty/running refusals
- [ ] `pr_merge`, `pr_close`, `pr_repoint`, `pr_set_parents` via the daemon (journal kept)
- [ ] `pr_resolve_conflicts` + `delegate`
- [ ] `pr_spawn_child` via the start path
- [ ] Forwarding
- [ ] Remove the `tddy-tools` copies and the relay

## Technical Changes

### State A (Current)

- `pr_set_parents` → `realign_node_to_effective_base` (`stack_ops/repoint.rs:141`) →
  `git_ops::rebase_onto(repo_root, …)` (`git_ops.rs:39`, `git checkout` + rebase + force-push in
  `TDDY_REPO_DIR`).
- `pr_resolve_conflicts` merges in the agent's `worktree_dir` (`server.rs:127`, `pr_actions.rs:76`).
- `pr_merge` / `pr_close` / `pr_repoint` are GitHub API only (`pr_actions.rs:36`, `patch_pr_base`).
- `pr_spawn_child` relays `spawn-child` over `TDDY_SOCKET` → `StackChildSpawnHandler`
  (`child_spawn_handler.rs:38`), bound only for recipe `pr-stack`.
- Worktree-correct pattern: `pull_base_into_node_branch` (`pull_base.rs:84`, `worktree_path_for_branch`).
- Daemon push: `session_git_ops/run.rs::push` → `origin`, no token.
- Journal `orchestrate_pr_stack/transient.rs:17` in the orchestrator session dir.

### State B (Target)

- As Responsibility; the journal for a stack op started by a non-pr-stack caller lives beside the stack
  (`stacks/<stack_id>.op.json`), and a pr-stack session's in-flight journal is still recovered.

### Delta

- pr-stack git_ops/repoint; tool-engine defs; daemon-rpc port; worktree-service push; workflow-recipes
  bridge entry points; tddy-tools and agent-launch removals.

## Implementation Milestones

- [ ] Push remote + askpass (existing git-tool suite + new case)
- [ ] Rebase in node worktree
- [ ] Merge / close / repoint / set parents
- [ ] Resolve conflicts + delegate
- [ ] Spawn child
- [ ] Removals

## Testing Plan

**Level**: exec-tool acceptance in `tddy-daemon-rpc` with real git repos and worktrees in temp dirs, a
local bare remote named `upstream`, fake GitHub; `delegate` asserted through n4's delivery seam
(terminal bytes received); crash-safety through the existing journal recovery tests.

## Acceptance Tests

### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/stack_git_tools_acceptance.rs`
- `main_merges_closes_and_repoints_a_nodes_pr_from_a_read_only_session`
- `set_parents_rebases_the_node_in_its_own_worktree_and_pushes_to_the_projects_remote`
- `a_node_with_a_dirty_worktree_or_a_running_session_is_refused_untouched`
- `resolve_conflicts_with_delegate_asks_the_nodes_session_to_resolve_the_named_paths`
- `resolve_conflicts_with_delegate_resumes_a_stopped_node_session_with_the_message`
- `spawn_child_from_main_starts_the_nodes_session_linked_to_main`
- `the_six_tools_act_on_a_stack_owned_by_another_host`

### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/project_home_git_tools_acceptance.rs` (new case)
- `push_session_branch_pushes_to_the_projects_remote_with_the_account_token`

### tddy-pr-stack — `packages/tddy-pr-stack/src/git_ops.rs` (unit)
- `rebase_onto_runs_in_the_worktree_that_holds_the_branch`

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Refuse, do not stash**, a dirty node worktree or a running node session — a daemon rebase under a
  working agent loses its work.
- **Journal beside the stack** for ops a non-pr-stack session starts; pr-stack sessions' journals keep
  their location so in-flight recovery is unchanged.
- Score 5 is a warning: if pass 2 shows the push-credential fix is large, it moves to a prerequisite PR
  off `master` before this node — raise it then.

## Refactoring Needed

### From @red (TDD Red Phase)
### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

### Change Validation (@validate-changes)
### Test Validation (@validate-tests)
### Production Readiness (@prod-ready)
### Code Quality (@analyze-clean-code)

## Successor PRs

n11 `feature/orchestrators/project-settings-pane`.

## TODO

- [x] Record initial discovery (`2026-11-10-stack-git-tools-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
