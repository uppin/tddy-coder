# PRD: Unified worktree base resolution across agent backends

**Status:** 🚧 In Progress
**Date:** 2026-07-27
**Amends:** [git-integration-base-ref.md](../git-integration-base-ref.md)

## Summary

The chain-PR base resolution logic that selects a worktree's starting ref from a stack parent session is duplicated across six call sites in the daemon, with drift: the Claude CLI paths resolve the chain base at spawn time, the Cursor CLI paths hardcode `None` (falling back to `origin/master`), the Telegram paths rely on a callback-written field, and the workflow-recipe path reads a persisted field. This amendment unifies the resolution into a single tddy-core function consulted by every spawn path, and fixes the Cursor CLI regression where a PR-stack child session bases off `origin/master` instead of its planned node's parent branch.

## Background

### The regression

Session `019fa4a7-85f5-7f73-837c-0b6c2fc8d12d` was created as a Cursor CLI PR-stack child of orchestrator `019f9dd5-716d-7071-96ac-464ff7b98c2a` with `new_branch_name=feature/session-attach-docs/attach-start`. The orchestrator's stack has node `attach-start` whose parents are `attach-proto` (branch `feature/session-attach-docs/attach-proto`) and `attach-store` (branch `feature/session-attach-docs/attach-store`). The expected base was `origin/feature/session-attach-docs/attach-store`.

The daemon log shows:

```
StartSession cursor-cli ...: stack_parent="019f9dd5-..." branch_worktree_intent="new_branch_from_base" new_branch_name="feature/session-attach-docs/attach-start"
setup_worktree_for_session_with_optional_chain_base: ... chain_opt_in=false
setup_worktree_for_session_with_optional_chain_base: intent=new_branch_from_base new_branch=feature/session-attach-docs/attach-start start_ref=origin/master
```

`chain_opt_in=false` and `start_ref=origin/master` — the stack parent was supplied to `StartSession` but never threaded to worktree setup.

### Root cause

`ConnectionServiceImpl::resolve_chain_base_ref` (in `tddy-daemon::connection_service`) resolves the chain base from a stack parent: for a pr-stack orchestrator, it finds the planned node matching the new branch and returns its nearest non-merged ancestor's `origin/<branch>`; for a code-session parent, it returns `origin/<parent-branch>`. Both Claude CLI spawn paths call it. Neither Cursor CLI spawn path does — `spawn_cursor_cli_session_inner` and `start_sandboxed_cursor_cli_session` hardcode `None` as the third argument to `setup_worktree_for_session_with_optional_chain_base`, and the dispatch site does not pass `stack_parent` to `spawn_cursor_cli_session_inner` at all.

### Why the duplication exists

The worktree *operation* (`setup_worktree_for_session_with_optional_chain_base`) is already a single tddy-core function. The *prelude* — parse the branch intent, resolve the chain base, compute the integration base override, build and write the seed changeset — is copy-pasted across the four daemon spawn paths (Claude CLI, sandboxed Claude CLI, Cursor CLI, sandboxed Cursor CLI) plus the two Telegram spawn paths. Each copy has drifted: the Cursor CLI copies skip chain resolution entirely, and the Telegram copies pass `None` relying on a different field (`selected_integration_base_ref`) written by a callback.

## Proposed changes

### 1. Move chain base resolution to tddy-core

`resolve_chain_base_ref` and its helpers (`parent_is_pr_stack_orchestrator`, `pr_stack_node_for_spawn`) are pure logic with no daemon dependencies — they read changesets from `sessions_base`, resolve stack nodes via `tddy_core::changeset::Stack`, and call `tddy_core::resolve_default_integration_base_ref`. Move them from `tddy-daemon::connection_service` to `tddy-core::session_chain` (which already houses `resolve_chain_integration_base_ref_from_parent_session`).

Add a new public function that consults both the persisted field and the stack parent, establishing a single precedence rule:

