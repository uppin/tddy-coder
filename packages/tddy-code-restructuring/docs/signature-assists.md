# Signature assists

`remove_unused_param` and `convert_tuple_return_to_struct` change a function's signature together with
every caller. rust-analyzer performs both; the backend decides where to ask and what to do with the
answer. Neither needs a transactional group, since each leaves a compiling tree on its own.

Product behaviour: [Rust code restructuring](../../../docs/ft/coder/rust-code-restructuring.md#rust-operations-v1).

## Where it lives

| Piece | File |
|---|---|
| The two `RefactorKind` variants | `src/plan/refactor_kind.rs` |
| The `name`-required refusals, raised before any server starts | `src/plan/codec.rs`, `parse_op` |
| `SUPPORTED` entries and the `assist_for` rows (`remove unused parameter`, kind `refactor`; `convert tuple return type to tuple struct`, kind `refactor.rewrite`) | `src/backends/rust.rs` |
| Carets, the used-parameter refusal and the struct rename | `src/backends/rust/signature.rs` |

Both operations resolve through `multi_file_assist`, the path `inline_method` takes, because their edits
reach files other than the anchor's.

## The caret

An item anchor names the function, at its name. rust-analyzer offers each assist elsewhere, so the
source between the two is read (`signature.rs`, over `masked_to_code`, which blanks comments and
strings):

- `remove_unused_param` is offered on the parameter's name. The parameter list is found from the
  function's name, past any generics, and a parameter is matched by its **pattern** (`name`, `mut name`),
  never by substring, so a type that mentions the name is not mistaken for it.
- `convert_tuple_return_type_to_struct` is offered inside the return type, just past the `->`.

A scan that meets unbalanced source returns no position rather than panicking, and the operation is
refused as seam-shaped (`` `x` is not a parameter… `` / no return type to convert).

## A used parameter

The server offers no removal for a parameter its function still reads, and that absence is the answer.
`refuse_used_parameter` turns a `SeamRefused` from the assist into
`` parameter `name` is used, so it cannot be removed (<server reason>) ``. Anything else — a server that
would not start, one still indexing — passes through untouched.

## Naming the struct

The conversion names its struct after the function and writes that name into every file it touches. The
plan's `name` is applied afterwards, and the backend writes no identifier itself:

1. Every document the assist touched is collected as path, what is on disk, and what the assist left.
   A file *creation* is a server defect, since the assist has no reason to make one.
2. The introduced struct is found by what the assist **added**: the name declared more often after than
   before, in the function's own file. No spelling is guessed.
3. When that name differs from `name`, the touched documents are opened on the server with the assist's
   edits applied, and `textDocument/rename` is asked at the struct's name.
4. Each rename edit is folded back into the document it belongs to. An edit reaching a file the
   conversion did not touch is a server defect.
5. The result is a minimal edit per file against what was on disk.

The struct keeps the function's visibility (`pub fn` yields `pub struct Name(pub A, pub B);`).

## Tests

`tests/signature_assists_acceptance.rs` runs item-anchored plans against a live rust-analyzer and a
real `cargo check`. `signature.rs` and `plan.rs` carry unit tests for the position scans, the unbalanced
cases and the serde surface.
