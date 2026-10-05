# `repoint_call`: re-point a call's callee, or the receiver of every call of a method - PRD

**Date**: 2026-10-05
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — `## Signature and call-site operations` (a new call-site operation), `## Rust operations (v1)` (the operation table), `## Verify` (a declared re-point is accounted for).

No other feature document changes. `restructure verify` gains one repeatable flag, `--repoint OLD=NEW`, beside the `--retarget` flag the `retarget_impl` PRD adds.

## Summary

A restructure plan can rewrite the part of a call that precedes its arguments. `repoint_call` replaces one call's callee (`self.slot(x)` -> `self.peer.slot(x)`, `slot(x)` -> `lookup::slot(x)`), or,
named on a method, inserts hops after the receiver of every call of that method (`host.slot(1)` -> `host.peer.slot(1)`). Arguments are never touched. `verify` accounts for the rewrite when told what it was.

## Background

After a method moves to another type its callers change by a few tokens. The existing call-site operations edit argument lists only, so on `#carve` 17/21 twenty-eight call sites and seven wrappers were rewritten by hand. The engine
already knows every reference to a method and already reads one call out of a range; what was missing is an operation that writes the callee.

## Proposed Changes

### What's Changing

- New operation `repoint_call`, anchored like the argument operations (single form: item anchor, relative range over exactly one call, field `callee` = one path or method chain) or on a method (bulk form: item anchor, no range, `callee` = `$receiver` + hops + the method's own name).
- Refusals, before anything is written: a callee that is not one chain, a range that is not one call, an old callee that contains a call (its arguments would be lost), a method-call turbofish a field-chain callee cannot restate, a callee equal to the current one, a bulk anchor that is not a method, and — listed all at once — every reference the bulk form cannot re-point (a path call, a function pointer, an import).
- References inside comments are left alone. A method with no reference is a no-op with a note.
- `restructure verify --repoint OLD=NEW` (repeatable; also through the index daemon) pairs each statement that differs only by the declared callee text. Without it `verify` behaves as before.

### What's Staying the Same

- The four argument operations, `rename_symbol`, the moves, `retarget_impl`. **`self.<field>` -> `state.<field>` stays a separate, open capability** (developer decision: calls and receivers only).
- No argument is added or removed, no method is renamed, no forwarding method is written.
- The wire shape of `verify` results (`repointed` counts the new pairs).

## Impact Analysis

### Technical Impact
- `tddy-code-restructuring`: a new operation module, one plan field, codec rules, wiring in `rust.rs`; `verify` extended through the carrier `retarget_impl` introduces.
- `tddy-index-daemon`, `tddy-tools`: one request field and its plumbing. A new live test binary joins the e2e filterset and the rust-analyzer test group.

### User Impact
- A moved method's callers are re-pointed by one plan line instead of by hand; `check --deep` reports every refusal first. No breaking change.

## Implementation Plan

1. Plan surface and codec (library tests). 2. Single form (text-level). 3. Bulk form over the server's references (live fixtures, `cargo check`). 4. Register the live binary. 5. `verify --repoint`. 6. Docs at wrap.

## Acceptance Criteria

- [ ] a call's callee is replaced and its arguments are byte-identical ([Rust code restructuring](../rust-code-restructuring.md))
- [ ] every method call of a method gets its receiver re-pointed across files and crates, and the tree compiles
- [ ] every refusal above is reported before any write, by `check` where the text allows and by `check --deep` otherwise
- [ ] `verify --repoint` holds for exactly the declared texts and still reports an undeclared hop or a changed argument
- [ ] the live binary is registered in both CI configuration files
- [ ] tests pass for `tddy-code-restructuring`, `tddy-index-daemon` and `tddy-tools` (scoped; CI for the rest)

## References

### Affected Features (Complete List)
- [Rust code restructuring](../rust-code-restructuring.md)

### Related Documentation
- Changeset: `docs/dev/1-WIP/2026-10-05-sharpen-repoint-call.md`
- Todos: `docs/dev/todo/2026-10-05-restructure-no-operation-re-points-a-calls-receiver-or-writes-a-delegator.md` (exists only on `feature/carve/lifecycle-ports-agents`, PR #532; deleted by `retarget-impl`'s wrap); `docs/dev/todo/2026-09-25-restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter.md` (stays open)
- Schema: `.agents/skills/code-restructuring/references/plan-schema.md` (dev-only)
