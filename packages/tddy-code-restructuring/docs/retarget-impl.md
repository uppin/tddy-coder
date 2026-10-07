# `retarget_impl`: moving an `impl`'s members to another type of the same crate

How the operation that rewrites an inherent `impl`'s self type is put together. Behaviour is in the
[feature doc](../../../docs/ft/coder/rust-code-restructuring.md#retarget_impl); this page is where the
code is, how a run flows, and what its limits are.

## Shape

rust-analyzer has no assist for changing an `impl`'s self type, so — like the same-crate moves — the
operation is authored here and **engine-informed**: the server answers where the block and its members
are and who names them, and everything else is a function of the file's text.

| Question to the server | Used for |
|---|---|
| the document outline of the file | the `impl` block (kind 19) the anchor lands in, and its member children |
| `textDocument/references` on each moved member's name | the `Old::` paths inside the moved members that name a moved member, which get re-pointed |

The moved bytes are copied by range, never printed again, so comments, attributes and formatting
arrive as they were; the only text authored is the header's self type, the repeated `impl` headers and
the one `use`. `backends/rust.rs` dispatches it: one `SUPPORTED` entry, one `check` arm that calls
`findings`, one `resolve` arm. The assembly is a function of texts, so it is unit-tested without a
server.

## `backends/rust/retarget_impl/`

| Module | Decides |
|---|---|
| `retarget_impl.rs` | the run: the static findings, the outline, the field refusal, the layout, the re-points, the `use`, then one `FileEdit` turned into minimal edits |
| `preflight.rs` | the two static findings, before any server: P7 (the `to_type` is in another package) and P8 (its module declares no such type), both read from the manifests and the module's text |
| `outline.rs` | the `impl` the anchor lands in (the one it overlaps), its members, and the run the anchor moves (each member's attached trivia included). Refuses S1 (no `impl`, or members in more than one), S2 (a member cut in half) and carries the self type |
| `fields.rs` | S4/S5: the fields a moved member reads through `self` or names in a `Self { … }`, read with `syn`, against the fields the new type declares. A declaration the file does not write plainly as a struct is refused only when a field is read |
| `rewrite.rs` | the header's self type (generics and `where` clause left as bytes), the split into up to three blocks in place, and the `[chain::]Old::` → `New::` replacement |
| `imports.rs` | the one `use crate::<module>::<Name>;`, unless the file already binds the name to that path or declares the type; S6 when the file binds the name to something else |

## The split

The anchor lowers to a line range. `M` is the members whose attached trivia begins inside it; `P` and
`A` are the members before and after. When `M` is every member the self type alone changes; otherwise
the block becomes up to three blocks **in place, in source order**: `[Old: P] [New: M] [Old: A]`, an
empty piece omitted. Cuts fall on member boundaries, so the free-standing comments and blank lines
before `M` stay with `P`, and the comments attached to `M`'s first member travel with it. The whole new
text is handed to `seam_survey::minimal_edits`, so the ledger sees insertions at the two cuts rather
than one rewrite over the block.

## Limits

- **Inherent impls only.** A trait impl is refused at parse time (P5): its members cannot change type
  without breaking the trait's contract.
- **Same crate only.** A `to_type` in another package is refused (P7). A move between crates is
  `move_module_to_crate`.
- **Callers are not re-pointed.** An outside caller of a moved member breaks (`E0599`) unless a later
  operation (`repoint_call`) re-points it in the same `group`. The compile gate names it, and it is
  the documented outcome, not a defect.
- **Fields are refused, not rewritten.** `self.f` → `state.f` is the open `…state-parameter` todo.
- **A bare `Old` in type position** inside a moved member is left as written; only `Old::<moved
  member>` paths are re-pointed.
- **The forwarding delegator is deferred.** `variant: "leave_delegator"` and `expr` are named by the
  schema and refused by the engine (`UnsupportedOp`); the emitter and its `verify` accounting are a
  follow-up. See
  [the delegator todo](../../../docs/dev/todo/2026-10-05-restructure-no-operation-re-points-a-calls-receiver-or-writes-a-delegator.md).
- **The block stays in its file.** To put the new block in the new type's module, run `move_item` over
  `<New>` afterwards.

## `verify` accounting

`restructure verify --against <ref> --retarget OLD=NEW` (repeatable) tells `verify` of a retarget the
author made, so it can excuse the differences one produces. `verify/retarget.rs` holds `Declared` and
the two rules, which run between visibility pairing and re-point pairing:

- **R1, rename pairing** pairs a lost and a gained statement equal once every whole-identifier `OLD`
  becomes `NEW` (`OLD::build(…)` with `NEW::build(…)`).
- **R2, header accounting** excuses the header a split repeats — `impl<T> Host<T>`, its `where` and
  its predicate lines, read out of the ref's own statements — once for the old block and once for the
  new.

Both count into `Excused::repointed`; there is no new wire field. What stays reported: a rename that
was not declared, a changed argument, any lost statement or comment. **Honest limit:** `verify` proves
"the differences are only of the shape a declared retarget produces", not that the plan did them — a
hand edit that renames `OLD` to `NEW` is excused too, because the author declared it.
