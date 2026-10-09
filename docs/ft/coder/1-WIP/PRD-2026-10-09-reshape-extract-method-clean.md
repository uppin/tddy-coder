# `extract_method`: keep the range's comments, write a signature the lint gate accepts, and lift a run of error guards - PRD

**Date**: 2026-10-09
**PRD Type**: Enhancement (and defect fixes)

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — `## Rust operations (v1)` (the
  `extract_method` row), `## LSP integration` › `### Refusal classes` (a server panic changes class) and
  `### Import restoration` (an anonymous trait import is carried), `## The tidy` (an unused `mut` is removed), and
  `## Known limitations` (the `extract_method` / `extract_variable` bullets).

No other feature document changes. No new operation, plan field, flag or wire message.

## Summary

An `extract_method` that compiles today still leaves CI's lint job red, and loses the comments that say *why*. After this
change the function it writes keeps every comment its range held, takes `&Path` / `&str` / `&[T]` where it borrowed a
`PathBuf` / `String` / `Vec<T>`, ends a unit body with `Ok(())` instead of wrapping it, writes `field` instead of
`field: field`, names its types the way the file names them, and loses the `mut` it does not need. A range that opens on a
block's `{` or on a comment is extracted instead of refused, a range that is a `let`'s initializer is extracted under the
plan's name, and a rust-analyzer panic is reported as the server's defect. `extract_module` also carries the parent's
`use Trait as _;` imports, so a seam that calls a trait's methods compiles.

