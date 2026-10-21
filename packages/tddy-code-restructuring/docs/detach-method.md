# `detach_method`: turning an `impl`'s members into free functions

How the operation that turns the members of one inherent `impl` into free functions, and re-points their
callers, is put together. Behaviour is in the
[feature doc](../../../docs/ft/coder/rust-code-restructuring.md#detach_method); this page is where the code
is, how a run flows, and what its limits are.

## Shape

rust-analyzer has no assist for this. `inline_method` copies `let session = self;` into every caller and
leaves the block behind, and the per-site route (`repoint_call` plus `add_call_arg`) costs two plan lines
per caller. The operation is authored here and informed by the server: the outline says where the block
and its members are, `textDocument/references` says who calls each member, and `textDocument/hover` says
what type each receiver has. Everything else is a function of the files' text.

`backends/rust.rs` holds the `pub mod detach_method;` line and one `SUPPORTED` entry; the kind is routed
through `same_crate_dispatch.rs`. The plan's own rules are `plan/codec/detach_fields.rs`.

## `backends/rust/detach_method/`

| Module | Decides |
|---|---|
| `detach_method.rs` | the entry points `findings` (plain `check`), `check` and `resolve`, and the `DetachServer` seam: `span`, `outline`, `references`, `hover`, each in the server's own shape |
| `server.rs` | `impl DetachServer for RustBackend`, answering from rust-analyzer |
| `survey.rs` | the survey both `check` and `resolve` read: the block, R5's text refusals, and every reported call read as a `Site` (re-pointed) or a refusal with its file and line |
| `block.rs` | the block the span lands in, read with `retarget_impl::outline::read` (`Operation::DetachMethod`), its header, `where`, and trait-ness |
| `member.rs` | one member's declaration: function or associated const/type, its receiver (`self`, `&self`, `&'a mut self`, typed), and `pub` |
| `refusals.rs` | R5's refusals the text decides: the declaration ones (trait member, const/type, by-value or typed receiver, `pub`), a parameter the body already writes, a name the module already declares |
| `calls.rs` | a reference read as a call: `R.m(a)` or `Self::m` / `T::m`; how the receiver is lent from its hovered type (refusing `Box<T>`, `Rc<T>`); the path a call from another module is written through, and the `use` it needs |
| `scan.rs` | byte-level reading: masking strings and comments, identifiers, brackets, macro invocations, LSP positions |
| `splice.rs` | the edits one file takes, composed over the original text: a call rewritten from its receiver and arguments, which may hold further splices |
| `build.rs` | the edits and the R6 note: call splices (R4), member splices (R1, R2, through `read_fields_through`'s self-mode rewrite), the block's replacement (R3), and one `FileEdit` per changed file |

The trait-impl refusal is this operation's own (`refusals.rs`, naming the member); the shared outline read
adds none for `Operation::DetachMethod`, and words its two-blocks refusal for a detach.

## `verify`

`verify/detach.rs` is rule R-detach: `Detach { ty, name }` parsed from `TYPE=NAME`, and `account`, a pass
between rebind and repoint in `verify::compare_with`. `Declared::with_detaches` (in `verify/retarget.rs`)
adds the implied rebind once per name. The carrier is `--detach` on `restructure verify`,
`runner::Options.detaches`, and `VerifyRequest.detaches` (field 6) through the index daemon and the
`tddy-tools` index client.

## Limits

- A generic or `where`-bound block is refused rather than carrying its parameters onto each function.
- A call inside a macro invocation is refused: the server's reference into a macro is not a call the text
  scan can rewrite safely.
- A receiver of another type (`Box<T>`, `Rc<T>`, any type other than `T` or a reference to it) is refused by name rather than dereferenced.
- Comments that name a member are not edited; the note says how many calls were re-pointed, not how many
  comments still say `self.m()`.

## Rule codes in the source

Comments in `detach_method/` name the rules by number. **R1** signature; **R2** body (`self`, `Self`);
**R3** placement and the emptied block; **R4** callers; **R5** refusals (`check --deep` and `apply`, with
the plain-`check` half in `detach_fields.rs`); **R6** the run note; **R7** `verify --detach`.
