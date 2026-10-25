# Changeset: Terminal snapshot on (re)connect over the bidi terminal stream

**Created:** 2026-10-25
**Status:** 🚧 In Progress
**Type:** Bug Fix
**Complexity**: 5 — six packages, a proto contract change, ordering/concurrency semantics (sequence point), new dependency (`avt`), web transport migration to bidi
**Stack:** `#term-snapshot 1/4` — branch `feature/term-snapshot/host-snapshot`, base `master`
**PRD:** [PRD-2026-10-25-terminal-snapshot-host.md](../../ft/daemon/1-WIP/PRD-2026-10-25-terminal-snapshot-host.md)

## Initial Discovery

[2026-10-25-term-snapshot-host-snapshot-initial-discovery.md](./2026-10-25-term-snapshot-host-snapshot-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-10-24-reattach-does-not-replay-the-alternate-screen.md](../todo/2026-10-24-reattach-does-not-replay-the-alternate-screen.md) | ✅ RESOLVED HERE: the snapshot carries the alternate screen, and `mode_prologue` replays it for the legacy modes. Delete at wrap |
| [2026-09-11-the-coder-terminal-coordinate-serves-seven-of-nine.md](../todo/2026-09-11-the-coder-terminal-coordinate-serves-seven-of-nine.md) | ⚠ during: the coder starts serving `StreamSessionTerminalIO`. Narrow the entry to `WatchTerminalControl` at wrap |
| `packages/tddy-terminal-rpc/docs/code-issues/complexity-bridge-open-replay-ack-live.md` | ⚠ during: the `SNAPSHOT` branch goes in its own function rather than inline. Splitting the existing two branches may be deferred (developer consent: code issues may be deferred in this stack) |
| `packages/tddy-task/docs/code-issues/complexity-terminal-capture-feed.md` | ⚠ during: widening the sniffer must not deepen `feed`'s nesting |
| `packages/tddy-web/docs/code-issues/oversized-file-ghostty-terminal-session.md`, [2026-10-24-ghostty-terminal-components-over-budget.md](../todo/2026-10-24-ghostty-terminal-components-over-budget.md) | ⚠ during: the snapshot begins with RIS, so `GhosttyTerminalSession.tsx` needs ~0 new lines. The "no terminal transport" message lives in the feed/pane wrapper, not the component |

## Affected Packages

- [ ] `tddy-task`: screen model, `ScreenSnapshot`, sequence point, input-mode sniffer ([terminal-capture.md](../../../packages/tddy-task/docs/terminal-capture.md))
- [ ] `tddy-terminal-rpc`: proto, `snapshot_and_subscribe` port, bridge `SNAPSHOT` branch ([terminal-session-service.md](../../../packages/tddy-terminal-rpc/docs/terminal-session-service.md))
- [ ] `tddy-cli-sessions`: `PtyHandle::resize` keeps the model's size
- [ ] `tddy-pty`: registry `resize` keeps the model's size
- [ ] `tddy-coder`: the session participant serves `StreamSessionTerminalIO`; its terminal adapter implements `snapshot_and_subscribe`
- [ ] `tddy-session-lifecycle`: the CLI and sandbox adapters implement the port (sandbox: `FAILED_PRECONDITION`)
- [ ] `tddy-daemon-sandbox`: broadcast under the capture lock
- [ ] `tddy-web`: host terminal feed moves to bidi `StreamSessionTerminalIO` with `SNAPSHOT` on every open ([terminal-session.md](../../../packages/tddy-web/docs/terminal-session.md))

## Responsibility

- `tddy_task::ScreenSnapshot { data, offset, rows, cols }`, `TerminalCapture::{snapshot, resize}`,
  the screen model fed by `TerminalCapture::append` (`avt 0.18`, incremental UTF-8), and the widened
  input-mode sniffer (mouse 1000/1002/1003/1005/1006/1015/1016, bracketed paste 2004, focus 1004,
  DECKPAM/DECKPNM, DECSCUSR; alternate screen in `mode_prologue`).
- **The sequence point:** `TaskChannel::write` and the sandbox stdio loop broadcast while holding
  the capture lock.