`extract_method` also extracts a run of **`return Err(..)` guards from the middle of a function**, which it refuses
today: the guards become a function returning `Result<()>` (spelled with the caller's own `Result`), and the range
becomes one `name(..)?;`. A range that is the body of a match arm or `if` branch in the function's tail position is
extracted with its `return`s as they are, since there they still return from the caller.

## Background

`#carve` 14/15 (#524) ran the lifecycle destructure's extract-method plans for real. Every one passed the compile gate and
then needed hand edits: plan `09b` alone lost 14 comment lines and left 18 clippy findings, `10b` lost 22 comment lines,
and `05`, `09c` and `10b` added `ptr_arg`, `unit_arg`, `redundant_field_names` and unused-`mut` findings of the same
mechanical shapes. Plans `09` and `09c` did not compile because a signature named a type the file never imports; plan
`02`'s split did not compile because the seam that calls `encode_to_vec` lost `use prost::Message as _;`. Two ranges hung
the warm index daemon, and one valid expression range was refused.

`extract_method` is the tool the stack's function-size nodes use (`#reshape` 16 and 19, 25 functions over 60 lines), so
every one of these defects would be paid again per extraction. The shapes were re-measured against the dev shell's
rust-analyzer (2026-03-30) on 2026-10-09; the initial discovery records the outputs verbatim.

## Proposed Changes

### What's Changing

**Comments (P).** rust-analyzer rebuilds the extracted body's statement list when the range holds a `?` and drops the
comments between statements and after them. The engine puts each lost comment back where it was: a full-line comment
before the statement that followed it in the range, a trailing comment at the end of its statement's line. Statements are
matched in order, since the assist keeps them in order and appends one tail. When the statements of the range and of the
new function cannot be paired one to one, the operation is **refused** as `rust-analyzer's answer was unusable:`, naming
the lost comments, and nothing is written. `check --deep` prints `the extracted function will keep N comment(s)
rust-analyzer dropped` as a progress line, the same way the function-local `use` carry is reported.

**Signature and body clean-up (Q).** These run on the function the assist introduced, after the rename, and only there:

| What the assist wrote | What the engine writes | How it is known to be safe |
|---|---|---|
| `p: &PathBuf`, `s: &String`, `v: &Vec<T>` (qualified or not) | `p: &Path`, `s: &str`, `v: &[T]`. `Path` is written bare where the file binds it, `std::path::Path` otherwise. No `use` is added. `p.clone()` in the body becomes `p.to_owned()` | rust-analyzer's pull diagnostics for the file after the rewrite. A new type-mismatch (`E0308`) in the new function or at its call puts that parameter back as the assist wrote it, and says so in a note: `kept \`p: &PathBuf\`: the body needs a \`&PathBuf\`` |
| `Ok(if … { … })` / `Ok(match …)` / `Ok({ … })` as the tail of a function returning `Result<(), _>` (`Some(…)` for `Option<()>`) | the expression as a statement, then `Ok(())` / `Some(())` | the return type is `()`-carrying, so the wrapped expression is `()` |
| `field: field` in a struct literal | `field` | the same binding by definition |
| more than seven parameters, or a tuple return of three or more | unchanged, with a note naming the count, since `clippy::too_many_arguments` / `type_complexity` will fire | grouping parameters is a naming decision, so it stays the plan's |

`&mut PathBuf` / `&mut String` / `&mut Vec<T>` are not rewritten, because they need the owned type.

**An unused `mut` (Q).** The server reports none, so it is the compiler's: [the tidy](../rust-code-restructuring.md#the-tidy)
removes rustc's `unused_mut` after the unused imports and before `rustfmt` — `MachineApplicable` suggestions only, in the
files the run wrote. A removal the re-check rejects is undone from the bytes held in memory and fails the run, as the
import step does. This catches both the callee's copied `mut` and a caller's `mut` an extraction
left unused.

**A type the signature names that the file does not import (K).** Where the new function's signature names a type
rust-analyzer reports unresolved, and that was resolved before the assist, the engine respells it the way the origin does:
the one qualified spelling ending in that name in the function the range came from (`deep::recipe::WorkflowRecipe`). It
keeps the rewrite only if that name's unresolved occurrences go down. No spelling, two different spellings, or a rewrite
that resolves nothing is **refused** as `this seam cannot be cut here:`, naming the type and the spellings found.

**A trait used only for its methods (I, and the lint gate's N3).** `extract_module` carries every
`use <path> as _;` of the parent into the new module, rebased one level deeper, in the same lexical pass that carries
prelude-shadowing imports. It over-imports on purpose, and the tidy removes what rustc reports unused. That removal is
trait-aware.

**A range that opens on `{` or on a comment (R).** The type probe now skips a leading `{`, whitespace, line comments and
block comments, across lines, to the first position inside the range that can be typed. The range given to the assist is
unchanged. A range holding nothing that can be typed keeps today's behaviour: the 30-second bound, then a refusal.

**A range that is a `let`'s initializer (S).** rust-analyzer names that function after the binding (`fn managed`), not
`fun_name`. The engine finds the function the assist introduced by what it added — one new `fn` in the changed span, as
it already does for `extract_variable`'s `let` — and renames it to the plan's name. Zero or several new functions is
refused as today.

**A rust-analyzer panic (U).** An LSP `-32603` ("request handler panicked") is reported as `rust-analyzer's answer was
unusable: …`, not `plan is malformed: …`.

**A run of error guards (developer addition, 2026-10-09).** Today a range holding a `return` is refused unless it runs to
the end of the function's tail expression. That refusal stops `#reshape` 16 from shrinking `plan/codec.rs::parse_op`
(206 lines, nearly all `if … { return Err(malformed(…)); }` guards) and the `Item` arm of
`plan_store/refresh.rs::refreshed`. `#reshape` 19 lifts one guard (`retarget_impl`) and otherwise uses tail ranges and
rule 14's return-type spelling. After this change:

| The range holds | In the middle of the function | Result |
|---|---|---|
| only `return Err(<e>)` exits (any depth: inside `if`, `match`, `for`; not inside a closure, `async` block or nested `fn`), in a function returning `Result<T, E>` or a one-argument alias of it, and nothing the code after the range reads | **lifted**: `fn name(..) -> Result<()>` (the caller's spelling with `()` for `T`) whose returns are the guards' own `return Err(<e>);`, ending `Ok(())`; the range becomes `name(..)?;` | rust-analyzer writes `Option<Result<T, E>>` with `return Some(Err(..))`, `None`, and `if let Some(value) = name(..) { return value; }`. The engine rewrites that one shape, and refuses any other the server writes |
| only `return None` exits, in a function returning `Option<T>` | **lifted** the same way: `-> Option<()>`, ending `Some(())`, called with `?` (F9) | |
| a `return` of a value (`return Ok(v)`, `return v`, `return;`) | **refused** as today, naming the line (F7) | a value return cannot be re-expressed with `?` |
| both error guards and value returns | **refused**, naming the value returns (F8) | |
| error guards and a binding the code after the range reads | **refused**, naming the binding: cut the run after it (F10) | |
| any `return`, and the range is the whole body of a match arm or `if`/`else` branch in the function's tail position (recursively) | **extracted with its `return`s verbatim**; the call is that arm's value (F7) | the existing "runs to the end of the function" exception, extended to tail position. The `Item` arm of `refreshed` is this shape |

When the range also holds `?` calls, rust-analyzer writes the lifted form (`Result<(), E>`, `name(..)?;`) itself, and the
engine keeps it.

**Every** `extract_method` — lifted, tail-position, or a plain range that only propagates with `?` — writes the new
function's return type the way the caller spells it. When the caller returns a one-argument alias (`Result<T>`: this
crate's `crate::Result`, `anyhow::Result`), the new function returns `Result<X>`, not the `Result<X, E>` rust-analyzer
writes. That two-argument form is `E0107` in every file that imports the alias, which is all nine of `#reshape` 16's
files. `check` and `check --deep` accept the same ranges
and refuse the same ones, before any server starts.

### What's Staying the Same

- Which other ranges `extract_method` accepts: a mid-function value `return` is still refused, as are the unit-tail
  refusal and bottom-up composition. The function-local `use` carry and the inferred-placeholder check are unchanged.
- A `return` a macro expands to (`bail!`) and a `break`/`continue` that leaves the range are still not seen; the compile
  gate catches them.
- No parameter struct, type alias or new `use` line is invented for an extraction. Arity and tuple returns stay the plan's.
- `extract_module`'s other import passes, `extract_variable`'s borrow handling, and every move operation.
- The tidy's existing steps, order, bounds and refusals. It gains one more lint to apply, nothing else.
- The generic wait deadline is node 10's (`apply-robust`): it extends the 30-second ready-but-silent bound to every
  type-inference wait and adds a stall bound while loading. This node only moves the probe to a position that can be typed.
- `check --deep` still does not compile. An unused `mut` is found by the tidy in `apply`, not by `check --deep`.

## Impact Analysis

### Technical Impact

- `tddy-code-restructuring` only. New modules under `src/backends/rust/` hold the comment carry, the signature
  clean-up and the respelling. `backends/rust.rs` gains wiring lines only: it is ≈2,983 production lines, and node 17
  splits it. The probe change goes in `backends/rust/selection.rs`, the server-named function in
  `backends/rust/introduced.rs`, the `as _` carry in `backends/rust/prelude_shadow.rs`, `unused_mut` in a new
  `runner/tidy/unused_mut.rs` (`runner/tidy.rs` gains one call: that file's size is `#reshape` 15's), and the panic class in
  `backends/lsp_bridge.rs`. No growth of `imports.rs` (524) or `early_return.rs` (531 production lines), and no growth of
  any function on the stack's over-60-lines list (`assisted_edit`, `check`, `resolve_opening`): the new logic is in new
  functions, and the existing `extract_method` lines in those functions are replaced by one call each.
- The guard lift lives in a new `src/backends/rust/extracted_fn/guards.rs`, and the return-type spelling for every extraction in `extracted_fn/return_type.rs`. `early_return.rs` (531 production lines)
  only has `early_returns` widened to `pub(super)`. `resolve_opening` and the `check` path swap their
  `refuse_early_returns` call for `extracted_fn::refuse_unliftable_returns`, one line for one line.
- Two new live test binaries (`extract_method_clean_acceptance`, `extract_method_guard_lift_acceptance`) join `.config/rust-e2e.filterset` and the `rust-analyzer` group in `.config/nextest.toml`.
  The tidy and panic tests need no server.
- `runner/tidy.rs` is shared with `#reshape` 3 (`tidy-facades`), and `prelude_shadow.rs` / `introduced.rs` with
  `#reshape` 2 (`multi-seam-extract`). The overlap is textual, not behavioural.

### User Impact

- A plan of extract-methods applies lint-clean, with its comments, in the shapes #524 fixed by hand. `check --deep` names
  the comments it will put back and the parameters it could not narrow.
- One behaviour change for existing plans: a range starting on `{` or a comment, which was refused after 30 s, now
  extracts. A server panic now reads as `rust-analyzer's answer was unusable`, mapped to `Internal` over the wire instead
  of `InvalidArgument`.

## Implementation Plan

1. The probe position (R) and the server-named function (S): pure text, unit tests first.
2. The comment carry (P): statement alignment and its refusal, unit-tested on the reproduced texts.
3. The lexical clean-ups (Q: `unit_arg`, `redundant_field_names`, notes on arity and tuple returns).
4. The `ptr_arg` rewrite and its pull-diagnostics verification.
5. The respelling of an unresolved signature type (K).
6. The `as _` carry for `extract_module` (I / N3).
7. `unused_mut` in the tidy; the panic class (U).
8. The guard lift and the tail-position extension: classification first (text only, `check` parity), then the rewrite
   of the server's `Option<Result<…>>` shape, then the return-type spelling.
9. One live binary over the #524 shapes and the guard shapes, and its registration.
10. Docs at wrap. The two claimed entries are narrowed or deleted, and the apply-gaps entry loses K and I.

## Acceptance Criteria

- [ ] an `extract_method` whose range holds `?` keeps every full-line and trailing comment of the range, each beside the
  statement it annotated, and the tree compiles ([Rust code restructuring](../rust-code-restructuring.md))
- [ ] statements that cannot be paired refuse the operation, naming the lost comments, with the file unchanged
- [ ] the extracted function takes `&Path`, `&str` and `&[T]` for borrowed `PathBuf`, `String` and `Vec<T>` locals, and
  `cargo clippy -- -D warnings` over the fixture reports no `ptr_arg`, `unit_arg` or `redundant_field_names`
- [ ] a parameter whose body needs the owned type keeps it, with a note, and the tree compiles
- [ ] a unit body holding `?` ends in `Ok(())`; `field: field` is written `field`
- [ ] an `apply` leaves no unused `mut` in the files it wrote, in the callee or the caller
- [ ] a signature naming a type the origin spells qualified compiles with no `use` added, and a type with no unique
  spelling is refused, naming it
- [ ] an `extract_module` whose seam calls a trait method the parent imports `as _` compiles, and the tidy leaves no
  unused copy of that import
- [ ] ranges starting on a block's `{` and on a line comment are extracted within the ready bound
- [ ] a range that is a `let`'s initializer is extracted under the plan's name
- [ ] an LSP `-32603` reads `rust-analyzer's answer was unusable:`
- [ ] a run of `return Err(..)` guards in the middle of a function becomes `fn name(..) -> Result<()>` called as
  `name(..)?;`, spelled with the caller's `Result` alias, and the tree compiles and passes clippy
- [ ] a run of `return None` guards in an `Option` function is lifted the same way
- [ ] a mid-function value `return`, a mix of guards and value returns, and a guard run whose binding is read afterwards
  are each refused, naming the line or binding, by `check` and by `apply`, with the file unchanged
- [ ] a range that propagates with `?` and holds no `return`, in a file importing a one-argument `Result` alias, returns
  `Result<X>` and compiles
- [ ] a guard run that also propagates with `?` is lifted, called with `?`, and compiles
- [ ] a range that is a tail-position match arm's body is extracted with its `return`s as they are, and the tree compiles
- [ ] an extraction shaped like #524's plan `09b` passes `cargo clippy --all-targets -- -D warnings` in its fixture
- [ ] tests pass for `tddy-code-restructuring` (scoped; CI for the rest)

## References

### Affected Features (Complete List)
- [Rust code restructuring](../rust-code-restructuring.md)

### Decisions

Approved by the developer on 2026-10-09 with the recommendations: a pairing that fails refuses (F1); a parameter whose body
needs the owned type is kept with a note (F2); `unused_mut` is the tidy's (F3); a type is respelled as the origin spells it
and no `use` is added (F4); two narrow import fixes and no compiler-guided repair, which is deferred to its own backlog
entry (F5); the test harness gains one clippy assertion (F6).

Decided 2026-10-09 (the guard lift; the developer accepted each recommendation): F7 value returns (refuse in the
middle, extract in tail position), F8 mixed returns (refuse), F9 `return None` guards (lift), F10 a guard run with an
output (refuse; deferred).

### Related Documentation
- Changeset: `docs/dev/1-WIP/2026-10-09-reshape-extract-method-clean.md`
- Initial discovery: `docs/dev/1-WIP/2026-10-09-reshape-extract-method-clean-initial-discovery.md`
- Backlog this resolves:
  [extract drops comments and writes clippy-failing signatures](../../../dev/todo/2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md),
  [apply leaves the lint gate red](../../../dev/todo/2026-09-24-restructure-apply-leaves-the-lint-gate-red.md) (N3);
  items K and I of [the lifecycle-destructure apply gaps](../../../dev/todo/2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run.md)
- Package docs: `packages/tddy-code-restructuring/docs/assist-output-repairs.md`, `packages/tddy-code-restructuring/docs/readiness-and-gates.md`
