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

## Hand edit?

Not done. The developer's rule for this carve is that an engine gap is not worked around by hand
without consent, so M0.1 waits for it (see
[lifecycle modules to re-parent by hand](./2026-09-24-lifecycle-modules-to-re-parent-by-hand.md) for
the sibling gap, "extract_module cannot put code under another parent").
