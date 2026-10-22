# `#satellite` left three files over the 500-production-line budget, each of them net-shrunk by the stack

**Date:** 2026-10-17 (narrowed 2026-10-22 by `#satellite` 16/16, the sweep)
**Source:** `/pr-wrap` file-length gate on #623 (`#satellite` 1/13). Deferred with the developer's consent (2026-10-17).

The sweep split every file the stack grew past the budget (`runtime.rs`, `build.rs`, `attach.rs`,
`pty_relay.rs`, the Telegram `mod.rs` and `session_start.rs`, and the files later nodes added to this entry)
and deleted their records. Three files the stack **shrank** keep their records and are not swept: they were
over budget on `master` before the stack and are smaller now.

| File | Production lines | Stack net | Record |
|---|---|---|---|
| `packages/tddy-daemon-livekit/src/session_room.rs` | 2,874 → 2,652 | −222 | `packages/tddy-daemon-livekit/docs/code-issues/oversized-file-session-room.md` |
| `packages/tddy-daemon-livekit/src/livekit_peer_discovery.rs` | 1,658 → 1,390 | −268 | `packages/tddy-daemon-livekit/docs/code-issues/oversized-file-livekit-peer-discovery.md` |
| `packages/tddy-session-lifecycle/src/connection_service.rs` | 527 → 516 | −15 | `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-connection-service.md` |

`packages/tddy-host-service/src/service.rs` was split by #693.
`packages/tddy-screen-sharing/src/screen_sharing_service.rs` (the +16 from #626) was split by #653.

**What would close it:** the splits named in each record, applied with `code-restructuring`.

The Telegram `warn!` paragraph this entry carried (an unwired terminal exposure logged as a failure) is
fixed: `tddy-rpc` moved to a dependency of `tddy-telegram-control` and an unwired exposure is no longer a
warning.
