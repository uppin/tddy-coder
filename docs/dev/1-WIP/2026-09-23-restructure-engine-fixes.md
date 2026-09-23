# Changeset: three restructure-engine defects that block the lifecycle destructure

**Date**: 2026-09-23
**Status**: 🚧 In Progress — planned
**Type**: Bug Fix
**PR**: #527. **Stack**: `#carve` 13/15, between `core-split` (#522) and `lifecycle-wiring` (#524, the destructure node)

## Initial Discovery

Planning the destructure node ran 14 restructure plans through `tddy-tools restructure check --deep`
against a warm index, on `tddy-session-lifecycle` at `17537a73`'s parent tree. Eight were clean. The
other six were refused by three engine defects. The evidence is the plans themselves, in the
destructure node's `docs/dev/1-WIP/2026-09-23-carve-lifecycle-wiring-plans/`:

- `01-connection-service-clusters.jsonl` and `08-session-coordinate-handlers.jsonl`: E1
- `05-spawn-split-agent.jsonl` and `10-start-session-core-extract-methods.jsonl`: E2
- `02-cli-session-manager-dir.jsonl`: E3

Related, already on master:
- [2026-09-09 restructure defects from the `connection_service.rs` split](../todo/2026-09-09-restructure-defects-from-the-connection-service-split.md).
  Its D8 was fixed by `alias_target` / `aliased_bindings` / `with_module_import`. E1 is a defect **in
  that fix's path**.
- [2026-09-17 restructure refusal truth and authoring gates](./2026-09-17-restructure-refusal-truth-and-authoring-gates.md).
  Its milestones landed on master, but the changeset was never wrapped. It added the third import
  tier and the refusal taxonomy that this change's refusals must respect.

## Affected Packages

- **`tddy-code-restructuring`**: [README.md](../../../packages/tddy-code-restructuring/README.md).
  The import pass (`backends/rust.rs`, `next_import`, `choose_import` and the alias path), the
  extract-method signature handling, and the refusal check for `impl`-cutting seams.
- **`tddy-tools`**: no change expected. If one proves necessary, it is only in how `restructure`
  reports these refusals.

## Related Feature Documentation

- [`packages/tddy-code-restructuring/README.md`](../../../packages/tddy-code-restructuring/README.md)
- [`.agents/skills/code-restructuring/SKILL.md`](../../../.agents/skills/code-restructuring/SKILL.md)
  and `references/plan-schema.md`: the "operations compose" claim is wrong for extract-methods run
  top-down, so correct it here.

## Summary

Fix the three defects so the destructure node's plans run through the engine, not by hand:

| | Defect | Blocks |
|---|---|---|
| **E1** | The import pass loops forever and writes the same `use` 512 times. It collects unresolved names file-wide, not only in the new module. Its alias and parent-binding branches return an import without checking whether it makes progress, and never mark the name unimportable. It is triggered in files with a `use … as …` alias, even for a 24-line seam with no free names. | every `extract_module` in `connection_service.rs` (1,647) and `session_coordinate_handlers.rs` (818) |
| **E2** | Extract-method writes `req: _` / `&_` for parameters whose type is generated into `OUT_DIR` (`StartSessionRequest`), even against a warm index. Suspected cause, unconfirmed: the generated `include!` module is not indexed (build scripts off?). | extract-methods in `start_session_core` (857) and `spawn_split_agent` (271), and probably `resume_session_at_session_coordinate` |
| **E3** | A seam that cuts an `impl` is refused because same-file `self.method()` calls "would resolve nowhere". Method calls resolve wherever the type is in scope, so this is over-strict. | 5 of the 9 `cli_session_manager` seams; moving extracted helpers out of `svc_start_*` |

The same work also covers two smaller things:
- the grouped-`use` ambiguity (`tokio::sync::{…, mpsc, …}` offered two ways);
- correcting the plan-schema's "operations compose" claim, since extract-methods compose only when
  ordered bottom-up.

## Background

