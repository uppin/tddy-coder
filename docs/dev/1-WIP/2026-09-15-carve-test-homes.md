# Changeset: carve-test-homes

**Date**: 2026-09-15
**Status**: 🚧 In Progress
**Type**: Refactor (tooling capability + mechanical relocation)
**Stack**: `#carve` 4/10 — inserted after the two tooling nodes
**PR**: [#498](https://github.com/uppin/tddy-coder/pull/498)

PRD: [`2026-09-15-carve-test-homes-prd.md`](./2026-09-15-carve-test-homes-prd.md)

## Initial Discovery

[`2026-09-15-carve-test-homes-initial-discovery.md`](./2026-09-15-carve-test-homes-initial-discovery.md)

## Affected Packages

- **`tddy-code-restructuring`**: gains `move_test_binary_to_crate`.
- **`tddy-daemon`**: keeps 17 suites, loses 122 and its `lib.rs` facade, and drops 17 runtime
  dependencies nothing in its `src/` names.
- **Thirteen destination crates**, `tddy-session-lifecycle` chief among them — it has **no `tests/`
  directory at all** today.

## Responsibility

- Add `move_test_binary_to_crate`: anchor `<crate>/tests/<name>.rs`, `git mv`, header re-point
  through up to two facades, destination `[dev-dependencies]`. No `mod` declaration to touch and no
  facade, ever.
- Move the 122 misplaced `tddy-daemon` suites and the one misplaced `tddy-workflow-recipes` suite to
  the crates they exercise.
- Delete `tddy-daemon/src/lib.rs`'s two `pub use` blocks and its four re-export shims.
- Remove the 17 `[dependencies]` entries no `tddy-daemon` source file names.

## Boundaries

- Does **not** move the **17** suites that exercise `runtime`, `server`, `startup`,
  `daemon_settings`, `daemon_config_service`, `local_socket_server` or `relay_idle`. The daemon is
  the composition root and a test of the composition belongs with it.
- Does **not** change what any test asserts. Files move; import headers are re-pointed.
- Does **not** touch `tddy-core`'s 44 suites — the audit found every one correctly placed.
- Does **not** carve any production module. No `src/` file moves between crates in this node.
- Does **not** split any oversized test file.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `1/10` restructure-moves | `defining_crate(workspace, origin, path)` — resolves an origin-named path through a re-export to the crate that **defines** it | **the load-bearing reuse.** A moved test's `use tddy_daemon::host_registry` must be re-pointed at `tddy-host-service`, not at `tddy-session-lifecycle`, which merely re-exports it. Two hops | re-implement facade resolution, or touch `module_home` / `unrunnable_moves` |
| `3/10` restructure-clusters | `state_directory_for_plan` — run state keyed by plan | 123 moves span several plans, and `.restructure/` is per-repository until then | rely on cluster moves; test binaries reference nothing, so there is no cluster |
| `2/10` recipe-parsers | nothing consumed | — | touch `parser/` or the hooks |

## Draft PR contract

Published first:

**Published** (commit 2):

1. `RefactorKind::MoveTestBinaryToCrate` in the plan vocabulary, with its two **real** refusals —
   a facade (meaningless: nothing can reference a test binary) and a missing `to`.
2. `TestBinaryMove { source, name, origin, destination }` + `moved_to()`, `read_test_binary_move`
   and `resolve_test_binary_move` in `crate_move.rs`, bodies `todo!()`.

`TestBinaryMove` is deliberately **not** `Move`: that struct carries `module`, `origin` and
`reexport`, and a test binary has no module name to declare, no `mod` line in any origin to remove,
and no facade it could ever leave behind.

This PR goes on to implement all of it. **It must not merge in that state.**

### Boundary departure: this node fixes `module_home`, with consent

The `1/10` row above says this PR does **not** touch `module_home` / `unrunnable_moves`. It now
touches `module_home`, and the developer authorised that on 2026-09-19 after the alternative was
put to them.

`defining_crate` could not resolve either facade in this workspace, so the header pass did nothing
at all. Executing the real plans moved 31 suites that each kept `use tddy_daemon::…` and each gave
its new crate a `tddy-daemon` `[dev-dependency]` — a leaf crate depending back on the daemon, and a
test that compiles while naming the wrong crate. That is the precise failure the operation exists to
prevent, so the node cannot deliver its `## Responsibility` without the fix. All 31 moves were
reverted.

Two defects, both in `module_home.rs`:

1. `re_export_target` matches within a single line, and both facades are multi-line braced groups.
   `pub use tddy_session_lifecycle::{` carries no member to match, so the walk stopped on hop zero.
2. `defining_module_in_crate` reads only `<crate>/src/lib.rs`, so a `pub mod config;` whose
   `src/config.rs` is itself `pub use tddy_daemon_kernel::config::*;` reads as locally defined. The
   same shape holds for the other three shims this node deletes.

Node `1/10` has merged, so there is no parent branch left to carry the fix. The alternative —
re-implementing facade resolution inside this PR's `defining_home` — is what the `## Dependencies`
row exists to forbid, and would leave module moves broken for the nodes above. Reviewers of `5/10`
through `10/10` should know this function changed under them; every existing `module_home` test is
kept green for that reason.

## Green wave

**Wave:** 3 of 5
**Greenable independently:** **no** — FR1's header pass needs `defining_crate` to exist as
*behaviour*, not just as a signature, or every moved test is re-pointed one hop short
**Concurrent with:** `#carve` 5/10 `core-foundations`, 6/10 `git-plumbing`
**Blocks:** nothing structurally — but every node above it is **cheaper** once it lands, because
each then carves a crate whose tests travel with its code

Real dependency edges:

    n1 → n3, n4, n5, n6, n7, n10      n3 → n4, n8, n10      n5 → n7, n9, n10      n6 → n10

## Prerequisites

| Entry | Verdict | What this node does with it |
|---|---|---|
| [2026-09-15-122-of-tddy-daemons-139-test-suites-belong-to-other-crates.md](../todo/2026-09-15-122-of-tddy-daemons-139-test-suites-belong-to-other-crates.md) | ✅ **RESOLVED HERE** | This node is that entry. **Its wrap deletes the file.** |
| [2026-09-15-stack-progress-contract-acceptance-tests-only-tddy-core.md](../todo/2026-09-15-stack-progress-contract-acceptance-tests-only-tddy-core.md) | ✅ **RESOLVED HERE** | The one misplaced `tddy-workflow-recipes` suite moves with the rest. **Its wrap deletes the file.** The entry's own advice was to wait for `#carve` 9/9 so it is moved once — inserting this node ahead of the carving nodes satisfies that differently: it moves before `changeset.rs` is split, so its imports are rewritten once, here. |
| [2026-09-09-tddy-daemon-untested-complexity-hotspots.md](../todo/2026-09-09-tddy-daemon-untested-complexity-hotspots.md) | ⚠ **DURING** | Its by-file coverage figures are stated against `tddy-daemon`, and after this node those files are in other crates. **Annotate it**; do not claim it. |

## State A → State B

### State A

- `tddy-daemon`: 2,377 production lines, 139 test binaries, 55,682 test lines.
- `src/lib.rs` re-exports 82 modules from `tddy-session-lifecycle`, which re-exports 49 of them from
  ten further crates.
- **0 of 139** test files name `tddy_session_lifecycle`; **133** name `tddy_daemon::`.
- `tddy-session-lifecycle` has **no `tests/` directory**.
- 17 `tddy-*` `[dependencies]` are named by no file in `tddy-daemon/src/`.
- `move_module_to_crate` refuses any anchor outside `<crate>/src/`.

### State B

- `tddy-daemon/tests/` holds 17 suites; every other suite is in the crate it exercises.
- The facade and the four shims are gone; the 16 dependencies with them.
- `tddy-session-lifecycle` has its own 38,629-line acceptance suite.
- The tooling can move a test binary, and refuses the shapes that are not one.

## Implementation phases

| Phase | Kind | Work |
|---|---|---|
| **A** | manual | `move_test_binary_to_crate` — the operation, its refusals, and the `[dev-dependencies]` pass. Hand-written: it is new tooling, and there is no assist behind a cross-crate move |
| **B** | mechanical | The 123 moves as restructure plans, batched by destination crate. Thirteen plans, one per destination, each verifiable on its own |
| **C** | manual | Delete the `lib.rs` facade and the four shims; **re-point the 16 staying suites that name one** (see below); drop the 16 dependencies; add the dev-dependencies each destination now needs (`tddy-testing-commons`, `tddy-session-tool-client`, `tddy-livekit-testkit` and the rest currently sitting in `tddy-daemon`'s `[dev-dependencies]`) |
| **D** | manual | Annotate the CRAP backlog entry with the suites' new homes; `README.md` for `tddy-daemon` and `tddy-session-lifecycle` |

Phase B is the node's bulk and is entirely intents. Phase A exists to make Phase B expressible at
all, which is why they are one node and not two — an operation with no use and a use with no
operation are the layer split the boundary contract forbids.

## Measurements, as measured

The plan's figures were taken against 139 test binaries on 2026-09-15. Master has moved since —
`#carve` 1/10, 2/10 and 3/10 merged, and the index-daemon work added suites. Restated from the
tests themselves on 2026-09-19:

| Quantity | Planned | Measured | Why it moved |
|---|---|---|---|
| Test binaries in `tddy-daemon` | 139 | **140 on master** | one suite added on master; this node adds `test_placement.rs` as the 141st |
| Suites that stay | 17 (+ this file) | **21 (+ this file)** | four cannot move — see below |
| Suites that move | 122 | **119** from `tddy-daemon`, **+1** from `tddy-workflow-recipes` = **120** | 140 − 21, plus the outlier the plan also named |
| Runtime dependencies named by no `src/` file | 17 | **16** | one is now named in `src/`; the earlier correction to 17 has itself been overtaken |

### Four suites the plan counted as strays are about `tddy-daemon` itself

- `index_daemon_lifecycle_acceptance.rs` names `tddy_daemon::index_daemon`, which this crate defines.
- `local_socket_reachability_acceptance.rs` and `unbundle_endpoint.rs` read this package's own `src/`.
- `unbundle_tools_dependency_dropped.rs` reads this package's own `Cargo.toml`.

The last three cannot move at all: `CARGO_MANIFEST_DIR` would name whichever crate they landed in,
so each assertion would go on passing while silently being about something else — the same reason
the discovery gave for leaving `proto_workflow_contracts.rs` in `tddy-workflow-recipes`.

### Phase C also has to re-point what the shims leave behind

The four shims are `config`, `relay_idle`, `tddy_user_config` and `user_sessions_path`, each a
two-line `pub use` over the crate that owns it. AC6 asserts only that none of the four still
contains `pub use tddy_session_lifecycle::`, and **`config` forwards to `tddy-daemon-kernel`, not to
`tddy-session-lifecycle`** — so it already satisfies the assertion and stays. Only `relay_idle`,
`tddy_user_config` and `user_sessions_path` have to go.

That makes the re-pointing much smaller than first recorded here. It is **4 staying suites**, not
16: `relay_e2e_acceptance.rs`, `relay_runtime_acceptance.rs` and `relay_idle_wired_acceptance.rs`
name `tddy_daemon::relay_idle`, and `local_token_uds.rs` names `tddy_daemon::user_sessions_path`.
The 15 suites naming `tddy_daemon::config` need no change at all.

**The daemon's own `src/` also names two of the three**, which the plan did not record: `crate::relay_idle`
twice and `crate::user_sessions_path` six times. Deleting a shim without re-pointing its callers in
`src/` breaks the crate itself, not just its tests.

### Two moved suites reach sibling source by string path

`session_chaining_phase2_acceptance.rs` and `telegram_chain_workflow_dispatch_acceptance.rs` read
`concat!(env!("CARGO_MANIFEST_DIR"), "/../tddy-session-lifecycle/src/telegram_bot.rs")`. The header
pass rewrites `use` declarations, not string literals, so both need a hand edit once moved —
recorded here rather than discovered when they fail.

## Closing measurements

Re-measured on 2026-09-19 at wrap, from the tree rather than from this document's claims. These are
the numbers the deleted `code-issues` records carried, so they survive their deletion.

| Record | First detection (2026-09-15) | At wrap (2026-09-19) |
|---|---|---|
| `heavy-dependency-tests-only-runtime-deps.md` | 17 `tddy-*` runtime dependencies named by no file in `src/` | **0** of 27 |
| `misplaced-tests-integration-suites.md` | 122 of 139 suites never reach this crate's production code | **0** of 22 |

`src/lib.rs` carries **0** `pub use tddy_session_lifecycle::` lines, down from the two blocks that
re-exported 82 modules. Both records are deleted by this wrap; both measured clean, so neither is a
partial fix being closed.

CI on `5e91f2b2`: **7,006 Rust tests passed, 0 failed; 2,637 web tests passed, 0 failed**, with both
build arches, lint, generated-code and every VM check green. That whole-workspace count is the
evidence for AC9 — a relocation that silently dropped a suite would show up there and nowhere else.

### Deferred, with consent

Three files sit at or over the 500-production-line budget. All three were deferred on 2026-09-19
with the developer's explicit consent, and each carries a record rather than a mention:

| File | Before → after | Why deferred |
|---|---|---|
| `crate_move/test_binary.rs` | 0 → **966** | Created by this node at 1.9× budget. [`oversized-file-test-binary.md`](../../../packages/tddy-code-restructuring/docs/code-issues/oversized-file-test-binary.md) + [TODO](../todo/2026-09-19-test-binary-rs-is-950-production-lines.md) |
| `backends/rust.rs` | 4,772 → **4,788** | +16 lines. **#491 also touches it**, so the `/pr-wrap` stack-overlap stop applies — splitting it here would conflict a PR in flight |
| `tddy-daemon/src/runtime.rs` | 1,420 → **1,423** | +3 reflowed lines from the shim re-points, on a composition root already 2.8× budget |

## Validation Results

Both gates run **statically** on `5e91f2b2` — `target/` had been cleared, so no cargo run was made
locally; the test evidence is CI on this exact commit (7,006 Rust / 2,637 web, 0 failed).

### `/validate-tests` — 2026-09-19 — PASS

25 newly authored tests analysed: `tddy-code-restructuring/tests/test_binary_move.rs` (18),
`tddy-daemon/tests/test_placement.rs` (4), `crate_move/module_home.rs` inline `mod tests` (3). The
119 relocated suites are pre-existing code; their diffs are import re-points plus rustfmt reflow
only — no behavioural edit, no `#[ignore]` added anywhere.

Fluent-tests compliant: Given/When/Then throughout, one behaviour per test, named fixture builders
(`a_workspace_whose_moved_test_reads`, `ACrate::holding`, `the_resolved_move_in`), whole-file
equality assertions, no sleeps, no ports, no network, no branching.

Warnings (none blocking):

- `tddy-daemon/tests/test_placement.rs:138` — `assert!(!suites.is_empty())` passes for 1 suite as
  well as for the 97 the doc comment claims; a loose matcher without the justification comment the
  guidelines require.
- `tddy-daemon/tests/test_placement.rs:113-125` — the shim check reads each file with
  `unwrap_or_default()`, so a typo in a shim path asserts against an empty string and passes. Also
  two behaviours (facade + shims) and a loop in one test.
- `tddy-daemon/tests/test_placement.rs:175-188` — substring matching (`source.contains("tddy_x")`) means
  a crate whose extern name prefixes another's is reported as used; the manifest scan also takes
  every `tddy-` line before `[dev-dependencies]`, whatever table it is in.
- `tddy-daemon/tests/test_placement.rs:75` — one-directional: a `BELONGS_HERE` entry that is deleted
  fails nothing.
- `tests/test_binary_move.rs:22,228` — `"path":"whatever"` / `path: "whatever"` is placeholder data
  with no semantic meaning (`an_irrelevant_symbol()` would say the same thing deliberately).

### `/analyze-clean-code` — 2026-09-19 — Score A (file length excluded, measured separately above)

Function length, nesting, parameter count, magic values, duplication and naming, over this node's
own code. No parameter-count violation (every new signature takes 4 or fewer). One must-refactor and
one needs-attention item, both in the new file:

- `crate_move/test_binary.rs:880` `destination_dev_dependencies` — **71 lines**, over the 60-line
  gate. Three decisions in one loop (already-declared, authored path line, carried-across line);
  extracting `dev_dependency_line_for(...)` leaves a ~20-line loop.
- `crate_move/test_binary.rs:815` `defining_home` — 48 lines; the early refusals and the facade walk
  split cleanly.

Duplication worth folding (minor, all in this node's code):

- The "try `[dependencies]`, then `[dev-dependencies]`" pair is written four times —
  `test_binary.rs:320`, `:895`, `:916`, `destination.rs:72`. One `manifest_edits` helper per
  question would carry it.
- The keyword guard `matches!(head, "crate" | "self" | "super" | "std" | "core" | "alloc")` is
  duplicated at `test_binary.rs:255` and `:304`.
- The `strip_prefix("pub") → trim_start_matches(|c| c != ' ')` visibility skip is identical at
  `test_binary.rs:414` (`use_trees`) and `:764` (`module_declared_by`).

Noted, not a defect: `names_bound_in`/`modules_declared_in` scan raw lines rather than
[`readable_spans`], so an unindented `use`/`mod` line inside a raw-string fixture — the shape this
very crate's own tests are written in — is read as a binding. The failure mode is a dev-dependency
line not written, which is a compile error at CI, never a silent wrong edit.

Pre-existing, untouched by this node: `backends/rust.rs` (`next_import` 137 lines / 7 params,
`resolve` 99, `assisted_edit` 92), `plan.rs:306` `parse_op` (109), `tddy-daemon/src/runtime.rs`
(`build` 808, `spawn` 110) and `main.rs:31` `main` (177). This node adds 15 lines to `rust.rs` and
re-points paths in `runtime.rs`; neither function was restructured here.

### `/validate-changes` — 2026-09-19 — ⚠️ Gaps (no blockers)

Static analysis only — `target/` was cleared, so the build/test evidence is CI on `5e91f2b2`
(7,006 Rust / 2,637 web, 0 failed; both arches, lint, generated code and every VM check green).

**Stack gate.** Base `master`; `origin/master..HEAD` is this PR's 16 commits only — leak-free.
Deletions are exactly the three two-line shims (`relay_idle.rs`, `tddy_user_config.rs`,
`user_sessions_path.rs`); nothing else is deleted anywhere in the diff.

**Boundary.** `unrunnable_moves` lives in `crate_move/preconditions.rs`, which is **not in the
diff** — untouched, as the `## Dependencies` row requires. `module_home.rs` is modified, which is
the authorised departure recorded above; `module_home()` and `defining_crate()` keep their
signatures and every change is in their private helpers.

**Behaviour preservation across the 120 relocations.** Every `+`/`-` line in the rename diff is a
crate-path rewrite or rustfmt reflow of a `use` group (459 added / 429 removed; the +30 is `};`
lines from re-wrapping). No assertion text, no expectation, no `#[test]`/`#[ignore]` attribute and
no test function changed; the only `fn` signature edits are `tddy_daemon::config::DaemonConfig` →
`tddy_daemon_kernel::config::DaemonConfig` in three helper returns. Every `CARGO_MANIFEST_DIR`
string path in a moved suite still resolves from its new crate (`../../target/debug/…` keeps its
depth; the two `/../tddy-session-lifecycle/src/…` paths are now self-referential but correct).

Gaps, in order of what they cost:

1. **`packages/tddy-workflow-recipes/tests/stack_progress_contract_acceptance.rs` did not move.**
   All 120 relocations have a `tddy-daemon` source. `## Responsibility` bullet 2 asks for "the one
   misplaced `tddy-workflow-recipes` suite" too, and the `## Prerequisites` row for
   `2026-09-15-stack-progress-contract-acceptance-tests-only-tddy-core.md` says ✅ **RESOLVED
   HERE — its wrap deletes the file**. It is not resolved: the suite still sits in
   `tddy-workflow-recipes` and its whole import block is `tddy_core::`. Either move it before wrap
   or change that row to a deferral, or `/wrap-context-docs` deletes a backlog entry for work
   nobody did.
2. **`packages/tddy-daemon/Cargo.toml` — five dev-dependencies are now named by no file in
   `tests/`**: `tddy-session-tool-client` (with `features = ["livekit"]`, the heavy one),
   `tddy-supervisor`, `tddy-sandbox-cgroups`, `rstest`, `scopeguard`. Their comments still describe
   suites that left (`supervisor_spawn_delegation.rs`, "the four suites that stayed here", "the
   remote-git acceptance suites"). AC8 measures `[dependencies]` only, so the dev-side mirror of the
   same waste is unmeasured and survived the cut.
3. **`packages/tddy-daemon/Cargo.toml:60-63, 71-76` and the `[dependencies]` tail — five comment
   blocks were orphaned by the dependency removals** and now sit directly above unrelated lines,
   mis-attributing themselves: the `tddy-session-sync` mirror paragraph now heads `tddy-actions`;
   the `tddy-daemon-sandbox` and `tddy-spawn` paragraphs are stacked above `tddy-spawn` alone (and
   `tddy-daemon-sandbox` was re-added lower with its own comment); the `tddy-telegram` paragraph
   heads `tddy-daemon-auth`; and the `tddy-session-files` / `-activity` / `-agents` paragraphs are
   stacked above `tddy-session-lifecycle`. A reader is now told the wrong thing about six lines.
4. **`packages/tddy-code-restructuring/src/plan.rs:328-334, 351` — a comment sentence was severed**
   by the insertion. "…so it honours the field for the same reason and with the same" runs straight
   into the new test-binary paragraph, and its ending, "`// failure mode if the field were
   ignored.`", is left dangling alone above the generic `reexport` refusal at `:352`.
5. **Count:** 120 suites moved, not 119. `141 − 22` subtracts `test_placement.rs`, which this node
   *adds*; the arithmetic is `141 − 21 stayed = 120`. Affects the Measurements table, the AC7 doc
   comment ("97 of those suites" — it is 96), and the scope note added to
   `2026-09-09-tddy-daemon-untested-complexity-hotspots.md`.
6. **Working tree is not clean at the time of this run** — two `code-issues` records staged for
   deletion and four untracked doc files (`oversized-file-*.md`,
   `2026-09-19-test-binary-rs-is-950-production-lines.md`). CI's green is for `5e91f2b2` and does
   not cover them; they need committing before the PR is read as complete.

Not defects, checked and clear: `src/config.rs` correctly stays (it forwards to
`tddy-daemon-kernel`, not to `tddy-session-lifecycle`, so AC6 is satisfied as written — the
"four shims" wording in `test_placement.rs` and above is about the four candidates, three of which
go); `local-model` now forwards through `tddy-session-lifecycle`, which declares that feature; the
`Table` parameterisation of `manifest_edits` passes `Table::Dependencies` at every pre-existing
call site, so module moves are unchanged.

### `/validate-prod-ready` — 2026-09-19 — ✅ Ready

Production files checked: the 11 `tddy-code-restructuring` sources, `tddy-daemon`'s `lib.rs`,
`main.rs`, `runtime.rs`, `tddy-desktop`'s `lib.rs`, and six manifests. Test files excluded.

| Category | Count | Status |
|---|---|---|
| Mock / fake / stub code in production paths | 0 | ✅ |
| Development fallbacks, test-environment branches | 0 | ✅ |
| TODO / FIXME / HACK added by this node | 1 | ✅ documented |
| Debug output (`println!`, `eprintln!`, `dbg!`) | 0 | ✅ |
| Commented-out code | 0 | ✅ |
| `unwrap` / `expect` added outside `#[cfg(test)]` | 0 | ✅ |
| `unsafe`, secrets, unvalidated input | 0 | ✅ |

The single marker is `plan.rs:1738` `TODO(carve-test-homes)` — `check` has no static preflight for
a test-binary move. Deliberate, named, and explained in the doc comment it sits in. Every `expect`
added is inside `module_home.rs`'s `#[cfg(test)] mod tests`. `test_binary.rs` is 604 non-comment
lines across 33 small functions with no test-only branch in any of them; its size is the deferral
already recorded above, not a readiness gap. Nothing here blocks the merge.

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset — this document
- [x] Publish the draft-PR contract (operation surface + failing tests)
- [x] Failing acceptance tests — **USER REVIEW** (approved 2026-09-15, gates delegated)
  - `tddy-daemon/tests/test_placement.rs` — 4 failing: 122 strays still present, the facade and its
    four shims still there, `tddy-session-lifecycle` still has no `tests/`, and **17** runtime
    dependencies named by no file in `src/`.
- [x] Failing unit/integration tests
  - `tddy-code-restructuring/tests/test_binary_move.rs` — 1 failing on `read_test_binary_move`;
    **3 passing**, because the vocabulary refusals are real logic rather than stubs.
  - **Correction:** these documents first said *16* unused runtime dependencies. The test measured
    **17** — the original list had seventeen entries and was miscounted. Corrected throughout.
- [x] Implement production code making tests pass (`/green`) — Phases A–D; `test_placement.rs` 4/4
- [x] Annotate the CRAP backlog entry — scope note, nothing claimed
- [x] `/validate-changes` + `/validate-prod-ready` — see **Validation Results**; 6 gaps, 0 blockers
- [ ] `/pr-wrap` — correct the title, ready for review
- [ ] Add a changeset entry under `docs/dev/changesets/` (`/wrap-context-docs`)

## Verification

```bash
./test -p tddy-code-restructuring          # the new operation
./test -p tddy-daemon                      # the 17 that stay
./test -p tddy-session-lifecycle           # the 97 that arrive — a suite this crate never had
cargo clippy -p tddy-code-restructuring -p tddy-daemon -p tddy-session-lifecycle -- -D warnings
```

**AC9 is the load-bearing check and belongs on CI**: the whole-workspace test *count* must be
unchanged. A relocation that silently drops a suite — a file moved to a crate whose
`[dev-dependencies]` cannot build it, so cargo never compiles it — looks identical to a clean move
from any single scoped run.
