# 2026-11-09 — Two `move_item` defects found splitting `tddy-coding-subagent/src/session.rs`

**Category:** Known limitation (engine capability)

**Source:** #835 (`#agent-tools 27/32`), `/pr-wrap` step 3.5 — see
`docs/dev/changesets/2026-11-09-agent-tools-acp-subagent-leftovers.md` § Restructuring

## What happened

1. **A chained plan's anchor is invalidated by an earlier move in the same file.** A seven-line plan of
   `move_item`s out of `session.rs` (all v2 item anchors, run against the warm index daemon) applied
   five and stopped at the sixth: `anchor invalidated: position in …/session.rs fell inside text
   removed by an earlier operation`. `check --deep` reported the same at the fifth. The sixth item
   (`ToolCallRecord, collect_tool_calls, MessageShape, <MessageShape>`) was nowhere near the moved
   text; re-anchoring the two remaining moves and applying them as a second plan worked.
2. **The type a moved function returns is not widened.** Moving `ToolCallRecord` (private struct),
   `collect_tool_calls` and `MessageShape` into `session::message_shape` widened `collect_tool_calls`
   and `MessageShape` (with its fields and methods) to `pub(super)`, but not `ToolCallRecord`, which
   the parent still names (`E0603 … is private`). The compile gate stopped the run, so the import
   tidy did not run either (see `2026-10-09-restructure-import-tidy-on-a-tree-that-does-not-compile.md`).

Both were fixed forward by hand in #835 (one `pub(super)`, `cargo fix` for the copied imports).

## What would close it

Re-resolve each pending item anchor after a move rewrites its file, rather than mapping positions
through the earlier edit; and widen every item of the moved set that a caller left behind names, not
only the ones the outline reports as referenced through a moved function's callers. Pin both with
fixtures in `packages/tddy-code-restructuring` that reproduce this file's shape.

**Deferred by the user, 2026-11-10** (#agent-tools TODO gate after the sweep #876): asked "continue the stack or defer?", the user answered "Defer". Left open for a later, separately planned node.