The developer chose to fix the engine rather than hand-split the refused seams (2026-09-23). That
keeps the destructure node engine-driven, and the fix outlives it.

## Responsibility

- **E1.** The alias and parent-binding branches of the import pass return an import only if applying
  it reduces the unresolved occurrences. Otherwise they mark the name unimportable, and the pass ends
  with a `SeamRefused` naming it, not after 512 passes. Unresolved names are collected from the
  **produced module** only.
- **E2.**
  - Find the cause first: index configuration, or the engine reading an inference result it should
    not trust.
  - Then either make the types resolve (for example, by enabling build scripts / `OUT_DIR` in the
    rust-analyzer config the engine starts), or refuse with a `SeamRefused` or `ServerDefect` that
    names the untyped parameter, instead of writing `_`.
  - A signature containing `_` must never be applied.
- **E3.** An `impl`-cutting seam is not refused for `self.method()` calls to methods that remain on
  the same type in another `impl` block. A refusal remains for free functions and associated items
  that really would stop resolving.
- **Grouped `use`.** A name bound by a grouped `use` in the parent is recognised as the file's own
  binding (the `choose_import` tier).
- **Docs.** Plan-schema and skill: extract-methods in one plan compose only bottom-up (or the engine
  re-anchors, if that proves cheap; decide during green).
- **Tests** for each fix, at the level the engine's existing suites use (fixture crates).

## Boundaries

- **Nothing in `tddy-session-lifecycle` is touched.** The destructure node applies the plans.
- **No new external dependencies.** If E2's fix needs a rust-analyzer configuration change, that is
  config, not a crate.
- **Refusals stay truthful.** A defect is fixed by making the operation correct, or by refusing it
  with the right class (the 2026-09-17 taxonomy). It is never fixed by making a wrong result pass.
