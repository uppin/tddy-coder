# Changeset: stack merge, repoint and rebase run by the daemon in the node's worktree

**Date**: 2026-11-10
**Status**: 🚧 In Progress — draft-PR contract (commit 2): surface stubbed, failing tests written
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
- [tddy-tool-engine](../../../packages/tddy-tool-engine/README.md) — six tool defs in `stack_git_tools`, the stack-tools family's git sibling
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
| n7 `stack-plan-tools` (#884) | `stack_tools` family, `StackToolPort`, `StackCaller` / `StackCallerRole`, `addressed_stack_id`, `stack_caller_of`, `run_stack_tool` routing and forwarding, `DaemonStackTools` (store + project-account GitHub), `attach_stack_github` | a **sibling module** `stack_git_tools` reusing `StackCaller`, `StackCallerRole`, `STACK_ID` and `addressed_stack_id`; the daemon port holds a `DaemonStackTools` whole; its serving is classified by `stack_caller_of` and forwarded the way `run_stack_tool` forwards | change the family's shape, caller kinds, the eleven tools, `STACK_TOOL_NAMES`, `StackOp` or `StackToolPort` |
| n4 `session-turn-control` (#881) — **done** | `ProjectHomeTurnPort::send_session_message` / `resume_session` on `DaemonProjectHome` (`project_home/turn.rs:28,59`) | `delegate` calls them for the node's session through the `DaemonProjectHome` the port holds | add another turn path |
| n2 `project-stack-store` (#879) | the store | via n7's `DaemonStackTools` | touch the store |

**Sequencing on n7 (pass 2, 2026-11-14).** This tree carries n7's **contract** only: `stack_caller_role`,
`is_stack_tool_offered_to`, `addressed_stack_id`, `execute_stack_tool`, `stack_caller_of`,
`run_stack_tool` and every `DaemonStackTools` port method are `TODO(stack-plan-tools)` stubs, and no stack
tool is routed from `run_locally` yet.

- **Every acceptance test in `stack_git_tools_acceptance.rs` keeps the real path and can pass only once
  n7 is green as well as this node**: the caller is classified by `stack_caller_of`, the stack resolved by
  `addressed_stack_id`, the store and GitHub reached through `DaemonStackTools`, and the cross-host test
  forwarded the way n7 forwards. The integration is the point of each, so no double is injected.
- **Tool-engine unit tests** (`stack_git_tools::tests`) inject a `StackCaller` directly (a double for
  n7's classification); those that run through `execute_stack_git_tool` with a stack still resolve it
  with n7's `addressed_stack_id`, so they too pass only once n7 is green. The offer tests
  (`is_stack_git_tool_offered_to`) are n10's alone.
- **`git_ops` and `session_git_ops` unit tests** and the `PushSessionBranch` acceptance case touch no
  n7 symbol: they are this node's alone.

## Draft PR contract

Added in pass 2 (2026-11-14), every body `TODO(stack-git-tools)`:

- **`tddy-tool-engine/src/stack_git_tools.rs`** (new module, the stack-tools family's git sibling):
  `PR_MERGE`, `PR_CLOSE`, `PR_REPOINT`, `PR_SET_PARENTS`, `PR_RESOLVE_CONFLICTS`, `PR_SPAWN_CHILD`,
  `STACK_GIT_TOOL_NAMES: [&str; 6]`, `is_stack_git_tool`, `is_stack_git_tool_offered_to(&StackCallerRole,
  &str)` (stub), `stack_git_tools_offered_to(Option<&StackCallerRole>)` (stub), `StackGitOp { Merge,
  Close, Repoint { new_base: Option }, SetParents { parents }, ResolveConflicts { delegate },
  SpawnChild { session_type, model } }`, `trait StackGitToolPort { run_stack_git_op(&StackCaller,
  stack_id, StackGitOp) }`, `stack_git_tool_defs()` (real schemas: `node_id` required, `stack_id` and
  `daemon_instance_id` on each; `pr_set_parents.parents` required), `execute_stack_git_tool` (stub).
- **`tddy-daemon-rpc/src/exec_tool/stack_git_tools.rs`** (new, private): `ExecToolRpcHandler::run_stack_git_tool`
  (stub, not yet routed), `DaemonStackGitTools { stacks: DaemonStackTools, home: DaemonProjectHome }` with
  its `StackGitToolPort` impl (stub).
- **`tddy-pr-stack::git_ops::rebase_onto`** — signature unchanged; its contract is now "in the worktree of
  `repo_root` that holds `branch`, refusing a dirty one" (body still the old checkout-in-`repo_root`).
- **`tddy-worktree-service::session_git_ops::PushIdentity`** — `remote_name: Option<String>`,
  `account_token: Option<String>`, a `Debug` that redacts the token; `run_git_op` still pushes to
  `origin` (TODO). `project_home/port.rs::push_identity` fills neither yet (TODO).
- `tests/support/mod.rs` — `a_host_running_claude_as_with_gh_at` (test support).

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no — needs n7's classification, addressing, port and forwarding green (behaviour); n4's message delivery is already green in this tree
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

Re-verified 2026-11-14 against `5ae9bc7af` (n2, n4 green and n7's contract in the tree); moved facts marked ↻.

- `pr_set_parents` (`tddy-tools/src/server.rs:1077` → `pr_set_parents_impl` :1404, ↻) →
  `set_stack_node_parents` → `realign_node_to_effective_base` (`stack_ops/repoint.rs:140`, ↻ was 141) →
  `git_ops::rebase_onto(repo_root, …)` (`git_ops.rs:39`, `git checkout` + rebase in `TDDY_REPO_DIR`),
  then `force_push_with_lease` to `detect_default_remote_name` (failure only logged). ↻ `realign` still
  reads the stack from the orchestrator's **changeset** (`read_session_stack(session_dir)`), not n2's
  store, and strips only an `origin/` prefix; `Stack::effective_base_refs` (`tddy-changeset`
  `changeset/stack.rs:194`) hard-codes `origin/<branch>` — n10's daemon path must name the project's
  remote itself.
- `pr_resolve_conflicts` (`server.rs:991`, ↻; `worktree_dir` param in `PrResolveConflictsInput` :121) runs
  `git merge --no-commit` + `--abort` in the agent-supplied worktree (`pr_actions.rs:76`).
- `pr_merge` (`server.rs:952`) / `pr_close` (:963) are GitHub-API-only (`pr_actions.rs:36,48`), addressed
  by `node_id` + `pull_number`; `pr_repoint` (:976) is `patch_pr_base(pull_number, new_base)` only. ↻ tool
  dispatch `call_tool_by_name` :516-529; `server.rs` is now 4,156 lines.
- `pr_spawn_child` (`server.rs:1043`) relays `{"type":"spawn-child"}` over `TDDY_SOCKET`
  (`tddy-toolcall/src/toolcall/client.rs:50`) → `StackChildSpawnHandler::spawn_child`
  (↻ `tddy-agent-launch/src/child_spawn_handler.rs:15-16`; struct in `stack_child_spawn.rs`), bound only
  for recipe `pr-stack` (↻ `svc_start_claude_cli_session.rs:28` `managed_spawn_handlers`, gate :37-45).
  It reads the orchestrator's changeset stack and inherits the orchestrator's model.
- Worktree-correct pattern: `pull_base_into_node_branch` (`pull_base.rs:84`, `worktree_path_for_branch`
  :122; `tddy-git/src/worktree.rs:218`).
- Daemon push: `session_git_ops/run.rs:112-120` → `origin`, no token. `PushIdentity`
  (`session_git_ops.rs:58`) was `{ env }` only, built at `exec_tool/project_home/port.rs:143-166`.
  ↻ `tddy-askpass` answers **SSH key-passphrase prompts only** (`main.rs:20-22` refuses
  `AskpassPrompt::Other`), so an HTTPS token channel is new work there (or a credential helper).
- Journal `orchestrate_pr_stack/transient.rs:17` (`StackOpJournal`), `.workflow/stack-op.json` in the
  orchestrator session dir (:24-29).
- ↻ n7's tool-engine family is closed: `STACK_TOOL_NAMES: [&str; 11]`, `StackOp`, `StackToolPort` and the
  private schema helpers live in `stack_tools.rs`; `DaemonStackTools`' fields are private to
  `exec_tool/stack_tools.rs`. Six more tools cannot be added *into* them without editing n7's symbols.
- n4 (green): `ProjectHomeTurnPort::send_session_message` / `resume_session`
  (`project_home_turn_tools.rs:136,150`), implemented on `DaemonProjectHome`
  (`exec_tool/project_home/turn.rs:28,59`).

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

## Closed-world lists green must extend

- `tddy-tool-engine/src/read_only.rs` — `project_home_allowed_tools()` and its exact-list test (the six
  `mcp__tddy-tools__pr_*` names main is allowed, beside n7's eleven).
- `tddy-tool-engine/src/project_home_catalogue.rs` — what `session_offered_catalog_for` offers an
  orchestrator (`stack_git_tools_offered_to`).
- `tddy-daemon-rpc/src/exec_tool/ports.rs` / `project_home.rs` — `run_locally` routing and `ListExecTools`
  (the six beside n7's family).
- `tddy-tools-cli/tests/mcp_tool_advertisement_audit.rs:66-73` — the six leave `tddy-tools`' static
  advertisement.
- `tddy-workflow-recipes/src/pr_stack/mod.rs:73-82` (allowed `mcp__tddy-tools__pr_*`) and the prompt's tool
  list :101-115.
- `tddy-toolcall/src/toolcall/client.rs:50` — the `spawn-child` relay mapping goes with the relay.
- `tddy-workflow-recipes/schema-manifest.json` if the `tddy-tools` schema changes — commit it.

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
- `rebase_onto_refuses_a_dirty_node_worktree_and_leaves_it_untouched`

### tddy-worktree-service — `packages/tddy-worktree-service/src/session_git_ops.rs` (unit)
- `a_push_goes_to_the_remote_its_identity_names`
- `a_push_identity_never_shows_its_token` (passes at contract: the redacting `Debug` is surface)

### tddy-tool-engine — `packages/tddy-tool-engine/src/stack_git_tools.rs` (unit)
- offers: `an_orchestrator_is_offered_every_git_tool`, `a_node_of_a_stack_is_offered_no_git_tool`,
  `a_plan_tool_is_not_a_git_tool_an_orchestrator_is_offered_here`; schemas:
  `every_git_tool_names_its_node_its_stack_and_its_host`; execution over a recording port (n7's
  `addressed_stack_id` sequenced): `pr_merge_is_run_on_the_named_stack_for_its_node`,
  `pr_repoint_naming_no_stack_acts_on_the_callers_own`,
  `pr_set_parents_with_an_empty_list_makes_the_node_a_root`,
  `pr_set_parents_without_parents_is_refused_before_the_port_is_asked`,
  `pr_resolve_conflicts_with_delegate_asks_the_port_to_delegate`,
  `pr_resolve_conflicts_does_not_delegate_unless_asked`,
  `pr_spawn_child_carries_the_type_and_model_it_names`,
  `a_git_tool_missing_its_node_is_refused_as_invalid_arguments`, `the_ports_refusal_is_the_tools_error`,
  `a_tool_outside_the_family_is_refused`

## Technical Debt & Production Readiness

### Pass-2 baseline (2026-11-14, scoped: `tddy-pr-stack`, `tddy-tool-engine`, `tddy-worktree-service`, `tddy-daemon-rpc`)

`./dev cargo clippy -p … --all-targets -- -D warnings` clean (before and after the contract).
`./test -p tddy-pr-stack -p tddy-worktree-service -p tddy-tool-engine -p tddy-daemon-rpc` — failures that
are **not** this node's (do not mistake them for its red):

- n7 contract red: `tddy-daemon-rpc` `stack_plan_tools_acceptance` (6), lib `exec_tool::stack_tools::tests`
  (4); `tddy-tool-engine` lib `stack_tools::tests`, `tests/stack_tools_catalogue` (1).
- n9 contract red: `tddy-daemon-rpc` `project_stacks_rpc_acceptance` (5), lib
  `pr_stack::project_stacks::project_stacks_unit_tests` (9).
- n5 contract red (inherited): `tddy-daemon-rpc` `project_home_lifecycle_tools_acceptance` (5),
  `worktree_tools_acceptance` (5); `tddy-tool-engine` `tests/project_home_lifecycle_tools` (18).
- LiveKit: `tddy-worktree-service` `remote_git_livekit_acceptance` (12).
- Timeouts under load: `project_home_turn_control_acceptance`
  `main_sends_a_running_claude_cli_session_a_turn_without_taking_its_terminal` and
  `resume_session_with_a_message_resumes_it_and_delivers_the_message` (PTY never showed `READY` in 10s,
  `--test-threads=1` run; n4 is green — re-run alone).

This node's red (all failing for its own missing implementation): the 7 `stack_git_tools_acceptance`
tests (refused `PermissionDenied: … a project-home session is read-only` — no git tool is routed yet),
`push_session_branch_pushes_to_the_projects_remote_with_the_account_token` (`'origin' does not appear
to be a git repository`), the 2 `git_ops` tests (`'feature' is already used by worktree`),
`a_push_goes_to_the_remote_its_identity_names` (pushes to `origin`), and 11 `stack_git_tools::tests`
(stubs).

## Decisions & Trade-offs

- **Refuse, do not stash**, a dirty node worktree or a running node session — a daemon rebase under a
  working agent loses its work.
- **Journal beside the stack** for ops a non-pr-stack session starts; pr-stack sessions' journals keep
  their location so in-flight recovery is unchanged.
- Score 5 is a warning: if pass 2 shows the push-credential fix is large, it moves to a prerequisite PR
  off `master` before this node — raise it then. Pass 2 kept it here: the acceptance case asserts only
  that the push's git can obtain the token (`git credential fill` from a `pre-push` hook), so green may
  extend `tddy-askpass` or hand git a credential helper — whichever is smaller.
- **Sibling module, not an edit of n7's family** — orchestrator decision (developer: finish without
  stopping), 2026-11-14. The changeset said "six tool defs added to `stack_tools.rs`", but that file's
  `STACK_TOOL_NAMES: [&str; 11]`, `StackOp` and `StackToolPort` are n7-owned and n7 is mid-green. The six
  live in `tddy_tool_engine::stack_git_tools` with their own `StackGitOp` / `StackGitToolPort`, reusing
  n7's `StackCaller`, `StackCallerRole`, `STACK_ID` and `addressed_stack_id`; the daemon side is a
  `DaemonStackGitTools` holding n7's `DaemonStackTools` and n4's `DaemonProjectHome` whole.
- **Orchestrators only** — orchestrator decision (developer: finish without stopping), 2026-11-14. A
  node of a stack is offered none of the six (it is offered n7's two reads only); an ordinary session none.
- **Addressed by `node_id`, not `pull_number`** — orchestrator decision (developer: finish without
  stopping), 2026-11-14. `pr_merge` / `pr_close` read the node's PR from the stack; `pr_repoint` takes
  `node_id` and an optional `new_base` (the effective base when omitted) and rebases as `pr_set_parents`
  does. Answers keep `tddy-tools`' shapes (`pr_set_parents` → `{node_id, parents}`).
- **`pr_spawn_child` takes optional `session_type` and `model`, defaulting to the caller's own** —
  orchestrator decision (developer: finish without stopping), 2026-11-14 (yolo-pr-stack parity with
  `StartSession`; the old handler always started claude-cli on the orchestrator's model). It answers the
  new `session_id`; the node is bound to it once the start records it.
- **`rebase_onto` keeps its signature** — orchestrator decision (developer: finish without stopping),
  2026-11-14. `repo_root` locates the worktree holding the branch (`worktree_path_for_branch`); a
  parameter rename would have been a no-op contract with nothing to fail on.
- **`PushIdentity` grows `remote_name: Option<String>` and `account_token: Option<String>`** (`None` →
  `origin` / the checkout's credentials), with a redacting `Debug` — orchestrator decision (developer:
  finish without stopping), 2026-11-14.
- **Conflicts found without leaving a merge behind** — the acceptance tests assert the node's worktree
  HEAD, status and `MERGE_HEAD` are unchanged even for a running node, so green may probe with
  `git merge-tree --write-tree` rather than merge-and-abort under a working agent.

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
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests — skipped by developer instruction 2026-11-14
- [x] TDD Red — write failing unit/integration tests
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
