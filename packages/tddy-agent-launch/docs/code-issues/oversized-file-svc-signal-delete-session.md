# oversized-file: svc_signal_delete_session.rs

**Location:** `packages/tddy-agent-launch/src/svc_signal_delete_session.rs`
**Category:** oversized-file
**Detected:** 2026-11-20 — `/pr-wrap` step 3.5 file-length gate on #958 (`#session-liveness` 1/4), production lines counted to the first `#[cfg(test)]`
**Metrics:** **511 production lines** of 616 total · budget 500 · **+11**
**Thresholds breached:** length 511 > 500
**Restructure:** `extract_module --to_file` — the signal half (`signal_held_session`, `held_agent_pid`, `deliver_signal`, `LaunchSessions::signal_session_dir`, ~90 lines) beside the delete half (`SignalSession`, `DeleteSession`, tmux and pointer clean-up); seam not proven with `check --deep`
**Status:** Open — crossed 500 in #958 (446 → 511); split deferred under the stack-branch rule — `#session-liveness` 2/4, 3/4 and 4/4 also edit this file — **unclaimed**
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-20 | 446 → 511 | **Crossed 500** in #958 (`#session-liveness` 1/4): `live_sessions`, `signal_session_dir`, `held_agent_pid` and `signal_held_session` (the held main's pid replaces the recorded one), `deliver_signal` split out of the old `signal_session_dir`, and `DeleteSession` stopping the session's terminals. Not split: the file is stack-shared with n2–n4 |

## What the tool found

One `impl LaunchSessions` with the two RPC bodies and their clean-up, then a second one with the liveness read and the signal path, then free helpers.

## Why it matters here

The signal path changes with each node of `#session-liveness` (the `tool` carve-out leaves in 3/4), so the file keeps growing and shrinking under open PRs.

## What would close it

After the stack lands, move the signal half (the second `impl LaunchSessions` and the three free functions after it) into `svc_signal_session.rs`. It is a move-only `extract_module`; its callers name the functions by path (`tddy_agent_launch::svc_signal_delete_session::signal_held_session`), so keep a `pub use`.
