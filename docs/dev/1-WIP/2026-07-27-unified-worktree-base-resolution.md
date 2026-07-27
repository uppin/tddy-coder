# Changeset: Unified worktree base resolution across agent backends

**Date**: 2026-07-27
**Status**: 🚧 In Progress
**Type**: Refactor + Bug Fix

## Affected Packages

- **tddy-core**: [README.md](../../packages/tddy-core/README.md)
  - [architecture.md](../../packages/tddy-core/docs/architecture.md) — new `resolve_chain_base_for_session_spawn` in `session_chain` module
  - [changesets.md](../../packages/tddy-core/docs/changesets.md) — changeset index entry
- **tddy-daemon**: [README.md](../../packages/tddy-daemon/README.md)
  - [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md) — shared `prepare_session_worktree_base` helper, cursor-cli stack_parent threading
  - [changesets.md](../../packages/tddy-daemon/docs/changesets.md) — changeset index entry
- **tddy-workflow-recipes**: [README.md](../../packages/tddy-workflow-recipes/README.md)
  - `ensure_worktree_for_session` alignment with shared resolver (no behavior change)
  - [changesets.md](../../packages/tddy-workflow-recipes/docs/changesets.md) — changeset index entry

## Related Feature Documentation

- [PRD: Unified worktree base resolution](../ft/coder/1-WIP/PRD-2026-07-27-unified-worktree-base-resolution.md)
- [git-integration-base-ref.md](../ft/coder/git-integration-base-ref.md) — feature being amended

## Summary

Move `resolve_chain_base_ref` and its helpers from `tddy-daemon::connection_service` to `tddy-core::session_chain`. Add `resolve_chain_base_for_session_spawn` with a single precedence rule (persisted field → stack parent → default). Extract the duplicated worktree-setup prelude into a daemon-side `prepare_session_worktree_base` helper used by all four spawn paths. Thread `stack_parent` through the Cursor CLI paths, fixing the regression where a PR-stack child bases off `origin/master` instead of its planned node's parent branch.

## Background

The chain-PR base resolution logic is duplicated across six call sites with drift. The Cursor CLI paths (`spawn_cursor_cli_session_inner`, `start_sandboxed_cursor_cli_session`) hardcode `None` for the chain base and never receive `stack_parent`, so a PR-stack child session falls back to `origin/master`. See the PRD for the full regression analysis.

## Scope

- [ ] **Package Documentation**: Update package dev docs for tddy-core, tddy-daemon, tddy-workflow-recipes
- [ ] **Implementation**: Move resolver to tddy-core, add daemon helper, fix cursor-cli paths
- [ ] **Testing**: All acceptance tests passing (cursor-cli + claude-cli with stack parent, regression)
- [ ] **Integration**: Cross-package: tddy-core exports resolver, tddy-daemon + tddy-workflow-recipes consume it
- [ ] **Technical Debt**: Remove the 6-way duplication of the worktree prelude
- [ ] **Code Quality**: `clippy --workspace --all-targets -D warnings` + `fmt --check` clean

## Technical Changes

### State A (Current)

- `ConnectionServiceImpl::resolve_chain_base_ref` lives in `tddy-daemon::connection_service.rs` (line 990). It is a method on `ConnectionServiceImpl` but has no `&self` dependencies — it's pure logic.
- Helpers `parent_is_pr_stack_orchestrator` (line 1134) and `pr_stack_node_for_spawn` (line 1047) are also on `ConnectionServiceImpl`, also pure.
- `spawn_claude_cli_session_inner` (line 1483) and `start_sandboxed_claude_cli_session` (line ~2100) call `resolve_chain_base_ref` and pass the result to `setup_worktree_for_session_with_optional_chain_base`.
- `spawn_cursor_cli_session_inner` (`cursor_cli_spawn.rs:61`) does NOT receive `stack_parent`; the dispatch site (`connection_service.rs:4687`) drops it. It hardcodes `None` (line 182).
- `start_sandboxed_cursor_cli_session` (line 2655) also does NOT receive `stack_parent`; it hardcodes `None` (line 2793).
- Telegram paths (`telegram_session_control.rs:2724, 2901`) hardcode `None`, relying on `workflow.selected_integration_base_ref` written by the branch callback.
- `ensure_worktree_for_session` (`tddy-workflow-recipes/src/tdd/hooks_common.rs:127`) reads `cs.worktree_integration_base_ref` and passes it directly.
- The prelude (parse intent, resolve base, build ChangesetWorkflow, write seed changeset) is duplicated across all daemon spawn paths with minor variations.

### State B (Target)

- `tddy-core::session_chain` exports:
  - `resolve_chain_base_ref(sessions_base, stack_parent, repo_root, new_branch_name) -> Result<Option<String>, String>` (moved from daemon, same logic).
  - `resolve_chain_base_for_session_spawn(sessions_base, stack_parent, repo_root, new_branch_name, persisted_worktree_integration_base_ref) -> Result<Option<String>, String>` (new; precedence: persisted → stack parent → None).
  - Helpers `parent_is_pr_stack_orchestrator`, `pr_stack_node_for_spawn` moved alongside.
