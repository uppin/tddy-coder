# WIP Changeset: Terminal Mode Replay (mouse tracking survives capture eviction)

**Feature slug:** `terminal-mode-replay`
**Status:** Green phase complete — production logic implemented; all tests passing

## Problem / Motivation

Only the newest terminal session in `tddy-web` responds to the mouse. Older sessions drop every
click, drag and scroll while keyboard input keeps working.

`GhosttyTerminal` forwards mouse events only while its own VT instance has seen a mouse-tracking
DECSET (`hasMouseTracking()`), and that state is per-VT-handle — it is set solely by observing
`ESC[?1000h` / `ESC[?1002h` / `ESC[?1006h` in that instance's own byte stream. Two independent
gaps kept those bytes from ever reaching a late-attaching browser:

1. **The capture ring evicts them.** The daemon retains the last 64 KB of PTY output and trims
   from the front. An agent TUI enables mouse tracking once at startup; after ~64 KB of output
   those bytes are gone, and nothing re-emits them. `trigger_redraw()` is SIGWINCH only, and
   SIGWINCH does not re-issue private modes.
2. **The browser gets no replay at all.** `connection_service.rs` gates the replay on
   `!has_initial_dims`, and a browser always measures its grid before opening the stream — so on
   the exact path the web terminal uses, the capture was never sent in the first place.

## Solution

Make the capture **mode-aware** instead of a plain byte ring. `TerminalCapture` sniffs
DECSET/DECRST as bytes stream through, keeps the mouse-tracking modes currently in effect as
sticky state, and can re-issue them as a **prologue** in front of the retained output. The
prologue costs no ring space, so it survives eviction indefinitely.

All three attach paths send it:

- ConnectRPC `StreamTerminalOutput` (the web terminal) — sent as its **own first frame**,
  independent of the replay decision, so the dimensions-supplied branch is covered too.
- LiveKit bidi `PtyLiveKitService` — `cap.replay()` (prologue ++ retained output).
- `tddy-coder`'s session participant — same.

Trimming now stops at an escape-sequence boundary, so a cut can no longer leave an orphan
fragment (e.g. a bare `1m`) to be rendered as literal text at the head of the replay.

Non-mouse sticky modes (cursor visibility 25, alternate screen 1049, bracketed paste) are
deliberately **not** replayed: the application redraws them, and re-issuing them out of context
would fight the application's own state.

## TODO

- [x] Failing tests written (`/red`)
  - `packages/tddy-task/tests/terminal_capture_replay.rs` (10 tests)
  - `packages/tddy-daemon/tests/terminal_mode_replay_acceptance.rs` (1 acceptance test)
- [x] Implement production logic (`/green`)
- [x] `/validate-changes` — 0 critical, 2 warnings, 4 info (see Validation Results)
- [x] `/validate-tests`
- [x] `/validate-prod-ready`
- [x] `/analyze-clean-code`
- [ ] Wrap changeset

## Files Changed

### tddy-task
- `packages/tddy-task/src/terminal_capture.rs` *(new)* — `TerminalCapture` (bounded ring +
  sticky mouse-mode state) and an incremental `EscapeParser` fed one byte at a time so a DECSET
  split across two PTY reads is still recognised. Public API: `append`, `replay`,
  `mode_prologue`, `buffered_bytes`, `CAPTURE_LIMIT_BYTES`.
- `packages/tddy-task/src/task.rs` — `TaskChannel.capture` is now `Arc<Mutex<TerminalCapture>>`;
  `write()` delegates to `TerminalCapture::append` (the private `CHANNEL_CAPTURE_LIMIT_BYTES`
  and the inline trim are gone). `replay_capture()` keeps its original meaning — the process's
  own bytes, no prologue — so the non-terminal consumers of task output are unaffected.
- `packages/tddy-task/src/lib.rs` — `pub mod terminal_capture;` + re-export.

