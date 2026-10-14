# 2026-10-14 — the cycle refusal names the module's own file, not the carried child whose path forced it

**Category:** Restructure engine, misleading message
**Source:** #reshape 5/19 (`feature/reshape/move-children`); `refusals::refuse_a_dependency_cycle`, pinned by `a_carried_childs_path_back_into_the_origin_makes_the_move_refuse_as_a_cycle` in `packages/tddy-code-restructuring/tests/crate_move_children.rs`

## What happens

Every carried file's edges back to the origin are gathered into one list, and `refuse_a_dependency_cycle` is
called once per moved module with that list and the module's `source`. When the path that forces the cycle is
written in a carried child (`crates/origin/src/x/a/b.rs` calls `crate::runtime::Tick`), the refusal reads

```text
plan is malformed: `crates/origin/src/x/a.rs` still names `origin` (origin::runtime::Tick), so the destination …
```

The path (`origin::runtime::Tick`) is right; the file it is attributed to (`x/a.rs`, the module's own file) is
not the file that writes it. A reader opens `a.rs`, finds nothing, and has to search the children.

What is known: the test pins this text exactly, with the offending path in the child `b.rs` and the module's
file named. What is not known: whether any caller depends on the file named in the message; none was found.
The path list does not say which file each path comes from, and the refusal has no per-file view of it.

## What would close it

Keep the file with each origin path when the edges are gathered (`header::repointed_header_at` returns them per
file, and `member_changes` unions them), pass the pairs to `refuse_a_dependency_cycle`, and name each file once
beside the paths it holds. Change the pinned assertion to the new text.

## Why deferred

The refusal is correct about the cycle and the path; only the attribution is loose. Fixing it changes the
signature `refuse_a_dependency_cycle` shares with the single-file move and the message a pinned test asserts,
which was outside this node's boundary.