- `TerminalSession::snapshot_and_subscribe() -> Result<(ScreenSnapshot, broadcast::Receiver<Bytes>), Status>`:
  a provided default implementation over `capture()` + `subscribe_stdout()` under the lock.
  Adapters that cannot size a model override it to refuse.
- Proto: `StreamReplayMode::STREAM_REPLAY_MODE_SNAPSHOT = 2`; `SessionTerminalOutput.snapshot = 8`,
  `snapshot_rows = 9`, `snapshot_cols = 10`; in a `SNAPSHOT` stream **every** data frame carries
  `start_offset`/`end_offset`.
- Bridge: the `SNAPSHOT` branch for both `StreamSessionTerminalIO` and `StreamTerminalOutput`: no
  prologue frame, no raw replay, no `trigger_redraw`; resize only when the open holds the lease and
  the grid differs, applied to the PTY **and** the model before the snapshot.
- The coder participant serves `StreamSessionTerminalIO` through the shared bridge.
- Web: `openDaemonTerminalFeed` → bidi `streamSessionTerminalIO` (open message: identity, control
  token, grid, `mode = SNAPSHOT`; then stdin chunks with cumulative `input_offset`); `TerminalFrame`
  gains `snapshot` and `startOffset`; `TerminalStreamOffset` takes absolute offsets from every
  frame; an HTTP-only host connection yields a feed that reports "no terminal transport" and opens
  nothing.

## Boundaries

- **No mid-stream resync.** `request_snapshot`, lagged-subscriber resync and the client continuity
  guard are `#term-snapshot 2/4` (`feature/term-snapshot/resync`). Lagged behavior stays as it is here.
- **No room wire.** `terminal.proto` and `PtyTerminalService` are `#term-snapshot 4/4`.
- **No jail sizing.** Jailed terminals refuse `SNAPSHOT` with `FAILED_PRECONDITION`, and
  `#term-snapshot 3/4` lifts that.
- `TAIL`, `FROM_OFFSET`, `StreamTerminalOutput` and `SendTerminalInput` keep their behavior for non-web
  clients. Their `trigger_redraw` is untouched.
- No grid reconciliation for watchers whose grid differs from the PTY's.

## Dependencies

None. Root node, based on `master`.

## Draft PR contract

The first push of this PR (pass 2) publishes:

- `tddy-task`: `ScreenSnapshot`, `TerminalCapture::snapshot` / `resize` with `todo!()`-free stub
  bodies marked `// TODO(host-snapshot): implement` (returning an empty snapshot so dependents
  compile), and the `avt` dependency declared.
- `tddy-terminal-rpc`: proto fields and enum value (generated code), `TerminalSession::snapshot_and_subscribe`
  with its provided default.
- `tddy-web`: `TerminalFrame.{snapshot, startOffset}`, regenerated `terminal_session_pb.ts`.
- The failing acceptance tests below.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes. Root node, every test uses this node's own surface.
**Concurrent with:** none (only node in wave 1)
**Blocks:** `#term-snapshot 2/4` (resync re-takes `snapshot_and_subscribe` mid-stream),
`#term-snapshot 3/4` (jail sizing calls `TerminalCapture::resize`), `#term-snapshot 4/4` (room wire
serializes `TerminalCapture::snapshot`)

    n1 → n2, n3, n4      n2 → n4

## State A

See [initial discovery](./2026-10-25-term-snapshot-host-snapshot-initial-discovery.md) § Combined
conclusions 1–4, 6, 9. In short: raw-byte replays, SIGWINCH as the repaint trigger, no sequence
point, no server screen model, and the web terminal on server-stream + unary input.

## State B

1. **Screen model.** `TerminalCapture` holds an `avt::Vt` sized to the PTY. `append` decodes
   UTF-8 incrementally (a split code point is held until it completes) and feeds the `Vt`. `resize`
   resizes it. `snapshot()` returns RIS + `Vt::dump()` + the input modes from the sniffer, with
   `offset = end_offset`.
