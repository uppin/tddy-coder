# Changeset: Terminal resync at any point

**Created:** 2026-10-25
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 5 — two packages, a proto contract change, ordering semantics (a new sequence point inside a running stream, lagged recovery)
**Stack:** `#term-snapshot 2/4` — branch `feature/term-snapshot/resync`, base `feature/term-snapshot/host-snapshot` (#760)
**PR:** [#761](https://github.com/uppin/tddy-coder/pull/761)
**PRD:** [PRD-2026-10-25-terminal-resync.md](../../ft/daemon/1-WIP/PRD-2026-10-25-terminal-resync.md)

## Initial Discovery

[2026-10-25-term-snapshot-resync-initial-discovery.md](./2026-10-25-term-snapshot-resync-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| `packages/tddy-terminal-rpc/docs/code-issues/complexity-bridge-open-replay-ack-live.md` | ⚠ during: the resync select arm goes in the live-bridge loop. Keep it in its own function. Deferral allowed (developer consent) |

## Affected Packages

- [ ] `tddy-terminal-rpc`: proto field, the bidi input→output resync channel, lagged → snapshot
- [ ] `tddy-web`: the continuity guard in the host terminal feed

## Contract (pass 2, approved 2026-10-25)

| Test | File | Fails today because |
|---|---|---|
| `a_mid_stream_request_yields_a_snapshot_that_renders_the_servers_screen_then_only_later_deltas`, `a_lagged_subscriber_is_resynced_with_a_snapshot_not_a_hole` | `packages/tddy-terminal-rpc/tests/stream_resync_acceptance.rs` | #760's snapshot open is still a stub; once it is green, the resync itself is missing |
| `tail_streams_keep_their_lagged_behaviour` | same | passes today, as a guard |
| 6 continuity cases | `packages/tddy-web/src/lib/terminalFrameContinuity.test.ts` | `TerminalFrameContinuity.accept` stub |
| `does not paint across a gap and asks for a snapshot on the same stream` | `packages/tddy-web/cypress/component/TerminalResyncOnGap.cy.tsx` | no bidi open until #760 is green |

Surface: `SessionTerminalInput.request_snapshot = 11` (17 existing struct literals got
`request_snapshot: false`), regenerated `terminal_session_pb.ts`, `TerminalFrameContinuity` /
`ContinuityVerdict` / `OffsetStampedFrame` in `packages/tddy-web/src/lib/terminalFrameContinuity.ts`,
and test-support helpers `StubTerminal::sized_with_broadcast_capacity` and
`typing_and_requesting_a_snapshot`.

## Responsibility

- Proto: `SessionTerminalInput.request_snapshot = 11`, valid on **any** message of
  `StreamSessionTerminalIO`.
- Bridge: the input forwarder applies the message's `data` first, then signals the output task. The
  output task calls `snapshot_and_subscribe()`, **replaces** its broadcast receiver, emits a
  snapshot frame and continues stamping live offsets from the new `end_offset`.
- Bridge: `RecvError::Lagged` → the same resync (fresh snapshot frame), in every stream opened with
  `SNAPSHOT`. `TAIL`/`FROM_OFFSET` streams keep their current lagged behavior.
- Web: `terminalFrameContinuity` guard. A data frame whose `startOffset` is not the last
  `endOffset` is dropped unpainted, and the feed sends `requestSnapshot: true` on its open bidi
  stream. It is exported for the room feed (`#term-snapshot 4/4`) to reuse.

## Boundaries

- Opening with `SNAPSHOT`, the screen model and the sequence point belong to `#term-snapshot 1/4`.
- No room wire (`#term-snapshot 4/4`) and no jail sizing (`#term-snapshot 3/4`).
- No timer-driven or heuristic resync. Only an explicit request, a lag, or a detected gap triggers one.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `#term-snapshot 1/4` host-snapshot (#760) | `TerminalSession::snapshot_and_subscribe`, `ScreenSnapshot`, proto `SNAPSHOT` + `snapshot*` fields, offsets on live frames, the bidi web host feed, `TerminalFrame.{snapshot,startOffset}` | re-takes `snapshot_and_subscribe` mid-stream; the guard reads `startOffset`/`endOffset` | change the port, the snapshot serializer, the open path, or the feed's transport |

## Draft PR contract

The first push (pass 2) publishes the proto field (regenerated Rust + TS), the exported
`terminalFrameContinuity` guard signature in tddy-web (stubbed, `// TODO(resync): implement`), and
the failing tests below.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no. The tests exercise n1's `snapshot_and_subscribe` and open path
(behavior), so they need #760 green.
**Concurrent with:** `#term-snapshot 3/4` jail-size
**Blocks:** `#term-snapshot 4/4` room-wire (reuses the continuity guard)

    n1 → n2, n3, n4      n2 → n4

## State A (after #760)

A `SNAPSHOT` open gives a correct screen and contiguous deltas. After that, nothing can resync: a
lagged broadcast still `continue`s past dropped bytes, and the client cannot ask for a new snapshot
on a live stream.

## State B

The bidi output task `select!`s over {live broadcast, resync requests, ACK watch, pty_done}. A resync
(explicit or lag) emits a snapshot frame at a new sequence point. Bytes the old receiver had queued
but not yet forwarded are discarded, because the snapshot covers them. The client guard keeps the
painted screen consistent and asks for a resync on the first gap.

## Delta

### New
- `SessionTerminalInput.request_snapshot`; the bridge resync channel; `terminalFrameContinuity` (tddy-web)
### Modified
- `spawn_live_bridge` (`SNAPSHOT` streams): lagged → resync; bidi forwarder: `request_snapshot` → resync
- `openDaemonTerminalFeed`: continuity guard before painting
### Removed
- The silent lagged drop, in `SNAPSHOT` streams

## Testing Strategy

### Acceptance tests

**`tddy-terminal-rpc`**: `packages/tddy-terminal-rpc/tests/stream_resync_acceptance.rs`
- [ ] `a_mid_stream_request_yields_a_snapshot_that_renders_the_servers_screen_then_only_later_deltas`:
  a fake session over a real `TaskChannel` with a scripted TUI running. The client opens with
  `SNAPSHOT`, consumes, and sends `request_snapshot` on a message that also carries input bytes.
  Assertions: the input reached the PTY before the snapshot's sequence point; the next data frame is
  `snapshot = true` with `end_offset` ≥ the previous frame's `end_offset`; the reference emulator,
  reset by it, equals the server screen at `end_offset`; the following deltas are contiguous from it;
  the reference equals the server screen at the end.
- [ ] `a_lagged_subscriber_is_resynced_with_a_snapshot_not_a_hole`: a broadcast of capacity 1, a
  burst while the consumer is paused. The test asserts the next data frame is `snapshot = true`, the
  offsets are contiguous after it, and the reference ends equal to the server screen.
- [ ] `tail_and_from_offset_streams_keep_their_lagged_behaviour`: no snapshot frame appears.

**`tddy-web`**
- [ ] **Unit** `packages/tddy-web/src/lib/terminalFrameContinuity.test.ts`: accepts contiguous frames,
  rejects a gap and an overlap, and resets on a snapshot frame.
- [ ] **Component** `packages/tddy-web/cypress/component/TerminalResyncOnGap.cy.tsx`:
  `a gap is not painted and asks for a snapshot on the same stream`. The test pushes a snapshot of
  screen A, a contiguous delta, then a delta with a gap. It asserts the screen still shows the
  pre-gap state, the recorded bidi messages contain `requestSnapshot: true` on the **same** open (no
  re-open), and after a snapshot of screen B the screen shows B.

### Test level decisions
| Aspect | Level | Rationale |
|---|---|---|
| Mid-stream sequence point | Integration over a real `TaskChannel` | The ordering is the behavior |
| Lag recovery | Integration, forced lag | Deterministic with a capacity-1 broadcast |
| Client guard | Unit + one component spec | Pure logic, plus the user-visible effect |

## Decisions & Trade-offs
- Lag → snapshot (developer, 2026-10-25), and only in `SNAPSHOT` streams, so legacy clients keep
  byte semantics.

## TODO

- [x] Record initial discovery (`2026-10-25-term-snapshot-resync-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
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
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

## Refactoring Needed
### From /red (TDD red phase)
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |
