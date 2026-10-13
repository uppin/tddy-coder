# 2026-10-13 — `extract_method` keeps comments, writes lint-clean signatures and lifts a run of error guards

Feature: [Rust code restructuring](../rust-code-restructuring.md#extract_method).

A plan of `extract_method`s that compiled still needed hand edits: comments restored, clippy findings of
four mechanical shapes, a signature naming a type the file never imports, a seam missing a trait import.

- **Comments stay.** Each comment the range held and rust-analyzer dropped is put back beside its
  statement. Statements that cannot be paired refuse the operation, naming the comments; nothing is
  written. `check --deep` says how many it will keep.
- **Lint-clean signatures.** `&PathBuf`, `&String` and `&Vec<T>` parameters are `&Path`, `&str` and
  `&[T]` unless the body needs the owned type (a note says so); a unit body is not wrapped as
  `Ok(if …)`; `field: field` is `field`. More than seven parameters, or a tuple return of three or more,
  is noted, not rewritten. The tidy removes an unused `mut`, in the new function or the caller.
- **Types as the file spells them.** An unimported type in the new signature is spelled the way the
  function the range came from spells it; no spelling or two is a refusal. No `use` is added.
- **The caller's `Result`.** Where the caller returns a one-argument `Result` alias, the new function
  returns `Result<X>`, not the two-argument form that is `E0107` there.
- **Guards are lifted.** A mid-function run of `return Err(..)` guards (`return None` in an `Option`
  function) becomes a function returning `Result<()>`, called with `?`. A range that is the body of a
  match arm or `if` branch in tail position is extracted with its `return`s. A value `return`, guards
  mixed with one, and a guard run whose binding is read afterwards are refused, naming the line or
  binding, by `check`, `check --deep` and `apply` alike.
- **Ranges that were refused or hung.** A range opening on `{` or a comment is extracted, and a range
  that is a `let`'s initializer is extracted under the plan's name.
- **`extract_module`** carries the parent's `use … as _;` imports, so a seam that calls a trait method
  compiles; the tidy removes a copy nothing uses.
- **A rust-analyzer panic** (`-32603`) reads `rust-analyzer's answer was unusable:`, `Internal` over the
  wire, instead of `plan is malformed`.
- **Still by hand:** a guard run that binds something read afterwards, a `return` a macro expands to,
  and a named trait import used only for method calls.
