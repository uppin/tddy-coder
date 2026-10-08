# 2026-10-08 — `tddy-session-lifecycle`'s `test_util` (366 lines) is not behind a `test-util` feature

**Category:** Deferred from `lifecycle-moves` (#536): ruled work not done, and the reason lifecycle ends ~1k over its size target
**Source:** #carve 21/21 (PR #536), R10 and acceptance check B5 of
[`2026-09-26-carve-lifecycle-moves`](../1-WIP/2026-09-26-carve-lifecycle-moves.md)

## What was ruled, and what was not done

The developer's ruling of 2026-10-08 was "a `test-util` feature" for `tddy_session_lifecycle::test_util`
(`packages/tddy-session-lifecycle/src/test_util.rs`, **366 lines**, `pub mod test_util;` at `lib.rs:148`).
The changeset assumed its users were lifecycle's own integration suites. They are not, so the feature was **not added** in this PR.

## Who names it

`tddy_session_lifecycle::test_util` is named by 36 files outside lifecycle (grep, 2026-10-08):

- `tddy-daemon-rpc/src/test_util.rs` (non-test source, so it needs the feature on a **normal** dependency of lifecycle) and 26 more files in `tddy-daemon-rpc`;
- 9 files in `tddy-daemon`;

and by 26 files inside lifecycle (its `src/` and `tests/`).

A `test-util` feature therefore needs feature lines in `tddy-daemon-rpc`'s and `tddy-daemon`'s manifests
(`tddy-session-lifecycle = { …, features = ["test-util"] }`, as a normal dependency in `tddy-daemon-rpc`), plus a
self-referencing dev-dependency or `required-features` for lifecycle's own suites. Those are consumer edits, which this
node's Boundaries ("Consumers are unedited") did not allow and the developer did not lift.

## Effect on the node

Lifecycle ends at **5,587 production lines** (the changeset's counter, re-measured 2026-10-08) against the ~4.4k-4.6k target: the 366 lines are the
largest part of the ~1k overshoot. The rest is:

- `PeerRouted*` and the port adapters, which **stay by the developer's ruling** of 2026-09-25 (D13, accepted at ~4.5k);
- the three wiring modules the host-block node created (`svc_conversation_worktree_wiring.rs` 48,
  `svc_session_identity_wiring.rs` 57, `svc_worktree_observer_wiring.rs` 15 lines: 120 in all);
- the facade lines for each receiver.

## What would close it

`test_util` is behind a feature (or moved to a testkit crate), the two consumer manifests carry the feature, and lifecycle's
production count is re-measured. Plan it as its own change: it edits `tddy-daemon` and `tddy-daemon-rpc`, which a move PR must not.
