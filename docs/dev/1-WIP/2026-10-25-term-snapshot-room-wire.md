# Changeset: Terminal snapshot on the session-room wire

**Created:** 2026-10-25
**Status:** 🚧 In Progress
**Type:** Bug Fix
**Complexity**: 5 — three packages, a proto contract change (`terminal.proto`), ordering semantics (sequence point on the room wire), two servers of one RPC
**Stack:** `#term-snapshot 4/4` — branch `feature/term-snapshot/room-wire`, base `feature/term-snapshot/jail-size`
**PRD:** [PRD-2026-10-25-terminal-snapshot-room.md](../../ft/daemon/1-WIP/PRD-2026-10-25-terminal-snapshot-room.md)

## Initial Discovery

[2026-10-25-term-snapshot-room-wire-initial-discovery.md](./2026-10-25-term-snapshot-room-wire-initial-discovery.md)

## Affected Packages

- [ ] `tddy-service`: `terminal.proto` fields; `TerminalServiceVirtualTui` answers the contract
- [ ] `tddy-cli-sessions`: `PtyTerminalService` snapshot open, offsets, `request_snapshot`, no `trigger_redraw`
- [ ] `tddy-web`: the room feed requests a snapshot on every open and on a continuity gap

## Responsibility

- Proto `terminal.proto`: `TerminalInput.request_snapshot = 2`; `TerminalOutput.snapshot = 2`,
  `start_offset = 3`, `end_offset = 4`.
- `PtyTerminalService`: open = `snapshot_and_subscribe()` on the handle's channel, then a snapshot
  frame, then live frames stamped with contiguous offsets. `request_snapshot` on any message (applied
  after the message's `data`) → a fresh snapshot at a new sequence point. A lag → a snapshot. The
  `trigger_redraw()` call and the raw `replay()` are **removed**.
- `TerminalServiceVirtualTui`: frames stamped with per-connection offsets. The first frame and each
  `request_snapshot` produce a full repaint (RIS + full draw) flagged `snapshot = true`.
- Web `openRoomTerminalStream`: the first input message of every open carries
  `requestSnapshot: true`. Frames pass `#term-snapshot 2/4`'s continuity guard, and a gap sends
  `requestSnapshot` on the same stream. `TerminalFrame.{snapshot,startOffset}` is filled from the
  room frames.

## Boundaries

- History for room-carried sessions still comes from the host's `GetTerminalHistory` (unchanged).
- `tddy-tools pty-relay` is not changed. It reads `data` only, and the snapshot's leading RIS keeps
  it correct.
- No change to `TerminalSessionService`, the screen model or the guard's logic.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `#term-snapshot 1/4` host-snapshot (#760) | `ScreenSnapshot`, `TerminalCapture::snapshot`, broadcast-under-lock in `TaskChannel::write`, `TerminalFrame.{snapshot,startOffset}` | `PtyTerminalService` takes the snapshot and subscribes under the capture lock | change the capture, the serializer or `TerminalFrame` |
| `#term-snapshot 2/4` resync (#761) | `terminalFrameContinuity` guard (tddy-web) | the room feed runs every frame through it | change the guard |
| `#term-snapshot 3/4` jail-size (#762) | nothing; a line position only | — | — |

## Draft PR contract

The first push (pass 2) publishes the `terminal.proto` fields (regenerated Rust + TS) and the failing
tests below.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no. It needs #760 (snapshot behavior) and #761 (guard behavior) green.
**Concurrent with:** none
**Blocks:** none

    n1 → n2, n3, n4      n2 → n4

## State A

The room wire replays up to 64 KiB of raw output after a gap-prone snapshot-then-subscribe, sends
SIGWINCH, and carries no offsets or control messages.

## State B

Both room servers open with a snapshot at a sequence point, stamp contiguous offsets, and answer
`request_snapshot` at any time. The web room feed requests a snapshot on every open, and on a gap.

## Delta
### New
- `TerminalInput.request_snapshot`, `TerminalOutput.{snapshot,start_offset,end_offset}`
### Modified
- `PtyTerminalService::start_bidi_stream`; `TerminalServiceVirtualTui::stream_terminal_io`; `openRoomTerminalStream`
### Removed
- `PtyTerminalService`'s raw `replay()` on open and its `trigger_redraw()`

## Testing Strategy

### Acceptance tests
- [ ] **Integration** `packages/tddy-cli-sessions/tests/pty_terminal_service_snapshot.rs`
  - `a_room_open_renders_the_ptys_screen_then_only_later_deltas_without_a_redraw`: a PTY handle over a
    real `TaskChannel` with a scripted TUI. Assertions: frame 1 is `snapshot = true`; the reference
    emulator equals the server screen at `end_offset`; the deltas are contiguous; the redraw counter
    is 0.
  - `a_mid_stream_request_snapshot_resyncs_at_a_new_sequence_point`.
- [ ] **Integration** `packages/tddy-service/tests/virtual_tui_snapshot.rs`:
  `request_snapshot_repaints_the_whole_tui_flagged_as_snapshot`.
- [ ] **Unit** `packages/tddy-web/src/rpc/connections/livekit/roomTerminalFeed.test.ts`:
  `every open's first message asks for a snapshot` and `a gap asks for a snapshot on the same stream`.
- [ ] **Component** `packages/tddy-web/cypress/component/SessionMainPaneRoomTerminal.cy.tsx`: extend it
  with `a room terminal renders the snapshot on reconnect without a resize`.

### Test level decisions
| Aspect | Level | Rationale |
|---|---|---|
| Room server ordering | Integration over a real `TaskChannel` | Same guarantee as #760, other server |
| VirtualTui snapshot | Integration | Full-repaint contract |
| Web room feed | Unit + one component spec | Wire messages + user-visible render |

## Decisions & Trade-offs
- `TerminalServiceVirtualTui` joins the contract too. It renders fresh per connection, but the web
  room feed now sends `request_snapshot` and checks offsets, so a server that ignored both would be
  exactly the silent mismatch the guard exists to catch.

## TODO

- [x] Record initial discovery (`2026-10-25-term-snapshot-room-wire-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
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