### tddy-daemon
- `packages/tddy-daemon/src/cli_session_manager.rs` — `PtyHandle.capture` type change; the
  LiveKit bidi attach replays `cap.replay()`.
- `packages/tddy-daemon/src/connection_service.rs` — `stream_terminal_output` sends
  `cap.mode_prologue()` as the first frame before (and independently of) the replay branch; the
  legacy no-dimensions replay chunks `cap.buffered_bytes()`.

### tddy-coder
- `packages/tddy-coder/src/session_participant/{mod.rs,terminal_manager.rs}` — `PtyHandle.capture`
  type change; attach replays `cap.replay()`.

### Tests
- `packages/tddy-task/tests/terminal_capture_replay.rs` *(new)* — sticky-mode behaviour, prologue
  ordering, mode-off, non-mouse modes, ring bound, escape-boundary trimming, plus the two
  `TaskChannel`-level replays.
- `packages/tddy-daemon/tests/terminal_mode_replay_acceptance.rs` *(new)* — end-to-end over
  `StreamTerminalOutput`: a stub TUI enables mouse tracking, floods the ring until the DECSETs
  are provably gone from `buffered_bytes()`, then a browser-shaped client (with dimensions)
  attaches and must receive the prologue as its first frame.
- Six existing daemon acceptance tests repointed from `&cap` to `cap.buffered_bytes()`
  (`claude_cli_permission_mode_acceptance.rs`, `claude_cli_session_acceptance.rs`,
  `managed_codebase_workflow_acceptance.rs`, `telegram_start_claude_acceptance.rs`,
  `telegram_start_cursor_acceptance.rs`, `terminal_session_acceptance.rs`) — byte-identical
  behaviour.

## Validation Results

### `/validate-changes`

- **0 critical.**
- **[WARNING] `replay_capture()` semantics.** Prefixing the prologue there would have rippled
  into four non-terminal consumers (`tddy-actions`, `tddy-build`, `tddy-core` session actions,
  `tddy-daemon` `WatchTask`, `tddy-tool-engine`) that treat the value as command output.
  **Resolved:** `replay_capture()` stays byte-exact; terminal callers take
  `capture_arc().lock().replay()`. Pinned by
  `a_task_channel_replay_carries_only_what_the_process_wrote`.
- **[WARNING] `sandbox_session.rs` is not covered.** It keeps its own raw, **unbounded**
  `Arc<Mutex<Vec<u8>>>` capture (`cap.extend_from_slice` with no trim, lines ~25/40/334/353/852)
  and therefore gets no prologue. Pre-existing and out of scope for this change — **follow-up.**
- **[INFO]** Eviction chases to the end of a cut sequence; a cut inside an unterminated OSC/DCS
  can transiently empty the ring until the terminator arrives.
- **[INFO]** No RIS (`ESC c`) handling — sticky mode state survives a hard reset.
- **[INFO]** On the legacy no-dimensions path a client may see the DECSETs twice (prologue plus
  a still-retained copy in the buffer). DECSET is idempotent.
- Verified independently that every capture write flows through `TaskChannel::write`, so the
  sniffer cannot be bypassed.

### `/validate-tests`

- **[WARNING → fixed] Acceptance-test premise was never asserted.** The test waited on a marker
  file written by the stub, which raced the daemon's PTY reader; it would have passed even with
  the DECSETs still in the ring. Replaced with
  `wait_until_the_ring_has_trimmed_past_the_modes`, which polls `cap.buffered_bytes()` under a
  bounded deadline and fails loudly if the premise never holds.
- **[WARNING → fixed] Magic sample width.** `assert_begins_with(&[FILLER_BYTE; 8])` replaced by
  `assert_is_entirely(FILLER_BYTE)`, which reports the offending byte and offset.
- **[INFO]** `readable()` and the escape constants are duplicated across the two test binaries;
  this follows existing repo convention (`wait_for_capture_contains` is duplicated in six daemon
  test files) and separate Rust test binaries cannot share a module without a support crate.
