# Method bodies leave a type that stays: `read_fields_through` and `retarget_impl`'s forwarding delegator - PRD

**Date**: 2026-10-09
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md):
  - `## Rust operations (v1)` gets a new operation, `read_fields_through`, with its own section next
    to `### retarget_impl`.
  - `### retarget_impl`: the forwarding delegator (`variant: "leave_delegator"` with `expr`) changes
    from "named by the schema and refused" to implemented.
  - `## Verify`: two new shapes are accounted for (a rebound `self`, and a forwarding delegator), plus
    one new declaration.
  - `## CLI`: `restructure verify` gets `--rebind`.

No other feature document changes. The new operation reuses existing plan fields (`name`, `expr`,
`variant`), so `RefactorOp` gets no new field.

## Summary

Two engine capabilities let a method's body leave a type whose `impl` cannot move, with no hand edits:

1. **`read_fields_through`** is a new operation anchored on a range inside a method. It inserts
   `let <name> = <expr>;` before the range and rewrites every `self` in the range to `<name>`. The
   range can then be extracted as a free function and moved to another crate. It has two modes:
   - **field mode**: `<expr>` is a borrowed view of the host's fields, `self.agent_roster_state()`.
     Only field reads are rewritten, every field is checked against the view's type, and a `&` that
     would become a `needless_borrow` is dropped.
   - **self mode**: `<expr>` is `self`. Method calls are rebound too.
2. **`retarget_impl` with `variant: "leave_delegator"`** moves the members as `retarget_impl` already
   does, and leaves one forwarding method per moved method on the old type, so callers keep compiling.
   The run notes every delegator that has no caller left.

`restructure verify` accounts for both, on a declaration the author makes.

## Background

- **State-parameter entry (2026-09-25).** A method of a type that cannot leave its crate
  (`impl DaemonSessionHost`, `E0116` anywhere else) can only have its body moved once the body stops
  naming `self`. In the `#carve` 15 T3 port-move pilot, every `self.<field>` became `state.<field>`
  by hand, token by token, and was counted as a gray-zone edit, for every method moved.
- **Delegator entry (2026-10-05).** `#carve` 17 hand-wrote 7 forwarding delegators. The compiler's
  `dead_code` warning was the only thing that found 2 wrappers left with no caller.