```
resolve_chain_base_for_session_spawn(
    sessions_base,
    stack_parent: Option<&str>,
    repo_root,
    new_branch_name,
    persisted_worktree_integration_base_ref: Option<&str>,
) -> Result<Option<String>, String>
```

Precedence: a persisted `worktree_integration_base_ref` wins (it was explicitly chosen, e.g. by a Telegram callback); otherwise a `stack_parent` is resolved; otherwise `None` (default base). This matches both existing behaviors — daemon Claude CLI paths resolve at runtime (no persisted field yet), Telegram paths use the persisted field (no stack_parent passed), workflow-recipes use the persisted field (no stack_parent available).

### 2. Daemon: shared prelude helper

Extract the duplicated prelude into a daemon-side helper `prepare_session_worktree_base` used by all four spawn paths. It takes the request-shaped inputs (branch intent strings, stack_parent, project default branch, managed recipe), calls the tddy-core resolver, builds and writes the seed `Changeset`, calls `setup_worktree_for_session_with_optional_chain_base`, and returns `(worktree_path, effective_base_ref, intent, branch)`. Each backend keeps only its post-worktree tail (hooks install, jail, semantic index, binary spawn, handlers).

### 3. Cursor CLI: thread stack_parent

The dispatch site in `connection_service.rs` (the `spawn_cursor_cli_session_inner` and `start_sandboxed_cursor_cli_session` call sites) passes `stack_parent` through. Both cursor-cli functions receive `stack_parent: Option<&str>` and feed it to the shared helper. This is the core bug fix.

### 4. Telegram: consult the resolver

The Telegram spawn paths currently pass `None` and rely on the branch callback having written `workflow.selected_integration_base_ref`. They instead call the tddy-core resolver with `stack_parent = None` and the persisted `worktree_integration_base_ref`, so the chain base is read from the field the callback wrote (or resolved default when absent). No behavior change for existing Telegram flows; this just routes through the shared function.

### 5. Workflow recipes: alignment (no behavior change)

`ensure_worktree_for_session` in `tddy-workflow-recipes` already reads `cs.worktree_integration_base_ref` and passes it to `setup_worktree_for_session_with_optional_chain_base`. It optionally calls the new tddy-core resolver instead of reading the field directly, so the precedence rule is shared. Since it has no `stack_parent`, the result is identical — this is code alignment, not a behavior change.

## Acceptance criteria

- [ ] A Cursor CLI session spawned with a pr-stack orchestrator parent and a `new_branch_name` matching a planned node bases its worktree off that node's nearest non-merged ancestor's `origin/<branch>`, not `origin/master`.
- [ ] A sandboxed Cursor CLI session does the same.
- [ ] A Claude CLI session spawned with a pr-stack orchestrator parent still resolves the correct chain base (regression).
- [ ] A session spawned without `stack_parent` and without a persisted `worktree_integration_base_ref` bases off the default integration base (regression).
- [ ] A session with a persisted `worktree_integration_base_ref` and no `stack_parent` honors the persisted field (Telegram / workflow-recipe compatibility).
- [ ] `resolve_chain_base_ref` lives in `tddy-core` and is unit-tested without a daemon instance.
- [ ] All four daemon spawn paths call the shared `prepare_session_worktree_base` helper; no path hardcodes `None` for the chain base.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] `cargo fmt --check` clean.

## Out of scope

- Unifying the post-worktree tails (hooks, jail, semantic index, binary spawn) — those are legitimately per-backend.
- Changing the Telegram callback protocol that writes `selected_integration_base_ref`.
- Adding `stack_parent` to the workflow-recipe `ensure_worktree_for_session` path (it runs inside the agent and has no stack parent available).
- The `workspace_session.rs` path (no agent, no stack concept — stays as-is).

## Related

- [git-integration-base-ref.md](../git-integration-base-ref.md) — the feature being amended.
- [pr-stacking.md](../pr-stacking.md) — PR-stack orchestrator and planned nodes.
- [telegram-session-control.md](../daemon/telegram-session-control.md) — Telegram chain base selection callback.
