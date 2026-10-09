# `RustBackend`'s operations become functions over a session, through a new `detach_method` - PRD

**Date**: 2026-10-09
**PRD Type**: Enhancement (one new restructure operation) + behaviour-preserving restructure (the Rust backend, by that operation)
**Stack**: `#reshape` 18/19 (`feature/reshape/backend-session`, base `feature/reshape/rust-backend-split`; real parents
`feature/reshape/rust-backend-split` and `feature/reshape/methods-leave-type`)

> **Decided 2026-10-09 (developer, PRD review):** F1 = (c), a new `detach_method` operation inside this node (the stack
> stays at 19 nodes); F3 = defer the dispatcher seam to stack 2 as a todo; F2 and F4–F11 as recommended. The changeset
> records each decision. F11 is resolved against node 17's changeset. F4 (decided 2026-10-09): node 2's `projection.rs` is
> the sixth session module, so `stage_projection` and `open_staged_projection` stay methods.

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md). Three sections change:
  - `## Rust operations (v1)`: a new operation, `detach_method`, beside `### retarget_impl`.
  - `## Verify`: a new declaration, `--detach TYPE=NAME`, and what it accounts for.
  - `## CLI`: `restructure verify` gets `--detach`.

No other feature document changes. The operation reuses the existing `name` field, so `RefactorOp` gets no new field. The
Rust backend's restructure changes no command, plan line, output line or wire message of the other operations.

## Summary

1. **`detach_method`** turns the members of one inherent `impl` block into free functions over a named parameter, and
   re-points every caller the language server knows about:
   - `fn m(&mut self, a: A) -> R` becomes `fn m(session: &mut T, a: A) -> R`, written in the same module with the same
     visibility, doc comments and attributes;
   - in the body, `self` becomes `session` and `Self` becomes `T`;
   - `x.m(a)` becomes `m(x, a)`, or `module::m(x, a)` from another module, and `Self::m(a)` becomes `m(a)`;
   - nothing is left on the type, and a block the run empties is removed.
2. **The Rust backend is restructured with it.** After this node, `RustBackend` has inherent members in only two places:
   - in `rust.rs`, its construction surface: `new`, `from_lsp_client` and the `with_*` builders (node 10's
     `with_silence_bounds` among them);
   - in the session modules `transport`, `readiness`, `documents`, `references` and node 2's `projection`, the one LSP
     session it is.

   Every operation (`resolve_opening`, `move_items`, `retarget_impl`, the assist and extraction code, and so on) is a free
   function whose first parameter is `session: &mut RustBackend`. The trait impls stay. The registry still holds a
   `Box<dyn LanguageBackend>`, and the runner still constructs a `RustBackend`.

## Background

- **Why.** A type's inherent impls must be written in the crate that defines it (E0116). Stack 2 splits
  `tddy-code-restructuring` into engine crates
  (`docs/dev/todo/2026-10-08-split-tddy-code-restructuring-into-wiring-and-engine-crates.md`). It cannot spread the Rust
  backend across crates while each operation is an `impl RustBackend` block in its own module. There are 79 such members
  in 11 blocks today, and the lower nodes of this stack add 9 more.
- **Node 6 does half the job.** `#reshape` 6 (`feature/reshape/methods-leave-type`) put self mode into `read_fields_through`
  for this node. It turns the body into `let session = self; …` so that `extract_method` writes a free function. Nothing
  then rewrites the callers, and the method stays as a forwarding wrapper:
  - `inline_method`, rust-analyzer's "inline into all callers", binds every argument that is not a local name with `let`,
    and copies `let session = self;` into each caller. There it moves `self`, so a later `self.` is E0382. It also leaves
    the emptied `impl` block behind, and it has no live test in this repository.
  - The per-site route (`repoint_call` plus `add_call_arg`) writes exact calls, but takes two plan lines per caller (about
    140) and leaves dead forwarders. No operation deletes an item.

  A forwarder left on the type is the exact edge, from the type's crate up to the operation's crate, that stack 2 has to
  cut. So an engine-only conversion needs one more operation. It reuses node 6's self-mode text rewrite, so that edge
  stays real.
- **Precedent.** One operation already has the target shape: `repoint_call`'s bulk form is
  `repoint_receivers(backend: &mut RustBackend, …)` (`backends/rust/repoint_call/sites.rs:29`).
- **What free functions do not buy on their own.** `impl LanguageBackend for RustBackend` must be written in
  `RustBackend`'s crate, and its `resolve` reaches every operation. So the crate split still needs one seam: a type split
  or a dispatch table. This node writes that todo and does not build it (changeset F3).

## Proposed Changes

### What's Changing

**`detach_method` (new operation)**

