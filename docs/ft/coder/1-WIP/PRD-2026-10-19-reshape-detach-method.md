# `detach_method` turns an inherent impl's members into free functions and re-points their callers - PRD

**Date**: 2026-10-19
**PRD Type**: Enhancement (one new Rust restructure operation)
**Stack**: `#reshape` 15/24 (`feature/reshape/detach-method`, base `feature/reshape/move-widen`; no open real parent —
wave 1; `backend-fn-sizes` waits on it because it adds backend functions)

> Carved out of the closed `#reshape` 18/19 (#615, `backend-session`). That node also applied the operation to
> `RustBackend` itself ("operations become session functions") for the stack-2 crate split; that half is dropped —
> the language-support interface (K11) replaced it. The operation, its refusals and `verify --detach` are kept as
> approved on 2026-10-09.

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md):
  - `## Rust operations (v1)`: new `detach_method`, beside `### retarget_impl`;
  - `## Verify`: new declaration `--detach TYPE=NAME`;
  - `## CLI`: `restructure verify --detach`.

## Summary

**`detach_method`** turns the members of one inherent `impl` block into free functions over a named parameter, and
re-points every caller the language server knows about:
- `fn m(&mut self, a: A) -> R` becomes `fn m(session: &mut T, a: A) -> R` in the same module, same visibility, doc
  comments and attributes;
- in the body `self` becomes `session` and `Self` becomes `T`;
- `x.m(a)` becomes `m(x, a)` (or `module::m(x, a)` from another module, adding a `use` of the module when missing),
  `Self::m(a)` becomes `m(a)`;
- nothing is left on the type; a block the run empties is removed.

## Background

- `inline_method` copies `let session = self;` into every caller (E0382 on a later `self.`) and leaves the emptied
  block; the per-site route (`repoint_call` + `add_call_arg`) costs two plan lines per caller and leaves dead
  forwarders. No operation converts a method to a function with its callers in one step.
- Node 6 (`methods-leave-type`, merged) built the self-mode text rewrite this operation reuses.

## Proposed Changes

### What's Changing

- **Plan line**: `{"op":"detach_method","anchor":{"kind":"items",…},"name":"session"}` — an `items` anchor over a
  contiguous run of members of one inherent `impl` (as `restructure anchors --items` emits), or an `item` anchor on one
  member; `name` is one identifier, not `self`, not a keyword.
- **The function**: written in the member's module; receiver `&mut self` → `name: &mut T`, `&self` → `name: &T`
  (lifetimes kept); associated functions keep their parameters; `self` → `name`, `Self` → the header's self type;
  functions follow the block in member order; an emptied block is replaced by them.
- **The callers**: every server-reported reference is rewritten: `R.m(args)` → `m(R, args)` with arguments byte for
  byte and a place receiver borrowed (`&mut R` / `&R`); `Self::m` / `T::m` paths → `m`; from another module
  `M::m(…)` plus a `use` when the file does not bind `M`; comments are not edited.
- **Refused before anything is written** (`check --deep`, `apply`), each naming file and line: a by-value `self`
  member; an `impl` with generics or `where`; a trait-impl member; a `pub` member; an associated const or type; `name`
  already written in the body; a module that already declares or imports the function's name; a receiver of another
  type (`Box<T>`, `Rc<T>`); a reference inside a macro invocation. Plain `check` (no server): a missing or malformed
  `name`, a wrong anchor kind, an unknown field.
- **Run note**: `detach_method: T::a, T::b became functions over `session`; 3 calls in 2 files re-pointed (1 `use`
  added); the block was emptied and removed`.
- **`restructure verify --detach TYPE=NAME`**: carried by the library, CLI, daemon and index client
  (`VerifyRequest` field 6); implies `--rebind NAME`; pairs a receiver that became `NAME: &mut TYPE`/`&TYPE` with its
  original, a call `R.m(a)` with `m(R, a)` / `M::m(R, a)` / `m(&mut R, a)`, `Self::m(a)` with `m(a)`; excuses one lost
  `impl TYPE {` header per emptied block.

### What's Staying the Same

- `RustBackend` itself is **not** detached in this node (dropped with #615).
- `read_fields_through`, `extract_method`, `inline_method`, `retarget_impl`, `repoint_call` are unchanged.

## Impact Analysis

- Packages: `tddy-code-restructuring` (operation, verify), `tddy-index-daemon` and `tddy-tools` (the `--detach` carrier).
- `SUPPORTED` grows by one (`RefactorKind::DetachMethod`); the op is a `RustSupport` operation only.
- Test cost: one new live (rust-analyzer) suite — capped at ~2 live tests under the 2026-10-17 rebalance rule, the
  rest at library level against recorded LSP answers.

## Acceptance Criteria

- [ ] A two-member block detaches into two functions, the block is removed, and calls in two files (one in another
      module) are re-pointed; the result compiles (`assert_compiles_with_its_tests`).
- [ ] Each refusal in the list is reported by `check --deep` with file and line, and nothing is written.
- [ ] `restructure verify --detach T=session` holds for that run, and fails when a call's arguments changed.
- [ ] Plain `check` reports a missing `name` and a non-items anchor without a server.

## Open questions

- **F1 — node scope.** The 2026-10-09 approval bundled the operation with its use on `RustBackend`. (a) Operation +
  verify only (recommended; the self-application served the dropped stack-2 split); (b) also detach `RustBackend`'s
  operation blocks here.

## References

- Closed #615 PRD (`feature/reshape/backend-session`, `docs/ft/coder/1-WIP/PRD-2026-10-09-reshape-backend-session.md`).
- `docs/dev/todo/2026-10-17-restructuring-acceptance-suite-pays-a-rust-analyzer-start-per-test.md` (live-test cap).
