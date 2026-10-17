# 2026-10-17 — The restructuring acceptance suite pays a rust-analyzer cold start per test, mostly for logic that does not need one

**Category:** Future enhancement (test architecture)
**Source:** analysis of the `Rust e2e tests` leg's run time while landing the build-once/shard change
(PR #637). Numbers are from the green master run `38684159232` JUnit unless stated.

## The problem

52 `tddy-code-restructuring` acceptance binaries (320 tests) run one at a time — the
`rust-analyzer = { max-threads = 1 }` group in `.config/nextest.toml` — and sum to **24.5 min**, which
is the whole run phase of the e2e leg. Sharding (#637) spreads that group over runners; it does not
make the tests cheaper. They are also what makes `./test -p tddy-code-restructuring` slow on a laptop.

The cost is not spread evenly. It is bimodal:

| Test cost | Tests | Summed |
|---|---:|---:|
| under 1 s (never asks the server anything) | 54 | 0.2 min |
| 2.5 s or more (each pays one cold start) | 266 | **24.4 min** |

A test that queries the server costs about **4–7 s**, nearly all of it rust-analyzer's cold start
(measured on `move_item_acceptance`: one resolve ≈ 4.7 s; the `cargo check` compile gate is only
0.1–0.2 s because the fixtures are three tiny path-only crates). So **the saving per test moved off a
live server is about 5.5 s**, and the number of tests that need one is the only lever that matters.

## What was tried and does not help (do not redo)

- **One rust-analyzer per test binary** (add each fixture as a workspace folder): rust-analyzer
  reloads every workspace on each change. Second fixture 3.8 s against 3.4 s fresh, third 6.3 s
  against 3.4 s.
- **Lighter server settings in the harness** (no cache priming, check on save, build scripts or
  proc macros): time to quiescent fell from about 4.2 s to 1.8 s in a standalone start, but ten
  acceptance binaries showed no suite-level gain (`retarget_impl` 81 s against 82 s, `multi_seam_extract`
  88 s against 86 s). The first query does the work priming would have done.
- **The harness already pays a start only where a query is made**: static-check refusals and
  `anchors_command` cost 0.3–0.4 s.

## The idea

Keep **live rust-analyzer only where the test is about rust-analyzer's answers**, and run every other
case — the same fixture, the same assertion — without one, through the seams the engine already has.

### Where the seams are

- `ModuleReferences` (`src/crate_move.rs`): the crate-move family asks the server for one thing,
  `outside_references`, and decides everything else itself (survey, facades, manifest, widening). A
  known-reference-set fake already exists and is used (`tests/crate_move_children.rs`,
  `tests/grouped_use_crate_move.rs`, `tests/crate_move_restricted_declarations.rs`,
  `src/crate_move/new_crate_tests.rs`) — none of those is in the rust-analyzer group.
- `LanguageBackend` (`src/registry.rs`): `resolve` returns an edit; the runner, journal, groups,
  resume and plan store sit above it and do not care where the edit came from.
- The deciding half of assists is already unit tested over JSON fixtures (1,063 unit tests in
  `src/`), and `docs/assist-output-repairs.md` documents the repairs applied to the server's output.

### Three groups, by what a live server buys the test

Assigned from binary and test names and the module docs, **not read test by test** — step 0 below
replaces this with a measurement.

| Group | Binaries | Tests | Summed | What the test checks | Without a live server |
|---|---:|---:|---:|---|---|
| **A. Orchestration** — rollback, journal, resume, plan store, groups, deep-check parity | 5 | 30 | 1.8 min | `transactional_groups`, `plan_store_resume`, `live_plans`, `plan_store`, `same_crate_deep_check`: what the runner does with an edit | A fake `LanguageBackend` returning canned edits or errors; the real `cargo check` gate is cheap and stays |
| **B. Move survey, facades, manifest** | 9 | 60 | 4.8 min | `move_paths`, `move_facades`, `cluster_move`, `nested_module_move`, `move_module_to_crate`, `reparent_module*`, `move_item_outside_facade`: path survey, facade lines, manifest edits | The existing known-reference-set fake over `ModuleReferences` |
| **C. Text produced from the server's output** | 33 | 218 | 17.6 min | `extract_method_*`, `retarget_impl*`, `read_fields_through`, `move_fidelity`, `same_crate_widening`, `move_item*`, `repoint_call`, `item_anchor`, `multi_seam_extract`, …: comments kept, shorthand, alias spelling, doc links, widening | Needs the server's answers as data — recorded responses, or unit tests of the repair and rewrite functions over JSON fixtures |
| Keep live | 5 | 12 | 0.4 min | `index_health`, `unlinked_file`, `rename_cross_file`, `spawn_record`, `anchors_command`: facts about rust-analyzer itself | n/a |

A and B are the cheap, high-confidence part: **90 tests, about 6.6 min** of the serialized 24.5, with
a seam and a precedent in place. C is where the bulk is and where the design work is.

### What stays live

A deliberately small set, chosen by rule rather than by taste:

1. **One end-to-end happy path per operation kind** (13 `RefactorKind`s): real server, real edit,
   real `cargo check`.
2. **Every test whose assertion is rust-analyzer's behaviour**: assists, rename, reference sets on
   shapes the engine relies on (impl blocks, glob re-exports, `#[cfg(test)]`), indexing health.
3. **Tests that would change meaning with a fake**, such as warm-versus-cold (`ServerState`) tests.
4. **One live canary per recorded family** (see below), so a rust-analyzer upgrade that changes an
   answer fails a live test instead of passing against a stale recording.

If roughly 55 tests stay live, the serialized group falls from 24.5 min to about **5 min** (55 × 5.5 s);
the rest runs in parallel in the ordinary `Rust tests` leg in seconds. This is an estimate: it assumes
group C can be reduced to about 40 live tests, which step 0 has to confirm.

## The constraint: the same cases, not fewer

Moving a test must not drop a case. For every test that leaves the live suite:

- the **fixture and the assertion carry over unchanged** (a test moved to a fake still asserts the
  same bytes or the same refusal text);
- it is listed in a **parity table** — old `binary::test` → new home — in the changeset that does the
  move, so a reviewer can check that nothing disappeared;
- repo rules still apply: fluent-tests style, no deleting a test without asking.

## Proposed order

0. **Measure, do not guess.** Make the harness record which LSP requests each test makes (method and
   count), and publish it as a table. A test whose only server traffic is `textDocument/references`
   (through `ModuleReferences`) is group B by construction; a test that issues no request is already
   cheap; a test that issues `codeAction`/`rename` is group C or keep-live. This replaces the
   name-based grouping above.
1. **Group B** onto the existing known-reference-set fake. Smallest step, existing precedent.
2. **Group A** onto a fake `LanguageBackend`.
3. **Group C**: pick the replay mechanism with a measured prototype on two binaries
   (`extract_method_clean`, `retarget_impl`) — recorded server responses replayed by a fake client,
   against unit tests of the repair functions over JSON fixtures — and decide what to recommend only
   then. Do not build recording infrastructure first.
4. Shrink `[[profile.default.overrides]]` in `.config/nextest.toml` as binaries leave the group; the
   group list there is derivable from the binaries that still spawn a server.

## Risks

- **Drift.** A replayed or faked answer stops describing rust-analyzer. The live canaries and a
  documented re-record step (a `workflow_dispatch` job, not every PR) are what bound this.
- **A fake can hide semantics.** `AKnownReferenceSet` returns the references a test author wrote
  down; the real server may differ on a shape nobody thought of. Each moved test should have a live
  sibling for the same *shape*, not the same assertion.
- **Two places to maintain.** Recorded responses are data that goes stale with the fixture; keep
  fixture and recording generated from one source.
- **The win is smaller than it looks in CI** once sharding has landed (a shard's run phase is already
  about 5 min); the larger beneficiaries are the local `./test -p tddy-code-restructuring` run and CI
  compute. Weigh it that way.

## Related

- [CI guide](../guides/ci.md) — where the e2e leg's time is read
- [assist output repairs](../../../packages/tddy-code-restructuring/docs/assist-output-repairs.md)
