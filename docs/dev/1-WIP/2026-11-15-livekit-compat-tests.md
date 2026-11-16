# Changeset: LiveKit suites move into one compatibility package

**Date**: 2026-11-15
**Status**: 🚧 In Progress
**Type**: Refactor

## Initial Discovery

[2026-11-15-livekit-compat-tests-initial-discovery.md](./2026-11-15-livekit-compat-tests-initial-discovery.md)

## Related Feature Documentation

None — behaviour-preserving restructure. No PRD.

## Summary

A new crate, `tddy-livekit-compat-tests`, holds every suite whose job is to prove that a LiveKit
adapter connects and honours the RPC contract: 24 test binaries moved out of `tddy-livekit`,
`tddy-daemon-livekit`, `tddy-e2e` and `tddy-coder-cli` with `move_test_binary_to_crate`.
Nothing outside a moved suite changes path: a test binary is referenced by nothing, the origin
crates keep every production module, and the moved suites keep their test names, so the nextest
binary names CI reports are unchanged — only the owning package changes.

What stays reachable: every moved suite still runs, now selected by
`package(tddy-livekit-compat-tests)` in a new `.config/rust-livekit-e2e.filterset` and run by a new
PR check, `Rust LiveKit e2e tests`. The old `.config/rust-e2e.filterset` loses those binaries.

## Background

