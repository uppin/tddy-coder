# Changeset: three restructure-engine defects that block the lifecycle destructure

**Date**: 2026-09-23
**Status**: 🚧 In Progress — green; the destructure re-run is #524's
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
  Gaps A–C (2026-09-24): `backends/rust/imports.rs`, `backends/rust/impl_seam.rs` and the new
  `backends/rust/nested_modules.rs`; `rust.rs` only wires the last in and rewords one refusal.
- **`tddy-tools`**: no change. It renders the new refusals through the library's existing error
  path.
- **`tddy-lsp`** (added 2026-09-24): `LspClient::server_status`, the latest `experimental/serverStatus`
  kept for every reader, so the health gate holds on a warm, already-drained client.
- **`tddy-index-daemon`** (added 2026-09-24): its `apply_plan` calls the compile gate, and `status_of`
  classifies the two new error variants.
- **Root script `run-index-daemon`** (added 2026-09-24): the daemon gets the dev shell's whole
  environment and a durable TMPDIR, and the log is truncated before launch. **`.config/nextest.toml`**:
  the six new live suites have joined the `rust-analyzer` test group.

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

Three explicit-failure guards were added on 2026-09-24, **approved by the developer** ("We must not
have any implicit failures"). Each turns a silent success into a truthful failure:

| | Guard | Replaces |
|---|---|---|
| **Health** | an index rust-analyzer reports as degraded (`health` other than `ok`) is refused as `ServerDefect`, quoting its message | a run over answers from an index whose build scripts failed |
| **E4** | an `extract_method` whose range holds a `return` exiting the enclosing function is refused as `SeamRefused`, naming the lines | plan 10: "applied 6 of 6", then seven `E0308`s |
| **Compile gate** | `apply` ends with `cargo check --all-targets` over the touched packages; a failure fails the run | "applied N of N" over a tree that does not compile |

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
- **Health gate** (2026-09-24). Once readiness is reached, a last-reported health other than `ok`
  (including `warning`) fails the operation with a `ServerDefect` quoting rust-analyzer's message. It
  must hold on the cold path and against a warm index daemon, including one whose status transitions
  another reader already consumed.
- **E4** (2026-09-24). An `extract_method` whose range contains a `return` targeting the enclosing
  function is refused (`SeamRefused`) before the assist runs, in `check`, `check --deep` and `apply`.
  A `return` inside a closure, an `async` block or a nested `fn` in the range does not count.
- **Compile gate** (2026-09-24). `apply` runs `cargo check --all-targets -p …` for every package
  owning a changed file, on both apply paths (CLI and index daemon). A failure is a non-zero, explicit
  failure carrying the compiler's errors; the edits stay applied for inspection and the message says
  how to roll back. A tree that did not compile before a fresh apply is refused, writing nothing. **No
  opt-out flag** (none was consented to).
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

- [x] Failing tests: E1, E2, E3, grouped `use`; see "Red-phase findings"
- [x] E1 fixed: progress check in the alias and parent-binding branches, refusing by name; collection scoped to the names the seam lost (see Implementation, deviation)
- [x] E2 cause found (readiness declared before build scripts load); fixed by waiting for `serverStatus` quiescence; a `_` signature is still never applied
- [x] E3 fixed: `self.method()` on the same type is not a refusal, and the assist's `self.modname::method()` rewrite is undone
- [x] Grouped-`use` binding recognised
- [x] Skill and plan-schema corrected on extract-method ordering (and on the `impl`-cut table)
- [x] Health gate: `ServerChatter` records `health`/`message`; readiness refuses a degraded index; `tddy-lsp` keeps the latest status for a warm client (developer-approved 2026-09-24)
- [x] E4: early-return refusal for `extract_method`, static tier (developer-approved 2026-09-24)
- [x] Compile gate on `apply`, with a baseline check, on the CLI and daemon paths (developer-approved 2026-09-24)
- [x] Plan 10 re-checked on the real repo with `check --deep`: ops 1–5 refused by E4, op 0 clean (see Implementation)
- [x] Gap A (2026-09-24): a relative `use` the parent wrote is rebased for the child module (`super::X` → `super::super::X`, `self::X` → `super::X`) in both reconstructions
- [x] Gap B (2026-09-24): the assist's `Self::modname::f` / `Type::modname::f` rewrite of a moved associated function's call is undone
- [x] Gap C (2026-09-24): a call inside a module the file already had, beside that module's own import of the moved item, is put back; the refusal's module wording names that case
- [x] Index daemon given the dev shell's whole environment — the real-repo cause of E2 (PATH-only made rust-analyzer's `webrtc-sys` / `sqlx-macros` builds fail to link, so the index was degraded); durable TMPDIR; log truncated before launch
- [x] Destructure plans re-run for real (2026-09-24, local branch off #524): every plan `check --deep` clean or truthfully refused; `apply` moved the code in all ten, four compile, six are failed by the compile gate with compiler-named errors
- [ ] ⏭️ Deferred by the developer (2026-09-24: "it's important that it moves the code and the compilation can be fixed manually"): gaps G–M → [2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run](../todo/2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run.md); stale-plan re-anchoring → [2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan](../todo/2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md)

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

## Implementation

Built one commit per milestone on top of the test commits (21 commits in all, gaps A–C and the
docs included). `backends/rust.rs` was not grown: every changed piece moved to a sibling under
`backends/rust/`, and the file went from 4,788 to 4,316 production lines (recorded in its code-issue).

| Module | Holds |
|---|---|
| `backends/rust/imports.rs` | `restore_imports`, `next_import`, the verified reconstruction, the seam-lost filter, `names_bound` |
| `backends/rust/impl_seam.rs` | `refuse_impl_sibling_references`, `is_inherent_impl`, `with_method_calls_restored` |
| `backends/rust/chatter.rs` | `ServerChatter`, re-exported at `backends::rust::ServerChatter` so `tddy-index-daemon` and the harness are untouched |
| `backends/rust/readiness.rs` | `ensure_indexed`, `wait_until_resolved`, `refuse_degraded_index` |
| `backends/rust/early_return.rs` | `refuse_early_returns` (E4), and the lexical scan behind it |
| `runner/compile_gate.rs` | `refuse_a_broken_baseline`, `refuse_a_broken_result` (both apply paths call them) |

- **E1.** *Cause:* `already_bound` read `use a::B as C;` as binding `B`. The alias branch rebuilt the
  parent's declaration, wrote it into the module, and on the next pass did not see it, so it wrote it
  again until `IMPORT_PASSES`. The trigger was a name unresolved everywhere in the file, collected
  file-wide. *Fix:* `names_bound` reads what a `use` binds: the alias, nothing for `as _`, and
  aliases inside nested groups. `bound_names` (the pruning pass) reads the same way now. Both
  reconstructions (alias and parent binding) are verified like an offered import. If the unresolved
  occurrences of the name do not drop, the pass ends with a `SeamRefused` naming the name and the
  declaration it tried.
- **E1, deviation from the plan: "produced module only" was too strong.** A live probe showed that
  the file-wide pass also restores names the *parent* loses to the cut. Example: a trait moved while
  the parent still writes `impl Named for Thing`, which the assist does not rewrite. Collected from
  the module alone, the run reported success over a parent that did not compile. The pass now weighs
  the names **the seam lost**: every unresolved occurrence inside the module, plus an occurrence in
  the parent only for a name the file resolved everywhere before the cut. It reads a baseline once,
  with one `semanticTokens` request against the original text. The E1 trigger was already
  unresolved before the cut, so it is still ignored. The reconstructions write into the module, so
  they answer only for occurrences inside it. Pinned by
  `imports_into_the_parent_a_trait_the_seam_moved_out_from_under_it`.
- **Grouped `use`.** *Cause:* `choose_import` read the post-assist text, from which the assist had
  already removed `mpsc`. *Fix:* the choice reads the pre-assist file and the current text together
  (`Seam::evidence_with`). The original holds the binding the assist removed, and the current text
  holds what earlier passes restored. The existing first tier then decides.
- **E3.** *Cause:* the refusal assumed that a call left behind "would resolve nowhere". For an
  inherent `impl` it resolves through the type. *Fix:* only a cut through a trait `impl`, or through
  an `impl` whose name the survey did not carry, stays refused. The message now names the `impl`
  and the E0119/E0046 reason. The outline name `impl Meter for Gauge` was confirmed live. **Found in
  green:** once the refusal lifted, the assist turned out to rewrite the call left behind as
  `self.modname::doubled()`. That is not Rust, and the rename cannot reach it.
  `with_method_calls_restored` undoes exactly that rewrite: `.` + placeholder + `::` + a moved
  inherent member's name, whole-word. A private method keeps the assist's `pub(crate)`, which
  `impl_widenings` already reports and nothing narrows back.
- **E2 and the fresh-server alias test.** *Cause:* both readiness waits took the first non-null
  hover as ready. rust-analyzer answers hover while it is still running build scripts, so `OUT_DIR`
  types did not exist yet and no name showed as unresolved. *Fix:* `ServerChatter` records whether
  any `experimental/serverStatus` arrived, and `loading()` is true while the server has said it is
  not quiescent. Both waits keep polling while that holds. A server that never sends the extension,
  or whose transition was read by another consumer of the same client (a warm daemon), still gets
  through on hover alone. `client_capabilities()` already advertised `serverStatusNotification`.
  `server_settings()` needed no `cargo.buildScripts.enable` / `procMacro.enable`: the defaults are
  on, and the E2 fixture passes without them. `refuse_inferred_placeholder` is unchanged.
- **E2 on the real repo: not reproduced; now caught explicitly.** Investigated by the developer on
  2026-09-24. The readiness defect is real and fixed: the E2 fixture proves it. But it is **not**
  shown to be the cause of the real-repo E2. Plans `05` (shifted), `10` and `10a` produce real types
  (`req: &StartSessionRequest`) on today's tree with **both** the pre-fix engine (`origin/master`) and
  the fixed one, cold and against the warm index daemon. The likely cause at the time was a degraded
  index (build-script output missing or stale). The engine ignored rust-analyzer's own account of
  that, the `health` and `message` of `experimental/serverStatus`, and the health gate below now
  refuses on it. `refuse_inferred_placeholder` stays as the post-condition.
- **Health gate.** `ServerChatter` records the latest `health` and `message`. `degraded()` returns a
  refusal quoting the message (whitespace collapsed) for anything but `ok`. `readiness.rs` refuses
  with a `ServerDefect` at every point it declares ready (`refuse_degraded_index`). **`warning` fails
  too**, a deliberate choice: a failed build script arrives as `warning` ("Failed to run build scripts
  of some packages"), which is exactly E2's hazard. rust-analyzer's other warnings (an unreloaded
  manifest change, build scripts or proc macros needing a rebuild, a config error, no workspace
  discovered) also mean the graph is not the tree on disk. Verified live: a fixture crate whose
  `build.rs` panics is reported as `warning` with that message, and the operation is refused.
  **Warm path:** a status is sent only on a transition, and `drain_notifications` is destructive and
  capped at 256, so a second backend on a warm client (the daemon's second request) never saw it.
  `tddy-lsp` now keeps the latest `serverStatus` (`LspClient::server_status`), and the bridge's
  `notifications_to_fold` appends it after what it drained. The daemon builds its backends through
  `runner::registry_for`, so the same readiness gate holds there. The daemon's own `Warm` RPC still
  reports `ready` for a degraded root: it answers "is the graph loaded", and the gate refuses at the
  first operation.
- **E4, early returns in `extract_method`** (found on the real repo, 2026-09-24). `apply` of plan 10
  (six `extract_method`s inside `start_session_core -> Result<Response<StartSessionResponse>,
  Status>`) reported "applied 6 of 6", then `cargo check -p tddy-session-lifecycle` failed with seven
  `E0308`s. The ranges held `return Ok(Response::new(inner));` / `return self.start_…().await;`, which
  rust-analyzer's "extract into function" copied verbatim into functions returning `Result<(),
  Status>` / `Result<(String, Vec<String>), Status>`. *Fix:* `backends/rust/early_return.rs`, run in
  the static `check` tier and in `resolve` before a server is asked. **Detection is lexical, and
  says what it relies on:** strings, raw strings, character literals and comments are masked by the
  lexer the test-binary move already had (`readable_spans`, widened to `pub(crate)`; no new lexer).
  Closures (block, `-> T` block and expression bodies, with `|` read as a closure only where an
  expression may start), `async` blocks and nested `fn name` open a body whose `return` does not
  count. Only the range is scanned, from depth zero, so statements lifted out of a closure body
  cannot carry that closure's `return` either. **Not seen:** a `return` a macro expands to (`bail!`);
  the compile gate catches what that leaves. **Real repo, plan 10, `check --deep` (fixed engine,
  cold, daemon stopped):** ops 1–5 refused, naming lines 569/598, 479/507, 289/371/404/407/426, 239
  and 104/133 (each checked against the source as a plain early exit of `start_session_core`). Op 0
  (776–893, no `return`) resolved clean, against an index whose health was `ok`.
- **Compile gate.** `runner/compile_gate.rs`: `refuse_a_broken_result` runs `cargo check
  --all-targets --message-format short -p …` over the packages owning every file the journal records
  a completed edit to (this run's, plus an earlier run's on resume). Packages are resolved by walking
  up to the nearest manifest with a `[package]` name (`declared_package_name`, widened), because a
  renamed-away file no longer exists for `cargo metadata` to place. **`--all-targets`**, because moves
  re-point imports that test targets use and a moved test binary is a test target; a lib-only check
  passes exactly the breakage these operations cause. A failure is
  `RestructureError::AppliedTreeDoesNotCompile`, carrying the compiler's error lines, the touched
  paths and the plan's journal directory. **Journal semantics:** the edits stay on disk and in the
  journal for inspection, and nothing is rolled back automatically: no rollback command exists. The
  message says to restore the touched paths from git and to remove the journal so the plan can run
  again. **Baseline:** `refuse_a_broken_baseline` runs the same check first on a fresh, writing run,
  over the packages owning the files the plan names (snapshot and anchors). A failure is
  `BaselineDoesNotCompile`, and nothing is written, so a pre-broken tree is never blamed on the plan.
  It is skipped for a dry run and for a run continuing a journal (that tree holds the earlier run's
  edits, which the result gate covers), the same line `open_run` draws for the snapshot. **Two new
  variants, deliberately:** none of the existing classes is true of either. The plan is not
  malformed, nothing was refused, and no server answer was unusable. `status_of` maps the baseline
  to `FailedPrecondition` and the applied tree to `Internal`. **Both paths:** `runner::apply` (CLI)
  and the index daemon's `apply_plan`, which judges before emitting its outcome event, so a stream
  never ends with "applied N of N" over a broken tree. **No opt-out flag**, as directed.
- **Docs.** `plan-schema.md` now says that several `extract_method`s in one function compose only
  bottom-up. The engine does not re-anchor them: re-anchoring would mean re-deriving each later
  anchor from the produced text, which is not cheap, so it was not attempted. The `impl`-cut table
  now separates inherent cuts (which succeed) from trait cuts (refused). `SKILL.md` gains the
  ordering rule.
- **Backlog.** No item in
  [2026-09-09 … connection-service split](../todo/2026-09-09-restructure-defects-from-the-connection-service-split.md)
  or the other restructure todos is closed as a side effect. D8's alias reconstruction is kept, and
  is now verified. The D8 guard test (`imports_the_alias_the_moved_code_names_exactly_once`) passes.

### Three more gaps, from running #524's plans against this engine (2026-09-24)

Each was reproduced in a live fixture before the fix; the assist's output quoted is what the
fixture's residual-placeholder refusal showed, and matches the real plan's refusal line for line.

| Module | Holds |
|---|---|
| `backends/rust/imports.rs` | `rebased_for_child` (Gap A) |
| `backends/rust/impl_seam.rs` | `with_method_calls_restored` widened, `reached_through_the_type` (Gap B) |
| `backends/rust/nested_modules.rs` (new) | `with_nested_references_restored` (Gap C) |

- **Gap A, a relative `use` one level off** (blocked plan 05a). The alias and parent-binding
  reconstructions wrote the parent's declaration verbatim into the module the seam becomes, which
  is the parent's *child*. *Fix:* `rebased_for_child` rewrites `super::X` as `super::super::X` and
  `self::X` as `super::X` before the trial; `crate::`, `::` and extern-crate paths are unchanged.
  Grouped trees are read one flat path per member, so a group member is rebased the same way.
  *Reproduced:* the alias form (`use super::Failure as HostFailure;` in `service/host.rs`) was
  refused exactly as 05a was ("left 3 unresolved occurrence(s) of it, where there were 3"). The
  plain `use super::Failure;` form did **not** reproduce: in the fixture rust-analyzer offers
  `super::super::Failure` itself, so the pass never reaches the parent-binding fallback. Its test is
  a guard. A `self::inner::rules` module binding did not reproduce either: the assist wrote
  `use crate::service::host::inner::rules;` itself. **Not handled:** a bare path through an item the
  parent declares (`sibling::X`, 2018 uniform paths) cannot be told from an extern crate by reading;
  it is left as written, and the verification refuses it by name.
- **Gap B, associated-function path calls** (blocked plan 02 op 3). *What the assist writes:* it
  inserts `modname::` straight before the moved member's name in every form of call:
  `Self::modname::doubled(self.level)` from a member left behind, and `Gauge::modname::doubled(2)` /
  `Meter::modname::doubled(2)` (type alias) from the file's `mod tests`. On the real file:
  `Self::modname::build_cursor_argv(…)`, `Self::modname::build_claude_argv(`, and
  `ClaudeCliSessionManager::modname::build_claude_argv(` twice. *Fix:* `with_method_calls_restored`
  removes the placeholder wherever it follows a `.` or an identifier qualifier other than `super`,
  `self` and `crate` (those reach a moved *free* item and are the rename's). An associated function
  is reached through its type wherever its `impl` lives, and a module path cannot name one. A bare
  `modname::f` for an associated function is left alone: nothing says which type it was called
  through. *Found in red:* the first fixtures called through the type inside `assert_eq!(…)`, and
  the assist does **not** rewrite a reference inside a macro call; they passed before the fix. The
  fixtures bind the call with a `let` now, as the real tests do.
- **Gap C, a reference inside the file's existing module** (blocked plan 04 op 0). *What the assist
  writes:* in `mod tests { use super::base; … let read = base(); }` it repoints the import to
  `use super::modname::base;` **and** rewrites the call to `modname::base()`. `modname` is a child of
  the file's module, not of `tests`, so the call names nothing and the rename cannot reach it. On the
  real file it was `modname::split_claude_extra_args(session_dir, "/usr/bin/tddy-tools", &[])` inside
  `mod withdrawal_contract_tests`. With `use super::*;` instead, the rewritten call resolves through
  the same glob and the rename finishes it (a guard test). *Fix:* `with_nested_references_restored`
  removes the placeholder from a path it starts, in a module other than the placeholder's whose own
  `use` declarations bind the moved name. The assist rewrites only references to what it moved, so
  that binding is the one the call resolved through. The module blocks are read lexically (brace
  depth); a brace in a string literal could mislead it, and the compile gate on `apply` catches what
  that leaves. *Refusal:* the old module advice ("extract a definition before the items that
  reference it") was wrong for a module the file already had, and the two cannot be told apart
  lexically. The advice now names both: reorder for an already-extracted module; for one the file
  had, reach the item through `use super::*;` or cut the seam elsewhere.

## Decisions & trade-offs

- **Fix the engine rather than hand-split.** The developer's decision (2026-09-23).
- **Inside the stack, directly below #524** (developer, 2026-09-23, after first trying it standalone).
  #524 then runs its plans against the fixed engine without waiting for a merge to `master`.

## Refactoring needed

### From @green

- (2026-09-24) E4's scan is lexical. It does not see a `return` a macro expands to (`bail!`,
  `ensure!`). It also misreads a leading `|` in a match arm (`match x { | A => … }`) as a closure. The
  first is caught by the compile gate on `apply`, but not by `check --deep`. `break`/`continue`
  targeting a loop outside the range are the same hazard and are not refused.
- (2026-09-24) `tddy-index-daemon`'s `Warm` reports `ready` for a degraded root; only the first
  operation refuses. `GraphLoad` could carry the health, so `Warm` says so too.
- (2026-09-24, gaps A–C) Two more lexical repairs of the assist's output
  (`with_method_calls_restored`, `with_nested_references_restored`) now sit between the assist and
  the rename, each with its own scanner. A generic qualifier (`Foo::<T>::modname::f`) is not undone
  and is still refused. If a free item and an inherent member with the same name both move, the
  type-qualifier rule could strip a legitimate `file_module::modname::f`; not seen, not guarded.
- (2026-09-24, gap A) The parent-binding reconstruction's `super::` case was not reproducible in a
  fixture (rust-analyzer offers the import itself there); only the alias path is proven live.
- (2026-09-24) The compile gate adds a `cargo check --all-targets` before and after every writing
  apply. On a warm target directory that is incremental, but on a cold one it is the price of a
  check. No opt-out exists, by direction. If one is ever wanted, it needs the developer's consent.

- Three walkers read the same `use` tree: `expand_use` (paths), `collect_aliases` (alias pairs), and
  `collect_bound` (bound names). One leaf walker yielding `(path, alias)` would serve all three.
- The seam-lost baseline is **name-level**. If a name was already unresolved somewhere before the
  cut, a parent occurrence the seam newly strands is not weighed either. A per-name count would
  close that gap.
- `already_bound` still checks the module's block even for an occurrence in the parent. This is
  pre-existing, and harmless while the reconstructions are gated to the module.
- A cut through a trait `impl` with **no** sibling reference is not refused, though it is E0119 all
  the same. This is pre-existing, and out of this change's scope ("no other engine features").
- `ServerChatter::quiescent`'s doc links to the private `RustBackend::ensure_indexed`, now in
  another module.

### From /validate-changes (2026-09-24)

- ⚠️ `run-index-daemon:636`: `"$(env PATH="$DEV_SHELL_PATH" command -v setsid)"` runs `command`
  as an external binary. macOS ships `/usr/bin/command`, but Debian, Ubuntu and NixOS do not. There
  the substitution is empty and `env -i … ""` fails, so the script cannot start a daemon on those
  Linux hosts. The old `env PATH=… setsid` form worked. Fix: resolve it in a subshell,
  `(PATH="$DEV_SHELL_PATH"; command -v setsid)`, and refuse when that is empty.
- ⚠️ `run-index-daemon:592-595`: a docs claim the code contradicts. The comment says "The caller's
  own environment is not inherited either: a host `LDFLAGS`/`CPPFLAGS`…". But
  `nix develop -c env -0` without `--ignore-environment` keeps the caller's environment, so a host
  `LDFLAGS` the dev shell does not override lands in `DEV_SHELL_ENV` and reaches the daemon. Fix:
  either run `nix develop -i` (keeping `HOME`/`USER` explicitly), or reword the comment.
- ⚠️ `runner/compile_gate.rs:274-307`: `failing_check` runs `cargo check` synchronously and ignores
  the run's `CancellationToken`. On the daemon path (`spawn_blocking` in `operations.rs:178`), a
  caller that stops waiting cannot cancel a cold `cargo check --all-targets`. It also contends for
  the checkout's `target/` lock with the developer's own builds. Fix: spawn it, poll the child
  against `cancel`, and kill it when cancelled.
- ℹ️ `runner/compile_gate.rs:299`: `line.contains("error")` also keeps warning lines whose text
  says "error". Match `error:` / `error[` prefixes instead.
- ℹ️ `runner/entry_points.rs:329`: the baseline runs after `open_run`, which has already called
  `ensure_self_ignoring()`. The message's "Nothing was written" is true of the tree but not of
  `.restructure/`. The baseline also runs before the static tier, so a plan `check` would refuse
  still pays a full `cargo check` first.
- ℹ️ `backends/rust/nested_modules.rs:71`: `raw.split("//")` truncates a line at any `//`, including
  one inside a string (`"http://…"`), and can then misread brace depth. `readable_spans` is already
  `pub(crate)` and could mask it the way `early_return.rs` does.
- ℹ️ `backends/rust/chatter.rs:207-215`: the library's degraded-index refusal names this repo's
  `./run-index-daemon` script. That is fine while the engine is repo-internal, but it is host
  knowledge inside a library.
- ℹ️ `.agents/skills/code-restructuring/references/plan-schema.md` (the `impl` table): it documents
  only the `self.modname::doubled()` undo. Gap B (`Self::`/`Type::modname::f`) and Gap C (the
  nested-module call) are not in the skill docs.
- ℹ️ `backends/rust.rs:3067`: a comment line over 100 columns, from a hand-edit of the wrapped
  paragraph.

### From /validate-tests (2026-09-24)

- ⚠️ `tests/apply_compile_gate_acceptance.rs:49` `leaves_the_edits_of_a_failed_apply_on_disk_for_inspection`:
  it discards the result (`let _refusal`) and asserts only that the moved file exists. That also
  holds if the gate were removed and the apply "succeeded", so the test does not discriminate.
  Assert that the apply failed as `AppliedTreeDoesNotCompile` first, or fold this assertion into
  the failure test.
- ⚠️ `tests/harness/mod.rs` `until_quiescent`: it subscribes after the handshake and waits for a
  `quiescent: true` *transition*. If the server settles before the subscription attaches, the wait
  runs out its 180 s and panics. That window is small on a fixture, but it is a real race, and
  `LspClient::server_status()` (added in this PR) closes it. Fix: check
  `client.server_status()` first, then subscribe.
- ⚠️ `packages/tddy-index-daemon/tests/detached_daemon_production.rs`
  `a_restart_announces_the_daemon_it_started_not_the_previous_ones_log`: it guards a scheduling
  race it cannot force, so it passes most of the time with the bug present. `assert!(first.status.success())`
  has no message. A panic before `stop_the_daemon` (for example in `recorded_pid`) leaks a detached
  daemon. Both tests are `#[ignore]`d with a justification.
- ℹ️ Mystery-guest line ranges: `extract_method_control_flow_acceptance.rs:141` (`4..=7`),
  `index_health_acceptance.rs:671,692` (`4..=5`) and `extract_method_signature_acceptance.rs:188`
  (`10..=11`) are bare numbers into harness fixtures. `impl_seam`/`import_pass`/`test_module_reference`
  name theirs as constants; do the same here.
- ℹ️ `extract_method_signature_acceptance.rs`: correctness rests on a 15 s `sleep` in the fixture's
  build script outlasting the first hover. It is justified in the fixture doc, but it is a timing
  race by design, and it costs at least 15 s per run.
- ℹ️ Two-assertion tests (content plus `assert_compiles`) in `impl_seam`, `import_pass` and
  `test_module_reference`. This is acceptable as one behaviour, "moved and still compiles". The
  compile assertion is the one the suites exist for.
- ℹ️ `backends/rust.rs` unit test `names_a_module_the_file_already_had_when_the_leftover_sits_in_a_module`
  has no Given/When/Then; every sibling test has it.
- ℹ️ The same `include_str!` test-binary fixture is written twice, in the restructuring harness and
  in `code_index_service_acceptance.rs`. Crate boundaries force that, so it is noted, not a defect.

### From /validate-prod-ready (2026-09-24)

- ⚠️ `run-index-daemon:559`: `getconf DARWIN_USER_TEMP_DIR 2>/dev/null || printf '/tmp'` is a
  silent platform fallback. It is the same shape as the old `${TMPDIR:-/tmp}`, so it is not new
  debt in kind. It is recorded because the rule is to name every fallback.
- ℹ️ `backends/rust.rs:2602`: `bound_names` is now a one-line pass-through to `names_bound`. Inline
  it at its two call sites.
- ℹ️ `runner/compile_gate.rs:302-306`: when no stderr line names an error, the whole stderr is
  returned. That is deliberate and commented. It is a failure reported in full, not one masked.
- No mock or fake code, no test-only production branches, no `println!`/`eprintln!`/`dbg!`, and
  no TODO/FIXME in the production diff. The `#[cfg(rust_analyzer)]` trick lives only in fixture text.

### From @red (TDD Red Phase)

- `tests/harness/mod.rs` now has fixture builders for single-crate seams and a lexical
  `the_module_named`. If more extraction suites follow, the builders could move to a
  `harness/fixtures.rs` sibling, since the harness file is past 1,000 lines.
- `.config/nextest.toml`'s `rust-analyzer` group still omits older live binaries
  (`nested_module_move_acceptance`, `cluster_move_acceptance`, `facade_cycle_acceptance`, …).

## Validation results

**2026-09-24, `/pr-wrap` validation (validate-changes, validate-tests, validate-prod-ready).** These
steps only report. The fixes go to the refactor pass. Scoped to `tddy-code-restructuring`,
`tddy-index-daemon` and `tddy-lsp`; whole-workspace health is CI's.

- **Stack gate:** the base is `master` and the branch is on its tip. `origin/master..HEAD` holds only
  this PR's 21 commits, so there is no leak. The diff has 37 files, all claimed by this changeset.
  Nothing is implemented from `## Dependencies`, and `tddy-session-lifecycle` is untouched.
- **Build:** `cargo clippy -p tddy-code-restructuring -p tddy-index-daemon -p tddy-lsp --all-targets
  -- -D warnings` is clean.
- **Tests:** `./test -p tddy-code-restructuring -p tddy-index-daemon -p tddy-lsp` exited 0. All 44
  result lines are `ok`: 637 passed, 0 failed, 9 ignored. The 9 are `#[ignore]`d tests, the
  real-script `detached_daemon_production` suite among them.
- **Risk summary:** 0 critical, 6 warnings, and several info items. They are listed under
  "Refactoring needed", in the `/validate-changes`, `/validate-tests` and `/validate-prod-ready`
  subsections.
- **Changeset sync, corrected here:**
  - The `## TODO` "Re-run the destructure plans" item was unticked while Scope said it was done.
  - The Implementation's "4,788 to 4,294" figure is stale; the code issue now records 4,316.
  - "Six commits on top of the two test commits" is stale; there are 21 commits.
  - `run-index-daemon` and `.config/nextest.toml` were missing from Affected Packages.
- **Production readiness:** ⚠️ gaps, no blockers. No mock code, no `println!`/`eprintln!`/`dbg!`,
  and no TODO/FIXME in production code. There is one platform fallback in `run-index-daemon`
  (pre-existing in form), and one pass-through wrapper (`bound_names`).

**2026-09-24, `/pr-wrap` refactor pass.** Each validation finding above, and what became of it.

- **Fixed:** `run-index-daemon` resolves `setsid` in a subshell, `SETSID="$(PATH="$DEV_SHELL_PATH";
  command -v setsid)"`. It refuses when that is empty, and it launches `"$SETSID"`. `env … command`
  is gone.
- **Fixed (comment only):** the environment comment now says the daemon gets what `./dev` gets,
  the caller's environment included. `nix develop` is impure, and that is intended.
  `durable_tmpdir`'s comment now names its choice: the per-user temp dir on macOS, `/tmp` elsewhere,
  which is the script's old default.
- **Fixed:** `failing_check` spawns `cargo check`, drains stderr on a thread, and polls `try_wait`
  against the run's token. On cancel it kills the child and returns `CallerStopped`, which the daemon
  already maps to `Status::cancelled`. A cancel during the result check first tells the progress
  sink that the applied edits are on disk and unchecked. The error filter keeps `error…` lines and
  `: error` lines (the short format's `path:l:c: error[E…]`), and it falls back to the whole stderr.
  Both paths are unit-tested. `rustc` children cargo already started are not killed; they only
  write into `target/`.
- **Fixed, by reordering:** `open_run_after` takes a `before_writing` gate. It runs after the
  cheap, read-only refusals (git worktree, repo-scoped journal, `JournalExists`, snapshot) and before
  `.restructure/` is created. Both `runner::apply` and the daemon's `apply_plan` pass the baseline
  check as that gate. `open_run` is `open_run_after` with no gate. Resume and dry-run behave as before:
  the baseline still skips them, and adopting a repository-scoped journal still happens only when a
  run is continuing. `writes_nothing_to_a_tree_that_did_not_compile_before_the_plan` now also asserts
  that `.restructure/` is absent. The per-op static backend checks still run after the baseline,
  inside the apply loop, because they belong to each operation's resolve. That is unchanged.
- **Fixed:** `leaves_the_edits_of_a_failed_apply_on_disk_for_inspection` asserts the refusal is
  the applied-tree-does-not-compile failure before it checks the file.
- **Fixed:** `until_quiescent` folds in `client.server_status()` before it waits. It reads that
  *after* subscribing, not before: a status read first could be superseded before the subscription
  attached. Read after, any newer status arrives on the stream.
- **Fixed:** `detached_daemon_production` has a drop guard (`ASuiteRuntime`) that owns the
  runtime dir and runs `--stop` when a test ends, pass or panic. Every start asserts with its
  stderr. The restart test's doc comment says it cannot force the race it guards: a pass is
  evidence, not proof. The tests are still `#[ignore]`d.
- **Fixed:** `nested_modules::module_blocks` reads brace depth over `early_return::masked_to_code`
  (built on `readable_spans`, now `pub(super)`) instead of `split("//")`. `bound_names` is inlined
  into `names_bound`. The comment at `rust.rs:3065` is rewrapped. The unit test
  `names_a_module_the_file_already_had_…` has Given/When/Then comments. The bare fixture ranges are
  now named constants (`A_RANGE_THAT_RETURNS_EARLY`, `STATEMENTS_NEEDING_NO_BUILD_SCRIPT`,
  `STATEMENTS_READING_THE_REQUEST`). Gaps B and C are documented in
  `references/plan-schema.md`, in the inherent-`impl` row and the placeholder-leftover paragraph.
- **Left, by decision:** the degraded-index refusal still names `./run-index-daemon`. The library
  lives in this repo, and the advice is actionable where it is read.
- **Left, by design:** the E2 fixture's 15 s build-script sleep. The fixture doc justifies it.
- **Left:** the restart test's timing race cannot be forced from outside the script. It is now
  documented in the test's doc comment.

Verification of the refactor pass is scoped to the three packages. Whole-workspace health is CI's.
`./test -p tddy-code-restructuring -p tddy-index-daemon -p tddy-lsp` gave 44 result lines, all
`ok`: 639 passed (the 637 before, plus 2 new filter unit tests), 0 failed, 9 ignored. Scoped
`cargo clippy --all-targets -D warnings` is clean, `cargo fmt --check` is clean, and
`bash -n run-index-daemon` passes. `detached_daemon_production -- --ignored --test-threads=1` ran
4 tests against the real script, and all 4 passed in 54 s.

**2026-09-24, after gaps A–C.** Scoped to `tddy-code-restructuring`; whole-workspace health is CI's.

- `./test -p tddy-code-restructuring`: every binary green. Lib 389, `import_pass_acceptance` 8,
  `impl_seam_acceptance` 7, `test_module_reference_acceptance` 2 (new), and every other suite in the
  package.
- `cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings`: clean. `cargo fmt --check`:
  clean.
- **Real plans, `check --deep` against the index daemon rebuilt from this tree** (restarted, cold
  then warm, this branch's `tddy-session-lifecycle`): `05a-spawn-split-agent-teardown` **no
  findings**; `04-split-session` (2 ops) **no findings**; `02-cli-session-manager-dir` (9 ops) **no
  findings**. Before the fixes, a daemon built with only diagnostics added reported 04 op 0 and 02
  op 3 refused exactly as #524 saw them. **05a was already clean on that build**, before Gap A's fix:
  its original refusal did not recur on a freshly started daemon, so its real-repo trigger is not
  confirmed (see Gap A).

**2026-09-24, after the three guards.** Scoped to the packages touched; whole-workspace health is CI's.

- `./test -p tddy-code-restructuring`: every binary green. Lib 369, `apply_compile_gate_acceptance` 5,
  `index_health_acceptance` 2, `extract_method_control_flow_acceptance` 1,
  `extract_method_signature_acceptance` 1, `library_returns_its_results` 6, `import_pass_acceptance` 6,
  `impl_seam_acceptance` 4, and every other suite in the package.
- `./test -p tddy-index-daemon -p tddy-lsp`: every binary green (`code_index_service_acceptance` 25,
  `tddy-lsp` lib 17).
- `cargo clippy -p tddy-code-restructuring -p tddy-index-daemon -p tddy-lsp --all-targets -- -D
  warnings`: clean. `cargo fmt --check`: clean. `cargo check -p tddy-tools --all-targets`: clean.

**2026-09-23, before the guards:**

Scoped to the packages touched (2026-09-23). Whole-workspace health is CI's.

- `./test -p tddy-code-restructuring`: every binary green. Lib 353, `import_pass_acceptance` 6,
  `impl_seam_acceptance` 4, `extract_method_signature_acceptance` 1, and every other suite in the
  package.
- `cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings`: clean. `cargo fmt`: clean.
- `cargo check -p tddy-tools --all-targets` and `cargo check -p tddy-index-daemon --all-targets`
  (both consume `ServerChatter` / the library): clean.

## TODO

- [x] Discovery (from the destructure node's `check --deep` runs)
- [x] Changeset: this document
- [x] Failing tests (red)
- [x] Green
- [x] Re-run the destructure plans (see Scope; 2026-09-24)
- [x] `/validate-changes`, `/validate-tests`, `/validate-prod-ready` (2026-09-24, report only; refactor pending)
- [ ] `/pr-wrap` refactor pass, `/wrap-context-docs`