2. **Sequence point.** `TaskChannel::write`: `let mut cap = lock; cap.append(&data); let _ = output_tx.send(data); drop(cap)`.
   The sandbox stdio loop does the same. `snapshot_and_subscribe` locks, snapshots, subscribes, and
   unlocks.
3. **Wire.** A `SNAPSHOT` open sends one snapshot frame (`snapshot = true`, `start_offset = 0`,
   `end_offset = snapshot.offset`, rows/cols), then the initial ACK, then live frames each stamped with
   `[start, end)` computed by accumulating from `snapshot.offset`.
4. **Coder.** `build_terminal_session_entry` registers `StreamSessionTerminalIO` for the coder too.
   The participant's store and adapter supply the port.
5. **Web.** `openDaemonTerminalFeed` opens one bidi call per feed. Input is queued into the
   request iterator, using the same 20 ms drain batching as the room feed. ACK frames are handled as
   today. Every open is `SNAPSHOT`. `TerminalResumePoint` no longer chooses a mode for the web.
   `GhosttyTerminalSession` writes the snapshot into the live terminal as-is, and the snapshot's
   `endOffset` anchors history. A host connection whose transport is Connect-HTTP produces a feed
   whose `ended` settles at once with a "no terminal transport" reason that the pane renders.

## Delta

### New
- `ScreenSnapshot`, `TerminalCapture::{snapshot, resize}`, incremental UTF-8 feed (`tddy-task`)
- `TerminalSession::snapshot_and_subscribe` (`tddy-terminal-rpc/src/session.rs`)
- `StreamReplayMode::Snapshot`, `SessionTerminalOutput.{snapshot, snapshot_rows, snapshot_cols}`
- `TerminalFrame.{snapshot, startOffset}`; the "no terminal transport" feed state (tddy-web)
- Dependency: `avt = "0.18"` in `tddy-task`

### Modified
- `TaskChannel::write`, the sandbox stdio loop (broadcast under lock)
- `PtyHandle::resize`, `tddy_pty` registry `resize`, coder `terminal_manager::resize`: also `capture.resize`
- `bridge.rs`: the `SNAPSHOT` branch in its own function; offsets on live frames in `SNAPSHOT` streams
- `tddy-coder` session participant: serves `StreamSessionTerminalIO`
- `openDaemonTerminalFeed`, `TerminalStreamOffset`, the host-connection `openTerminal` implementations

### Removed
- Nothing.

## Milestones

- [ ] M1: screen model + sequence point (`tddy-task`)
- [ ] M2: `SNAPSHOT` on the bridge (both RPCs), coder bidi, adapters
- [ ] M3: web host feed on bidi with `SNAPSHOT` on every open; HTTP-only state

## Testing Strategy

### Acceptance tests

**`tddy-task`**
- [ ] **Integration** `packages/tddy-task/tests/terminal_capture_snapshot.rs`
  - `a_snapshot_renders_identically_to_the_screen_it_was_taken_from`: a scripted alternate-screen TUI
    (DECSTBM region, saved cursor, SGR truecolor, hidden cursor, bracketed paste on, mouse 1002+1006),
    more than 64 KiB of diffs. `snapshot().data` is written into a fresh **reference** `vt100` parser
    (dev-dependency, not the crate under test), and a second reference is fed the original stream.
    The test asserts the cells (contents + attributes), cursor position and visibility, the
    alternate-screen flag, application cursor, bracketed paste and the mouse modes are equal.
  - `the_snapshot_keeps_the_scroll_region_and_saved_cursor`: both references get the same further
    bytes (a scroll inside the region, DECRC), and the test asserts they are still equal.
  - `a_chunk_boundary_inside_an_escape_or_a_code_point_does_not_corrupt_the_snapshot`: the stream is
    split at every byte of a CSI and of a 4-byte UTF-8 character.
  - `a_resize_reserializes_the_snapshot_at_the_new_grid`: asserts `rows`/`cols` and the reference
    render after the resize.