- **No other engine features.** D6, D7, D9 and the other backlog items stay where they are unless a
  fix here closes one as a side effect, and that is recorded.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `12` core-split (#522) | `tddy-core` as facades | nothing directly; it is the parent only because the stack is a line | touch `tddy-core` or the nine crates |

## Draft PR contract

The failing tests reproducing E1, E2 and E3 are published first, one commit after this plan. The
destructure node consumes only the fixed engine binary, not an API, so it does not code against a
surface here.

## Green wave

**Wave:** after #522, and before #524.
**Greenable independently:** **yes.** Its tests use the engine's own fixture crates.
**Concurrent with:** nothing.
**Blocks:** #524's refused plans (`01`, `02`, `05`, `08`, `10`). #524 sits directly on this branch, so
it gets the fixed engine in its own tree.

## Prerequisites

| Item | Verdict | What this change does about it |
|---|---|---|
| [2026-09-09 … connection-service split](../todo/2026-09-09-restructure-defects-from-the-connection-service-split.md), D8 | ⚠ **DURING** | E1 is in D8's alias path. Do not regress the four seams D8's fix proved |
| [2026-09-17 refusal truth and authoring gates](./2026-09-17-restructure-refusal-truth-and-authoring-gates.md) | ⚠ **DURING** | Unwrapped, and owned elsewhere. Use its `SeamRefused` / `ServerDefect` classes; do not wrap or edit it here |
| `packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md` | ⚠ **DURING** | `backends/rust.rs` is already oversized. Do not grow it; put new logic in a sibling module |
| [2026-09-18 verify cannot exit zero for an extract_module](../todo/2026-09-18-restructure-verify-cannot-exit-zero-for-an-extract-module.md) | — unrelated | |

## Scope

- [~] Failing tests: E1, E2, E3, grouped `use` (commit 2) — written, not yet committed; see "Red-phase findings"
- [ ] E1 fixed: progress check and unimportable marking in the alias and parent-binding branches; collection scoped to the produced module
- [ ] E2 cause found; fixed or refused truthfully; never applies a `_` signature
- [ ] E3 fixed: `self.method()` on the same type is not a refusal
- [ ] Grouped-`use` binding recognised
- [ ] Skill and plan-schema corrected on extract-method ordering
- [ ] Re-run the destructure node's refused plans with `check --deep` and record the results

## Testing plan

Fixture-crate tests in the engine's existing style, one per defect:

- **E1:** a file with `use a::B as C;` and a seam that names nothing. `extract_module` applies,
  producing no repeated `use`; the pass ends in a bounded number of passes.
- **E1:** a seam whose moved code names `C`. The module receives `use a::B as C;` exactly once. This
  guards against regressing D8.
- **E2:** an extract-method whose statements read a value of a type from an `include!`d `OUT_DIR`
  module. The signature names the type, or the operation is refused with a message naming the
  untyped parameter. It is never applied with `_`.
- **E3:** an `impl` split where the moved part calls `self.other()` defined in the part that stays.
  It applies, and the crate still compiles.
- **Grouped `use`:** a parent with `use x::{A, B};`. The moved code naming `B` gets one unambiguous
  import.

Scoped verification: `./test -p tddy-code-restructuring` (and `-p tddy-tools` if touched).

## Red-phase findings

Each finding was reproduced against the live server (rust-analyzer 2026-03-30) in fixture crates:

- **A cold server is not ready when the engine says it is.** `ensure_indexed` and
  `wait_until_resolved` accept the first non-null hover. For about three more seconds, a small
  crate's semantic tokens carry no `unresolvedReference`, so the import pass has nothing to act on.
  An alias seam then applies without its import and does not compile. Build-script output (`OUT_DIR`)
  is also not loaded yet. The harness therefore gains `ServerState::Settled`, which waits for
  `serverStatus quiescent: true` (the daemon's own definition of warm). The E1, E3 and grouped-`use`
  tests run settled, because that is where they were observed.
- **E1 reproduces deterministically** when the alias names a type the server cannot see, across the
  whole file. The fixture uses `#[cfg(not(rust_analyzer))]` to stand in for unloaded `OUT_DIR`
  code. Mechanism: `already_bound` reads `use a::B as C` as binding `B`, not `C`, so the alias branch
  never sees its own insertion.
- **E2's cause, in the fixture:** the build-script race above. RA answers hover before the build
  script's output loads, and writes `fn resumed_session(req: _)`. The existing single-line post-condition
  catches it: RA writes the signature on one line, so the multi-line escape does not arise. Its
  advice to "retry against a warm server" does not help here. The real warm-index failures remain
  unexplained. One unconfirmed candidate is `tddy-service`'s `prost-build` script failing inside
  RA's environment.
- **E3's premise holds.** For inherent-`impl` members, RA writes `mod m { use super::T; impl T { … } }`.
  A trait `impl` cut stays refusable (E0119/E0046). No inherent-`impl` case where a reference really
  stops resolving was found.
- **Grouped `use`:** the assist removes `mpsc` from the parent's group when the seam holds its only
  use. `choose_import` then sees `std::sync` and `shared::sync` as equal evidence. The binding has to
  be read from the pre-assist text.

## Decisions & trade-offs

- **Fix the engine rather than hand-split.** The developer's decision (2026-09-23).
- **Inside the stack, directly below #524** (developer, 2026-09-23, after first trying it standalone).
  #524 then runs its plans against the fixed engine without waiting for a merge to `master`.

## Refactoring needed

### From @red (TDD Red Phase)

- `tests/harness/mod.rs` now has fixture builders for single-crate seams and a lexical
  `the_module_named`. If more extraction suites follow, the builders could move to a
  `harness/fixtures.rs` sibling, since the harness file is past 1,000 lines.
- `.config/nextest.toml`'s `rust-analyzer` group still omits older live binaries
  (`nested_module_move_acceptance`, `cluster_move_acceptance`, `facade_cycle_acceptance`, …).

## Validation results

## TODO

- [x] Discovery (from the destructure node's `check --deep` runs)
- [x] Changeset: this document
- [~] Failing tests (red)
- [ ] Green
- [ ] Re-run the destructure plans
- [ ] `/validate-changes`, `/pr-wrap`, `/wrap-context-docs`
