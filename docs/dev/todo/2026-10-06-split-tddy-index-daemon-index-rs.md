# 2026-10-06 — `tddy-index-daemon/src/index.rs` crossed the file-length budget

**Category:** Deferred from `#sharpen` 4/8 (#591) — file-length gate
**Source:** `/pr-wrap` step 3.5; standing record
`packages/tddy-index-daemon/docs/code-issues/oversized-file-index.md`

#591 grew `packages/tddy-index-daemon/src/index.rs` from 491 to 508 production lines (budget 500):
the `WorkspaceIndex::wait_heartbeat` field and its two accessors, carried so a run queued behind
another on a root can beat at the host's cadence. The heartbeat's logic lives in `operations.rs`
(`hold_saying_so`) and `tddy-code-restructuring`; only the field and its accessors are here.

**Why the split is deferred, with the developer's consent (2026-10-06):** #591 is a feature node
(the wait heartbeat), not a decomposition, and the crossing is seventeen lines of wiring. Splitting
`index.rs` inside the node would expand a feature PR's scope and risk with an engine-driven
restructure that was never planned for it. No other open `#sharpen` node's own commits touch this
file, so this is a scope decision rather than the stack-overlap stop.

The decomposition belongs on its own follow-up branch. The standing record carries the measurement
and the seams to cut.
