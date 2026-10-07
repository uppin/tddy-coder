# `repoint_call`: re-pointing a call's callee, or the receiver of every call of a method

How the operation that rewrites the part of a call **in front of** its argument list is put together.
Behaviour is in the
[feature doc](../../../docs/ft/coder/rust-code-restructuring.md#repoint_call); this page is where the
code is, how a run flows, and what its limits are.

## Shape

rust-analyzer has no assist for changing a call's callee, so — like the argument operations and the
same-crate moves — the operation is authored here and **engine-informed**: the server answers *which
places name the method* (the bulk form only), and everything else is a function of the file's text.

| Question to the server | Used for |
|---|---|
| `textDocument/references` on the anchored method's name | the bulk form's reference set: every place in the workspace that names the method |
| the document outline | lowering the bulk anchor to the zero-width range at the method's name, which is the position `textDocument/references` needs |

The **single form** asks the server nothing: it is a pure function of the text and the range. The
**bulk form** starts a server for the reference set and reads the text around each returned name
token. The arguments of every call are copied by range, never printed again, so they arrive byte for
byte. `backends/rust.rs` dispatches it: one `SUPPORTED` entry, one `check` arm that calls `findings`,
one `resolve` arm. `findings` and the single form are functions of texts, so both are unit-tested
without a server.

## `backends/rust/repoint_call/`

| Module | Decides |
|---|---|
| `repoint_call.rs` | the run: `findings` (the static refusals, from a range anchor and the text alone), and the dispatch on the lowered range — a range with width is the single form, the zero-width range an item anchor with no range lowers to is the bulk form |
| `single.rs` | the single form: `rewrite_callee` replaces everything before the argument list of the one call the range covers, keeping the arguments and the parentheses byte for byte, and refuses a callee that is the current one, an old callee that holds a call, and a method-call turbofish a field chain cannot restate |
| `sites.rs` | the bulk form's references: classify each site the server returned (a method call to re-point, a comment to skip, anything else to refuse), refuse every non-call at once before anything is written, and turn the insertion offsets into per-file edits |
| `receivers.rs` | the bulk form's receiver walk: `receiver_span` finds the maximal postfix chain ending at the `.` before the name, `insertions_for` gives the offset after the receiver, and a receiver ending in a block or holding a comment is refused |

## The single form

An `item` anchor on the function that holds the call, with a **relative range over exactly one call**
(the same anchor the four argument operations take; a v1 `range` anchor is accepted too), and
`callee` = the complete new callee — one path or method chain, no `$receiver`.

- The range must be exactly one call: `call_in` is reused unchanged, so "is not a call expression"
  and the argument-count `server_defect` are the existing refusals.
- The span replaced is everything before the argument list's `(`: the original callee, **including**
  a turbofish. The arguments, any comment between the callee and the `(`, and the `(`…`)` are
  untouched.
- The **old callee must hold no call** (paths, fields, `self`, tuple indexes, `?`, `.await` only).
  `a.m(x).n(y)` is refused naming the inner call: its arguments would be dropped silently. The *new*
  callee may hold calls — they are the author's text.
- An original method call with a turbofish (`recv.m::<T>(x)`) cannot be re-pointed by a field-chain
  callee, which has nowhere to restate `::<T>`; it is refused, naming the call. A path callee may
  carry its own turbofish.
- A callee equal to the current text re-points nothing and is refused.
- The old receiver is dropped when the new callee does not contain it (`self.dir_for(id)` →
  `lookup::dir_for(id)`): that is the request, not an error. Arguments are never added or removed —
  compose with `add_call_arg` / `remove_call_arg` in one `group`.

## The bulk form

An `item` anchor on a **method** (a `self` receiver, read from the item's text with `syn`), with
**neither** `start` nor `end` — it lowers to a zero-width range at the method's name. `callee` is a
template: `$receiver` + one or more hops + `.` + **the method's own name** (a rename is
`rename_symbol`'s). The `$receiver` must occur once, as the leftmost segment; the last segment is read
back and must equal the anchored method's name.

1. References come from `RustBackend::sites_of` at the position of the lowered anchor. The server's
   whole workspace answers, including other packages, `tests/` and `examples/`.
2. Each site is classified from the text around the name token on the **masked** text: a **method
   call** (`.name`, an optional `::<…>`, then `(`) is re-pointed; a site inside a comment is skipped
   and counted in the note; **everything else in code** (a path call `Host::name(&h, 1)`, a function
   pointer `.map(Host::name)`, a `use`, a bare call) is refused. All refusals are collected and named
   `file:line` in **one** error, and nothing is written.
3. The receiver is the maximal postfix chain ending at the `.`: paths, fields, tuple indexes, method
   calls, calls, indexing, `?`, `.await`. A receiver that ends in a block or a closure, or that holds
   a comment, is refused. The candidate `receiver.name(args)` range is handed to `call_in`: `syn` must
   read it as one `Expr::MethodCall` whose method is `name`.
4. The edit is an **insertion** of the hops (`.agent_roster()`) after each receiver's end, so
   `a.m(b.m(1))` and `a.m().m()` compose without overlapping edits. Insertions in one file are applied
   last-first, through `edits_of`.
5. No references at all is a **no-op with a note**, not an error.

## Refusal classes

At plan read (library level, `plan/codec/repoint_call_fields.rs`), every one a `plan is malformed:`:
a missing `callee`; a `callee` that is not one chain; `$receiver` in a single form, or missing,
repeated or not leftmost in a bulk form; a single anchor without a range or a bulk anchor with one; a
field the operation cannot honour (`variant`, `name`, `to`, `with_private_deps`); a bulk anchor that
does not name a method.

At `check` / `check --deep` / `apply` (text-level, before any server for the single form): a single
range that is not exactly one call (`this seam cannot be cut here:`, from `call_in`); a callee equal
to the current text; an old callee containing a call; a turbofish the callee cannot restate; a bulk
anchor whose item is not a method (`this seam cannot be cut here:`); a site that is not a method call
(`this seam cannot be cut here:`, listing every site); a receiver that is a block, a closure, or holds
a comment.

A plain `check` judges only a range anchor, so it says nothing about an item-anchored `repoint_call`
— the runner's own "anchors by item … run `check --deep`" finding covers it.

## Limits

- **Comments are skipped.** A reference inside a comment is left alone and counted in the apply note;
  a doc-link target, which rust-analyzer reports no position for, is read from the files the run
  looked at and skipped the same way.
- **A method nothing calls is a no-op with a note**, not an error: a re-run is harmless.
- **The bulk form re-points method-call syntax only.** UFCS sites, function-pointer uses and imports
  are refused, not rewritten. References in code rust-analyzer evaluates as inactive (it reports only
  the active `cfg`) and macro-generated call sites are out of reach; the compile gate names what
  results.
- **Calls and receivers only.** `self.<field>` → `state.<field>` is a field read, not a call, and
  stays the open
  [state-parameter todo](../../../docs/dev/todo/2026-09-25-restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter.md).
  This operation adds a hop to a method call's receiver; it never rewrites a field read.
- **Not the delegator, not `retarget_impl`.** No forwarding method is written, no `impl` is edited,
  the method's declaration is untouched.
- **No argument edits and no rename.** `add_call_arg` and its siblings edit arguments; a rename is
  `rename_symbol`.
- **No `check --deep` site listing.** `Rehearsal` drops `Resolution.notes`; carrying them is
  `repoint_facade`'s plumbing. The bulk form's count is a note printed by `apply`.
- **`verify` stays plan-less.** It learns a declaration, never a plan; the pairs are counted under
  `repointed`, with no new wire counter.

## `verify` accounting

`restructure verify --against <ref> --repoint OLD=NEW` (repeatable; also through the index daemon)
tells `verify` of a call re-point the author made, so it can excuse the difference one produces.
`verify/repoint.rs` holds `Repoint` and the one rule, which runs beside `retarget-impl`'s R1/R2,
between the visibility pairing and the re-point pairing:

- **R-call** pairs a lost and a gained statement 1:1 when the gained one equals the lost one with
  every occurrence of `OLD` immediately followed by `(` (or `::<`) replaced by `NEW`, outside
  strings, comments and lifetimes.

The pairs count into `Excused::repointed`; there is no new wire field. What stays reported: a hop
nobody declared, a replaced hop, a changed argument, and a different method. **Honest limit:** a plain
re-point through a lowercase module qualifier (`f(` becoming `m::f(`) is already excused by the
re-point pass with no declaration; what needs one is a hop on a receiver (`self.slot(` becoming
`self.peer.slot(`), a method chain, or a path whose new qualifier is a type. And as with a retarget,
the declaration proves only that the differences are of the shape a declared re-point produces, not
that the plan made them.
