# Changeset: `extract_method` keeps the range's comments, writes a signature the lint gate accepts, and lifts error guards

**Date**: 2026-10-09
**Status**: 🚧 In Progress
**Type**: Fix + enhancement (post-processing of an existing operation; a wider set of accepted ranges; one tidy lint; one error class)
**Stack**: `#reshape` 4/19, branch `feature/reshape/extract-method-clean`, wave 1, PR [#601](https://github.com/uppin/tddy-coder/pull/601) (draft). PR title:
`feat(code-restructuring): extract_method lifts error guards, keeps comments, writes lint-clean signatures (#reshape 4/19)`.
Base in the linear stack: `feature/reshape/tidy-facades` (K=3). **Real edges**: none in (no parent's behaviour is
consumed); out: `extract-method-clean -> fn-sizes-rest` (K=16) and `extract-method-clean -> fn-sizes-backend` (K=19), which
use `extract_method` to shrink functions. `fn-sizes-rest` needs the guard lift for `plan/codec.rs::parse_op`, tail position
for `plan_store/refresh.rs::refreshed`'s `Item` arm, and rule 14 for its `?` seams. `fn-sizes-backend` needs tail ranges
and rule 14, plus one guard lift (`retarget_impl`).

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-09-reshape-extract-method-clean-initial-discovery.md)
(Exploration 1 is the whole-work discovery; Exploration 2 is this node's, with rust-analyzer 2026-03-30's output reproduced
verbatim).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

`grep -rl 'Claimed by:'` over `packages/tddy-code-restructuring/docs/code-issues/` finds no claim on the files this node
edits: **no 🚧 claimed issue is in the path, no wait-or-proceed fork.**