- [ ] **Integration** `packages/tddy-task/tests/terminal_capture_sequence_point.rs`
  - `snapshot_and_subscribe_neither_loses_nor_duplicates_a_byte_under_concurrent_output`: a producer
    thread writes 10 000 numbered chunks through `TaskChannel::write`, and a reader takes the
    snapshot and subscription mid-run. The assertion: the received bytes are exactly the producer's
    stream from `snapshot.offset` on. 200 iterations.

**`tddy-terminal-rpc`**
- [ ] **Integration** `packages/tddy-terminal-rpc/tests/stream_snapshot_acceptance.rs`
  - `a_snapshot_open_renders_the_servers_screen_then_only_later_deltas`: a fake `TerminalSession` over
    a real `TaskChannel`. A bidi `StreamSessionTerminalIO` opens with `SNAPSHOT` mid-TUI, and output
    is written into a reference emulator. Assertions: the first data frame is `snapshot = true` with
    `end_offset == capture.end_offset()` at open; the reference equals the server screen; the first
    delta's `start_offset == snapshot.end_offset`; each delta's `start_offset` equals the previous
    `end_offset`; the concatenated deltas equal `capture.replay_from(snapshot.end_offset)` byte for
    byte; at the end the reference equals the server screen.
  - `a_snapshot_open_sends_no_redraw_and_no_prologue_frame`: the fake counts `trigger_redraw`, and the
    test asserts 0 calls and that frame 1 is the snapshot.
  - `stream_terminal_output_honours_snapshot_mode_too`.
  - `a_jailed_terminal_refuses_snapshot_with_failed_precondition`: an adapter overriding the port.
- [ ] **Integration** `packages/tddy-coder/tests/two_server_parity_acceptance.rs`:
  `both_servers_answer_a_bidi_snapshot_open_identically`.

**`tddy-web`** (Cypress component; only these specs run locally, unsandboxed)
- [ ] **Component** `packages/tddy-web/cypress/component/TerminalSnapshotOnReconnect.cy.tsx`
  - `every open asks for a snapshot and the screen renders without a resize`: an in-memory
    `TerminalSessionService` client implements bidi `streamSessionTerminalIO` and records each
    open's messages. Screen A is drawn, then a blip (null client) remounts the terminal. The test
    asserts the re-open's first message has `mode === SNAPSHOT`, pushes a snapshot of screen B, and
    asserts the live terminal's visible rows equal B with the grid unchanged and no grid change sent.
    A contiguous delta then updates the screen.
  - `typing travels on the same bidi stream with cumulative input offsets`.
  - `a host reached only over HTTP shows no terminal transport and opens no stream`.
- [ ] Rewrite `GrpcSessionTerminalResume.cy.tsx` (it pins `TAIL`→`FROM_OFFSET`) and the specs and unit
  tests driving `streamTerminalOutput`/`sendTerminalInput` fakes for the web feed
  (`TerminalInputAckAcceptance.cy.tsx`, `terminalFeed.test.ts`, …). This is a requirements change,
  not a weakened test.

### Test level decisions
| Aspect | Level | Rationale |
|---|---|---|
| Snapshot fidelity | Integration (crate tests) | Deterministic; an independent reference emulator is the oracle |
| Sequence point | Integration, concurrent | The defect is a race; only concurrency exposes it |
| Wire ordering | Integration over the real `TaskChannel` | Real capture and broadcast, no network |
| Web reconnect | Cypress component, in-memory client | The user-visible symptom; existing harness |

## Decisions & Trade-offs

- `avt` over `vt100`/`alacritty_terminal` (see discovery § 5). The developer approved it 2026-10-25.
- The byte offset is the sequence number. The snapshot begins with RIS, so a byte-only client
  renders it unaided.
- Complexity 5: the boundary was revisited at decomposition. Mid-stream resync was split out to n2
  by capability, and the rest is one vertical slice.
- Terminals run over RPC over LiveKit / RPC over Tauri IPC only (developer, 2026-10-25). HTTP-only
  shows a message and does not fall back.

## Technical Debt
- Measure emulator CPU on a recorded Claude session and record bytes/s here.

## TODO

- [x] Record initial discovery (`2026-10-25-term-snapshot-host-snapshot-initial-discovery.md`)
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