`docs/dev/todo/2026-11-05-tests-use-cheap-rpc-livekit-only-for-compatibility.md`: behaviour tests
should talk over a cheap transport, and LiveKit tests should exist only to check compatibility, in
a dedicated package. Measured on this branch, 40 test files import the LiveKit testkit; 24 of them
are contract suites for a LiveKit adapter and 15 are logic suites using a real room as transport.
This changeset does the contract half and the CI half. The logic half (an in-memory participant/peer
transport for the daemon's cross-host suites) and the media port stay in that TODO, narrowed.

## Scope

- [x] **Plan**: layout proposed, seam order fixed, snapshot taken
- [x] **Apply**: dry run clean, plan applied, per-seam checks green
- [x] **Baseline**: build and full suite back to the recorded numbers
- [x] **Code Quality**: clippy and fmt clean on touched packages
- [x] **Documentation**: Final Checklist derived

## Technical Changes

### State A

| Origin | Suites moving | Examples |
|---|---:|---|
| `tddy-livekit` | 9 | `rpc_scenarios`, `rpc_three_participant_forward`, `room_connector`, `broadcast_and_room_metadata` |
| `tddy-daemon-livekit` | 9 | `livekit_peer_daemons_acceptance`, `forward_to_peer_shared_registry`, `session_room_livekit_acceptance`, four `common_room_*` repros |
| `tddy-e2e` | 4 | `livekit_terminal_rpc`, `terminal_service_livekit`, `token_generation_livekit`, `token_service_livekit` |
| `tddy-coder-cli` | 2 | `coder_publishes_session_metadata_to_participant`, `coder_serves_connection_service_from_participant` |

Kept in place (no real server needed): `tddy-livekit`'s `participant_metadata_unit`,
`room_roster_deadline`, `signal_tls`; every non-LiveKit suite in the origin crates.

### State B

| Module | Contents |
|---|---|
| `packages/tddy-livekit-compat-tests/Cargo.toml` | package only; all suite dependencies arrive as `[dev-dependencies]` written by the moves |
| `packages/tddy-livekit-compat-tests/src/lib.rs` | the contract-only rule, stated where a reader lands |
| `packages/tddy-livekit-compat-tests/tests/livekit_test_placement.rs` | pins: moved suites present here and gone from origins; `tddy-livekit-testkit` dev-dependency holders are this package plus the named remaining logic packages |
| `.config/rust-livekit-e2e.filterset` | `package(tddy-livekit-compat-tests)` and the testkit's own suites |
| `.github/workflows/ci.yml` | new `Rust LiveKit e2e tests` check running only that filterset against one shared LiveKit server |

### Delta — seams in dependency order

1. Create the destination crate (hand scaffold: manifest, empty lib, workspace member).
   `move_test_binary_to_crate` refuses `name`, so it cannot create its destination.
2. Move `tddy-livekit`'s 9 suites. One plan, one group.
3. Move `tddy-daemon-livekit`'s 9 suites. One plan, one group.
4. Move `tddy-e2e`'s 4 suites and drop their `#[cfg(feature = "livekit")]` gate: in this crate the
   suites are the point, so they are unconditional. Drop the now-unused `livekit` feature plumbing
   from `tddy-e2e` if nothing else names it.
5. Move `tddy-coder-cli`'s 2 suites.
6. Add the placement test; drop each origin's `tddy-livekit-testkit` dev-dependency where no
   remaining file names it.
7. CI: new filterset, new check, moved binaries removed from `rust-e2e.filterset`.

## Callers

| | |
|---|---|
| External importers | none — nothing can reference a test binary |
| Facade | none (`reexport` is refused on a test-binary move) |
| Consequence | CI selection changes only: `scripts/nextest-e2e-archive.ts` must still resolve every name left in `rust-e2e.filterset`, and the new filterset must resolve against the workspace |

## Visibility

One widening, made by hand before the `tddy-coder-cli` plan would resolve:
`packages/tddy-coder-cli/src/lib.rs` now declares `pub mod livekit_participant_host;` where it
declared `mod livekit_participant_host;`. The item it holds, `LiveKitParticipantHost`, was already
public through the crate-root re-export; the module is public so a moved suite can name the item by
the path that defines it, which is the shape `move_test_binary_to_crate` can re-point. No other
item changed visibility, and no apply reported a widening.

Preparation the plans required, also by hand: in the suites being moved, a grouped
`use <origin>::{…}` and any use of a crate-root re-export were rewritten to one `use` per item,
spelled by the module that defines it (for example `tddy_livekit::participant::LiveKitParticipant`).
The operation refuses a group outright, and its defining-crate walk follows a single-item re-export
to the module behind it and then has no manifest line to carry. Same items, same names in scope;
the suites compile unchanged in their origin crates before they move.

## Baseline

Measured after the moves (scoped, per AGENTS.md; the pre-move tree no longer exists to re-run,
so the comparison is against the suites' own results in their new home):

| Gate | Result |
|---|---|
| `tddy-livekit-compat-tests` + `tddy-livekit-testkit`, self-containered, `--test-threads=1` | 27 of 28 targets green: 76 tests passed, 0 failed (incl. placement 3/3) |
| `livekit_terminal_rpc` (in the compat package) | 1 of 4 tests passed locally; the other 3 hang on participant connection in this sandboxed network (past their own 180 s timeouts) — **unverified locally, left to the CI leg** |
| `./test -p tddy-livekit -p tddy-daemon-livekit -p tddy-coder-cli` | 301 passed, 0 failed |
| `cargo clippy -p tddy-livekit-compat-tests -p tddy-livekit -p tddy-daemon-livekit -p tddy-e2e -p tddy-coder-cli --all-targets -- -D warnings` | clean |
| `cargo fmt --all --check` | clean |
| `scripts/nextest-e2e-archive.ts` over both filtersets | e2e: 91 targets; LiveKit: 28 targets |
| CI script tests (`nextest-e2e-archive`, `nextest-serial-groups`, `livekit-ci-server`) | 41 passed |

Two adaptations surfaced by enabling `tddy-e2e`'s `livekit` feature from the new crate (feature
unification means workspace clippy now compiles that gated code): the stale three-argument
`Presenter::new` call is fixed, the helper's return type uses the existing `ViewConnectionFactory`
alias, three `collapsible_if` sites and one manual `ok()` in the moved suites / `rpc_demo` example
are collapsed. `terminal_service_livekit` also needs the real `tddy-coder` binary in
`target/debug`; the new CI job builds `tddy-coder-cli` before running nextest for exactly that.

## Plan

Four JSONL plans under `.restructure/livekit-compat/`, one per origin crate, generated with
`restructure snapshot` and gated with `check --deep` before each apply.

## Decisions & Trade-offs

- **The logic suites do not move.** Moving a cross-host logic suite into the compat package would
  put a use-case where the TODO forbids one. They stay in their crates until they run over the
  in-memory `PeerForwarder`/participant seam; the placement test names them as the remaining set.
- **`tddy-livekit-testkit`'s own tests stay in `tddy-livekit-testkit`.** They exercise the testkit
  library itself and name it as a library dependency, not a dev-dependency, so the "only package
  with a `tddy-livekit-testkit` dev-dependency" rule is still literally true when the logic suites
  are converted. The new CI check runs them beside the compat package.
- **Media is out of scope.** `tddy-screenshare`, `tddy-screen-sharing`, `tddy-vnc` keep their direct
  LiveKit dependencies in this slice; the TODO keeps that clause.

## Final Checklist

- [x] `packages/tddy-livekit-compat-tests/README.md` — what belongs here, what does not
- [x] `docs/dev/guides/ci.md` — the new `Rust LiveKit e2e tests` check and the filterset split
- [x] `docs/dev/todo/2026-11-05-tests-use-cheap-rpc-livekit-only-for-compatibility.md` — narrow to
      the logic-suite conversion and the media port; record what this changeset closed
- [x] Per-package `docs/changesets/2026-11-15-livekit-compat-tests.md` for each origin crate and the
      new crate, plus the cross-package entry in `docs/dev/changesets/`
