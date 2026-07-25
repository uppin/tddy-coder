# Changeset: pr-stack-branch-gated-spawn — a stack progresses on branches, not on child sessions

**Date:** 2026-07-25
**Branch:** `feature/pr-stack/decouple-session-from-stack`
**Packages:** `tddy-core`, `tddy-workflow-recipes`, `tddy-daemon`, `tddy-web`
**Feature PRD:** [docs/ft/coder/pr-stack-live-status.md](../../ft/coder/pr-stack-live-status.md) § capability 1, capability 5

## Problem

A PR-stack orchestrator gated every spawn on the parent node's `session_id`:
`Stack::base_ref_for_spawn` refused with *"non-merged parent '<id>' has not been started yet"*
whenever a parent carried no child session. A child session that was closed, cleaned up, or never
linked therefore wedged the whole stack below it, even though the parent's branch existed and was
a perfectly good base. `effective_base_refs` compounded it by fabricating `origin/<node_id>` for a
branchless parent — a ref nothing ever created — so a spawn that got past the gate could be based
on a name that does not resolve.

The gate was keyed on the wrong fact. What a child worktree needs is the parent's **branch**.

## Contract

- A spawn is gated on the parent's **branch**. Sessions are irrelevant to stack progression: a
  branch can be built on whether or not a session is still attached to it.
- A child **session** is only a *fallback* route to resolving a node's branch, for a node linked
  before its branch was known (or an older manifest).
- `branch` means "a branch that exists". A `branch_suggestion` is a planned name and never
  satisfies the gate — planning leaves `branch = None`.

## Changes

- [x] **`tddy-core`** (`changeset.rs`)
  - `Stack::base_ref_for_spawn` refuses on a non-merged parent with no `branch` (message names the
    parent and says *"has no branch to base onto yet"*), no longer on a missing `session_id`.
  - `Stack::effective_base_refs` only counts branch-bearing non-merged parents; the
    `origin/<node_id>` fabrication is gone.
  - New `resolve_stack_node_branch(sessions_root, node) -> Option<String>` — the node's own
    `branch`, else the `branch` in its child session's changeset. A missing session directory
    resolves to `None`, never an error.
  - New `read_stack_with_resolved_branches(sessions_root, orchestrator_session_id)` — the
    orchestrator's stack with every node's `branch` hydrated through the resolver; `Ok(None)` when
    the session carries no stack.
- [x] **`tddy-workflow-recipes`**
  - `plan_pr_stack::planned_prs_into_stack_nodes` and `pr_stack::add_planned_pr_node` leave
    `branch = None` (they previously copied `branch_suggestion`, contradicting the doc comment).
  - `pr_stack::reseed_stack_from_plan_if_unspawned` refuses once any node owns a **branch or** a
    session — the branch is real work that outlives the session that created it.
  - `orchestrate_pr_stack::assess::assemble_views` keys the PR lookup on the node's branch instead
    of `session_id.is_some()`, and resolves that branch via `resolve_stack_node_branch` instead of
    inventing `feature/<node_id>`. A node with no branch yields an empty `NodeView.branch`, which
    `effective_base_ref` now skips like an absent parent.
- [x] **`tddy-daemon`** (`connection_service.rs`)
  - `link_stack_parent_node_to_child` → **`link_stack_node_to_spawned_branch`** (both spawn call
    sites updated): its subject is the branch a spawn created, with the session recorded as the
    fallback. A new session claiming a branch a node already owns **repoints** it (last writer
    wins) instead of failing `FailedPrecondition` — restart/re-attach is normal.
  - `pr_stack_node_for_spawn` matches a node by `branch`, else by `branch_suggestion` for a node
    not yet materialized (planned nodes carry no `branch` now); the exact `branch` match wins, so a
    node renamed away from its suggestion still resolves to itself.
  - `pr_stack_node_for_spawn` reads the stack through `read_stack_with_resolved_branches`, so the
    session fallback reaches the spawn gate itself: a node whose branch only its child session
    recorded still supplies a base to its descendants. The hydrated stack is read-only — the
    forward link writes through the orchestrator's session dir, so a fallback-derived branch is
    never persisted onto a node that did not record it.
  - `StackChildSpawnHandler::spawn_child`'s duplicate guard is now "the node already owns a
    branch", and the branch to create comes from `branch_suggestion`.
- [x] **`tddy-web`**
  - `deriveStackBaseBranch` no longer previews `branch ?? branchSuggestion` — only a created
    `branch` is a ref, matching the daemon rule, so the dialog cannot promise a base the spawn then
    refuses. A branchless parent is passed over like an absent one.
- [x] PRD updated (capability 1, capability 5, D1, the `### Rust` bullets).

## Validation

- `./test` (full workspace gate): **2263 passed / 1 failed / 12 ignored**. The single failure is
  pre-existing and unrelated — `tddy-sandbox-recipes`
  `cursor_cli::tests::cursor_agent_prerequisite_reads_include_install_dir_and_share_root` asserts
  the traversal ancestors contain `/Users`, a macOS-only home root, so it cannot pass on a Linux
  host. That package is untouched by this branch.
- `tddy-core`: **380 passed / 0 failed** (incl. new `pr_stack_branch_resolution_acceptance` 9 and
  `pr_stack_spawn_base_acceptance` 7).
- `tddy-workflow-recipes`: **328 passed / 0 failed** (lib 180, incl. the reseed/plan/assess cases).
- `tddy-daemon`: green under `./test`, which builds `tddy-sandbox-runner` first (under a bare
  `cargo test -p tddy-daemon` the pre-existing
  `sandbox_session::tests::dial_and_bridge_drives_run_host_relay_over_a_stdio_sandbox_client`
  panics *"build tddy-sandbox-runner first"*). `connection_service::stack_child_link_tests` 7/7.
- `tddy-tools`: **208 passed / 0 failed** (consumes `assemble_views`).
- `tddy-web`: `bun test src/components/sessions/prstack` **7 pass / 0 fail**.
- `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt` clean.

## Notes for the wrap

Three pre-existing Rust tests asserted the *old* "definitive branch at creation" contract and were
inverted with the implementation (they cannot both hold):
`plan_pr_stack::tests::parser_happy_path_three_node_dag`,
`tests/plan_pr_stack_acceptance.rs::planned_prs_into_stack_nodes_maps_three_pr_dag`, and both cases
in `tests/pr_stack_branch_link_acceptance.rs` (retitled to "a planned PR owns no branch until a
child worktree creates one"). The web `deriveStackBaseBranch.test.ts` suggestion case was inverted
for the same reason, plus a new case covering a branchless parent being skipped.
