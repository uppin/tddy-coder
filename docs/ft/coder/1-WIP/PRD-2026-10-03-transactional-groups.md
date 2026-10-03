# Transactional groups in restructure plans - PRD

**Date**: 2026-10-03
**PRD Type**: Enhancement

## Affected Features

- **Primary**: [Rust code restructuring](../rust-code-restructuring.md) — plan schema (`group`), apply, `check --deep`, rollback.

## Summary

Some refactors cannot compile step by step — change a type, then adapt every use. This PRD adds
**transactional groups**: consecutive operations sharing a `"group"` id are applied as one unit;
`cargo check` runs at the group's end over the packages the group touched; if it fails, **the group
is rolled back** exactly (files restored from journaled pre-images, created files removed, renames
undone), the run stops naming the group and the compiler errors, and everything before the group
stays applied.

## Background

Today `cargo check` runs only before and after a whole run, and a failed run leaves its edits on disk
(pinned by `apply_compile_gate_acceptance`). The journal records hashes, not contents. A `"group"`
field parses today and is silently dropped (`RefactorOp` has no `deny_unknown_fields`).

## Proposed Changes

- `RefactorOp.group: Option<String>`; members must be consecutive (refused otherwise); unknown fields
  are refused from now on (`deny_unknown_fields`), so a typo cannot silently drop a group.
- Journal: `group_started{group, ops}` and per-member pre-images (bytes of each touched file before
  its first edit in the group); `group_completed` / `group_rolled_back`. Resume inside a group rolls
  the partial group back before continuing.
- Apply (both the CLI loop and the index daemon's): gate at each group's end; rollback on failure
  with `RestructureError::GroupDoesNotCompile{group, errors}`. Ungrouped operations keep today's
  end-of-run gate and leave-on-disk behaviour.
- `check --deep`: a member that is refused marks the whole group refused (one finding naming the
  group), and the rehearsal does not advance past a refused group.
- Plan store: a group's members are refreshed/folded together at the group's end.

## Acceptance Criteria
- [ ] A group whose members compile only together applies, and its end gate passes.
- [ ] A group that does not compile at its end is rolled back byte-for-byte (including created and
      renamed files); earlier operations stay applied; the error names the group and the errors.
- [ ] Non-consecutive members of one group are refused as malformed.
- [ ] An unknown field on an operation is refused as malformed.
- [ ] A crash inside a group, then `--resume`, rolls the partial group back and re-applies it.
- [ ] `check --deep` reports one finding for a group with a refused member.
- [ ] An ungrouped failing run still leaves its edits on disk (today's contract).