- `tddy-daemon` has a new `prepare_session_worktree_base` function (in `connection_service.rs` or a new `session_worktree_setup.rs` module) that:
  - Parses `branch_worktree_intent` string → `BranchWorktreeIntent`.
  - Computes `resolved_integration_base_ref` (client override → project default → None).
  - Calls `tddy_core::resolve_chain_base_for_session_spawn` with stack_parent + persisted field.
  - Builds and writes the seed `Changeset` (workflow, orchestrator_session_id, recipe, start goal).
  - Calls `setup_worktree_for_session_with_optional_chain_base` via `spawn_blocking_with_timeout`.
  - Returns `(worktree_path, effective_base_ref, intent, branch)`.
- All four daemon spawn paths call `prepare_session_worktree_base` and keep only their post-worktree tail.
- `spawn_cursor_cli_session_inner` and `start_sandboxed_cursor_cli_session` receive `stack_parent: Option<&str>`; the dispatch site passes `req.stack_parent.as_str()`.
- Telegram paths call `resolve_chain_base_for_session_spawn` with `stack_parent = None` and the persisted field.
- `ensure_worktree_for_session` calls `resolve_chain_base_for_session_spawn` with `stack_parent = None` and the persisted field (alignment; same result as before).

### Delta

#### tddy-core
- **session_chain module**: gains `resolve_chain_base_ref`, `resolve_chain_base_for_session_spawn`, `parent_is_pr_stack_orchestrator`, `pr_stack_node_for_spawn` (moved from daemon). `resolve_chain_integration_base_ref_from_parent_session` stays (used by the moved `resolve_chain_base_ref` for the non-stack case).
- **lib.rs**: re-exports the new functions.

#### tddy-daemon
- **connection_service.rs**: `resolve_chain_base_ref`, `parent_is_pr_stack_orchestrator`, `pr_stack_node_for_spawn` removed (moved to tddy-core). Call sites updated to `tddy_core::resolve_chain_base_ref`. New `prepare_session_worktree_base` helper. `spawn_claude_cli_session_inner` and `start_sandboxed_claude_cli_session` use the helper.
- **cursor_cli_spawn.rs**: `spawn_cursor_cli_session_inner` gains `stack_parent: Option<&str>` parameter; uses `prepare_session_worktree_base` instead of inline prelude + hardcoded `None`.
- **connection_service.rs dispatch site** (line ~4687): passes `req.stack_parent.as_str()` to `spawn_cursor_cli_session_inner`.
- **connection_service.rs `start_sandboxed_cursor_cli_session`**: gains `stack_parent: Option<&str>` parameter; uses `prepare_session_worktree_base`. Dispatch site (line ~4667) passes it.
- **telegram_session_control.rs**: both spawn paths call `tddy_core::resolve_chain_base_for_session_spawn` instead of hardcoding `None`.
- **tests**: existing `chain_base_resolution_tests` module in `connection_service.rs` moves to `tddy-core` tests (the tests call the moved function).

#### tddy-workflow-recipes
- **tdd/hooks_common.rs**: `ensure_worktree_for_session` calls `tddy_core::resolve_chain_base_for_session_spawn` with `stack_parent = None` and `cs.worktree_integration_base_ref` instead of passing the field directly. Same result; shared code path.

## Implementation Milestones

- [ ] Move `resolve_chain_base_ref` + helpers to `tddy-core::session_chain`; update re-exports
- [ ] Add `resolve_chain_base_for_session_spawn` with precedence rule
- [ ] Move existing `chain_base_resolution_tests` from daemon to tddy-core; add precedence test
- [ ] Create `prepare_session_worktree_base` daemon helper
- [ ] Migrate `spawn_claude_cli_session_inner` to the helper
- [ ] Migrate `start_sandboxed_claude_cli_session` to the helper
- [ ] Add `stack_parent` to `spawn_cursor_cli_session_inner`; migrate to the helper; update dispatch site
- [ ] Add `stack_parent` to `start_sandboxed_cursor_cli_session`; migrate to the helper; update dispatch site
- [ ] Update Telegram spawn paths to use the resolver
- [ ] Align `ensure_worktree_for_session` with the resolver
- [ ] Write acceptance tests (cursor-cli + claude-cli with stack parent)
- [ ] `clippy --workspace --all-targets -D warnings` + `fmt --check` clean

## Testing Plan

### Testing Strategy

**Primary test level: Integration** — the bug is a cross-module integration gap (daemon spawn path skips tddy-core resolver). The fix is verified by exercising the daemon's `StartSession` RPC with a stack parent and asserting the worktree's base ref.

**Secondary test level: Unit** — the moved resolver is pure logic; unit tests in tddy-core verify the precedence rule and the pr-stack orchestrator case without a daemon.

### Option 1: Daemon-level integration tests (primary)

**Test level**: Integration
**Location**: `packages/tddy-daemon/tests/cursor_cli_session_acceptance.rs` (extend) and `packages/tddy-daemon/tests/claude_cli_session_acceptance.rs` (extend)

