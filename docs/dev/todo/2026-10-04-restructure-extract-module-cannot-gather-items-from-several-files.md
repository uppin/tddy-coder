# 2026-10-04 — `extract_module` cannot gather items from several files into one module

**Category:** Future enhancement (engine capability)
**Source:** `#carve` 16/21, [#531](https://github.com/uppin/tddy-coder/pull/531), changeset
[`2026-09-26-carve-lifecycle-ports`](../1-WIP/2026-09-26-carve-lifecycle-ports.md), scope item M0.1

## What was planned

M0.1 asks for one T3 module, `peer_session_answer`, built with `extract_module` from four free
items that live in **four different files**:

| Item | File (under `packages/tddy-session-lifecycle/src/`) |
|---|---|
| `peer_has_no_such_session` | `connection_service/split_start.rs` |
| `split_pairing` | `split_session.rs` |
| `resolve_worktree_root_for_session` | `workspace_session.rs` |
| `resolve_exec_tool_worktree` (the free fn, not the host method of the same name) | `connection_service/svc_resolve_os_user.rs` |

## What the engine does

`extract_module` takes an `items` anchor, which carries **one** `file` and a contiguous run of
sibling items in it, and writes the new module as a child of that file's module
(`plan-schema.md`: "a run of sibling items, for `extract_module`"; "`items` must be contiguous").
It cannot name items of another file, and there is no operation that moves an item between modules of
one crate (`move_module_to_crate` and `move_cluster_to_crate` cross crates). Run once per file it would
produce four modules (`split_start::peer_session_answer`, `split_session::peer_session_answer`, …),
each still under the topic the item has to leave, which is the opposite of the cut M0.1 exists for.

Observed on the warm index daemon: `restructure anchors <file> --items …` accepts only names declared
in the one file named; nothing in the anchor schema can express the four-file group.

## What the engine should do instead

A same-crate `move_symbol` / `move_item_to_module` for Rust: move a named item (with `reexport` =
`glob` | `named` | `none`, and the import re-pointing `move_module_to_crate` already does) to an
existing or new module of the same crate, in any file. With it M0.1 is one `extract_module` to create
`peer_session_answer` from the first item, then three moves; M0.6's two type moves
(`SessionStdioEndpoint`, `ExecToolRoute`) need the same operation.

## Alternatives considered (2026-10-04)

- **Multi-file `items` anchors in `extract_module`** (one anchor listing items of several files, gathered
  into one new module). Smaller surface, but the engine would have to choose the new module's parent and
  resolve visibility across files. It composes from the move operation above, so build the move first.
- **Prerequisite for 16b.** The `#carve` 16b node consumes the M0.1 T3 group, so it re-plans around the
  missing `peer_session_answer` until this lands.

## Deferred work this blocks

Developer decision (2026-10-04): these items are **deferred until the engine can do them**, not done by hand:

- **M0.1** `peer_session_answer` (4 items from 4 files into one T3 module);
- **M0.6** `seeded_clone_guard.rs` split (`SessionStdioEndpoint` to T1, `ExecToolRoute` beside `LocalExecTools`);
- the "group `write_claude_hooks_settings` and `resolve_start_session_claude_binary` with T4" half of M0.2
  (not contiguous; each is one move once the operation exists; otherwise they move with T4 in 16c).

Follow-ups: [`#carve` same-crate re-parenting](./2026-09-24-lifecycle-modules-to-re-parent-by-hand.md) (M0.4, also
awaiting developer consent D8) and
[engine ergonomics seen during #carve 16a](./2026-10-04-restructure-anchors-and-snapshot-friction-seen-in-carve-16a.md).

## Hand edit?

Not done. The developer's rule for this carve is that an engine gap is not worked around by hand
without consent, so M0.1 waits for it (see
[lifecycle modules to re-parent by hand](./2026-09-24-lifecycle-modules-to-re-parent-by-hand.md) for
the sibling gap, "extract_module cannot put code under another parent").
