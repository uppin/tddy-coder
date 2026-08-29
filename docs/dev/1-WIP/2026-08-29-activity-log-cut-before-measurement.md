# Changeset: A tick attributes only the calls its measurement could have seen

**Date**: 2026-08-29
**Status**: 🚧 In Progress — **not compiled or run** (no Rust toolchain in this workspace)
**Type**: Fix

## Affected Packages

- **tddy-daemon**: [README.md](../../packages/tddy-daemon/README.md)
  - `src/session_room.rs` — the poll loop cuts the activity log **before** `measure()`; the read
    moves out of `broadcast_new_activity` into a new `read_activity_log`, which hands it down
  - `tests/session_room_livekit_acceptance.rs` — `applying()` names an empty patch for what it is
  - changeset index entry still to write (`packages/*/docs` is not edited directly)

## Related Feature Documentation

- [Session worktree sync](../ft/daemon/session-worktree-sync.md) — AC4/AC5 (which tick a call
  belongs to) and AC6 (the call is served the patch its tick produced)
- [2026-08-15 session-worktree-sync](2026-08-15-session-worktree-sync.md) — the in-progress
  changeset this defect belongs to

## Symptom

`session_room_livekit_acceptance::a_call_recorded_during_a_tick_is_served_the_patch_that_tick_produced`
fails in CI at `tests/session_room_livekit_acceptance.rs:396` — the `git apply` assert inside the
`applying()` helper. `git apply` exits 128 with `No valid patches in input` when its input is
**empty**, which is what that panic really reports: the delta served for the call carried an empty
patch. (Reproduced directly: `printf '' | git apply -` → 128.) The suite is in the LiveKit
flaky-retry group in `.config/nextest.toml`, so it fails only some of the time.

## Cause

`SessionRoomPoll::run` measured the checkout first and read the activity log last:

```
sleep → measure() → announce() [git write-tree, diff, update-ref, LiveKit metadata write]
      → broadcast_new_activity() [reads agent-activity.jsonl, attributes its new rows to this tick]
```

A call is stamped with the seq of a delta, and that delta has to be able to hold the call's change.
Reading the log *after* the measurement admits rows the measurement could not have seen: a tool
writes its file and then records the completed call, so a row appended anywhere in the
measure-and-announce span — two git shell-outs and a network write, a large fraction of a poll
interval — was stamped with a delta cut **before** its write existed. `DELTA_SCOPE_CALL` then
selects no section of that tick's patch and answers empty, permanently; the bytes surface in the
residual instead, so nothing is lost on the wire, but AC6 is not met and a client that trusts a
call's own delta never sees the edit.

The window is `[measurement, log read]` — hundreds of milliseconds against a 200 ms test poll, which
is why CI sees it and a laptop usually does not.

## Change

Cut the log **before** the measurement and attribute only the rows in that cut; rows appended later
wait for the next tick, whose measurement is later than the write they describe. `read_activity_log`
holds the read (and its two failure paths, unchanged); `broadcast_new_activity` takes the cut it was
given. A tick whose log read failed still measures and announces, exactly as before.

## Residual, deliberately not fixed here

A tick boundary falling **between** a write and the record that describes it still mis-attributes
that one call: the write is measured by tick N, the row is cut by tick N+1, and N+1's diff no longer
names the path. That window is the microseconds between a tool's write and its log append, against a
2 s shipped poll interval — where the one this change closes was a large fraction of every interval.
Eliminating it needs attribution to search the ring for the most recent tick whose patch names the
declared paths, rather than always crediting the current tick, which is a change to what
`activity_seq` means on the wire and belongs to the feature's owner.

## Tests

- The AC6 acceptance above is the test; it was failing on the defect, and its `applying()` helper now
  names an empty patch instead of leaving `git apply`'s "No valid patches in input" to be decoded.
- ⚠️ **No deterministic regression test.** Reproducing the window on demand means appending to the
  log *during* a measurement, and the loop cannot be built from a test: `SessionRoomRegistry::register`
  is private and `BroadcastPublisher` cannot be constructed outside the crate (the same wall this
  suite's module note describes). Extracting the per-tick body into a function that takes its
  measurement and its log cut would make it reachable — worth doing, and larger than this fix.
- ⚠️ **Unverified**: no `cargo` in this workspace, so this has not been compiled. `./test -p
  tddy-daemon` plus the LiveKit suite must run before wrap.
