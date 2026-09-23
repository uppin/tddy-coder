# 2026-09-23 — Four intra-doc links point up the dependency order and no longer resolve

**Category:** Future enhancement — rustdoc
**Source:** `#carve` 12/14 (`carve-core-facade`, PR #522), found at green

The carve moved these doc comments unchanged into crates that sit **below** the item they link to,
so rustdoc cannot resolve the links. This produces rustdoc warnings only; no gate reads them.

| Site | Link | Target now lives in |
|---|---|---|
| `packages/tddy-toolcall/src/toolcall/mod.rs:7` | ``[`crate::presenter::Presenter::poll_tool_calls`]`` | `tddy-presenter` |
| `packages/tddy-toolcall/src/toolcall/transition.rs:10` | ``[`crate::workflow::controller::WorkflowController`]`` | `tddy-workflow-engine` |
| `packages/tddy-agent-backend/src/backend/mod.rs:857` | ``[`crate::workflow::task::BackendInvokeTask`]`` | `tddy-workflow-engine` |
| `packages/tddy-changeset/src/changeset/model.rs:233` | ``[`crate::worktree`]`` | `tddy-session-worktree` |

A link cannot name a crate its own crate does not depend on, and adding the dependency would close
a cycle.

**What would close it.** Turn each one into plain code text naming the owning crate, for example
`` `tddy_presenter::presenter::Presenter::poll_tool_calls` ``. That keeps the pointer for a reader
without the link. This fits the same repoint pass as the `crate::` shims
(`2026-09-23-carved-crates-name-their-siblings-through-private-crate-root-shims.md`).