- **Plan line:**
  ```jsonl
  {"op":"detach_method","anchor":{"kind":"items","file":"…/retarget_impl.rs","items":["tddy_code_restructuring::backends::rust::RustBackend::retarget_impl","tddy_code_restructuring::backends::rust::RustBackend::retarget_range"],"fingerprints":["sha256:…","sha256:…"]},"name":"session"}
  ```
  - The anchor is an `items` anchor over a contiguous run of members of one inherent `impl` (as `restructure anchors
    --items` emits it), or one `item` anchor on one member. This is the anchor `retarget_impl` already takes.
  - `name` is the parameter the receiver becomes: one identifier, not `self`, not a keyword.
- **The function.**
  - It is written in the member's module, with its doc comments, attributes and visibility unchanged. The visibility of
    an inherent method is relative to the module of its block, so the function reaches exactly the callers the method
    did, and nothing is widened.
  - `&mut self` becomes `name: &mut T`, `&self` becomes `name: &T`, a lifetime on the receiver is kept, and an associated
    function keeps its parameters.
  - In the body, every `self` becomes `name` (node 6's self-mode rewrite), and every `Self` in the signature and body
    becomes the self type as the block's header writes it.
  - The functions follow the block in member order. A block the run empties is replaced by them.
- **The callers.** Every reference the server reports, in every file, is rewritten:
  - a method call `R.m(args)` becomes `m(R, args)`. The arguments are kept byte for byte. A receiver that is a place of
    type `T` is borrowed (`&mut R`, `&R`), and a reference is passed as it is;
  - a path call `Self::m(args)` or `T::m(args)` becomes `m(args)`, and a path used as a value becomes `m`;
  - from outside the function's module, the call is written `M::m(…)`, with `M` the module's name. A `use` of the module
    is added when the caller's file does not bind it yet. This is the house style (`item_move::findings(op, workspace)`).
  - comments are not edited, and the note counts them.
- **Refused before anything is written**, by `check --deep` and `apply`. Each refusal names the file and the line:
  - a member that takes `self` by value (the builders);
  - an `impl` with generic parameters or a `where` clause;
  - a member of a trait impl;
  - a `pub` member, which may have callers outside the workspace;
  - an associated const or type;
  - a `name` already written in the body (node 6's shadowing rule);
  - a module that already declares or imports the function's name;
  - a receiver of any other type, such as `Box<T>` or `Rc<T>` (its type is named);
  - a reference inside a macro invocation;
  - plain `check`, with no server: a missing or malformed `name`, a wrong anchor kind, or a field the operation does not
    define.
- **Run note:** `detach_method: RustBackend::retarget_impl, RustBackend::retarget_range became functions over `session`; 3 calls in 2 files re-pointed (1 `use` added); the block was emptied and removed`.

**`restructure verify --detach TYPE=NAME`**

- Declares that methods of `TYPE` became functions over `NAME`. It travels where `--retarget`, `--repoint` and node 6's
  `--rebind` travel: the library, the CLI, the daemon and the index client (`VerifyRequest` field 6).
- It implies `--rebind NAME`. With it, a signature whose receiver became `NAME: &mut TYPE` / `NAME: &TYPE` pairs with its
  original. So does a call `R.m(a)` with `m(R, a)` / `M::m(R, a)` / `m(&mut R, a)`, and `Self::m(a)` with `m(a)`. A lost
  `impl TYPE {` header is excused once for each block the run emptied.
- A call whose arguments changed, a receiver changed other than by the borrow, and a detach nobody declared are still
  reported.

**The Rust backend**

- Every inherent member of `RustBackend` outside `rust.rs` (the construction surface) and the session modules is
  detached, one plan line per block, with `name: "session"`. That is about 50 members at this node's base: 43 of today's
  (node 12 deletes `anchor_opening` and `module_outline`; node 17 puts `inference_ready_at` in `assists.rs`), plus the 7
  that nodes 1, 4, 6 and 13 add.
- The existing free function's parameter, `repoint_receivers(backend: …)`, is renamed to `session` (`rename_symbol`).
- Every move is made by `tddy-tools restructure`. A hand edit only fixes the build after an engine move, and each one gets
  a todo. A refusal stops the run, and the developer is asked.

### What's Staying the Same

- Every operation's behaviour, refusal text, report line and note.
- The trait impls on `RustBackend` (`Drop`, `LanguageBackend`, `ModuleReferences`, `ItemResolver`, `ItemAtResolver`) and
  their signatures. The registry's `Box<dyn LanguageBackend>` and its item-anchor dispatch.
- The public surface: `backends::rust::RustBackend` with `new`, `from_lsp_client` and the `with_*` builders,
  `ProgressSink`, `discard`, `WAIT_HEARTBEAT`, `client_capabilities` and `server_settings`. No file outside
  `tddy-code-restructuring` is edited, except the `--detach` carriers in `tddy-index-daemon` and `tddy-tools`.
- `RustBackend` keeps its fields. No session type is introduced (changeset F2).
- `read_fields_through`, `extract_method`, `inline_method`, `retarget_impl` and `repoint_call` are unchanged.

## Impact Analysis

### Technical Impact

- **`tddy-code-restructuring`:**
  - a new operation module `backends/rust/detach_method/` and a codec rule file;
  - `verify/detach.rs`;
  - wiring: one `RefactorKind` variant, one `SUPPORTED` entry, and one `check` arm and one `resolve` arm in the dispatcher
    (`language_backend.rs` after node 17);
  - visibility only: node 6's self-mode text functions and `retarget_impl::outline` are reused from the new module;
  - the restructure: about 20 modules under `backends/rust/` lose their `impl RustBackend` block, and about 70 call sites
    are re-pointed by the operation;
  - a new shape test, `tests/rust_backend_session_shape.rs`.
- **`tddy-index-daemon`:** `VerifyRequest.detaches = 6`, plus pass-through. **`tddy-tools`:** `index_client.rs` carries it.
- **Tests:** one new thin live binary, registered in `.config/nextest.toml` (the `rust-analyzer` group) and
  `.config/rust-e2e.filterset`. Everything else runs at library level.

### User Impact

- An author who wants a method out of its type writes one plan line, instead of node 6's two plus `inline_method`, or two
  more per caller.
- Engine developers find the Rust backend's operations as plain functions over `session`, which stack 2 can move between
  crates with the existing moves. One seam still has to be built first (todo).
- No breaking change.

## Implementation Plan

1. `detach_method` plan surface and its parse-time refusals (library).
2. The function text: receiver, `self`/`Self`, re-indentation, placement, block removal (unit-tested over text).
3. The callers: survey, classification, receiver typing, the call text, the module path and its `use` (live).
4. `verify --detach` through the library, the CLI and the daemon.
5. Register the live binary.
6. The restructure: baseline, inventory at base, one plan per module group, the parameter rename, verify, shape test green.
7. Docs and todos at wrap.

## Acceptance Criteria

- [ ] A method called from its own type, from a free function in another module and from a closure becomes a free function
  over `session`. Every caller calls it, the method is gone, and the workspace compiles with its tests and is clean
  under clippy `-D warnings`.
- [ ] `&self` gives `session: &T`. An associated function keeps its parameters, and `Self::m(…)` becomes `m(…)`. `Self` in
  a body becomes the type, and an owned receiver is borrowed at the call.
- [ ] Doc comments and attributes travel. The visibility is kept, and the function reaches every caller the method did with
  no widening. A block the run empties is removed.
- [ ] Each refusal (by-value `self`, generic `impl`, trait member, `pub` member, associated item, shadowing name, taken
  name, foreign receiver, call in a macro) names its line and writes nothing. `check --deep` reports what `apply` would
  do or refuse.
- [ ] `restructure verify --detach RustBackend=session` holds over a detach result, and without the flag it reports the
  result. The CLI and the daemon render the same lines.
- [ ] After the restructure, `RustBackend` has inherent members only in `rust.rs` (the construction surface) and in the
  five session modules. Every operation's resolver is a free function whose first parameter is `session: &mut RustBackend`.
  The five trait impls are unchanged, and no method of the type forwards to a detached function.
- [ ] `restructure verify --detach RustBackend=session --against <ref before the first plan>` holds, and the comment-line
  multiset is unchanged.
- [ ] Tests pass for `tddy-code-restructuring`, `tddy-index-daemon` and `tddy-tools`, with the failing set equal to the
  baseline's by name (scoped). CI is the authority for the rest.

## References

### Affected Features (Complete List)
- [Rust code restructuring](../rust-code-restructuring.md)

### Related Documentation
- Changeset: `docs/dev/1-WIP/2026-10-09-reshape-backend-session.md`
- Discovery: `docs/dev/1-WIP/2026-10-09-reshape-backend-session-initial-discovery.md`
- Motivation: [split `tddy-code-restructuring` into engine crates](../../../dev/todo/2026-10-08-split-tddy-code-restructuring-into-wiring-and-engine-crates.md) (stack 2)
- Package design: [`retarget-impl.md`](../../../../packages/tddy-code-restructuring/docs/retarget-impl.md), [`repoint-call.md`](../../../../packages/tddy-code-restructuring/docs/repoint-call.md)
- Plan schema: `.agents/skills/code-restructuring/references/plan-schema.md`
