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
| `rewrite.rs` | the header's self type (generics and `where` clause left as bytes), the split into up to three blocks in place, the `[chain::]Old::` → `New::` replacement, and `Layout::with_delegators`, the text of the block when delegators are left |
| `delegator.rs` | `variant: "leave_delegator"`: S7 (the members no forwarding method can be written for), the forwarding text of one member, and the delegators nothing calls |
| `imports.rs` | the one `use crate::<module>::<Name>;`, unless the file already binds the name to that path or declares the type; S6 when the file binds the name to a different item. A `use` leaf counts as binding the type when it resolves to it from the file's module (`crate::`, `self::`, `super::` or child-relative spelling), read with the shared `use`-path reading of `item_move/use_path.rs` |

## The split

The anchor lowers to a line range. `M` is the members whose attached trivia begins inside it; `P` and
`A` are the members before and after. When `M` is every member the self type alone changes; otherwise
the block becomes up to three blocks **in place, in source order**: `[Old: P] [New: M] [Old: A]`, an
empty piece omitted. Cuts fall on member boundaries, so the free-standing comments and blank lines
before `M` stay with `P`, and the comments attached to `M`'s first member travel with it. The whole new
text is handed to `seam_survey::minimal_edits`, so the ledger sees insertions at the two cuts rather
than one rewrite over the block.

## The forwarding delegator

`variant: "leave_delegator"` with `expr` moves the members as above and leaves a forwarding method in
each moved method's slot of the old block, so callers keep compiling. `expr` reaches the new type from the
old type's `self` (`self.roster`, `self.roster()`). The two go together: `variant` without `expr`, and
`expr` without `variant`, are refused as `plan is malformed:` (P9), by a plain `check`.

**Placement.** The old block keeps its header and every member that stays. Each moved member's byte range,
its attached comments and attributes included, is replaced **in place** by its delegator, and
`impl <New> { <the moved members' bytes> }` is inserted after the old block's closing `}`, separated by one
blank line. A whole-block retarget gives `impl Old { <delegators> }` followed by `impl New { <members> }`.
The `Old::` → `New::` path re-points inside the moved members are as without the variant.

```rust
impl Host {                                   impl Host {
    pub fn get(&self) -> u32 { self.n }           pub fn get(&self) -> u32 {
    /// Replace the count.                            self.roster.get()
    #[inline]                                     }
    pub fn put(&mut self, v: u32) { … }           #[inline]
    pub fn size(&self) -> u32 { … }               pub fn put(&mut self, v: u32) {
}                                                     self.roster.put(v)
                                                  }
                                                  pub fn size(&self) -> u32 { … }
                                              }

                                              impl Roster {
                                                  pub fn get(&self) -> u32 { self.n }
                                                  /// Replace the count.
                                                  #[inline]
                                                  pub fn put(&mut self, v: u32) { … }
                                              }
```

**A delegator** is written like this:

- its outer attributes are copied byte for byte, except doc comments, which stay on the moved member;
- its signature is copied byte for byte, from the visibility (or first qualifier) to the body's opening `{`;
- its body is on its own line, four spaces in from the member, and is `<expr>.<name>(<argument names>)`,
  with `.await` appended for an `async fn`;
- a member with no receiver forwards as `<New>::<name>(<argument names>)`, whatever `expr` is;
- the argument names are the parameter names read with `syn`, in order.

**Refused (S7)**, as `this seam cannot be cut here:`, by `check --deep` and `apply`, after S4 and before
any edit, in one refusal naming every member: a parameter written as a pattern (`(a, b): (u32, u32)`), an
associated const or type (no body to forward), a `const fn` (cannot forward to a non-const call). A member
whose text cannot be read as an `impl` member is refused too.

**A delegator nothing calls is noted.** The reference set the run already requests for every moved member,
in every file, minus the references inside the moved range, gives each member's outside callers. A member
with none still gets its delegator, and the run notes
``retarget_impl: the delegator `Host::put` has no caller in the workspace: remove it, or retarget without leave_delegator``.
No extra request is made. A `pub` delegator may have callers outside the workspace, which is why it is
written and noted rather than left out.

## Limits

- **Inherent impls only.** A trait impl is refused at parse time (P5): its members cannot change type
  without breaking the trait's contract.
- **Same crate only.** A `to_type` in another package is refused (P7). A move between crates is
  `move_module_to_crate`.
- **Callers are not re-pointed.** An outside caller of a moved member breaks (`E0599`) unless a later
  operation (`repoint_call`) re-points it in the same `group`, or the plan leaves a delegator (below). The compile gate names it, and it is
  the documented outcome, not a defect.
- **Fields are refused, not rewritten.** A moved member reads the new type's own fields. Reading a
  state value's fields through a local is [`read_fields_through`](read-fields-through.md).
- **A bare `Old` in type position** inside a moved member is left as written; only `Old::<moved
  member>` paths are re-pointed.
- **The block stays in its file.** To put the new block in the new type's module, run `move_item` over
  `<New>` afterwards.

## `verify` accounting

`restructure verify --against <ref> --retarget OLD=NEW` (repeatable) tells `verify` of a retarget the
author made, so it can excuse the differences one produces. `verify/retarget.rs` holds `Declared` and
the three rules, which run between visibility pairing and re-point pairing:

- **R1, rename pairing** pairs a lost and a gained statement equal once every whole-identifier `OLD`
  becomes `NEW` (`OLD::build(…)` with `NEW::build(…)`).
- **R2, header accounting** excuses the header a split repeats — `impl<T> Host<T>`, its `where` and
  its predicate lines, read out of the ref's own statements — once for the old block and once for the
  new.

- **R3, delegator accounting** excuses what `leave_delegator` leaves, for each declared retarget: a
  gained `fn` signature line (it ends `{` and holds `fn `) equal to a statement the ref already has, once
  per such ref statement; and a gained forwarding statement `<anything>.<m>(<p1, …, pn>)`, its `….await`
  form, or `NEW::<m>(…)`, when `m` and the parameter names are exactly those of an excused signature.

All three count into `Excused::repointed`; there is no new wire field. What stays reported: a rename that
was not declared, a changed argument, a forwarding method whose arguments differ from the moved
signature, any lost statement or comment. **Honest limit:** `verify` proves
"the differences are only of the shape a declared retarget produces", not that the plan did them — a
hand edit that renames `OLD` to `NEW`, or writes a forwarding method, is excused too, because the author
declared it. No separate declaration is needed for the delegator: `--retarget` already names it.
