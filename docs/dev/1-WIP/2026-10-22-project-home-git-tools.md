# Changeset: Project-home git tools

**Date**: 2026-10-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — more than one crate (tddy-worktree-service, tddy-tool-engine, tddy-daemon-rpc); git process seam; ordering semantics (gate vs session activity); integration tests on real git fixtures
**Stack**: #project-home 7/7. Base: `feature/project-home/home-auto-stop`.
**PR**: [#702](https://github.com/uppin/tddy-coder/pull/702)

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-22-project-home-git-tools-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [`packages/tddy-worktree-service/docs/code-issues/oversized-file-service.md`](../../../packages/tddy-worktree-service/docs/code-issues/oversized-file-service.md) | ⚠ DURING | Git operations in a new `session_git_ops.rs`. |
| [`packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md`](../../../packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md) | ⚠ DURING | Tool defs beside node 5's in `project_home_tools.rs`' sibling `project_home_git_tools.rs`. |

## Responsibility

- The seven git tools (eight behaviours, counting force-with-lease push): defs, schemas, execution on a resolved target.
- The **activity gate**: which operations require the target session to be inactive, and the refusal.
- Conflict handling for rebase (abort + report paths).

## Boundaries

- Does not change target resolution — calls node 5's `resolve_session_target`.
- No merge, PR, or branch-creation tools (the `pr_*` tools exist for those).
- Never operates on the home worktree.
- No UI.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/project-home/home-session-tools` (5/7) | `resolve_session_target`, `SessionTarget{is_active,…}`, `ProjectHomePort`, role gate | every tool resolves its target and reads `is_active` from it | change the resolver, its rules, or the four read tools |
| `feature/project-home/home-session` (4/7) | `session_role`, ensure, read-only catalog | tests run as an ensured home session; the git tools are added to its catalog | change ensure or the read-only filter's rule |
| `feature/project-home/home-auto-stop` (6/7) | idle stop | nothing — position only | — |

## Draft PR contract

```rust
// tddy-worktree-service::session_git_ops (new)
pub enum GitOp { Commit { message: String, paths: Vec<String> }, Push { force_with_lease: bool }, Rebase { onto: String }, Stash, Reset { mode: ResetMode, to: String }, Checkout { to: String }, Clean }
pub fn requires_inactive_target(op: &GitOp) -> bool;
pub enum GitOpOutcome { Done { summary: String }, RebaseConflict { paths: Vec<String> } }
pub enum GitOpError { TargetActive { session_id: String }, Git(String) }
pub fn run_git_op(target: &SessionTarget, op: GitOp, push_identity: &PushIdentity) -> Result<GitOpOutcome, GitOpError>;
// tddy-tool-engine::project_home_git_tools (new)
pub fn project_home_git_tool_defs() -> Vec<ToolDef>;
// names: CommitSessionWorktree, PushSessionBranch, RebaseSessionBranch, StashSessionWorktree,
//        ResetSessionWorktree, CheckoutSessionWorktree, CleanSessionWorktree (args in the module doc)
// PushIdentity { env: Vec<(String, String)> }
// Wire contract the tests pin: a gate refusal is an `is_error` result naming the session and
// "is active"; a rebase conflict's result_json carries {"conflict_paths": [...]}; a home-worktree
// target is refused naming "project-home".
```
Plus the failing tests below. Bodies `// TODO(home-git-tools): implement`.

## Green wave

**Wave:** 4 of 4
**Greenable independently:** no — every tool resolves its target through node 5's resolver.
**Concurrent with:** none
**Blocks:** none

Real dependency edges, as opposed to the branch line:

    n1 home-agent-setting → n4      n2 project-groups → n4      n3 activity-sidebar → n4      n4 home-session → n5, n6      n5 home-session-tools → n7

Waves: 1: n1, n2, n3 · 2: n4 · 3: n5, n6 · 4: n7 — line `1 1 1 2 3 3 4`.

## Affected Packages

- **tddy-worktree-service**: [README.md](../../../packages/tddy-worktree-service/README.md) — git ops + gate
- **tddy-tool-engine**: [README.md](../../../packages/tddy-tool-engine/README.md) — tool defs + dispatch
- **tddy-daemon-rpc**: [README.md](../../../packages/tddy-daemon-rpc/README.md) — port wiring, push identity

## Related Feature Documentation

- PRD: [PRD-2026-10-22-project-home-git-tools.md](../../ft/daemon/1-WIP/PRD-2026-10-22-project-home-git-tools.md)

## Summary

Role-gated git tools for the project-home agent on its project's session worktrees; destructive ones refused while the target session is active.

## Background

No tool commits, pushes, rebases or resets an arbitrary worktree today; `subagent_pull`'s
"refused while a turn runs" is the only comparable gate. `branch_owner.rs` holds the active-claimant rule.

## Scope

- [x] Git ops module + gate
- [x] Tool defs + dispatch
- [x] Push identity wiring
- [x] Tests (red tests pinned by the acceptance file and the `session_git_ops` table; CI runs them)

## Technical Changes

### State A
Git operations exist for PR stacks (`tddy-pr-stack/src/git_ops.rs`, `PullBaseIntoBranch` with a
clean-tree gate) but not as agent tools on a target session. Node 5 resolves targets with activity.

### State B
A project-home session runs commit / push always, and rebase / stash / reset / checkout / clean /
force-with-lease push only on inactive targets.

### Delta
- **tddy-worktree-service**: `session_git_ops.rs` (new) with the gate; its git commands in `session_git_ops/run.rs` (new).
- **tddy-tool-engine**: `project_home_git_tools.rs` (new): the seven defs, argument parsing, dispatch; `RebaseSessionBranch` conflicts answered as `{conflict_paths, aborted}`. The project-home role allow-list (`read_only::is_project_home_allowed_tool`) admits the seven names.
- **tddy-daemon-rpc**: `ExecToolRpcHandler` routes the seven names to the project-home handler beside n5's four; `DaemonProjectHome::push_identity` reads the target session's project identity through `SessionAccountAccess::session_identity_in`, the same resolution a session start uses.

## Testing Plan

Daemon acceptance on real git fixtures (a bare remote + a session worktree), driving the tools as an
ensured home session against an active and a stopped target; a unit table for `requires_inactive_target`.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `commit_and_push_work_on_an_active_target` | `packages/tddy-daemon-rpc/tests/project_home_git_tools_acceptance.rs` (new) | ungated ops |
| `rebase_on_an_active_target_is_refused_naming_the_session` | same | gate |
| `rebase_on_a_stopped_target_moves_its_branch_onto_the_base` | same | rebase |
| `a_conflicting_rebase_is_aborted_and_reports_the_paths` | same | abort |
| `reset_stash_checkout_and_clean_are_refused_on_an_active_target` | same | gate |
| `reset_on_a_stopped_target_moves_its_head` | same | reset |
| `force_with_lease_push_is_refused_on_an_active_target` | same | gate |
| `the_home_worktree_is_never_a_git_target` | same | home excluded |
| `only_history_or_file_rewriting_ops_require_an_inactive_target` | `packages/tddy-worktree-service/src/session_git_ops.rs` (`#[cfg(test)]`) | table |

## Decisions & Trade-offs
- **Green (2026-10-23)**: every git command of an operation runs with the target's `GIT_*` identity; when that identity does not resolve, the checkout's inherited identity applies, as at session start. No fallback identity is added.
- **Green (2026-10-23)**: a conflicting rebase is a result, not an error: `is_error` is false, `result_json` carries `conflict_paths` and `aborted: true`, and the worktree is restored by `git rebase --abort`.
- **Green (2026-10-23)**: pushes go to `origin` under the worktree's own branch name. The identity env carries only `GIT_*` pairs; a push to an HTTPS remote that needs the account's token has no env channel for it (the token reaches a session's tools only over its toolcall socket), so such a push uses the checkout's own credentials.
- **Green (2026-10-23)**: the seven defs are built by `project_home_git_tool_defs()`; wiring them into the project-home `ListExecTools` advertisement is left to node 5's per-session advertisement, which is in flight.
- **Green (2026-10-23)**: the acceptance fixture sets `user.name`/`user.email` on the source repository. Worktrees share its config, and a CI runner has no global identity for the commits and rebase replays the tests make.
- **Rebase gated** (developer's decision, 2026-10-22): it rewrites files under a running agent.
- **Force-with-lease push gated** too: it rewrites the remote branch the active agent is building on.
- **Commit allowed while active** (as requested): it changes no files; it can race the agent's next edit, which then lands in a later commit.

## Red-phase notes

- Verified failing (2026-10-23): 8/8 `project_home_git_tools_acceptance` (every failure is `unknown tool: …` — the tools are not in the catalog), 1/1 `session_git_ops` gate table.
- Active / stopped targets in the tests are workspace sessions whose metadata `pid` is the test process (alive → `is_active`) or none; the project-home caller is a workspace session marked with the role, as in node 5.
- Push identity: green derives it from the target's project the way a session push does (vault account askpass env, or local gh).

## Technical Debt & Production Readiness

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-22-project-home-git-tools-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived: the developer asked for wave 2 to run without per-node review, 2026-10-23)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run scoped tests for touched packages (deferred to CI: local build only, 2026-10-23); full-workspace result from CI
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
- [ ] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-22-project-home-git-tools-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
