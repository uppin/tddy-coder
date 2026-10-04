# Signature and call-site rewrites

`change_param_type`, `add_param`, `reorder_params`, `change_return_type` (with `type`), `add_call_arg`,
`remove_call_arg`, `change_call_arg` and `reorder_call_args` are edits this crate writes itself:
rust-analyzer has no assist that retypes or adds a parameter, reorders parameters, or edits one call's
arguments. `change_return_type` with a `variant` is the exception, below.

Product behaviour: [Rust code restructuring](../../../docs/ft/coder/rust-code-restructuring.md#signature-and-call-site-operations).

## Where it lives

| Piece | File |
|---|---|
| The eight `RefactorKind` variants, `RefactorOp.{type_, expr, order}`, `OrderKey`, `RefactorKind::edits_a_call_site` | `src/plan.rs` |
| `one_type`, `one_expr` (parsed with `syn`) and `permutation` (what `order` may be) | `src/plan/rust_syntax.rs` |
| The per-operation field refusals, raised when the plan is read | `src/plan/codec/signature_fields.rs` |
| Span and edit helpers shared by both halves, and the tests | `src/backends/rust/signature_rewrites.rs` |
| Declaration edits (`rewrite_declaration`, `returned_type`) | `src/backends/rust/signature_rewrites/declaration.rs` |
| Call edits (`rewrite_call`) | `src/backends/rust/signature_rewrites/call_site.rs` |
| The assist behind `change_return_type`'s `variant` | `src/backends/rust/return_type.rs` |
| Dispatch: `rewrite_signature`, `wrap_or_unwrap_return_type` | `src/backends/rust.rs` |

## Refused before a server exists

`parse_op` calls `refuse_a_signature_operation_it_cannot_honour` (`signature_fields.rs`): a field the
operation does not honour, a missing `name` / `type` / `variant` / `expr` / `order`, an unknown
position, `type` and `variant` together on `change_return_type`, and a call-site operation whose anchor
is not an item with a relative range. `type` and `expr` are each parsed with `syn` as exactly one
`Type` / `Expr`; an `expr` is also walked with `syn::visit` and refused if a statement sits anywhere
inside it. Whether `order` names every parameter or argument once needs the declaration or the call, so
`permutation` runs in the backend, once the text has been read.

The backend answers these operations by reading the file, before any server is spawned
(`rewrite_signature`), like the other refusals that need no index.

## Reading the text

Both halves read `masked_to_code(text)`, in which comments and string and character literals can carry
no delimiter, so a `,` or `)` inside one never splits an entry. Entries are the spans between the
delimiters' commas outside any brackets; for a parameter list a `<` also opens a bracket, and the `>`
of a `->` does not close one. A parameter is matched by its binding (`price` for `price: u32` and
`mut price: u32`), never by substring; a receiver binds nothing, so `add_param` `first` lands after it.

A call is read from the range the anchor names: it must parse as one `syn::Expr::Call` or
`Expr::MethodCall`, its last parenthesised group is found, and the argument spans are rebuilt by
joining comma-separated pieces until each parses as an expression. A count that disagrees with `syn`'s
is reported as a server defect, never guessed at.

## Exact spans, not `minimal_edits`

Every operation answers with the spans it changes (`Replacement`, turned into one-based `TextEdit`s by
`edits_of`), not with a rewritten file diffed back by `minimal_edits`. A line diff would report a
whole line for a change inside it, and a later operation of the same plan is anchored on that line, on
the function's own name. Exact spans leave the layout around each change as written, and leave that
anchor addressable after the plan store translates it through the edit. The assist path follows the
same rule: `wrap_or_unwrap_return_type` reports the server's edits as they came.

## `change_return_type`

With `type`, the declaration's `-> …` is replaced, or inserted after the parameter list when the
function declares none; the end of a return type is the first `{`, `;` or `where` outside brackets.
With `variant`, `return_type_assist` picks rust-analyzer's assist (`wrap return type in result` /
`option`, or `unwrap result` / `unwrap option` chosen by the current return type, read with `syn`) and
`wrap_or_unwrap_return_type` asks for it with the caret on the return type, in the function's own file.
The assist rewrites the declaration and the function's returns and leaves callers alone; `wrap_result`
writes `_` for the error type. The keyed-by-variant lookup is not one of `assist_for`'s arms because
one operation selects among several assists.

## Composition with groups

None of these operations leaves a compiling tree by itself when callers exist. A plan puts the
declaration operation and one call-site operation per caller in one `group`
([readiness-and-gates.md](readiness-and-gates.md#transactional-groups)): the group's end gate judges
the tree, and a group missing a caller is rolled back with that caller's compiler error. Edits stay
exact spans so each later member's anchor, translated through the earlier members' edits, still
addresses its call.

Tests: `tests/signature_rewrites_acceptance.rs` (live rust-analyzer, `--test-threads=1`) and unit tests
beside each module.
