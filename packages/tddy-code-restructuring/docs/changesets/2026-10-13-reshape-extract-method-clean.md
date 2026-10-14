# 2026-10-13 — `extract_method` keeps the range's comments, writes a signature the lint gate accepts, and lifts error guards

**Type:** Feature + Fix (post-processing of an existing operation; a wider set of accepted ranges; one tidy lint; one error class; no new operation, plan field, flag, wire message or `RefactorKind`)

`#reshape` 4/19 (PR [#601](https://github.com/uppin/tddy-coder/pull/601), `feature/reshape/extract-method-clean`,
no parent). Product entry:
[2026-10-13-reshape-extract-method-clean.md](../../../../docs/ft/coder/changelog/2026-10-13-reshape-extract-method-clean.md).
The repo-level test configuration is in
[the cross-package entry](../../../../docs/dev/changesets/2026-10-13-reshape-extract-method-clean.md).
How it works: [assist-output-repairs.md](../assist-output-repairs.md#the-function-extract_method-writes),
[readiness-and-gates.md](../readiness-and-gates.md#the-tidy).

## What changed

- **`backends/rust/extracted_fn.rs`** (new). `RustBackend::cleaned_extraction` runs, over the introduced
  function only, the function-local `use` carry (moved here from `assisted_edit`), the guard lift, the
  comment carry, the respelling of an unimported signature type, the `ptr_arg` narrowing, the lexical
  clean-ups with their shape notes, and the return-type spelling. `verified_narrowings` and
  `respelled_signature_types` ask the server; `extract_method_findings` and `refuse_unliftable_returns`
  are the static `check` side (the latter replaces `refuse_early_returns` at its two call sites).
- **`extracted_fn/` children** (new): `span.rs` (where the function sits; the byte scanning the rules
  share), `signature.rs` (parameters and return type over masked text), `comments.rs` (rule 1: put back,
  or refuse naming the comments; `at_risk` for `check`), `lints.rs` (`unit_arg`,
  `redundant_field_names`, notes for more than seven parameters and tuple returns of three or more),
  `ptr_args.rs` (`&PathBuf`/`&String`/`&Vec<T>` to `&Path`/`&str`/`&[T]`, `.clone()` to `.to_owned()`,
  the diagnostics that undo a narrowing), `respell.rs` (the origin function's one spelling of a type, or
  a refusal), `guards.rs` (`classify`, `lifted`), `tail.rs` (`in_tail_position`), `return_type.rs` (the
  caller's one-argument `Result` alias, for every extraction).
- **`backends/rust.rs`.** `mod extracted_fn;`; the `extract_method` placeholder has no name, so the
  server-named function is found and renamed; `check`, `resolve_opening` and `assisted_edit` each make
  one call in place of the lines they held. `early_return.rs`: `early_returns` is `pub(super)`, nothing
  else.
- **`backends/rust/selection.rs`.** `hover_bearing_position` reads across lines and skips a leading `{`
  and `//` and `/* */` comments, so a range opening on one is probed where it can be typed. The range
  handed to the assist is unchanged.
- **`backends/rust/introduced.rs`.** `introduced_declaration(keyword, …)` finds the one `let` or `fn`
  the assist added, by count and by the changed span.
- **`backends/rust/prelude_shadow.rs`.** `anonymous_trait_imports`: every `use <path> as _;` of the
  parent, a group member included, is carried into the module `extract_module` creates, rebased one
  level deeper.
- **`runner/tidy/unused_mut.rs`** (new) and `runner/tidy.rs` (one `mod` line and one call,
  `unused_mut::without_unused_mut`). rustc's `unused_mut` with a `MachineApplicable` fix in a touched
  file is applied after the imports and before `rustfmt`; a re-check that fails restores the bytes and
  fails the run.
- **`backends/lsp_bridge.rs`.** `INTERNAL_ERROR` (`-32603`) maps to `ServerDefect`.
- **Tests.** New `tests/extract_method_clean_acceptance.rs` (19 live tests) and
  `tests/extract_method_guard_lift_acceptance.rs` (10 live tests); three tests in
  `tests/apply_tidy_acceptance.rs`; unit tests beside every new function; harness helpers
  `a_workspace_whose_test_binary_carries_an_unused_mut`, `the_function_named` and a clippy assertion
  through `assert_lints_clean`. Two tests of the red phase pinned what neither the contract nor this
  code decides and were corrected: `reports_any_other_server_error_verbatim` stands in for "any other
  error" with `-32000` (`-32603` is now the server's defect), and the `Option` guards test pins
  rust-analyzer's parameter order `(b, n)`.

## Behaviour that differs for existing plans

- A range starting on a block's `{` or on a comment, which was refused after 30 s, is extracted.
- A range that is a `let`'s initializer is extracted under the plan's name instead of refused for not
  producing a `fn fun_name`.
- A mid-function run of `return Err(..)` / `return None` guards and a tail-position arm's body are
  extracted instead of refused; a value `return` in the middle is still refused, with its existing text.
- An LSP `-32603` reads `rust-analyzer's answer was unusable:` (over the wire `Internal`, was
  `InvalidArgument`).
- An `extract_method` can be refused where it was applied: comments that cannot be paired, a signature
  type with no unique spelling, a lift of a shape rust-analyzer did not write as expected.

## Measurements

Scoped to `tddy-code-restructuring`: `./test -p tddy-code-restructuring` — **74 targets, 1480 tests, 0
failed**; `cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings` and `cargo fmt` clean;
`cargo check -p tddy-index-daemon -p tddy-tools --all-targets` clean. The 35 acceptance tests pass. The
rest of the workspace is CI's.

- `backends/rust.rs`: **517 → 516** production lines by the file-length gate (to the first
  `#[cfg(test)]`), 3,008 → 3,000 to the first inline `#[cfg(test)] mod`. Over the 500 budget before and
  after; not grown. `check`'s early-return block left for `extract_method_findings`. The record
  `oversized-file-backends-rust.md` carries the row.
- Largest new file: `extracted_fn/guards.rs`, 431 production lines. No new file is over 500.
- `runner/tidy.rs`: one `mod` line and one call; the step is in `runner/tidy/unused_mut.rs`.
- No function on the over-60 list grew; the longest new function is 43 lines
  (`introduced_declaration`).

## Backlog entries

- **Resolved, deleted:**
  `2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures` (P comments, Q
  `ptr_arg`/`unit_arg`/`redundant_field_names`/unused `mut`, R the probe, S the server-named function,
  U the panic class; T, the `'_` read as `_`, was already fixed in `placeholder_checks.rs`) and
  `2026-09-24-restructure-apply-leaves-the-lint-gate-red` (the missing half of N3 was the `as _` carry).
  What it left open moved to
  `2026-10-09-restructure-extract-method-rust-analyzer-rewrites-that-do-not-build` (the three plan `10b`
  build breaks) and `2026-10-09-index-daemon-does-not-see-a-file-edited-outside-a-run` (item V).
- **Narrowed:** `2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run` loses gaps I
  (`use Trait as _;`) and K (an `extract_method` signature naming an unimported type); G, L, M, W and the
  first half of X remain.
- **Filed:** `2026-10-09-restructure-compiler-guided-import-repair` (developer decision F5: two narrow
  import fixes, no compiler-guided repair),
  `2026-10-09-restructure-extract-method-lifts-guard-runs-without-outputs-only`,
  `2026-10-09-restructure-named-trait-import-used-only-for-methods`.
- **Edited:** `2026-09-24-lifecycle-functions-still-over-150-lines` no longer links to the deleted
  entry.

## Still by hand

A guard run that declares a binding the code after it reads is refused, not lifted with an output. A
`return` a macro expands to (`bail!`) and a `break`/`continue` leaving the range are not seen. A named
trait import used only for method calls is not carried by `extract_module`.