**Scope**:
- Cursor CLI session with pr-stack orchestrator parent → worktree bases off the stack node's parent branch
- Sandboxed Cursor CLI same (gated on sandbox backend availability)
- Claude CLI session with pr-stack orchestrator parent → regression (still correct)
- Session without stack_parent → default base (regression)

**Assertions**:
- The spawned session's `changeset.yaml` `effective_worktree_integration_base_ref` equals the expected `origin/<ancestor-branch>`, not `origin/master`.
- The worktree HEAD equals the tip of the expected base ref.
- The worktree HEAD is a descendant of the expected ancestor branch's tip.

**Reliability**: deterministic — uses a stub binary (`/bin/cat`), a temp git repo with `origin` pointing at itself, and a pre-written orchestrator changeset. No network, no real agent process.

### Option 2: tddy-core unit tests (secondary)

**Test level**: Unit
**Location**: `packages/tddy-core/tests/unified_chain_base_resolution.rs` (new) and inline `#[cfg(test)]` in `session_chain.rs`

**Scope**:
- `resolve_chain_base_for_session_spawn` returns the stack node's base when stack_parent is a pr-stack orchestrator
- Returns None when stack_parent is None and no persisted field
- Returns the persisted field when present and no stack_parent
- Stack_parent takes precedence over persisted field
- Errors for a branchless code-session parent (ported from daemon)

**Assertions**: exact `Option<String>` equality on the resolved ref.

### Coverage Requirements

- [ ] **Happy path**: Cursor CLI + Claude CLI with pr-stack orchestrator parent resolve the correct node base
- [ ] **Error scenarios**: branchless code-session parent errors (ported)
- [ ] **Edge cases**: unmatched branch on orchestrator → None (default base); empty stack → None
- [ ] **Integration points**: tddy-core resolver consumed by tddy-daemon and tddy-workflow-recipes
- [ ] **Regression**: Claude CLI still resolves; no-stack-parent still defaults

## Acceptance Tests

### tddy-daemon
- [ ] **Integration**: `cursor_cli_pr_stack_child_bases_off_planned_node_parent` — Cursor CLI session with pr-stack orchestrator parent bases off the node's parent branch (tests/cursor_cli_session_acceptance.rs)
- [ ] **Integration**: `sandboxed_cursor_cli_pr_stack_child_bases_off_planned_node_parent` — same for sandboxed path (gated on sandbox backend)
- [ ] **Integration**: `claude_cli_pr_stack_child_bases_off_planned_node_parent` — Claude CLI regression (tests/claude_cli_session_acceptance.rs)
- [ ] **Integration**: `cursor_cli_session_without_stack_parent_uses_default_base` — regression (tests/cursor_cli_session_acceptance.rs)

### tddy-core
- [ ] **Unit**: `resolve_chain_base_for_session_spawn_returns_stack_node_base_for_pr_stack_orchestrator_parent` (tests/unified_chain_base_resolution.rs)
- [ ] **Unit**: `resolve_chain_base_for_session_spawn_returns_none_when_no_stack_parent_and_no_persisted_field`
- [ ] **Unit**: `resolve_chain_base_for_session_spawn_returns_persisted_field_when_no_stack_parent`
- [ ] **Unit**: `resolve_chain_base_for_session_spawn_stack_parent_takes_precedence_over_persisted_field`
- [ ] **Unit**: `resolve_chain_base_ref_errors_for_a_branchless_code_session_parent` (ported from daemon)
- [ ] **Unit**: `resolve_chain_base_ref_returns_none_for_a_branchless_pr_stack_orchestrator_parent` (ported from daemon)

## Technical Debt & Production Readiness

- [ ] The 6-way prelude duplication is the root cause of the regression; the helper eliminates it. No new debt introduced.
- [ ] The moved resolver is pure logic with no I/O beyond reading changeset files — no fallbacks, no test-only branches.

## Decisions & Trade-offs

- **Precedence: persisted field → stack parent → default.** A persisted `worktree_integration_base_ref` represents an explicit user choice (Telegram callback); a stack parent is a runtime request. The explicit choice wins. This matches both existing behaviors without change.
- **Move vs. wrap.** Moving `resolve_chain_base_ref` to tddy-core (rather than wrapping it from the daemon) makes it unit-testable without a daemon instance and available to `tddy-workflow-recipes`. The daemon's `ConnectionServiceImpl` loses the method but gains a thin re-export if needed for its own tests.
- **Daemon helper vs. tddy-core helper.** The prelude (parse intent, build ChangesetWorkflow, write seed changeset) is daemon-specific because it translates a gRPC request into a changeset. It stays in the daemon. The resolver (which base ref to use) is pure and moves to tddy-core.

## References

- [PRD: Unified worktree base resolution](../ft/coder/1-WIP/PRD-2026-07-27-unified-worktree-base-resolution.md)
- [git-integration-base-ref.md](../ft/coder/git-integration-base-ref.md)
- Daemon log: `/var/log/tddy-daemon/daemon` lines 26092–26162 (the regression evidence)
