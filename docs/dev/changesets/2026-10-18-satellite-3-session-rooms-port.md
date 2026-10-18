# 2026-10-18 — Session rooms through an extension port

**Type:** Architecture

`#satellite` 3/13 ([#625](https://github.com/uppin/tddy-coder/pull/625)). Closing a session's room, or
asking whether this daemon hosts one, no longer requires the LiveKit plugin crate.

## What changed

- **`SessionRooms` port and `NoSessionRooms`** in `tddy-daemon-extensions`: `hosts`, `close`,
  `close_for_worktree`. [Extension ports](../../../packages/tddy-daemon-extensions/docs/extension-ports.md#the-session-rooms-port-session_rooms).
- **`SessionRoomRegistry` implements the port** in `tddy-daemon-livekit` (`session_rooms_port`);
  `hosts` is the registry's `contains`.
- **Consumers hold `dyn SessionRooms`.** `close_session_room` (`tddy-session-activity`), the Telegram
  control harness (`with_session_rooms`, default `NoSessionRooms`) and `tddy-worktree-service`, whose
  `WorktreeRoomCloser` / `NoSessionRooms` are now re-exports of the port. `tddy-daemon`'s
  `SessionRoomWorktreeCloser` adapter is deleted; the runtime hands the shared registry over as
  `Arc<dyn SessionRooms>`.
- **Ratchet.** `tddy-session-activity` and `tddy-telegram-control` leave `KNOWN_OFFENDERS`; both
  manifests drop `tddy-daemon-livekit`.

## Not changed

Room opening, activity publishing, and the `Arc<SessionRoomRegistry>` fields in `tddy-agent-launch`,
`tddy-session-agents`, `tddy-session-split` and `tddy-session-lifecycle` (a later node of the stack).

## Verification

Scoped to `tddy-daemon-extensions`, `tddy-daemon-livekit`, `tddy-session-activity`,
`tddy-telegram-control`, `tddy-worktree-service`: the port's null object, the real registry through the
port, the deletion helper over a recording fake, and both ratchet tests pass.

## Code issues

`oversized-file-service` (`tddy-worktree-service/src/service.rs`): 777 → 772 production lines. Improved,
not clean; the record stays.
