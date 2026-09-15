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
| **C** | manual | Delete the `lib.rs` facade and the four shims; drop the 17 dependencies; add the dev-dependencies each destination now needs (`tddy-testing-commons`, `tddy-session-tool-client`, `tddy-livekit-testkit` and the rest currently sitting in `tddy-daemon`'s `[dev-dependencies]`) |
| **D** | manual | Annotate the CRAP backlog entry with the suites' new homes; `README.md` for `tddy-daemon` and `tddy-session-lifecycle` |

Phase B is the node's bulk and is entirely intents. Phase A exists to make Phase B expressible at
all, which is why they are one node and not two — an operation with no use and a use with no
operation are the layer split the boundary contract forbids.

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
- [ ] Implement production code making tests pass (`/green`)
- [ ] Annotate the CRAP backlog entry
- [ ] `/validate-changes`
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
