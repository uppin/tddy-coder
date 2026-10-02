# oversized-file: crate_move/moving.rs — the move writer and, now, the whole facade writer

**Location:** `packages/tddy-code-restructuring/src/crate_move/moving.rs`
**Category:** oversized-file
**Detected:** 2026-10-02 by the `/pr-wrap` file-length gate on #541
**Metrics:** **594 production lines** (349 before #541) · budget 500
**Restructure:** required — `extract_module --to_file` along the seam below
**Status:** Open — split deferred from #541 (`#live-plan` 4/7); **developer consent for the deferral is pending**
**Deferred by:** #541, see `docs/dev/todo/2026-10-02-crate-move-moving-rs-is-594-production-lines.md`

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-02 | 349 → 594 | #541 added the grouped-facade writer (`leaving` and its helpers, `written_facade`, `parent_reexports_of`, `declared_in_destination`) to a file that held the `Move` type and its edits |

## What the gate found

The file is two things: the `Move` type with the edits one move writes, and — new in #541 — the
writer of what a *set* of moves leaves behind in the declaring file and the destination root.

| Seam | What it is |
|---|---|
| The `Move` type | `Move`, its resolution and manifest/caller edits |
| The facade writer | `left_behind`, `leaving`, `earlier_facade`, `extended_facade`, `declaration_of`, `parent_reexport_edits`, `written_facade`, `parent_reexports_of`, `after_visibility`, `declared_in_destination` |

## Why it was not split in #541

`move-paths` (#540, the parent) also edits this file. A split inside this PR renames every symbol
the parent's diff touches and turns the stack into conflicts. Do it after the stack lands.

## What would close it

Extract the facade writer to `crate_move/facade_writer.rs`: it needs only `Move`, `Survey`,
`Workspace` and `manifest_edits`, and takes the file under budget on its own. Prove the seam with
`restructure check --deep` against a warm index before applying.
