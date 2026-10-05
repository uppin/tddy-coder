# `retarget_impl`: move impl members from one type to another, and have `verify` account for it - PRD

**Date**: 2026-10-05
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — a new operation, `retarget_impl`, in `## Rust operations (v1)` / `### Same-crate moves`; the operation count (twenty-two -> twenty-three); the plan format; `## Verify`; the CLI table's `verify` row.
- **Related Feature**: [Warm code intelligence daemon](../warm-code-intelligence-daemon.md) — `verify` is answered by the daemon when one is named, so its request gains one repeated field. Behaviour is otherwise unchanged.

## Summary

A plan can now say "these `impl` members belong to another type": `retarget_impl` rewrites the self type of an inherent `impl` block (or of the run of members it is anchored on, splitting the block), re-points the
paths the old type named for the moved members, adds the one `use` the new type needs, keeps every comment, and refuses before any write when a moved member reads a field the new type does not declare.
`restructure verify` can be told of a retarget and then accounts for it, instead of reporting it as a loss.

## Background

Moving 22 methods from `impl DaemonSessionHost` to `impl AgentRoster` was an `impl` header edit per file plus a `use`, and no operation does it: `change_param_type` on `self` is refused, and `move_item`
moves only a whole block, keeping its type. The engine's guarantees (the compile gate, comments kept, a refusal named before apply) did not reach that hand edit, and `verify` reported the hand retargets and receiver re-points of that node as 76 statements lost and 209 gained.

## Proposed Changes

### What's Changing

- **A new operation, `retarget_impl`**, anchored on an inherent `impl` block (`<Type>`) or a contiguous run of its members, with a required `to_type` (a path type of the same crate, rooted at the package name):
  - a whole-block anchor, or a run covering every member, rewrites the block's self type;
  - a run that is a proper subset **splits the block in place** into `Old`, `New` and `Old` blocks at the member boundaries, repeating the original header (generics, `where`, attributes) on each;
  - `Old::<moved member>` paths inside the moved members become `New::<member>` (from the language server's reference set); a method call, a caller elsewhere, or a path to an unmoved item is left alone;
  - one `use` of the new type is added unless the file already binds it; a name already bound to something else is refused;
  - doc comments, attributes and every comment travel as they were.
- **Refused before anything is written**, by `check --deep` and again by `apply`: a moved member that reads `self.field` (or `Self { field }`) the new type does not declare, naming every member and field; a trait impl
  (`<Old as Trait>`); a new type in another crate or one its module does not declare; the type the block already is; an anchor whose members sit in two blocks.
- **`restructure verify --against <ref> --retarget OLD=NEW`** (repeatable) accounts for a declared retarget: `impl OLD {` becoming `impl NEW {`, the headers a split adds, and `OLD::m(…)` becoming `NEW::m(…)` are
  paired and counted with the other re-points. Without the flag `verify` behaves as today, and it still reports a rename that was not declared, a changed argument or a lost comment.
- **Separable, may be cut to a follow-up**: an optional forwarding delegator (`"variant":"leave_delegator"` with an `expr` reaching the new type from `self`) that leaves, on the old type, one forwarding method per
  moved method, so callers keep compiling; `verify` accounts for it on a `--delegator` declaration.

### What's Staying the Same

- Every existing operation, anchor form, refusal class and the compile gate.
- `move_item` still moves a whole block keeping its type; to put a retargeted block in the new type's module, run `move_item` over `<New>` afterwards.
- **Callers are not re-pointed.** A caller of a moved member breaks the compile gate unless the delegator is used or a later operation (`repoint_call`) re-points it in the same transactional group.
- `self.field` -> `state.field` is still not an operation (the open backlog entry about reading a method's fields through a state parameter).
- `verify` with no declaration; the response's three counts (the retarget pairs are counted in the re-point count).

## Impact Analysis

### Technical Impact

- `tddy-code-restructuring`: a new operation module (all logic out of the over-budget `backends/rust.rs`), one `RefactorKind` variant (in `plan/refactor_kind.rs`, where `tidy-engine-files` moves the kind), one `RefactorOp` field (`to_type`, in `plan.rs`, with a `to_type: None` on the 20 full struct literals in 15 files), a codec child module, a `verify` child module, two visibilities widened in `item_move/text.rs`.
- `tddy-index-daemon`: one repeated field on `VerifyRequest`, the CLI request, the query handler.
- `tddy-tools`: the request is filled from the flag. No new subcommand.
- Two live test binaries to register in `.config/nextest.toml` and `.config/rust-e2e.filterset`. No new dependency (`syn` with `full` and `visit` is already one).

### User Impact

- A refactor that moves methods to another type is one plan line and one `check --deep`, with the field problem named before apply, instead of a hand edit per file found by the compiler afterwards.
- `verify` stops reporting a declared retarget as a loss; the author states the retarget they made, and the documentation says what that does and does not prove.
- A plan written before the operation existed is unaffected. No breaking change.

## Implementation Plan

1. Probe the three unverified premises with one live run (outline of an impl, references to an associated function, hunks of a split); amend the plan if one fails.
2. The plan surface and the whole-block retarget (the mechanical `to_type` commit first).
3. The block split, the path re-points and the refusals.
4. `verify` accounting for a declared retarget, through the library, the CLI, the daemon request and the proto.
5. Decide at that point whether the delegator joins this change or becomes its own node; if it joins, build it with its `verify` accounting.
6. Register the live binaries; stage the docs (operation count, plan schema, skill, feature doc, README, a behaviour page) for wrap.

## Acceptance Criteria

- [ ] `retarget_impl` of a whole `<Host>` block to `app::roster::Roster` rewrites the header, adds the `use`, keeps every comment, and the tree compiles ([Rust code restructuring](../rust-code-restructuring.md))
- [ ] a proper-subset run splits the block in place into three blocks whose `Host` headers equal the original, and the tree compiles
- [ ] `Host::build` (moved) becomes `Roster::build` inside the moved members; `Host::LIMIT` (not moved) is untouched
- [ ] a moved member reading a field `Roster` lacks is named by `check --deep`, and `apply` refuses it leaving the file byte-identical
- [ ] a trait impl, another crate, an undeclared type and the same type are each refused with the stated class
- [ ] a caller of a moved member that the operation does not touch fails the compile gate (documented outcome)
- [ ] `verify --retarget Host=Roster` holds for the retarget through the CLI and through the daemon, with the same lines; without the flag both report it
- [ ] the operation count reads twenty-three where it is stated; plan-schema, the skill, the feature doc and the README describe `retarget_impl` and `--retarget`
- [ ] tests pass for `tddy-code-restructuring`, `tddy-index-daemon` and `tddy-tools` (scoped; CI for the rest)
- [ ] (if the delegator joins) a delegator keeps an outside caller compiling and `verify --delegator` accounts for it

## References

### Affected Features (Complete List)

- [Rust code restructuring](../rust-code-restructuring.md) — the operation, the plan format, `verify`
- [Warm code intelligence daemon](../warm-code-intelligence-daemon.md) — the `Verify` request

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-10-05-sharpen-retarget-impl.md`
- Todos this closes (exist only on `feature/carve/lifecycle-ports-agents`):
  `docs/dev/todo/2026-10-05-restructure-no-operation-retargets-an-impl-to-another-type.md`; the delegator half of
  `docs/dev/todo/2026-10-05-restructure-no-operation-re-points-a-calls-receiver-or-writes-a-delegator.md` (its receiver half is `repoint_call`'s)
- Schema: `.agents/skills/code-restructuring/references/plan-schema.md` (dev-only)
