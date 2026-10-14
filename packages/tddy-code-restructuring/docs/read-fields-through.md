# `read_fields_through`: reading a method's `self` through a local

How the operation that rebinds the `self` a range of a method reads is put together. Behaviour is in the
[feature doc](../../../docs/ft/coder/rust-code-restructuring.md#read_fields_through); this page is where
the code is, how a run flows, the refusals, and what its limits are.

A method of a type that cannot leave its crate can have a range of its body moved only when the range
names no `self`: rust-analyzer's "Extract into function" writes a free function for such a range, and a
method of the same `impl` otherwise. The operation inserts `let <name> = <expr>;` before the range and
writes `<name>` wherever the range read `self`, so a following `extract_method` produces a free function
that takes `<name>`. It writes no state type, no builder method and no function, and moves nothing.

## Plan line

```jsonl
{"op":"read_fields_through","anchor":{"kind":"range","file":"src/svc.rs","start":{"line":375,"col":9},"end":{"line":392,"col":15}},"name":"state","expr":"self.agent_roster_state()"}
{"op":"read_fields_through","anchor":{"kind":"range","file":"src/backend.rs","start":{"line":9,"col":9},"end":{"line":69,"col":10}},"name":"backend","expr":"self"}
```

| Field | Meaning |
|---|---|
| `anchor` | A `range`, or an `item` anchor **with** a relative range (lowered to a range, as for `extract_method`). A `symbol` anchor, an `items` anchor, or an `item` anchor with no range is refused |
| `name` | The new binding: one identifier, not `self`, not a keyword |
| `expr` | One expression, written verbatim as the `let`'s initialiser. `self`, `&*self` or `&mut *self` (compared as tokens) select **self mode**; any other expression selects **field mode** |
| `id`, `group` | As on every operation. The operation composes with `extract_method` in one `group` |

The operation defines no other field. `to`, `to_type`, `variant`, `callee`, `type`, `order`, `reexport`,
`also`, `to_file`, `canonical_paths` and `with_private_deps` are refused, and `RefactorOp` carries no field
for it.

## Modes

**Field mode** (`expr` is a borrowed view of the host's fields, `self.agent_roster_state()`). Only
`self.<field>` reads are rewritten, each to `<name>.<field>`. Every field is checked against the view's
type on a trial text (below). A `&` or `&mut ` written directly before a read is dropped when the view's
field is a reference to the host field's type, so clippy's `needless_borrow` does not fire; it stays when
both types are the same (a `Copy` value held by both). A method call on `self`, or a bare `self`, in the
range is refused.

**Self mode** (`expr` is `self`, `&*self` or `&mut *self`). Every `self` in the range becomes `<name>`,
method receivers included, and every `Self` becomes the enclosing `impl`'s self type as its header writes
it, generics included. The following `extract_method` then writes a free function whose parameter is
`<name>`.

In both modes a comment, a string and everything outside the range are byte-identical.

## `backends/rust/read_fields_through/`

| Module | Decides |
|---|---|
| `read_fields_through.rs` | the run: the plan line read against the file's text, the static findings, the outline, then field mode or self mode, then one `FileEdit` of minimal edits. Declared `pub(crate)` |
| `range.rs` | the lexical half, over text with comments and strings masked: the mode `expr` selects, every `self` and `Self` site of the range and how each is used (a field, a method call, a bare `self`, the type), and the refusals RS1-RS4. `pub(crate)` |
| `method.rs` | the outline's view of the method around the range: RS5, and the enclosing `impl`'s self type |
| `typing.rs` | the type a hover states for a field, and whether the view's type agrees with the host's (RS7) |
| `edits.rs` | the `let` and the substitutions: `rebound` builds the rewritten text, `rewrites` the spans. `pub(crate)` |

`backends/rust.rs` dispatches it: one `SUPPORTED` entry, one `check` arm that calls `findings`, one
`resolve` arm. `range` and `edits` are `pub(crate)` so a sibling module of `backends/rust` can reuse the
self-mode rewrite and the shadowing refusal.

## How a run flows

1. The plan line is read against the file: the range as byte offsets and the mode.
2. RS1-RS4 run on the text, before any server is started. They are the same refusals a plain `check`
   reports.
3. The server is started, the document opened and the outline read; RS5 checks the method around the
   range and reads its `impl`'s self type.
4. **Self mode:** every site is rewritten and the edit is built. No further question is put to the server.
5. **Field mode:**
   - the host type of the first read of each field is asked with `textDocument/hover` on the file as it
     is;
   - a **trial text**, with every read written through `<name>`, is sent to the server with `did_change`
     and the state value's fields are asked the same way. A read the semantic tokens report unresolved is a
     field the state value lacks (RS6); the hover types are compared (RS7);
   - the original text is sent back, and the edit is built with the borrows the view provides dropped.
6. The edit is returned as one `FileEdit::Change` of minimal edits, one per substitution, so the ledger
   can translate positions across them, and the run reports one note.

The trial text is how the typing works across crates: the view's type may be declared in a crate the
host's package does not read, so the answer comes from the server, never from a `syn` read of a
declaration.

## Refusals

`plan is malformed:`, from the plan line alone and reported by plain `check`, `check --deep` and `apply`:

| # | Refused |
|---|---|
| RP1 | `name` missing, or not one identifier other than `self` |
| RP2 | `expr` missing |
| RP3 | an anchor that is not a range (a `range`, or an `item` with a relative range) |
| RP4 | a field the operation does not define |

`this seam cannot be cut here:`. RS1-RS4 read only text, so they are also reported by a plain `check` for a
`range` anchor; RS5-RS7 need the server and are reported by `check --deep` and `apply`. Every one is raised
before any edit is built, and a refused run writes nothing:

| # | Refused | When |
|---|---|---|
| RS1 | the range does not start at a statement: a `let` cannot be inserted before it | the code byte before the range start is not `;`, `{` or `}` |
| RS2 | the range names no `self`: there is nothing to rebind | in self mode a `Self` counts as something to rebind |
| RS3 | the range calls `self.m(…)` and uses `self` bare: read through `<name>` only fields, or rebind every `self` | field mode only; every site is listed |
| RS4 | `<name>` is already written in the enclosing function: the new binding would shadow it | `name` occurs as a whole word in the code of the enclosing `fn` |
| RS5 | the range is in no method with a `self` receiver | the enclosing symbol is not a method, or its parameter list has no `self` |
| RS6 | `<expr>` has no field `f`, `g`: the range reads `self.f` and `self.g` | field mode; every missing field is listed |
| RS7 | `self.f` is `T` and `<name>.f` is `U`: they differ by more than one reference | field mode; lifetimes are ignored, `U == T` and `U == &T` / `&mut T` are accepted |

A hover with no `name: Type` line in its code block is refused as `rust-analyzer's answer was unusable:`,
not skipped: a field whose type cannot be read is not given the benefit of the doubt.

A field of the same type is accepted whether the view holds it by value or by clone. Types cannot tell the
two apart.

## The note

`apply` and `check --deep` both print one note, and the two are equal:

- field mode: ``read_fields_through: rebound 4 reads of self through `state` (fields a, b); dropped 1 borrow``
  (the `dropped` clause only when a borrow was dropped);
- self mode: ``read_fields_through: rebound every self (7 sites) through `backend` ``.

## Limits

- **`Self` in field mode is not rewritten.** Only self mode writes the `impl`'s self type in its place.
- **A generic `impl`'s `Self::X` in self mode is written `W<T>::X`**, the self type as the header writes
  it. That is not valid in expression position (`W::<T>::X` is), so the compile gate refuses the result.
- **Borrow conflicts and `!Send` borrows are left to the compile gate.** A borrow of the state value alive
  across an `.await` in a future that must be `Send` is the one thing a lexical check cannot see.
- **A field read a macro generates is invisible**, since it is not written in the range.
- **One `expr` for the whole range.**
- **The state type and its builder are written by hand.** The operation binds a value the author already
  has; it does not create one.
- **The scan is lexical.** Sites come from text with comments and strings masked, not from the server's
  references, so a `self` a macro takes as a token is read as written.

## Composing

The operation exists to prepare a range for extraction. In one `group`:

1. `read_fields_through` over the range, with `expr` the state value (field mode) or `self` (self mode);
2. `extract_method` over the same lines, which names no `self` and so is written as a free function;
3. `extract_module`, and a move to another module or crate.

The `let` is inserted at the range's start, so the later operation's anchor is translated by the ledger to
begin **after** it, and the extracted range does not take the `let` in.

## `verify`

`restructure verify --against <ref> --rebind NAME` (repeatable) tells `verify` that `self` was rebound to
`NAME`. `verify/rebind.rs` holds `Rebind` and rule **R-rebind**, which runs after R1/R2 and before R-call:

- a lost and a gained statement pair 1:1 when the gained one equals the lost one with every
  whole-identifier `self` (outside strings, comments and lifetimes) written `NAME`, allowing one `&`
  directly before a rebound `self.` to have gone (the dropped borrow);
- then **one** gained statement `let NAME = …;` is excused per declaration;
- `NAME` is one identifier other than `self`; anything else is refused when the declaration is read.

The pairs count into `Excused::repointed`; there is no new response field. A rebind nobody declared, a
second `let`, and a changed argument stay reported. **Honest limit:** `verify` proves the differences have
the shape a declaration produces, not that the plan produced them; a hand edit that writes `NAME` for
`self` is excused too.

The declaration travels as `RestructureVerifyArgs.rebind`, `runner::Options.rebinds`,
`Declared::with_rebinds`, `VerifyRequest.rebinds` (field 5 of `code_index.proto`), and
`tddy-tools`'s `index_client::verify`.