- **What already landed.** `repoint_call` (#594) closed the receiver half of the delegator entry.
  `retarget_impl` (#593) ships the delegator's schema and refuses it (`UnsupportedOp`). The `#sharpen`
  6/8 design for the delegator (P9, S7, R3) was cut because of module size, not because of a design
  problem. This PRD takes that design over and fills the gap it left: where delegators go when the
  block is split.
- **Downstream need.** `#reshape` 18/19 (`feature/reshape/backend-session`) has to turn
  `RustBackend` operations into free functions so the backend can later span crates. Those bodies are
  almost all `self.<method>(…)` calls (for example, 6 of the 6 `self` uses in `retarget_impl.rs`). An
  operation limited to field reads, as the 2026-09-25 entry specified it, would refuse them, so this
  PRD adds **self mode**.

## Proposed Changes

### What's Changing

**`read_fields_through` (new operation)**

- **Plan line:**
  ```jsonl
  {"op":"read_fields_through","anchor":{"kind":"range","file":"…/svc.rs","start":{…},"end":{…}},"name":"state","expr":"self.agent_roster_state()"}
  {"op":"read_fields_through","anchor":{…},"name":"backend","expr":"self"}
  ```
  - The anchor is a `range`, or an `item` anchor with a relative range.
  - `name` is the new binding: one identifier, not `self` and not a keyword.
  - `expr` is one expression, checked by the existing `one_expr` rule. When it is exactly `self`,
    `&*self` or `&mut *self`, the operation runs in **self mode**. Any other expression is
    **field mode**.
- **Edit:** the engine inserts `let <name> = <expr>;` at the range's start, at the line's indentation.
  - **Field mode:** every `self.<field>` in the range is written `<name>.<field>`. `&self.<field>`
    becomes `<name>.<field>` (the `&` is dropped) when the view's field is a reference to the host
    field's type, and stays `&<name>.<field>` when the two have the same type (a `Copy` value).
  - **Self mode:** every `self` in the range is written `<name>`, method receivers included, and
    every `Self` is written as the enclosing impl's self type.
  - Comments and strings are never edited.
- **Refused before anything is written**, by `check --deep` and `apply`. Each refusal names the file
  and the line:
  - the range does not start at a statement boundary;
  - the range lies in no method with a `self` receiver;
  - the range names no `self`, so there is nothing to rebind;
  - `name` is already written somewhere in the method's body, so the new binding would shadow it;
  - field mode only:
    - a `self.<method>(…)`, or a bare `self`, in the range (each one listed);
    - a field the view does not have (every one listed);
    - a field whose type differs from the host's by more than one reference (both types named). A
      same-typed field is accepted, so a clone and a copy are not told apart;
  - plain `check`, with no server: a missing `name` or `expr`, a wrong anchor kind, or a field this
    operation does not define (`to`, `to_type`, `variant`, `callee`, …).
- **Run note:** `rebound N reads of self through `<name>`: fields a, b, c`, or
  `… : every self` in self mode. `apply` and `check --deep` both print it.
- The borrow checker and `Send` across an `.await` are left to the compile gate, which is the one
  thing a lexical check cannot see. A failure there is the documented outcome.

**`retarget_impl` with `variant: "leave_delegator"` (implemented)**

- `expr` reaches the new type from the old type's `self`, for example `self.roster()`. P9 is now
  enforced at parse time: `variant` without `expr`, or `expr` without `variant`, is a malformed plan.
- **Placement:** each moved method's slot in the old block holds its delegator, and `impl New { … }`
  with the moved members follows the old block. A whole-block move leaves `impl Old { <delegators> }`
  followed by `impl New { <members> }`. The members, their doc comments and their attributes move as
  bytes, as today.
- **A delegator** is written like this:
  - its signature is copied byte for byte, from the visibility to the body's `{`;
  - the outer attributes are copied, except doc comments;
  - the body is `<expr>.<name>(<argument names>)`, with `.await` added for an `async fn`;
  - an associated function with no receiver forwards as `New::<name>(<argument names>)`.
- **Refused (S7), before anything is written:** a parameter written as a pattern, an associated const
  or type, or a `const fn`. None of these can be forwarded.
- **Dead delegators are noted.** When the server reports no reference to a moved member outside the
  moved members, the run notes
  `the delegator `Old::m` has no caller in the workspace: remove it, or retarget without
  leave_delegator`. That reference set is the one `retarget_impl` already requests, so the note costs
  no extra request.
- `retarget_impl` without the variant behaves exactly as today.

**`restructure verify`**

- `--rebind NAME` (repeatable) declares that `self` was rebound to `NAME`. The declaration travels
  wherever `--retarget` and `--repoint` already do: the library, the CLI, the daemon and the index
  client (`VerifyRequest` field 5).
  - With it, a statement whose only change is the whole-identifier `self` → `NAME` pairs with its
    original.
  - With it, one gained `let NAME = …;` is excused per declaration.
- With a declared `--retarget OLD=NEW`, rule **R3** pairs each delegator:
  - the duplicated signature (a gained `fn` line equal to one the ref already has) is excused;
  - the forwarding statement (`<recv>.<m>(<that signature's parameter names>)`, its `.await` form, or
    `NEW::<m>(…)`) is excused.
  No new declaration is needed for this.
- Every pair counts in `repointed`. No response field is added.
- **Honest limit, written in the docs:** `verify` proves that the differences have the shape the
  declarations produce. It does not prove that the plan produced them.

### What's Staying the Same

- `retarget_impl` without `leave_delegator` keeps all of its current behaviour: the split, the path
  re-points, S1-S6.
- `repoint_call` is unchanged. It is still how outside callers move to the new type when the author
  does not want a delegator.
- `extract_method`, `extract_module` and the crate moves are unchanged. `read_fields_through` only
  prepares a range for them.
- The tidy's existing `warning remains: … is never used` line, which already reports a private
  wrapper that lost its last caller in a touched file. No new generic dead-wrapper survey is added.
- No new `RefactorOp` field. `verify` without the new declarations reports exactly what it does today.

## Impact Analysis

### Technical Impact

- **`tddy-code-restructuring`:**
  - a new operation module `backends/rust/read_fields_through/` and a new codec rule file;
  - a `delegator` child of `backends/rust/retarget_impl/`;
  - R3 and the rebind rule in `verify/`;
  - wiring: one variant in `RefactorKind`, `SUPPORTED` 25 → 26, one `check` arm and one `resolve` arm
    in `backends/rust.rs`.
  - Its first use of hover *contents*: hover is already requested, but only checked for null.
  - `retarget_impl()` does not grow. It is on `#reshape` 19's list of functions over 60 lines.
- **`tddy-index-daemon`:** `VerifyRequest.rebinds = 5`, plus pass-through in `cli.rs` and `queries.rs`.
- **`tddy-tools`:** `index_client.rs` carries `rebinds`.
- **Tests:** two new thin live binaries, registered in `.config/nextest.toml` (the `rust-analyzer`
  group) and `.config/rust-e2e.filterset`. Everything else runs at library level, with no server.

### User Impact

- In a T3 port-move, the token-for-token hand substitutions become one plan line per method.
- Hand-written forwarding delegators become one variant on the `retarget_impl` line, and dead ones are
  named as part of the run.
- Breaking change: a plan with `variant: "leave_delegator"` used to be refused and is now honoured.
  That plan could not have been applied before, so nothing that worked changes.

## Implementation Plan

1. `read_fields_through` plan surface and parse-time refusals (library).
2. Range rules: statement boundary, shadowing, `self` sites from masked code (library, unit-tested
   over text).
3. Field mode: a trial text shown to the server, unresolved fields, hover types, the `&` rule (live).
4. Self mode (live).
5. Delegator: P9, S7, the forwarding text (unit-tested), placement in the old block, the dead-delegator
   note (live).
6. `verify`: R3 and `--rebind` through the library, the CLI and the daemon.
7. Register the two live binaries. Docs and the two todo entries at wrap.

## Acceptance Criteria

- [ ] A method of `Host` whose range reads two host fields through `state` is rewritten by
  `read_fields_through`. The range then names no `self`, and the workspace compiles with its tests.
- [ ] The view type lives in **another crate** (the T3 shape), and the operation still applies and
  compiles.
- [ ] Where the view's field is a reference, `&self.f` becomes `state.f`. Where the types are the
  same, it stays `&state.f`. The workspace is clean under `cargo clippy -D warnings`.
- [ ] In field mode, a `self.method()` in the range is refused naming its line, and so is a field the
  view lacks, a type mismatch, and a binding that would shadow an existing name. Nothing is written,
  and `check --deep` reports the same refusal `apply` gives.
- [ ] In self mode, `expr: "self"` rebinds method calls. An `extract_method` that follows in the same
  plan writes a free function, and the workspace compiles.
- [ ] `retarget_impl` with `leave_delegator` keeps callers, outside the moved members and in another
  file, **byte-identical and compiling**. This covers an `async` method (forwarded with `.await`) and
  an associated function (forwarded through `New::`), for a whole block and for a proper subset.
- [ ] A delegator has the original signature and outer attributes, and no doc comment. The moved
  member keeps its doc comment.
- [ ] A pattern parameter, an associated const and a `const fn` are each refused (S7). A `variant`
  without `expr`, and an `expr` without `variant`, are malformed at plain `check` (P9).
- [ ] A delegator with no caller left is noted by both `check --deep` and `apply`.
- [ ] `restructure verify --rebind state` holds over a `read_fields_through` result, and without the
  flag it reports the result. `verify --retarget Host=Roster` holds over a delegator result, and a
  forwarding method that does not match a moved signature is still reported. The CLI and the daemon
  render the same lines.
- [ ] `retarget_impl` without the variant, and every existing `verify` case, are unchanged.
- [ ] Tests pass for `tddy-code-restructuring`, `tddy-index-daemon` and `tddy-tools` (scoped). CI is
  the authority for the rest.

## References

### Affected Features (Complete List)
- [Rust code restructuring](../rust-code-restructuring.md)

### Related Documentation
- Changeset: `docs/dev/1-WIP/2026-10-09-reshape-methods-leave-type.md`
- Discovery: `docs/dev/1-WIP/2026-10-09-reshape-methods-leave-type-initial-discovery.md`
- Todo entries this resolves:
  [state-parameter](../../../dev/todo/2026-09-25-restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter.md),
  [delegator](../../../dev/todo/2026-10-05-restructure-no-operation-re-points-a-calls-receiver-or-writes-a-delegator.md)
- Package design: [`retarget-impl.md`](../../../../packages/tddy-code-restructuring/docs/retarget-impl.md),
  [`repoint-call.md`](../../../../packages/tddy-code-restructuring/docs/repoint-call.md)
- Plan schema: `.agents/skills/code-restructuring/references/plan-schema.md`
- Successor that reuses the self-mode body rewrite and the shadowing refusal (from its own
  `detach_method`, with no forwarding wrapper): `feature/reshape/backend-session` (`#reshape` 18/19)
