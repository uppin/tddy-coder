# 2026-09-23 — The crates carved out of `tddy-core` name their siblings through private `crate::` shims

**Category:** Future enhancement — deferred to the repoint pass
**Source:** `#carve` 12/14 (`carve-core-facade`, PR #522), found by `/analyze-clean-code`

Seven of the nine crates `tddy-core` was carved into start their `lib.rs` with private `use`
declarations. These put a sibling crate's module at the `crate::` path the moved code used inside
`tddy-core`:

```rust
// packages/tddy-workflow-engine/src/lib.rs
use tddy_agent_backend::{backend, SharedBackend};
use tddy_changeset::{changeset, session_lifecycle};
use tddy_session_store::{atomic_file, error};
use tddy_toolcall::toolcall;
```

25 such declarations sit across `tddy-changeset`, `tddy-session-worktree`, `tddy-session-actions`,
`tddy-toolcall`, `tddy-agent-backend`, `tddy-workflow-engine` and `tddy-presenter`
(`grep -n '^use ' packages/tddy-*/src/lib.rs`).

**Why they exist.** They let every moved body stay byte-identical except for `use` lines, which is
what made the 152 `git mv` renames reviewable as renames. That was the node's no-behaviour-change
boundary.

**Why they are debt.** A reader of `crate::changeset::read_changeset` inside
`tddy-session-actions` sees no crate boundary at all. The real dependency is visible only in
`lib.rs`. The shims also let a crate's modules name a sibling's *private-looking* path, which hides
exactly the edges the carve exists to make legible.

**What would close it.** In the pass that repoints consumers from `tddy_core::…` to the owning
crates, rewrite each `crate::<sibling module>::…` inside a carved crate to
`tddy_<crate>::<module>::…` and delete the shim. This is mechanical, and `restructure` can drive it.
Do it per crate, bottom-up, so each step is its own reviewable diff.
