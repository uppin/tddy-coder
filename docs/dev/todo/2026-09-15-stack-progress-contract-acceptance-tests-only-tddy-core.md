# 2026-09-15 — `stack_progress_contract_acceptance` tests only `tddy-core`

**Category:** Test placement
**Source:** test-placement audit during `#carve` planning

`packages/tddy-workflow-recipes/tests/stack_progress_contract_acceptance.rs` (169 lines) names no
symbol from the crate it lives in. Its whole import block is `tddy-core`:

```rust
use tddy_core::changeset::{
    read_changeset, sync_stack_node_from_child, update_stack_atomic, write_changeset, Changeset,
    ChangesetState, ChangesetWorkflow, GithubPrStatus, Stack, StackNode,
};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::workflow::ids::WorkflowState;
```

What it actually pins is `tddy_core::changeset`'s stack model — that `sync_stack_node_from_child`
mirrors a child session's state and PR status into a `StackNode`, and that `update_stack_atomic`
applies mutations atomically. Both functions are `tddy-core`'s, and so is every type it asserts on.

It is one of **two** outliers in this crate's 56 test binaries; `tddy-core`'s own 44 are all
correctly placed. So this is a small, isolated fix rather than a pattern.

## What closing it would take

`git mv` it to `packages/tddy-core/tests/`, and drop any `tddy-workflow-recipes` dev-dependency it
was the only reason for.

**Sequence it against `#carve`.** Node 4/9 splits `changeset.rs` into
`changeset/{stack,model,io,merge}.rs` and node 9/9 moves the PR-stack data model to `tddy-pr-stack`.
Every symbol this suite imports is in the half that moves, so:

- moving the file **before** node 4 lands means rewriting its imports twice;
- moving it **after** node 9 lands means it belongs in `tddy-pr-stack`, not `tddy-core`.

The cheapest order is to leave it until node 9 has landed, then move it once, to wherever
`sync_stack_node_from_child` and `update_stack_atomic` ended up.

## Checked and correctly placed

`packages/tddy-workflow-recipes/tests/proto_workflow_contracts.rs` (15 lines) names no `tddy-*`
crate either, but it is **not** misplaced: it asserts that
`packages/tddy-workflow-recipes/proto/` exists, which is a claim about its own package and can only
be made from inside it.
