# Changeset: no function outside `backends/` runs past 60 lines, cut by the crate's own `extract_method`

**Date**: 2026-10-09
**Status**: 🚧 In Progress
**Type**: Refactor (behaviour-preserving; engine-driven function extractions) + a new source-scanning test gate
**Stack**: `#reshape` 16/19, branch `feature/reshape/fn-sizes-rest`, wave 2, PR [#613](https://github.com/uppin/tddy-coder/pull/613). PR title:
`refactor(code-restructuring): cut the nine functions past 60 lines outside the backend (#reshape 16/19)`.
Base in the linear stack: `feature/reshape/oversized-files` (K=15). **Real edges**: in `extract-method-clean -> fn-sizes-rest`
(K=4: the extractions rely on its guard lift, comment carry, signature clean-up and type respelling); out: none.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-09-reshape-fn-sizes-rest-initial-discovery.md)
(Exploration 1 is the whole-work discovery, whose Exploration 3 measured the function sizes; Exploration 2 is this node's:
per-function seams, parameter counts and collisions).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

`grep -rl 'Claimed by:'` over `packages/tddy-code-restructuring/docs/code-issues/` finds no record on any of the nine
functions: **no claimed issue is in the path, no wait-or-proceed fork.**

| Item | Verdict | What this change does about it |
|---|---|---|
| The nine functions >60 lines outside `backends/` (whole-work discovery, Exploration 3; no todo or code-issue record exists for them) | ✅ **RESOLVED HERE** (planned) | Cut to ≤ 60 by `extract_method`; pinned by the new gate (acceptance test 1) |
| [../../../packages/tddy-code-restructuring/docs/code-issues/complexity-rust-facade-lines.md](../../../packages/tddy-code-restructuring/docs/code-issues/complexity-rust-facade-lines.md) | — Unrelated | Under `backends/`; claimed by node 19 (`fn-sizes-backend`) |
| [2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md](../todo/2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md) | — Unrelated (consumed) | Resolved by `extract-method-clean`, which this node depends on. A defect of that kind found here is a new todo, not a reopen |
| `docs/dev/todo/2026-10-09-apply-held-plan-run-state-was-grouped-by-hand.md` (new, written with this node's planning commit) | ⚠ **DURING** | Records the one consented hand design edit (F2); stays open until an engine operation can group parameters |
| `docs/dev/todo/2026-10-09-restructure-has-no-operation-to-introduce-a-parameter-struct.md`, `docs/dev/todo/2026-10-09-restructure-check-budget-does-not-report-function-lengths.md` (new) | — Deferred | Proposed by this node; not resolved here |
| [2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md](../todo/2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md) | — Unrelated | File-length gate (node 15's). The new gate parses with `syn` and reads `#[cfg(test)]` per item, so it does not share the defect |

## Affected Packages

- **`tddy-code-restructuring`**: `src/plan/codec.rs`, `src/runner/entry_points/store_run.rs`,
  `src/runner/entry_points/check_entry_points.rs`, `src/crate_move/source_scan/sighting_walk.rs`,
  `src/crate_move/source_scan/module_items.rs`, `src/plan_store/refresh.rs`, `src/restructure_args.rs`,
  `src/crate_move/cluster.rs`, `src/crate_move/cluster/stranded.rs` (new private functions in the same files only); new
  `tests/function_length_budget.rs`; `Cargo.toml` (one dev-dependency: `proc-macro2 = { version = "1", features =
  ["span-locations"] }`, consented, F4). Docs at wrap:
  [README.md](../../../packages/tddy-code-restructuring/README.md) / package docs (the gate, one paragraph).
- **`tddy-tools`, `tddy-index-daemon`**: no source change (used to run the plans).

## Related Feature Documentation

- [PRD-2026-10-09-reshape-fn-sizes-rest.md](../../ft/coder/1-WIP/PRD-2026-10-09-reshape-fn-sizes-rest.md) (this PRD)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md) — `## Known limitations` (`extract_method`)

## Summary

Nine production functions outside `src/backends/` (`parse_op` 206, `apply_held_plan` 159, `check_plan` 115, `sightings`
95, `refreshed` 76, `options_for` 73, `stranded_siblings` 70, `resolve_cluster` 66, `items_of_module` 64) are cut to ≤ 60
lines by `extract_method` plans run with `tddy-tools restructure`. A new test fails on any production function past 60
lines outside `backends/`.

## Background

`/analyze-clean-code` says >60 lines must be refactored; nothing measures it. The developer added function sizes to the
`#reshape` scope (2026-10-09), and ordered the engine work first so that the size work costs engine operations, not hand
edits. `extract_method` is the operation; node 4 makes its output keep comments and pass clippy.

## Responsibility

- Re-measure the list on the post-node-15 base and re-read every seam (five of the nine are edited by earlier nodes).
- One restructure plan per function (one per guard for `parse_op`), each `check --deep` clean before `apply`.
- Seams of ≤ 7 inputs, never holding an outer-loop `break`/`continue`, and holding a `return` only as node 4 allows it: a
  guard whose every exit is `return Err(..)` (guard lift), or a range that runs to the function's tail.
- The one consented hand design edit in `apply_held_plan` (F2), marked `TODO(reshape-16)`, with its todo entry.
- The gate test `tests/function_length_budget.rs`.
- A todo entry per hand edit made after an engine operation; a stop-and-ask on every engine refusal.

## Boundaries

- **Engine-driven only.** Every cut is a `tddy-tools restructure apply` of an `extract_method` line. Hand edits only to
  make the tree build (or clippy-clean) after an operation; each gets a todo entry naming the plan, the op and the edit. A
  refusal stops the work and the developer is asked. F2(a) and F3's two-line fallback are hand design edits **only with
  consent**; F2(a) has it (2026-10-09), F3's fallback does not yet.
- **No behaviour change**: messages, refusals, output lines and public signatures are byte-identical. No test is edited.
- **No module structure change**: no new file or `mod`, no moved item, no new `use crate::…` edge. New functions are
  private, in the function's own file.
- **Not `backends/`** (node 19), **not 41–60** (deferred; node 19 writes the todo), **not file sizes** (node 15).
- **No engine change.** A missing engine capability becomes a todo.
- **New logic of nodes 1–15 is not moved here**: if a node grew one of these functions against the binding rule, it is cut
  like the rest and reported.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| **`extract-method-clean`** (K=4, `feature/reshape/extract-method-clean`) | **Rule 12, guard lift** (`Exits::ErrGuards`): a range whose every `return` is `return Err(..)`, in a function returning `Result`, with no top-level binding read after it and no labelled `break`/`continue`, becomes `fn name(..) -> Result<()>` ending in `Ok(())`, called as `name(..)?;`. Refused: a value return mid-function (F7), a mix of error guards and a value return (F8), a binding read after the range (F10). **Rule 13, tail position** (`Exits::Tail`): a range that ends at the tail of a match arm, `if`/`else` branch or block that is itself in tail position up to the function body keeps its `return`s, and the call is the value. **Rule 14:** the new function's return type uses the caller's one-argument alias (`Result<()>`, `Result<Anchor>`), for `ErrGuards`, `NoneGuards` and `Tail`. Also: `extract_method` keeps the range's comments (P), writes `&Path`/`&str`/`&[T]`, `Ok(())` tails and shorthand fields (Q), respells an unimported signature type (K), accepts a range starting on `{` or a comment (R), names a `let`-initializer extraction after the plan (S); the tidy removes `unused_mut`; arity > 7 is a note only | `parse_op`'s seven guards are rule-12 lifts (checked against F8 and F10 on the master text: none is refused), in today's order, so the first refusal a line meets and its message are unchanged. Every other extraction relies on P/Q/K/R; comments in `parse_op`'s rules and `apply_held_plan`'s loop survive; no hand clean-up of signatures expected | use the guard lift for a `return Ok(..)` exit (`refreshed`'s `Item` arm is a rule-13 tail seam instead), lift a guard run with outputs (F10, node 4's todo), fix an extraction defect by hand silently (a todo each), work around arity > 7 with lints (seams are chosen ≤ 7), or edit the engine |

Nodes 1–15 are not rows: nothing of theirs is consumed. They land first and edit `parse_op` (6, 9, 13), `apply_held_plan`
(3, 10), `check_plan` (2, 5, 7, 8, 12), `resolve_cluster` (3, 5, 7, 8, 9), `stranded_siblings` (5, 8), `sightings` and
`items_of_module` (5, 8, 9, 11) — a textual overlap this node absorbs by re-measuring.

## Draft PR contract

Published with the wave-2 contract commit (the first push of this PR, not its deliverable); **owned surface, new today**:

- `packages/tddy-code-restructuring/tests/function_length_budget.rs`, with its measurement (`fn` line to closing brace,
  inclusive; `ItemFn`, `ImplItemFn`, trait methods with a body; skipping `*_tests.rs`, `tests.rs` and `#[cfg(test)]` /
  `#[cfg(all(test, …))]` items) and the 60-line cap outside `src/backends/`.
- The `proc-macro2` `span-locations` dev-dependency (F4, consented).
- No production signature is owned: the extracted functions are private and named in the plans.
- Failing tests: acceptance test 1 (red: names the nine functions); tests 2–5 are green pins of the measurement.

## Green wave

**Wave:** 2 of 4.
**Greenable independently:** yes, once `feature/reshape/extract-method-clean` (with its guard lift) is on its base (and, to avoid re-cutting, the
rest of nodes 1–15).
**Concurrent with:** `feature/reshape/move-impl-members`, `feature/reshape/tests-follow`, `feature/reshape/oversized-files`.
**Blocks:** none.
Real dependency edges (whole stack): `1→13`, `5→14`, `2→15`, `3→15`, `4→16`, `13→17`, `2→17`, `3→17`, `17→18`, `6→18`,
`4→19`, `17→19`.

## Successor PRs

None depend on this node. `feature/reshape/fn-sizes-backend` extends the same gate to `backends/` (it consumes the test
file's layout, not a behaviour; not an edge).

## Scope

- **In:** the nine functions, plus `crate_move.rs::surveyed` (the tenth: it grew past 60 on the wave-2 base, see Validation results); the gate; the F2 hand edit and its todo; one todo per hand fix after an engine operation.
- **Deferred:** `backends/` (node 19); 41–60 (node 19's todo; first outside `backends/` is `verify.rs:166 compare_with`, 58);
  an operation that introduces a parameter struct (`todo/2026-10-09-restructure-has-no-operation-to-introduce-a-parameter-struct.md`);
  function lengths in `check --budget` (`todo/2026-10-09-restructure-check-budget-does-not-report-function-lengths.md`).
  The guard lift is node 4's.

## Technical changes

### State A (master `4a5c42b1b`; line numbers move with nodes 1–15)

| Function | Lines | Why it is long |
|---|---|---|
| `plan/codec.rs:246 parse_op` | 206 | ~20 cross-field validation rules, each `if … { return Err(malformed(…)) }`, 46 comment lines. Not an op-name match |
| `runner/entry_points/store_run.rs:237 apply_held_plan` | 159 | Run setup + an in-closure loop threading journal, ledger, overlay, group, done, registry |
| `runner/entry_points/check_entry_points.rs:207 check_plan` | 115 | Registry choice, anchor resolution, plan-wide findings, per-op static + deep checks, budget |
| `crate_move/source_scan/sighting_walk.rs:33 sightings` | 95 | Token state machine; `continue` in three arms; a local `modules` closure |
| `plan_store/refresh.rs:61 refreshed` | 76 | `match` over anchor kinds; `Item` arm has `let … else { return }` |
| `restructure_args.rs:206 options_for` | 73 | One literal per subcommand |
| `crate_move/cluster/stranded.rs:54 stranded_siblings` | 70 | Per-module finding with two inline `format!` blocks |
| `crate_move/cluster.rs:111 resolve_cluster` | 66 | Per-member accumulation + set-wide manifest edits |
| `crate_move/source_scan/module_items.rs:32 items_of_module` | 64 | Top-level scanner; the `mod` arm |
| `crate_move.rs:276 surveyed` (added in wave 2) | 65 | Per reached item, a loop over its outside references building caller and planned rewrites (57 on master; node 7's contract commit grew it) |

### State B

Each function ≤ 60, by these seams (master lines; re-read in wave 2):

| Function | Seams (inputs) | ≈ after |
|---|---|---|
| `parse_op` (F1) | seven guard lifts (node 4 rule 12, `Exits::ErrGuards`), bottom-up, each `fn(op: &RefactorOp) -> Result<()>` (rule 14 writes the crate's one-argument alias) called `refuse_…(&op)?;`: `:418-444` → `refuse_a_missing_name_or_malformed_syntax` (`name` guards **and** the `type`/`expr` `?` checks, in one range so that it is a guard run and not a plain `?` extraction, see Decisions); `:396-417` → `refuse_a_field_only_another_operation_honours` (`also` both ways, `to_file`); `:360-394` → `refuse_a_facade_or_destination_the_move_cannot_honour` (`outside`, cross-crate named facade, cross-crate `to`); `:316-358` → `refuse_a_same_crate_move_without_its_fields`; `:308-315` → `refuse_a_facade_the_operation_cannot_write`; `:277-306` → `refuse_what_a_test_binary_move_cannot_honour`; `:250-260` → `refuse_code_text_in_the_line(&raw)`. **F8/F10 check (master text):** every `return` in the seven ranges is `return Err(…)` (14 of them, plus the one in the code-bearing loop), so none is a mix (F8); no range declares a top-level binding (`object`, `type_`, `expr` are `if let` bindings inside their `if`), so none is refused under F10. `:316-358` and `:418-444` also hold `?` calls (`names_a_destination_and_anchors_by_item(..)?`, `one_type(..)?`), a shape node 4's test 31 does not cover — `check --deep` first; a refusal stops and asks | 33 (guards 10–43) |
| `apply_held_plan` (F2) | **hand, consented, `TODO(reshape-16)`**: `let mut run = open_plan_run(..)?` kept whole, and `overlay`, `done`, `group`, `stopped_early` gathered in `struct HeldLoop` (fields of the same names, same file, private). **Then engine**: held-plan lookup `:245-251` (2 inputs); resolve phase from `translate_op` to `resolve` `:295-313` (`registry`, `run`, `op`, `options`, `index`, `root`, `state`: 7); dry-run phase `:322-332` before its `continue` (7); apply-and-settle `:334-366` (`run`, `state`, `index`, `op`, `resolved`, `gate`, `options`: 7, returns `Settled`); refresh-and-report after the `let … else` (`store`, `key`, `ready`, `registry`, `run`, `options`, `state`: 7); `AppliedRun` + refusal `:383-390` | ≤ 60 (≈ 126 engine-only, without the edit) |
| `check_plan` | registry choice `:217-226` (3); item anchors `:232-239` (5); plan-wide findings `:247-263` (3); per-op static check `:265-285` (7, returns `statically_sound`); rehearsal `:295-311` (7, progress line stays); budget `:314-318` (3) | 40 |
| `sightings` (F3) | `modules` closure body → `enclosing_modules(&[Frame])` (1); `{` arm `:62-80` (6); `use` arm `:92-106` before `continue` (6, reads the closure); path arm `:108-123` before `continue` (7, reads the closure) | 56 (74 if the closure seams are refused) |
| `refreshed` | **`Item` arm, node 4 rule 13 (tail position):** `:87-105`, from `if edited.contains(&file.as_str())` to the arm's tail `Ok(anchor)` → `refreshed_item(anchor, file, item, start, end, edited, resolver) -> Result<Anchor>` (7 inputs). The range ends at the tail of a match arm whose `match` is the function's tail, so `Exits::Tail`: its two `return Ok(anchor)` stay verbatim and the call is the arm's value (F7a). Not the whole arm: that would also take the `followed` closure and the six destructured fields (9 inputs). **`Items` arm:** `:113-133` → `refreshed_items(file, items, fingerprints, edited, resolver)` (5), also ending at an arm tail | 40 |
| `options_for` | `Apply` arm `:208-223` (1); `Anchors` arm `:236-245` (1) | 52 |
| `stranded_siblings` | `whereabouts` match `:88-104` (3); finding `format!` `:105-121` (4) | 40 |
| `resolve_cluster` | set-wide tail `:159-176` (6, runs to the end); post-loop absorbs `:150-155` (4) | 43 |
| `items_of_module` | `"mod"` arm `:54-71` (4) | 49 |
| `surveyed` | the inner `for reference in outside_the_set { … }` loop (its `let … else { continue }` continues its own loop, so the range carries it): `outside_the_set`, `workspace`, `moving`, `callers`, `rewrites` (5); holds `?`, so it is a plain `?` extraction (the rule-14 gap in Decisions) | ≈ 43 |

### Delta

New private functions in the same nine files; one private struct `HeldLoop` in `store_run.rs` (F2, hand); one new test
file; one `Cargo.toml` dev-dependency line; three new todo files. No other file.

## Implementation milestones

1. [x] Gate test written and red, naming the ten functions at the wave-2 base (acceptance tests 1–5).
2. [ ] Rebase onto the post-node-15 base; re-measure (`function_length_budget` output); re-read seams; record changes here.
3. [ ] `items_of_module`, `options_for`, `refreshed` (one plan each; `check --deep`, `apply`, clippy, scoped tests).
4. [ ] `stranded_siblings`, `resolve_cluster`.
5. [ ] `check_plan`.
6. [ ] `sightings` (F3 at the closure seams).
7. [ ] `parse_op`: the seven guard lifts and the syntax extraction, bottom-up (last range first), one plan; the
   `src/plan.rs` refusal tests unchanged and green.
8. [ ] `apply_held_plan`: the hand `HeldLoop` edit (`TODO(reshape-16)`, compiles and passes the scoped tests on its own
   commit), then the engine phases.
9. [ ] Gate green; hand-fix todos written; validation.

## Testing plan

- **Behaviour:** no new behaviour test; the existing suites are the oracle (`src/plan.rs` tests for `parse_op`'s refusals,
  `restructure_args.rs` tests, `crate_move/source_scan.rs` tests, `check_precondition_parity.rs`, `plan_store_acceptance.rs`,
  `plan_store_resume_acceptance.rs`, `transactional_groups_acceptance.rs`, `cluster_move*.rs`, `store_run.rs` tests). They
  run unchanged after every apply.
- **Per apply:** `apply`'s compile gate and tidy, then `cargo clippy -p tddy-code-restructuring --all-targets -- -D
  warnings` and `./test -p tddy-code-restructuring`. Scoped only; CI answers for the workspace.
- **Size:** `tests/function_length_budget.rs`, fluent style, no conditionals in test bodies; pure text, no server, so not in
  the rust-analyzer group or the e2e filterset.

## Acceptance tests

All in `packages/tddy-code-restructuring/tests/function_length_budget.rs`.

1. `no_production_function_outside_the_backend_runs_past_sixty_lines` — walks `src/`, skips `src/backends/`, asserts the
   list of functions over 60 is empty and prints each as `src/<file>:<line> <fn> (<N> lines)`. **Red on master:** lists the
   nine functions.
2. `a_function_is_measured_from_its_fn_line_to_its_closing_brace` — a fixture with a doc comment and an attribute above the
   `fn`; measures the `fn` line to the `}` inclusive. Green pin.
3. `a_method_and_a_trait_default_method_are_measured` — `impl` and `trait` bodies count; a trait method without a body
   does not. Green pin.
4. `test_code_is_not_measured` — a `#[cfg(test)] mod`, a `#[cfg(all(test, unix))]` fn and a `*_tests.rs` file are skipped.
   Green pin.
5. `the_backend_is_left_to_its_own_node` — a fixture tree with a long function under `backends/` passes. Green pin (node
   19 deletes this test with the exclusion).

## Final Checklist

Module structure does not change, so there is no after-state dependency graph to draw; the must-not edges are:

- [ ] No new `.rs` file and no new `mod` declaration under `src/` — checked by `git diff --stat --diff-filter=A
  <base>...HEAD -- packages/tddy-code-restructuring/src` being empty and no added `^\s*(pub.*)?mod ` line in the diff.
- [ ] No new cross-module `use crate::…` line in the nine files — checked by the diff having no added `use crate::` line
  (signature types are respelled by node 4's K, not imported).
- [ ] No public signature changes — checked by the diff adding only private `fn`s (no added `pub` item) in the nine files.
- [ ] Every cut was made by an engine `apply`, and every hand edit after one has a todo entry under `docs/dev/todo/`.
- [ ] The only hand design edit is `HeldLoop` in `store_run.rs`, marked `TODO(reshape-16)` and named in
  `docs/dev/todo/2026-10-09-apply-held-plan-run-state-was-grouped-by-hand.md` — checked by `grep -rn 'TODO(reshape-16)'`
  finding exactly that site.
- [ ] Every engine refusal was reported to the developer before going on.

## Technical Debt & Production Readiness

- Hand fixes after engine operations are debt by definition: each is a todo naming the plan, the op and the shape, so an
  engine node can remove the class.
- The F2 hand edit (`HeldLoop`) is debt until an engine operation can group parameters: `TODO(reshape-16)` in the code,
  `docs/dev/todo/2026-10-09-apply-held-plan-run-state-was-grouped-by-hand.md` in the backlog.
- An F3 exemption, if it comes to one, is explicit debt with a todo.

## Decisions & Trade-offs

Decided by the developer on 2026-10-09 (PRD approved).

- **F1 — `parse_op`: flat guard functions (decided).** Node 4's guard lift turns each run of `if … { return Err(…) }`
  rules into `refuse_…(&op)?;`, the flat shape of the existing `plan/codec/*.rs` rule files. That reads better than the
  four-stage tail chain the PRD first proposed, in which each stage calls the next. Order is preserved, so the
  first refusal and its message are unchanged. If the guard lift refuses a range, the work stops and asks (the tail
  chain is the alternative to offer). Table dispatch rejected: the rules span kinds.
- **F2 — `apply_held_plan`: one consented hand edit (decided).** `HeldLoop` + `PlanRun` kept whole, marked
  `TODO(reshape-16)`, with a todo entry; then engine extractions.
- **F3 — `sightings` (decided at the recommendation).** Try the closure seams under `check --deep`; if refused, ask
  whether to replace the closure's two calls by hand or exempt at ≈ 74.
- **F4 — gate dependency (consented).** `proc-macro2` dev-dependency with `span-locations`.
- **F5 — exemptions (decided at the recommendation).** None; a closed, shrink-only list only if F3 forces one.
- **Seam choice over arity.** Every seam has ≤ 7 inputs, sometimes leaving a progress line in the caller; this keeps clippy
  quiet without lint allowances.
- **A plain `?` extraction and the crate's `Result<T>` alias (dependency risk, raised with node 4).** Rule 14 respells the
  return type only for `ErrGuards`, `NoneGuards` and `Tail`. An extraction with `?` but no `return` (`Exits::None`) is
  today's path, where rust-analyzer writes `Result<X, RestructureError>`. Node 4 itself reports that this is `E0107` in a
  file binding `use crate::Result;`, and all nine files here bind it. Seams of that kind in this node: `check_plan`'s
  registry choice, item anchors, plan-wide findings, per-op check and rehearsal; `apply_held_plan`'s lookup, resolve,
  apply-and-settle and refresh phases; `sightings`' `use`/path arms do not use `?`. **Requested of node 4:** apply rule
  14 to every introduced function whose rust-analyzer return type is `Result<X, E>` while the caller spells the alias.
  Until it does, each such seam needs a one-line hand fix after the apply, which is a todo per occurrence. `parse_op`
  avoids it by keeping the `type`/`expr` checks inside its last guard range.
- **One plan per function.** The engine composes extractions bottom-up only and does not re-anchor, so each plan lists its
  ranges last first.

## Refactoring Needed

- (This node is the refactoring.) If an extraction exposes a defect of `extract_method` that node 4 should have covered,
  it is a todo, and the cut stops for the developer.

## Validation Results

**Contract commit (wave 2, 2026-10-09)**, on base `feature/reshape/oversized-files` @ `205e9854f`:

- Baseline, scoped: `cargo check -p tddy-code-restructuring --all-targets`, `cargo clippy -p tddy-code-restructuring
  --all-targets -- -D warnings`, `cargo fmt --all --check` — clean before and after this commit.
- `cargo test -p tddy-code-restructuring --test function_length_budget`: 4 passed, 1 failed.
  - 🔴 `no_production_function_outside_the_backend_runs_past_sixty_lines` — red because the cuts are not made yet. It lists
    ten functions (the `syn` measurement agrees line for line with the discovery's scan):

    ```
    src/crate_move/cluster/stranded.rs:54 stranded_siblings (70 lines)
    src/crate_move/cluster.rs:145 cluster_edits (69 lines)
    src/crate_move/source_scan/module_items.rs:32 items_of_module (62 lines)
    src/crate_move/source_scan/sighting_walk.rs:39 sightings (99 lines)
    src/crate_move.rs:276 surveyed (65 lines)
    src/plan/codec.rs:246 parse_op (210 lines)
    src/plan_store/refresh.rs:61 refreshed (76 lines)
    src/restructure_args.rs:220 options_for (79 lines)
    src/runner/entry_points/check_entry_points.rs:207 check_plan (115 lines)
    src/runner/entry_points/store_run.rs:237 apply_held_plan (159 lines)
    ```
  - 🟢 green pins: `a_function_is_measured_from_its_fn_line_to_its_closing_brace`,
    `a_method_and_a_trait_default_method_are_measured`, `test_code_is_not_measured`, `the_backend_is_left_to_its_own_node`.
- Divergence from planning: **ten, not nine.** `resolve_cluster`'s long body is now private `cluster_edits` (node 7) at 69.
  `surveyed` (`crate_move.rs:276`) grew 57 → 65 in node 7's contract commit, against the "do not grow listed functions"
  rule (it was in the 41–60 band, not on the list). It is added to this node's scope. Growth since master: `parse_op`
  206 → 210, `sightings` 95 → 99, `options_for` 73 → 79; `items_of_module` 64 → 62 (node 11). Several of these hold
  contract `todo!()`/`TODO(reshape-…)` surfaces whose green may grow them again — green re-measures with this test.

## TODO

- [x] Record initial discovery (`2026-10-09-reshape-fn-sizes-rest-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (`docs/ft/coder/1-WIP/PRD-2026-10-09-reshape-fn-sizes-rest.md`)
- [x] Create changeset (this document)
- [ ] Add the PRD reference to `docs/ft/coder/1-OVERVIEW.md` **at wrap** (a shared append-point: not edited while planning)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code (engine plans, per milestone)
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; deletes the initial discovery
- [ ] USER REVIEW — work complete, decide next steps