| Item | Verdict | What this change does about it |
|---|---|---|
| [2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md](../todo/2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md) | ✅ **RESOLVED HERE** | P (comment carry), Q (`ptr_arg`, `unit_arg`, `redundant_field_names`, unused `mut`; notes for arity and tuple returns), R (probe position), S (server-named function), U (panic class). T is already fixed (#542-era `placeholder_checks.rs`). The three 10b build breaks move to the new todo `2026-10-09-restructure-extract-method-rust-analyzer-rewrites-that-do-not-build.md`, V to the new `2026-10-09-index-daemon-does-not-see-a-file-edited-outside-a-run.md`, so the entry is **deleted** at wrap. Closed when acceptance tests 1-17 and 21-22 pass |
| Developer addition (2026-10-09): mid-function `return Err(..)` guard runs, tail-position arms | — (no backlog entry) | Rules 12-14 (rule 14 for every extraction: `#reshape` 16's `?` seams in `check_plan` / `apply_held_plan`). Unblocks `#reshape` 16's `parse_op` / `refreshed` and `?` seams, and 19's tail-range and `?` cuts (one guard lift, `retarget_impl`). Tests 23-35 |
| [2026-09-24-restructure-apply-leaves-the-lint-gate-red.md](../todo/2026-09-24-restructure-apply-leaves-the-lint-gate-red.md) | ✅ **RESOLVED HERE** | Only N3's missing half was open, and it is gap I: closed by the `as _` carry (test 16). Deleted at wrap |
| [2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run.md](../todo/2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run.md) — claimed by `#reshape` 2 (`multi-seam-extract`) | partial | Items **K** (respelling, tests 10-11) and **I** (`as _` carry, test 16). This node's wrap removes the K and I sections; M and W stay, and the "compiler-guided import repair" candidate is replaced by a link to the new `2026-10-09-restructure-compiler-guided-import-repair.md` |
| [`oversized-file-backends-rust.md`](../../../packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md), [2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md](../todo/2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md) | ⚠ **DURING** | `backends/rust.rs` gains `mod extracted_fn;` and loses lines: the `extract_method` blocks in `check` and `assisted_edit` become one call each. Net ≤ 0 production lines; history row at wrap |
| [`complexity-rust-facade-lines.md`](../../../packages/tddy-code-restructuring/docs/code-issues/complexity-rust-facade-lines.md) and the stack's over-60 function list (whole-work discovery, Exploration 3: `assisted_edit` 124, `check` 66, `resolve_opening` 163) | ⚠ **DURING** | None of the listed functions grows; `check` and `assisted_edit` shrink by the replaced blocks |
| [2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md](../todo/2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md) (`runner/tidy.rs` ~540 production lines, `#reshape` 15's) | — Unrelated | `runner/tidy.rs` gains one call; the step lives in the new `runner/tidy/unused_mut.rs` |
| [2026-09-25-restructure-test-binary-move-cannot-see-through-a-glob-facade.md](../todo/2026-09-25-restructure-test-binary-move-cannot-see-through-a-glob-facade.md), [2026-10-05-restructure-apply-did-not-return-after-a-clean-deep-check.md](../todo/2026-10-05-restructure-apply-did-not-return-after-a-clean-deep-check.md) (`#reshape` 10) | — Unrelated | The wait deadline and the gate's packages are node 10's; this node changes the probe's *position*, not the wait |
| `packages/tddy-code-restructuring/docs/code-issues/*` others (`broken-restructure-anchors-empty-outline.md`, `dead-code-plan-filehint-modified.md`, `oversized-file-test-binary.md`) | — | Not in the path |

## Affected Packages

- **`tddy-code-restructuring`**: [README.md](../../../packages/tddy-code-restructuring/README.md);
  `src/backends/rust.rs` (wiring: `mod extracted_fn;`, the `ExtractMethod` placeholder's name, one call in `check`, one in
  `assisted_edit`); **new** `src/backends/rust/extracted_fn.rs` + `extracted_fn/{span,comments,lints,ptr_args,respell,guards,return_type}.rs`; `src/backends/rust/early_return.rs` (`early_returns` → `pub(super)`);
  `src/backends/rust/selection.rs` (probe position); `src/backends/rust/introduced.rs` (a server-named `fn`);
  `src/backends/rust/prelude_shadow.rs` (the `as _` carry); **new** `src/runner/tidy/unused_mut.rs` and one call in
  `src/runner/tidy.rs`; `src/backends/lsp_bridge.rs` (`-32603`). Tests: **new** `tests/extract_method_clean_acceptance.rs`, **new** `tests/extract_method_guard_lift_acceptance.rs`,
  `tests/apply_tidy_acceptance.rs` (three tests), `tests/harness/mod.rs` (`assert_clippy_clean`, one tidy fixture).
  Registration: `.config/rust-e2e.filterset` and the `rust-analyzer` group in `.config/nextest.toml`.
  Docs at wrap: [assist-output-repairs.md](../../../packages/tddy-code-restructuring/docs/assist-output-repairs.md)
  (the extracted function's clean-up, the `as _` carry), [readiness-and-gates.md](../../../packages/tddy-code-restructuring/docs/readiness-and-gates.md)
  (the probe, the tidy's `unused_mut`), [docs/ft/coder/rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md)
  (`## Rust operations (v1)`, `### Refusal classes`, `### Import restoration`, `## The tidy`, `## Known limitations`),
  [plan-schema.md](../../../.agents/skills/code-restructuring/references/plan-schema.md) (the `extract_method` row: what it
  writes; no schema change).
- **`tddy-tools`, `tddy-index-daemon`**: no source change.

## Related Feature Documentation

- [PRD-2026-10-09-reshape-extract-method-clean.md](../../ft/coder/1-WIP/PRD-2026-10-09-reshape-extract-method-clean.md) (this PRD)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md) — `## Rust operations (v1)`, `## LSP integration`, `## The tidy`, `## Known limitations`

## Summary

After rust-analyzer's "extract into function" and the engine's rename, the function it introduced is cleaned up before the
edit is produced: comments the assist dropped are put back, `&PathBuf`/`&String`/`&Vec<T>` parameters are narrowed
(verified by the server's type diagnostics), a unit tail is taken out of `Ok(…)`, `x: x` becomes `x`, and a type the file
spells qualified is spelled the same way. The tidy removes an unused `mut`. The type probe skips a leading `{` or comment;
a function the server named after a `let` is found and renamed; `extract_module` carries the parent's `use … as _;`; and a
server panic is the server's defect. A mid-function run of `return Err(..)` guards becomes a function returning
`Result<()>` called with `?`, and a tail-position arm is extracted with its `return`s.

## Background

#524's extract-method plans compiled and still needed hand edits: 36 comment lines restored across `09b` and `10b`, 18+
clippy findings of four mechanical shapes, two signatures naming an unimported type, one split missing its
`use prost::Message as _;`, two hangs and one false refusal. `#reshape` 16 and 19 shrink 25 functions with this operation;
every defect here would be paid per extraction there. Exploration 2 reproduced each shape against the dev shell's
rust-analyzer and corrected three of the backlog's claims (P needs a `?` in the range; R is a 30 s false refusal since #542
on a ready backend; S is the server naming the function after the `let`).

## Responsibility

- A mid-function run of `return Err(..)` (or `return None`) guards extracts into a function returning `Result<()>` /
  `Option<()>` called with `?`; a tail-position arm extracts with its `return`s verbatim; every other range holding a
  `return` is refused as today, naming it — by `check`, `check --deep` and `apply` alike.

- The function `extract_method` writes keeps every comment its range held, in place, or the operation is refused naming
  the comments it would lose.
- That function's signature and body are free of `clippy::ptr_arg`, `clippy::unit_arg` and `clippy::redundant_field_names`
  of the shapes the assist produces, and of rustc's `unused_mut` once the tidy has run; arity and tuple returns are noted.
- A type its signature names resolves in the file without a new `use`, or the operation is refused naming the type.
- A range opening on `{` or a comment is probed where it can be typed; a range that is a `let`'s initializer is renamed
  to the plan's name; an LSP `-32603` is a `ServerDefect`.
- `extract_module` carries the parent's anonymous trait imports into the new module.
- Register the two new live binaries in `.config/rust-e2e.filterset` **and** the `rust-analyzer` group of `.config/nextest.toml`.

## The rules (the contract)

All rules act on **the introduced function only**: the `fn <name>` the rename produced, from its keyword to its closing
brace (`span::function_span`), read over text with comments and literals masked (`early_return::masked_to_code`). The
origin function and the rest of the file are untouched, except for the call site the assist wrote.

**1. Comments (P).** `comments::dropped(original, range, produced)` lists the `//` and `/* */` comments of the range that
the produced function lacks (multiset, trimmed text). For each:
- a **full-line comment** (or run of them) is written before the produced statement paired with the range statement it
  preceded, at that statement's indentation;
- a **trailing comment** is appended, after one space, to the last line of the produced statement paired with its own;
- pairing: the range's **top-level** statements (split on `;` and on a closing `}` at depth 0 of the range, masked) and the
  produced body's top-level statements are paired **in order**; the produced body has exactly one more (the assist's tail)
  when the range has outputs, the same count otherwise.
- A count that does not match, or a comment whose anchor statement is the range's last when the produced last statement is
  the assist's tail, **refuses** the operation (`ServerDefect`): `rust-analyzer dropped N comment(s) from the extracted
  function, and its statements cannot be paired with the range's to put them back: \`// …\`, …`. Nothing is written (F1).
- `check` and `check --deep`: `comments::at_risk(text, range)` counts the range's comments between or after top-level
  statements when the range holds a `?` (masked). A count > 0 prints the progress line `the extracted function will keep N
  comment(s) rust-analyzer drops`. Not a finding.

**2. `unit_arg` (Q).** When the introduced function's return type is `Result<(), …>` (resp. `Option<()>`) and its tail is
`Ok(<e>)` (resp. `Some(<e>)`) where `<e>` begins with `if`, `match`, `loop`, `while`, `for`, `unsafe {` or `{`: the tail
becomes `<e>` then a new line `Ok(())` (resp. `Some(())`) at the body's indentation. `lints::unit_tail_unwrapped`.

**3. `redundant_field_names` (Q).** Inside a struct-literal brace (a `{` directly after a path, masked) in the introduced
function, a field `ident: ident` with the same identifier on both sides becomes `ident`. `lints::field_shorthand`.

**4. Notes (Q).** More than 7 parameters (counting `self` as clippy does not: `&self` excluded), or a return type naming a
tuple of 3 or more elements: one note each, `\`<name>\` takes N parameters; clippy::too_many_arguments fires above 7` /
`\`<name>\` returns a N-tuple; clippy::type_complexity may fire`. `lints::shape_notes`.

**5. `ptr_arg` (Q).** `ptr_args::narrowings(text, name)`: each parameter whose type is `&PathBuf`, `&std::path::PathBuf`,
`&String`, `&std::string::String`, `&Vec<T>` or `&std::vec::Vec<T>` (not `&mut`) becomes `&Path` (bare when the file binds
`Path` by `use`, `&std::path::Path` otherwise; a qualified spelling keeps its qualifier), `&str`, `&[T]`. In the body,
`<param>.clone()` becomes `<param>.to_owned()`. Then `RustBackend::verified_narrowings` sends the narrowed text to the
server (`did_change`) and pulls diagnostics; for every narrowing with a new `E0308` whose range lies in the introduced
function or on the call line, that narrowing is undone and a note is added:
`kept \`<param>: <type>\`: the body needs the type rust-analyzer wrote` (F2). The pull is asked once for all narrowings,
then once more after undoing, and the second answer must hold no new `E0308` in the function — otherwise the whole
narrowing is undone, with one note per parameter. No `use` is ever added.

**6. Respelling (K).** `RustBackend::respelled_signature_types`: the server's `unresolved_names` over the produced text,
restricted to the introduced function's **signature** (keyword to `{`) and to names not unresolved in `original`. For
each, `respell::spellings(origin_fn, name)` collects the distinct qualified paths ending in `::<name>` in the **origin
function's text** (signature and body, masked). Exactly one spelling: every unresolved occurrence in the signature is
replaced with it, and the unresolved count for that name must drop to zero, or the operation is refused. Zero or several
spellings: `SeamRefused`: `the extracted signature names \`<name>\`, which this file does not import, and the function it
came from spells it <none | as \`a::X\` and \`b::X\`>; import it or cut the range elsewhere` (F4).

**7. Probe (R).** `selection::hover_bearing_position` skips, from the range start and across lines up to the range end:
whitespace, a leading `{`, `//` comments to end of line, `/* … */` comments, then the existing `&`/`&mut`/`*`/`!`/`-`/`(`
rule. A range with nothing left keeps `range.start`. The range handed to the assist is unchanged.

**8. Server-named function (S).** `assist_for(ExtractMethod).placeholder.name` becomes `None`;
`introduced::introduced_by` with `name: None` finds the one declaration of `placeholder.keyword` (`fn` here, `let` for
`extract_variable`) declared more often after than before, inside the changed span (`introduced_declaration(keyword, …)`,
generalising today's `introduced_binding`). Messages: `rust-analyzer's extraction introduced no \`fn\` to name` /
`… introduced N \`fn\` declarations (…)`. The rename then runs as today, and is skipped when the server already chose the
plan's name.

**9. `as _` carry (I).** `prelude_shadow::shadowed_imports` also returns every `use <path> as _;` of the parent (including
a member `<path> as _` of a group, flattened), rebased by `rebased_for_child`, unless the child already has that line.
Written in the same pass, so `extract_module`'s existing call in `assisted_edit` is unchanged.

**10. Tidy `unused_mut` (Q).** After `tidy_imports` reports `Compiles` and before `rustfmt`: the diagnostics of that last
check are filtered to `code == "unused_mut"` with a `MachineApplicable` fix in a touched file; the fixes are applied (bytes
held in memory first), the tree re-checked; a broken re-check restores the bytes and fails the run
(`Tidied::Broken`); a clean one reports `tidied: removed N unused \`mut\`(s) from <file>` per file. One round: removing a
`mut` cannot make another unused.

**11. Panic class (U).** `map_lsp_error`: `LspError::Server { code: -32603, message }` → `RestructureError::ServerDefect(
format!("rust-analyzer failed answering this request: {message}"))`.

**12. Guard lift (developer addition, 2026-10-09).** `guards::classify(text, range) -> Exits` reads the range (masked; a
`return` inside a closure, `async` block or nested `fn` does not count, as in `early_return::early_returns`) and the
enclosing function's header:

| `Exits` | When | What happens |
|---|---|---|
| `None` | no `return` | today's path |
| `Tail` | the range runs to the function's tail (today's exception), **or** it is the whole body of a match arm or `if`/`else` branch whose `match`/`if` is itself in tail position, recursively (rule 13) | the assist keeps the `return`s and the call is the value |
| `ErrGuards` | every `return` is `return Err(<e>)` (with or without `;`, at any depth inside `if`/`match`/loops); the enclosing return type is `Result<…>`, a path ending `::Result<…>`, or a one-argument alias the file binds (`Result<T>`); no binding declared at the range's top level is named after the range in the enclosing function (masked, whole word); no labelled `break`/`continue` | lifted |
| `NoneGuards` | the same, with every `return` being `return None` and an `Option<…>` return type | lifted (F9) |
| `Refused(…)` | a value `return` (F7), a mix (F8), a top-level binding read after the range (F10), error guards in a function that does not return `Result` | `SeamRefused`, naming each line or binding (texts below) |

Refusal texts (prefix `this seam cannot be cut here: `):
- F7: today's text, unchanged (`the range returns early from the function around it, on line N (…)`), so the existing
  `refuses_a_range_that_returns_early_from_the_enclosing_function` stays green.
- F8: ``the range holds error guards and a return of a value, on line N (`…`). Only a run whose every return is
  `return Err(..)` can become a function called with `?`; end the range before line N.``
- F10: ``the range's guards are followed by code that reads `<binding>`, which the range declares. Cut the run so it
  ends before `let <binding>`.``

The lift runs in `RustBackend::cleaned_extraction`, before rule 1. rust-analyzer writes one of two shapes for an
`ErrGuards` range, both reproduced in Exploration 2 §2b.

**Shape B** — the range also holds a `?`. The server writes the lifted form itself: `fn fun_name(..) -> Result<(), E>`, the
`return Err(..)` guards verbatim, the tail `Ok(())`, and the call `fun_name(..)?;`. The lift accepts it as written; only
rule 14 respells it.

**Shape A** — the range holds no `?` (`fun_name` before the rename):

```rust
    if let Some(value) = fun_name(b, n) {
        return value;
    }
fn fun_name(b: &str, n: u32) -> Option<Result<u32, String>> {
    if n > 10 {
        return Some(Err(format!("too big {n}").into()));
    }
    …
    None
}
```

`guards::lifted(origin_header, produced, name, exits)` rewrites exactly shape A:
- the call statement `if let Some(<v>) = <name>(<args>) { return <v>; }` becomes `<name>(<args>)?;`;
- the return type `Option<Result<T', E>>` becomes the caller's return-type spelling with its first argument replaced by
  `()` (`Result<()>`, `crate::Result<()>`, `Result<(), String>`);
- each `return Some(Err(<e>))` in the new function becomes `return Err(<e>)`, and its tail `None` becomes `Ok(())`.

For `NoneGuards` the same rewrite applies with `Option<Option<T>>`, `return Some(None)` → `return None` and `None` →
`Some(())`.

Any shape other than A or B — no `if let Some` and no `?` call, a return not wrapped in `Some(`, a different tail — is
**refused** as
`rust-analyzer's answer was unusable:`, naming what was expected, and nothing is written. `?` on `Result<(), E>` inside
`Result<T, E>` is the identity `From`, so the early exit returns the same value from the same call.

**13. Tail position.** `guards::in_tail_position(text, range)` holds when:
- the range ends at the end of a block's tail expression;
- that block is a match arm body, an `if`/`else` branch or a plain block;
- that construct is itself the tail of a block in tail position, all the way up to the function body.

rust-analyzer keeps the `return`s verbatim and writes the call as the arm's value (reproduced: `_ => { fun_name(k, v) }`).
This is today's exception applied one level down.

**14. Return-type spelling (every `extract_method`).** This applies to **every** function `extract_method` writes,
whatever its `Exits` — including a plain range that propagates with `?` and holds no `return`.
`return_type::respelled_return_type(origin_header, produced, name)` runs last in `cleaned_extraction`.

- **When it rewrites:** the caller's return type is spelled `<path>Result<T>` with **one** generic argument (an alias such
  as `crate::Result`, `anyhow::Result`, a module's own `Result<T>`), and rust-analyzer wrote the new function's return type
  as `<any path>Result<X, E>`. The new type is `<path>Result<X>` in the caller's spelling.
- **Otherwise it changes nothing:** a caller written `Result<T, E>`, an `Option`, or a return type that is not a `Result`.

The reason, reproduced: when the alias is **imported** (`use crate::aliased::Result;`), rust-analyzer writes
`fn fun_name(..) -> Result<(u32, u32), String>`, which is `E0107` (the name in scope takes one argument). Every one of
`#reshape` 16's nine files imports this crate's `Result`, and about nine of its seams in `check_plan` and `apply_held_plan`
propagate with `?`. When the alias is **defined in the same module**, the server writes `std::result::Result<u32, String>`.
That compiles, and it is respelled to `Result<u32>` too, so the output reads like the caller. `E` is not compared: the
function's `?`s and returns propagate into the caller's error type, so the server's `E` is the alias's.

## Boundaries

- **No parameter struct, type alias or `use` line is written.** Arity and tuple returns are noted, never fixed.
- **Nothing outside the introduced function is rewritten** by rules 1-6 except the assist's own call line (rule 5 reads it).
- **No compiler-guided import repair** (developer decision F5): the compile gate is untouched (`runner/compile_gate.rs`,
  node 10's). Deferred to `2026-10-09-restructure-compiler-guided-import-repair.md`.
- **The wait is untouched**: `READY_HOVER_BOUND`, `await_answer`, `ensure_indexed` are node 10's (`apply-robust`), which
  extends the bound to every inference wait.
- **The tidy's existing steps are untouched**; `runner/tidy.rs` gains one call (its size is `#reshape` 15's).
  `runner/tidy/gating.rs` and `format.rs` are node 3's.
- **No growth of** `imports.rs`, `early_return.rs` (only `early_returns` widened to `pub(super)`), or any function on the
  over-60 list: `resolve_opening`'s and `check`'s `refuse_early_returns` call becomes
  `extracted_fn::refuse_unliftable_returns`, one line for one line.
- **No lift with outputs**, of `let … else` guards that bind, of labelled `break`/`continue`, or of a `return` a macro
  expands to (`bail!`, not seen: stated limit).
- **No change** to `extract_variable`'s borrow handling, the early-return or unit-tail refusals, the function-local `use`
  carry, or any move operation. **No plan field, no wire change, no new `RefactorKind`.**

## Dependencies

This node has no parent: it consumes no other `#reshape` node's behaviour, and its base (`feature/reshape/tidy-facades`)
is a line position only (`#reshape` 3 edits `runner/tidy/gating.rs`/`format.rs` and `prelude_shadow.rs`'s neighbours;
the overlap is textual).

## Draft PR contract

Published with the wave-2 contract commit (the first push of this PR's code); **owned surface, new today** (all
crate-private under `backends::rust` / `runner::tidy` unless noted):

- `backends/rust/extracted_fn.rs`:
  `impl RustBackend { fn cleaned_extraction(&mut self, uri: &str, original: &str, named: &str, range: Range, name: &str) -> Result<Cleaned> }`
  with `pub(super) struct Cleaned { pub text: String, pub notes: Vec<String> }` — runs the function-local `use` carry
  (moved here from `assisted_edit`), then rules 1, 6, 5, 2, 3, 4 in that order;
  `impl RustBackend { fn verified_narrowings(&mut self, uri: &str, text: &str, name: &str) -> Result<Cleaned> }`;
  `impl RustBackend { fn respelled_signature_types(&mut self, uri: &str, original: &str, text: &str, range: Range, name: &str) -> Result<String> }`;
  `pub(super) fn extract_method_findings(text: &str, range: Range, progress: &ProgressSink) -> Vec<String>` (replaces the
  block in `check`).
- `extracted_fn/span.rs`: `pub(super) struct FnSpan { pub keyword: usize, pub open: usize, pub close: usize }`,
  `pub(super) fn function_span(text: &str, name: &str) -> Option<FnSpan>`.
- `extracted_fn/comments.rs`: `pub(super) fn dropped(original: &str, range: Range, produced: &str, name: &str) -> Vec<Comment>`,
  `pub(super) fn restored(original: &str, range: Range, produced: &str, name: &str) -> Result<(String, usize)>`,
  `pub(super) fn at_risk(text: &str, range: Range) -> usize`.
- `extracted_fn/lints.rs`: `pub(super) fn unit_tail_unwrapped(text: &str, name: &str) -> String`,
  `pub(super) fn field_shorthand(text: &str, name: &str) -> String`, `pub(super) fn shape_notes(text: &str, name: &str) -> Vec<String>`.
- `extracted_fn/ptr_args.rs`: `pub(super) struct Narrowing { pub parameter: String, pub from: String, pub to: String }`,
  `pub(super) fn narrowings(text: &str, name: &str) -> Vec<Narrowing>`,
  `pub(super) fn narrowed(text: &str, name: &str, chosen: &[Narrowing]) -> String`.
- `extracted_fn/respell.rs`: `pub(super) fn spellings(origin_fn: &str, name: &str) -> Vec<String>`.
- `extracted_fn/guards.rs`: `pub(super) enum Exits { None, Tail, ErrGuards, NoneGuards, Refused(String) }`,
  `pub(super) fn classify(text: &str, range: Range) -> Exits`,
  `pub(super) fn in_tail_position(text: &str, range: Range) -> bool`,
  `pub(super) fn lifted(origin_header: &str, produced: &str, name: &str, exits: &Exits) -> Result<String>`,
  `extracted_fn.rs`: `pub(super) fn refuse_unliftable_returns(text: &str, range: Range) -> Result<()>`.
- `extracted_fn/return_type.rs` (new; rule 14 for every extraction):
  `pub(super) fn respelled_return_type(origin_header: &str, produced: &str, name: &str) -> String`.
- `early_return.rs`: `early_returns` becomes `pub(super)` (visibility only).
- `introduced.rs`: `fn introduced_declaration(keyword: &str, original: &str, extracted: &str) -> Result<Introduced>`
  (replaces `introduced_binding`).
- `prelude_shadow.rs`: `fn anonymous_trait_imports(parent: &str) -> Vec<String>`.
- `runner/tidy/unused_mut.rs`: `pub(super) fn remove_unused_mut(tidying: &Tidying<'_>, diagnostics: &[Diagnostic]) -> Result<Removed>`,
  `pub(super) enum Removed { Nothing, Compiles(Vec<Diagnostic>), Broken(Failure) }`.
- `lsp_bridge.rs`: `const INTERNAL_ERROR: i64 = -32603;` and its arm in `map_lsp_error`.
- Test harness (`tests/harness/mod.rs`): `pub fn assert_clippy_clean(fixture: &AFixtureWorkspace)`,
  `pub fn a_workspace_whose_test_binary_carries_an_unused_mut() -> AFixtureWorkspace`.
- Failing tests: acceptance tests 1-35 below (27 is a green pin), all red on the first push; unit tests beside each new function.

## Green wave

**Wave:** 1 of 4.
**Greenable independently:** yes — no parent; it can go green on `master` alone.
**Concurrent with:** every other wave-1 node (`widen-same-crate`, `multi-seam-extract`, `tidy-facades`, `move-children`,
`methods-leave-type`, `move-widen`, `move-grouped-use`, `new-crate`, `apply-robust`, `move-item-paths`, `anchors-outline`).
Textual overlap: `backends/rust.rs` (all), `prelude_shadow.rs`/`introduced.rs` (`multi-seam-extract`), `runner/tidy*`
(`tidy-facades`), `readiness.rs`'s neighbourhood (`apply-robust`).
**Blocks:** `feature/reshape/fn-sizes-rest` (K=16) and `feature/reshape/fn-sizes-backend` (K=19).
Real edges (whole stack): `1→13`, `5→14`, `2→15`, `3→15`, `4→16`, `13→17`, `2→17`, `3→17`, `17→18`, `6→18`, `4→19`, `17→19`.

## Successor PRs

- `feature/reshape/fn-sizes-rest` — shrinks the 8 functions >60 lines outside `backends/` with `extract_method`. It
  consumes the guard lift (rule 12) for `plan/codec.rs::parse_op` (206 lines, mostly `return Err(malformed(…))` guards)
  and the tail-position extension (rule 13) for `plan_store/refresh.rs::refreshed`'s `Item` arm (its `return Ok(anchor)`s
  are value returns, which only tail position allows), plus the comment carry and lint clean-ups for every extraction.
- `feature/reshape/fn-sizes-backend` — shrinks the 17 functions >60 lines in `backends/` the same way. Per its own
  measurement:
  - the guard lift (rule 12) reaches only `retarget_impl`'s guard (`retarget_impl.rs:96-101`);
  - the dispatchers `resolve_opening` and `check` return values, so it cuts them with tail ranges (today's exception,
    and rule 13);
  - `assisted_edit` has no `return Err`;
  - it relies on rule 14's return-type spelling for about 30 cuts that propagate with `?`.

## Scope

- [ ] **Probe and naming** (R, S): `selection.rs`, `introduced.rs`, the placeholder's name
- [ ] **Comment carry** (P): `extracted_fn/{span,comments}.rs`, the `check` progress line
- [ ] **Lexical clean-ups** (Q): `extracted_fn/lints.rs`
- [ ] **Narrowing** (Q): `extracted_fn/ptr_args.rs`, `verified_narrowings`
- [ ] **Respelling** (K): `extracted_fn/respell.rs`, `respelled_signature_types`
- [ ] **`as _` carry** (I / N3): `prelude_shadow.rs`
- [ ] **Tidy `unused_mut`**: `runner/tidy/unused_mut.rs`, one call in `runner/tidy.rs`
- [ ] **Panic class** (U): `lsp_bridge.rs`
- [ ] **Guard lift and tail position** (rules 12-14): `extracted_fn/guards.rs`, `refuse_unliftable_returns`
- [ ] **Registration**: `extract_method_clean_acceptance` and `extract_method_guard_lift_acceptance` in `.config/rust-e2e.filterset` and the `rust-analyzer` group
- [ ] **Package documentation** at wrap (list under Affected Packages)
- [ ] **Testing**: acceptance tests pass; `./test -p tddy-code-restructuring`, scoped; CI for the rest
- [ ] **Code quality**: `cargo check -p tddy-code-restructuring --all-targets`, `cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings`, `cargo fmt`; every new file ≤ 500 production lines; no over-60 function grows

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical changes

### State A (Current)

- `assisted_edit`'s `!relocates` branch (`backends/rust.rs:1596-1604`) runs only `imports::carry_function_local_uses` for
  `extract_method`: no import pass, no comment check, no signature clean-up.
- The assist table names the `extract_method` placeholder `fun_name` (`backends/rust.rs:291-301`); `introduced_by` finds a
  server-named symbol only for `let` (`introduced.rs:70-110`) → S's refusal (`introduced.rs:49`).
- The probe skips `&`, `&mut`, `*`, `!`, `-`, `(` on the range's first line only (`selection.rs:15-46`); a hover at `{` or
  `//` is `null`, so a ready backend refuses after `READY_HOVER_BOUND` (`readiness.rs:26,187`) and a backend whose warm-up
  was skipped waits unbounded (`readiness.rs:68-70`).
- `shadowed_imports` carries a parent binding only when the moved code names it; `as _` binds nothing
  (`prelude_shadow.rs:47-62`, `imports/bound_names.rs:8`) → I.
- The tidy applies `unused_imports` only (`runner/tidy.rs:412-417`); `unused_mut` is reported as `warning remains:`
  (`tidy.rs:526-540`).
- `refuse_early_returns` (`early_return.rs:34-60`) refuses every range holding a `return` unless it runs to the end of
  the function's tail (and the function is not `()`). rust-analyzer's output for a mid-function range is
  `Option<Result<T, E>>` + `if let Some(value) = … { return value; }` (reproduced in Exploration 2 §2b, and described in
  the doc of `refuses_a_range_that_ends_with_the_functions_last_return`, `tests/extract_method_control_flow_acceptance.rs`).
- `map_lsp_error` maps every unclassified server error to `MalformedPlan` (`backends/lsp_bridge.rs:136-153`).
- rust-analyzer 2026-03-30 drops inter-statement and trailing comments from a range holding `?`, writes `&PathBuf`/
  `&String`/`&Vec<T>`, copies an unneeded `let mut`, wraps a unit tail in `Ok(…)`, writes `x: x`, and prints a short name
  for a type the file spells qualified (Exploration 2 §2).

### State B (Target)

Rules 1-11 hold. The #524 shapes extract with comments in place, lint-clean after the tidy, and compile with no hand edit.

### Delta (What's Changing)

#### `tddy-code-restructuring`
- **New** `backends/rust/extracted_fn.rs` (~120: `cleaned_extraction`, `verified_narrowings`, `respelled_signature_types`,
  `extract_method_findings`), `extracted_fn/span.rs` (~60), `comments.rs` (~180), `lints.rs` (~150), `ptr_args.rs` (~140),
  `respell.rs` (~60).
- **`backends/rust.rs`**: `mod extracted_fn;`; `Placeholder { keyword: "fn", name: None }` for `ExtractMethod`; in `check`,
  the `ExtractMethod` block (`:1154-1166`) becomes `findings.extend(extracted_fn::extract_method_findings(&text, planned,
  &self.progress))`; in `assisted_edit`, the `named` rebinding (`:1599-1603`) becomes one `self.cleaned_extraction(…)` call
  whose notes are returned in place of `Vec::new()`.
- **`selection.rs`**: `hover_bearing_position` reads across lines (one new private helper).
- **`introduced.rs`**: `introduced_declaration` generalises `introduced_binding` by keyword.
- **`prelude_shadow.rs`**: `anonymous_trait_imports`, chained into `shadowed_imports`.
- **New** `runner/tidy/unused_mut.rs` (~120); **`runner/tidy.rs`**: `mod unused_mut;` and one call in `tidy`.
- **`backends/lsp_bridge.rs`**: one constant, one match arm.
- **New** `extracted_fn/guards.rs` (~220: classification, tail position, the lift's rewrite of shape A, acceptance of
  shape B) and `extracted_fn/return_type.rs` (~70: rule 14, for every extraction).
- **`early_return.rs`**: `early_returns` → `pub(super)`. **`backends/rust.rs`**: the `refuse_early_returns` call in
  `resolve_opening` → `extracted_fn::refuse_unliftable_returns` (and the same inside `extract_method_findings`).

## Implementation milestones

- [ ] **M1** probe (rule 7) and server-named function (rule 8); tests 12-14, unit tests in `selection.rs`, `introduced.rs`
- [ ] **M2** `span.rs`, comment carry and its refusal, `check` line (rule 1); tests 1-3, 22
- [ ] **M3** lexical clean-ups (rules 2-4); tests 7-9
- [ ] **M4** narrowing and its verification (rule 5); tests 4-6
- [ ] **M5** respelling (rule 6); tests 10-11
- [ ] **M6** `as _` carry (rule 9); test 16
- [ ] **M7** tidy `unused_mut` (rule 10); tests 18-20, and the end-to-end tests 15, 17
- [ ] **M8** panic class (rule 11); test 21
- [ ] **M9** guard lift and tail position (rules 12-13); tests 23-33, unit tests in `guards.rs`
- [ ] **M9b** return-type spelling for every extraction (rule 14); tests 24, 34-35, unit tests in `return_type.rs`
- [ ] **M10** registration, docs staged, scoped gate, length and function-size gates

## Testing plan

### Testing Strategy

**Unit tests on text** for every pure function (`span`, `comments`, `lints`, `ptr_args`, `respell`, the probe,
`introduced_declaration`, `anonymous_trait_imports`), fed the verbatim outputs Exploration 2 recorded — milliseconds, exact.
**One live binary** (`extract_method_clean_acceptance`) resolves and applies through a real rust-analyzer, with
`assert_compiles` as the oracle and one `assert_clippy_clean` end-to-end. **Tidy tests without a server** through a
test-binary move (`apply_tidy_acceptance.rs`'s existing pattern).

#### Option 1 (chosen): text units + one live binary + server-free tidy tests
**Trade-off**: the live binary is load-sensitive and serialised (`rust-analyzer` group); the units carry the edge cases.

#### Option 2 (rejected): assert only through `restructure verify` and the compile gate
Would leave every Q shape unasserted — they compile.

### Coverage Requirements

- [ ] Happy: each rule's reproduced shape; the #524 `09b` shape end-to-end under clippy
- [ ] Refusals: unpairable statements (rule 1), zero / two spellings (rule 6)
- [ ] Kept as written: a parameter the body needs owned, `&mut PathBuf`, a range with no `?` (comments untouched, no
  duplication), an `extract_variable` probe on `&`
- [ ] Actual effects: bytes on disk, `cargo check --all-targets`, clippy `-D warnings`

## Acceptance tests

Names read as behaviour specifications. All are **red on `master`**, for the reason given.

### `packages/tddy-code-restructuring/tests/extract_method_clean_acceptance.rs` (new live binary; `performing` / `resolving`, `assert_compiles`)

1. `comments_between_the_statements_of_a_range_holding_a_question_mark_stay_beside_their_statements` — the async `.await?`
   fixture of Exploration 2. *Red*: both comments are gone.
2. `a_trailing_comment_stays_at_the_end_of_its_statement` — `let p = dir.join("x"); // trailing note`. *Red*: dropped.
3. `a_comment_inside_a_statement_is_kept_once` — the `// only the held ones` in a method chain appears exactly once.
   *Red today only through test 1's fixture failing; pins no duplication.*
4. `borrowed_path_string_and_vec_locals_are_taken_as_path_str_and_slice_and_the_tree_compiles` — `fn
   kept_joined(dir: &Path, label: &str, items: &[u32])`. *Red*: `&PathBuf`, `&String`, `&Vec<u32>`.
5. `a_borrowed_path_the_range_clones_becomes_an_owned_copy_of_the_path` — `dir.to_owned()`. *Red*: `&PathBuf` + `.clone()`.
6. `a_parameter_the_body_passes_where_a_path_buf_is_needed_keeps_its_type_and_says_so` — note text and `assert_compiles`.
   *Red*: no note is produced.
7. `a_unit_body_holding_a_question_mark_ends_in_ok_unit_rather_than_wrapping_the_if` — no `Ok(if`, ends `Ok(())`. *Red*.
8. `a_field_initialised_from_a_parameter_of_its_own_name_is_written_in_shorthand` — `Lookup { base, root }`. *Red*.
9. `an_extraction_taking_more_than_seven_parameters_says_so_in_a_note` — the note text. *Red*: no note.
10. `a_trait_the_origin_spells_qualified_is_spelled_the_same_way_in_the_new_signature_and_compiles` —
    `dyn deep::recipe::WorkflowRecipe + 'static`, no `use` added. *Red*: `E0405` on `WorkflowRecipe`.
11. `a_type_the_origin_spells_two_ways_is_refused_naming_both_spellings_and_nothing_is_written`. *Red*: applied, broken.
12. `a_range_opening_on_a_blocks_brace_is_extracted_within_the_ready_bound` — under 60 s, compiles. *Red*: refused after 30 s.
13. `a_range_opening_on_a_line_comment_is_extracted_within_the_ready_bound_and_keeps_the_comment`. *Red*: same.
14. `a_range_that_initialises_a_let_is_extracted_under_the_plans_name` — `fn managed_recipe_for`. *Red*: "did not produce
    a `fn fun_name`".
15. `an_extraction_shaped_like_plan_09b_passes_clippy_with_warnings_denied` — `.await?`, `&PathBuf` ×2, a copied `mut`,
    two comments; applied through the runner (so the tidy runs), then `assert_clippy_clean`. *Red*: `ptr_arg`, `unused_mut`.
16. `an_extract_module_whose_seam_calls_a_trait_method_the_parent_imports_as_underscore_compiles` — a local trait with a
    provided method, `use crate::wire::Encode as _;` in the parent. *Red*: `E0599`.
17. `an_apply_leaves_no_unused_copy_of_the_carried_trait_import` — the sibling seam that calls nothing has no
    `as _` line after the tidy. *Red through 16; pins the tidy pruning the over-import.*

34. `a_range_propagating_with_a_question_mark_returns_the_callers_imported_result_alias_and_compiles` — the `imported`
    fixture of Exploration 2 §2b (`use crate::aliased::Result;`, `let y = parse(b)?; let z = x + y;`): `-> Result<(u32, u32)>`,
    `assert_compiles`. *Red*: `Result<(u32, u32), String>`, `E0107`.
35. `a_range_in_a_module_that_defines_its_result_alias_is_spelled_with_the_alias` — the alias defined at the crate root
    of the file: `-> Result<u32>`. *Red*: rust-analyzer writes `Result<u32, String>` there (measured on the first push;
    the `std::result::Result` spelling of Exploration 2 §2b was for an alias defined in an inner module), which is `E0107`
    as well.

### `packages/tddy-code-restructuring/tests/apply_tidy_acceptance.rs` (existing; no rust-analyzer)

18. `removes_an_unused_mut_from_a_file_the_run_wrote` — fixture `a_workspace_whose_test_binary_carries_an_unused_mut`.
    *Red*: the `mut` remains and is reported as `warning remains:`.
19. `says_which_file_it_removed_an_unused_mut_from` — `tidied: removed 1 unused \`mut\`(s) from <file>`. *Red*.
20. `a_dry_run_removes_no_unused_mut`. *Red only in that the fixture is new; pins the dry-run boundary.*

### Library level, in the crate

21. `packages/tddy-code-restructuring/src/backends/lsp_bridge.rs` (`mod tests`):
    `a_server_that_panics_answering_is_the_servers_defect_not_a_malformed_plan`. *Red*: `MalformedPlan`.
22. `packages/tddy-code-restructuring/src/backends/rust/extracted_fn/comments.rs` (`mod tests`):
    `statements_that_cannot_be_paired_refuse_naming_every_comment_that_would_be_lost`. *Red*: the module does not exist.

### `packages/tddy-code-restructuring/tests/extract_method_guard_lift_acceptance.rs` (new live binary; registered with the other in `.config/rust-e2e.filterset` and the `rust-analyzer` group)

23. `a_run_of_error_guards_in_the_middle_of_a_function_becomes_a_function_returning_result_unit_called_with_a_question_mark`
    — the `guards` fixture of Exploration 2 §2b: `fn guards_checked(b: &str, n: u32) -> Res<()>` and `guards_checked(b, n)?;`,
    `assert_compiles`. *Red*: refused today (`returns early … on line …`).
24. `the_lifted_function_is_spelled_with_the_callers_one_argument_result_alias` — a fixture with `type Result<T> =
    std::result::Result<T, Failure>;` and `use crate::Result;` in the file: `-> Result<()>`, compiles. *Red*: refused.
25. `an_error_guard_inside_a_loop_is_lifted_with_its_loop` — `for c in b.chars() { if c == 'x' { return Err(..); } }`.
    *Red*: refused.
26. `a_run_of_none_guards_in_an_option_function_is_lifted_to_option_unit` (F9). *Red*: refused.
27. `a_mid_function_range_returning_a_value_is_still_refused_naming_the_return` (F7) — **green pin**: the text of the
    existing `refuses_a_range_that_returns_early_from_the_enclosing_function`.
28. `a_range_mixing_error_guards_and_a_value_return_is_refused_naming_the_value_return_and_nothing_is_written` (F8). *Red*:
    today's refusal names every return; the new text names only the value return.
29. `a_guard_run_whose_binding_is_read_after_it_is_refused_naming_the_binding` (F10). *Red*: today's text.
30. `a_range_that_is_a_tail_match_arms_body_is_extracted_with_its_returns_verbatim` — the `arms` fixture of Exploration 2
    §2b (`refreshed`'s `Item` arm shape: a `let … else { return Ok(..) }` and a `return Err(..)`): `_ => { arm_value(k, v) }`,
    `assert_compiles`. *Red*: refused.
31. `a_parse_op_shaped_guard_run_passes_clippy_with_warnings_denied` — three `if … { return Err(malformed(…)); }` guards
    and a `for` guard; applied through the runner, `assert_clippy_clean`. *Red*: refused.

33. `a_guard_run_that_also_propagates_with_a_question_mark_is_lifted_and_called_with_a_question_mark` — the `mixed_q`
    fixture of Exploration 2 §2b (`check_len(b)?;`, an `if … { return Err(..); }`, `check_len(&b[1..])?;`) in a file that
    imports a one-argument `Result` alias: `fn …(..) -> Result<()>`, the call `…(b, n)?;`, `assert_compiles`, and
    `assert_clippy_clean`. This is `#reshape` 16's shape (two such ranges). *Red*: refused today (`returns early …`).

### Library level (guards)

32. `packages/tddy-code-restructuring/src/backends/rust/extracted_fn/guards.rs` (`mod tests`):
    `a_static_check_accepts_the_ranges_resolve_lifts_and_refuses_the_ranges_resolve_refuses` — `classify` over the six
    `Exits` cases through `extract_method_findings`, with the exact refusal texts. *Red*: the module does not exist.

## Technical Debt & Production Readiness

(empty; populated during development)

## Decisions & Trade-offs

Taken by the developer (2026-10-09, PRD review, all with the plan's recommendation):
- **F1** — unpairable statements **refuse** the operation (no extraction without its comments).
- **F2** — a narrowing the server's type diagnostics reject is **undone with a note**, not refused: the server's own
  output compiles.
- **F3** — an unused `mut` is **the tidy's** (rustc's `unused_mut`, `MachineApplicable`); rust-analyzer reports none, also
  with experimental diagnostics enabled.
- **F4** — an unimported signature type is **respelled as the origin function spells it**; no `use` is added; zero or two
  spellings refuse.
- **F5** — **two narrow import fixes** (rules 6 and 9), **no compiler-guided repair**; that becomes this node's deferral
  todo `2026-10-09-restructure-compiler-guided-import-repair.md`.
- **F6** — the harness gains **one clippy assertion** (`assert_clippy_clean`), used by test 15.

Decisions taken by this plan: rules act on the introduced function only; `.clone()` becomes `.to_owned()` for all three
narrowed types; S is fixed by finding the server-named `fn`, not by refusing; U maps `-32603` only (other codes keep
today's class).

**Decided** (the guard lift, added by the developer on 2026-10-09; the recommendation of each was accepted on 2026-10-09):
- **F7 — a `return` of something other than `Err`** (`return Ok(v)`, `return v`, `return;`).
  - (a) **Refuse it in the middle of the function, as today; extract it verbatim in tail position (rule 13)** —
    *recommended*. In tail position the `return` already returns from the caller, so nothing changes meaning, and
    `refreshed`'s `Item` arm needs exactly that. In the middle, no `?` form exists for a value.
  - (b) Lift into rust-analyzer's own `Option<T>` + `if let Some(value) = … { return value; }` form. It compiles, but it is
    the shape the clean-up exists to remove, and clippy flags it.
  - (c) Return a `ControlFlow`. It adds an `std::ops::ControlFlow` import and reads worse than the original.
- **F8 — mixed returns** (error guards and a value return in one range).
  - (a) **Refuse, naming the value returns**, so the author ends the run before them — *recommended*.
  - (b) Lift the error guards and handle the rest as in F7 (b): two rewrites in one function, hard to review.

  Tail position accepts a mix (rule 13), because every return keeps its meaning.
- **F9 — `return None` guards in an `Option` function.**
  - (a) **Lift them the same way, to `Option<()>` + `?`** — *recommended*: the same rewrite with `Some`/`None` swapped, a
    few lines.
  - (b) Refuse; lift error guards only.
- **F10 — a guard run that declares a binding the code after it reads.**
  - (a) **Refuse, naming the binding**, and defer it to the new todo
    `2026-10-09-restructure-extract-method-lifts-guard-runs-without-outputs-only.md` — *recommended*. The lifted function
    would return `Result<X>` with `let x = name(..)?;`, and rust-analyzer's output for outputs plus returns was not
    reproduced.
  - (b) Lift with outputs now, unmeasured.

## Refactoring Needed

### From @ft-dev (Acceptance Test Creation)
(empty)

### From @red (TDD Red Phase)
(empty)

### From @validate-changes (Change Validation)
(empty)

### From @validate-tests (Test Quality)
(empty)

### From @prod-ready (Production Readiness)
(empty)

### From @analyze-clean-code (Code Quality)
(empty)

### From @refactor (Completed Refactorings)
(empty)

## Validation Results

### Draft-PR contract push (wave 2, 2026-10-09)

Scoped to `tddy-code-restructuring`, on `feature/reshape/tidy-facades`'s commit 2:
`cargo check -p tddy-code-restructuring --all-targets`, `cargo clippy -p tddy-code-restructuring --all-targets -- -D
warnings` and `cargo fmt --all --check` are clean.

Only this node's binaries and filters were run (no full suite; baseline: `scratchpad/reshape/baseline-failures.txt`, none):

| Tests | Result | Why |
|---|---|---|
| `tests/extract_method_clean_acceptance.rs`, 19 tests (1-17, 34, 35) | 🔴 19 red | each on its behaviour: comments dropped (1-3, 15), `&PathBuf`/`&String`/`&Vec` kept (4, 5), no note (6, 9), `Ok(if` (7), `x: x` (8), short trait name (10), no refusal (11), the 30 s probe refusal (12, 13), did not produce a `fn fun_name` (14), `as _` not carried (16) and so the apply's compile gate (17), `Result<X, String>` under a one-argument alias (34, 35) |
| `tests/extract_method_guard_lift_acceptance.rs`, 10 tests (23-31, 33) | 🔴 9 red, 🟢 1 pin | 23-26, 30, 31, 33: today's `returns early` refusal; 28, 29: today's refusal text instead of the new one. **27 is the green pin** (the value-return refusal is unchanged) |
| `tests/apply_tidy_acceptance.rs`, 3 new (18-20) | 🔴 18, 19 red; 🟢 20 | 18-19: the `mut` survives and no `tidied:` line (`warning remains:` today). **20 is green by construction** (a dry run writes nothing) — a pin of the boundary, as the changeset says |
| `src/backends/lsp_bridge.rs` test 21 | 🔴 | `plan is malformed: lsp: lsp server error -32603: …` |
| `src/backends/rust/extracted_fn/comments.rs` test 22, `guards.rs` test 32, and every unit test in `extracted_fn/{span,comments,lints,ptr_args,respell,guards,return_type}.rs`, `introduced.rs` (`finds_the_function_the_assist_named_after_the_let_it_initialises`), `prelude_shadow.rs` (two `as _` tests) | 🔴 | `todo!()` in the surface |
| `selection.rs`: `a_range_opening_on_a_blocks_brace_is_probed_at_its_first_statement`, `a_range_opening_on_a_line_comment_is_probed_at_the_code_after_it` | 🔴 | the probe stays at `3:5` / `4:9` (no surface: the behaviour of `hover_bearing_position` changes) |

Every other test of `selection.rs`, `introduced.rs`, `prelude_shadow.rs`, `lsp_bridge.rs` stays green.

**Diverged from the contract, deliberately:**
- `assert_clippy_clean` was **not** added: the harness already has `assert_lints_clean` (`cargo clippy --workspace
  --all-targets -- -D warnings`), which tests 15, 31 and 33 use.
- The harness gained `the_function_named(text, name)` and `THE_UNUSED_MUT`, beside the planned
  `a_workspace_whose_test_binary_carries_an_unused_mut`.
- `comments::dropped` returns `Vec<Comment>` with `Comment { text, line, trailing }` (the contract named the type only).
- The uncalled surface is silenced with `#[allow(dead_code, reason = "TODO(reshape-extract-method-clean): …")]` on
  `mod extracted_fn;` (`backends/rust.rs`), `mod unused_mut;` (`runner/tidy.rs`), `INTERNAL_ERROR`
  (`lsp_bridge.rs`), `introduced_declaration` and `anonymous_trait_imports`. **Green removes each** with its first
  caller. `allow` rather than `cfg_attr(not(test), expect(…))`: the unit tests do not call every item, so an `expect`
  would be unfulfilled in one of the two builds.
- No wiring changed in this push: `check`, `resolve_opening` and `assisted_edit` still call today's code, so every
  existing test stays green.

## TODO

- [x] Record initial discovery (`2026-10-09-reshape-extract-method-clean-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (`docs/ft/coder/1-WIP/PRD-2026-10-09-reshape-extract-method-clean.md`)
- [x] Create changeset (this document)
- [ ] Add the PRD reference to `docs/ft/coder/1-OVERVIEW.md` **at wrap** (a shared append-point: not edited while planning)
- [x] Create failing acceptance tests (draft-PR contract push, #601)
- [x] Run acceptance tests (verify they fail) — see Validation Results
- [ ] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests (the surface's unit tests, `todo!()` bodies)
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the scoped tests (`./test -p tddy-code-restructuring`) — verify 100% pass; CI answers for the rest of the workspace
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; deletes the initial discovery, the
  extract-drops and lint-gate entries, and removes K and I from the apply-gaps entry
- [ ] USER REVIEW — work complete, decide next steps
