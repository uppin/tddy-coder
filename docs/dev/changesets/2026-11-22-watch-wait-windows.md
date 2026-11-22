# 2026-11-22 — Watch events: an opt-in idle wake and windowed waits

**Type:** Feature change · PR [#1017](https://github.com/uppin/tddy-coder/pull/1017)
**Packages:** `tddy-session-activity`, `tddy-tool-engine`, `tddy-daemon-rpc`, `tddy-tools`,
`tddy-agent-launch`
**Feature:** [docs/ft/daemon/project-concept.md § Watching sessions](../../ft/daemon/project-concept.md#watching-sessions)

## Problem

An idle orchestrator with matched watch events was sent a `[tddy watch] N watched events …` turn by
the daemon (`#activity-wake` 3/3). In practice the turns flooded the operator's conversation and
interleaved with what the operator was typing: their messages were cut mid-sentence by an injected
turn. `WaitSessionWatchEvents` — the alternative — held a call for at most 20 s and returned
whatever was waiting, all at once, with no way to narrow it.

## Change (operator decisions, 2026-11-22)

### The idle wake is opt-in per watch

- `CreateSessionWatch` takes `wake_when_idle` (boolean, default **false**); `RenewSessionWatch`
  takes it too — `true`/`false` turn the wake on/off, absent keeps the watch's choice. Every watch
  answer carries `wake_when_idle`.
- `SessionWatch.wake_when_idle` (serde default `false`): a watch persisted before this change no
  longer wakes.
- The waker concerns itself only with orchestrators holding a live **waking** watch
  (`SessionWatchStore::has_live_waking_watches`), and pastes only waking watches' events and endings
  (`SessionWatchStore::deliver_wake_with`, formerly `deliver_with`). A quiet watch's events wait for
  `WaitSessionWatchEvents`, a monitor, or the next tool answer's `watchEvents`.
- The "agent declares no `turn_input`" refusal guards only a waking watch now: a quiet watch
  promises no wake, so a Codex or ACP orchestrator may hold one.
- `kinds` filtering of a watch is unchanged.

### `WaitSessionWatchEvents` waits in a window

| Argument | Default | Bounds | Meaning |
|---|---|---|---|
| `timeout_ms` | 60 000 | ≤ 1 800 000 | the window: how long the call is held |
| `max_events` | 1 | 1–50 | return once this many are held; carry at most this many |
| `settle_ms` | 0 | ≤ 5 000 | after the first event, keep collecting this long (0: no shortcut) |
| `kinds` | every kind | watch kinds | return only these kinds |
| `session_ids` | every session | — | return only these sessions' events |

- The call returns at the first of: `max_events` events held; `settle_ms` after the first event
  (when `settle_ms` > 0); a `lost` count or an ended watch to report; the window elapsing.
- **Maximum hold, 30 min.** The 20 s cap (`MAX_WAIT_MS`, still `WaitProjectNotifications`') no longer
  applies. Claude Code moves a tool call past ~120 s to the background as a task whose result
  arrives later; the operator accepts that, so the maximum is generous (30 min, as long as
  `Monitor` runs one command) rather than kept under 120 s. Over LiveKit the call is still clamped
  to `MAX_REMOTE_BLOCK_MS` (20 s) by the remote hang guard, unchanged.
- **Exact acknowledgement.** A watch keeps, besides its position, `acked_above`: the seqs past the
  position already handed and acknowledged. `ack_seq` acknowledges what the stream's **latest
  answer** handed, up to that seq; the position advances to just before the first record the watch
  still owes, so a record a pick left out stays pending — for a later call, another stream, or a
  tool answer once the stream's lease lapses — and a handed, acknowledged one is never handed again,
  across restarts. A held call acknowledges once, on its first take.
- `tddy-tools monitor` holds each call 60 s and asks for 50 events (every waiting event, as it
  prints them all).

### Guidance

The orchestrator's launch prompt and the tool descriptions say to wait with
`WaitSessionWatchEvents` (or `tddy-tools monitor` under `Monitor`) rather than rely on injected
turns, and to opt into the idle wake only when it is really needed.

## Restructuring

`/pr-wrap`'s file-length gate found `tddy-session-activity/src/session_watches/stream.rs` grown past
the 500-production-line budget (428 → 553). One engine-driven `move_item` (`tddy-tools restructure`,
`check --deep` clean, compile-gated apply) moved `StreamPick` and `StreamRefusal` with their trait
impls into a new `session_watches::stream_call` module (`reexport: none`; the crate's
`session_watches::{StreamPick, StreamRefusal}` re-exports stay): `stream.rs` 553 → 491,
`stream_call.rs` 70. The private `impl StreamPick { fn picks }` and `fn acknowledge` stay in
`stream.rs`, so nothing was widened.

## Tests

- `tddy-session-activity`: `session_watch_wake_and_pick_unit` (opt-in wake, renewal, max events,
  kind/session picks, persistence of acknowledged picks).
- `tddy-tool-engine`: window and pick arguments, defaults (1 min, 1 event), bounds, the wake
  option on create/renew, schema.
- `tddy-daemon-rpc`: `project_home_watch_wait_windows_acceptance` (no wake without opt-in, wake
  with it, renewal turning it on, first event at once, `max_events`, window elapse, settle,
  kind/session picks without loss); existing wake/stream/message suites opt into the wake.
- `tddy-tools-cli`: the monitor's window.
- `tddy-agent-launch`: the prompt's guidance.
