# `#satellite` 1/13 left six files over the 500-production-line budget

**Date:** 2026-10-17
**Source:** `/pr-wrap` file-length gate on #623 (`#satellite` 1/13). Deferred with the developer's consent (2026-10-17).

Every file was already over budget on `master`; #623 grew each one and deferred the split. Two of them are
also edited by #624 (`#satellite` 2/13), so a split now would conflict it.

| File | Production lines | Grown by #623 | Record |
|---|---|---|---|
| `packages/tddy-daemon-livekit/src/session_room.rs` | 2,874 → 2,935 | +61 | `packages/tddy-daemon-livekit/docs/code-issues/oversized-file-session-room.md` |
| `packages/tddy-daemon/src/runtime.rs` | 1,781 → 1,811 | +30 | `packages/tddy-daemon/docs/code-issues/oversized-file-runtime.md` |
| `packages/tddy-daemon-livekit/src/livekit_peer_discovery.rs` | 1,658 → 1,697 | +39 | `packages/tddy-daemon-livekit/docs/code-issues/oversized-file-livekit-peer-discovery.md` |
| `packages/tddy-service/build.rs` | 723 → 737 | +14 | `packages/tddy-service/docs/code-issues/oversized-file-build.md` |
| `packages/tddy-session-sync/src/attach.rs` | 520 → 536 | +16 | `packages/tddy-session-sync/docs/code-issues/oversized-file-attach.md` |
| `packages/tddy-terminal-rpc/src/pty_relay.rs` | 774 → 776 | +2 | `packages/tddy-terminal-rpc/docs/code-issues/oversized-file-pty-relay.md` (new) |

**What would close it:** the splits named in each record, applied with `code-restructuring` after the
`#satellite` stack lands. The first targets are `session_room.rs` (largest, no dependent touches it) and
the two files #624 also edits, once #624 has merged.

## Grown by #624 (`#satellite` 2/13)

Deferred with the developer's consent (2026-10-17, "file sizes can be deferred"), on the same reasoning: later
`#satellite` nodes also edit both files.

| File | Production lines | Grown by #624 | Record |
|---|---|---|---|
| `packages/tddy-daemon-livekit/src/livekit_peer_discovery.rs` | 1,697 → 1,700 | +3 | `packages/tddy-daemon-livekit/docs/code-issues/oversized-file-livekit-peer-discovery.md` |
| `packages/tddy-daemon/src/runtime.rs` | 1,811 → 1,816 | +5 | `packages/tddy-daemon/docs/code-issues/oversized-file-runtime.md` |

`packages/tddy-host-service/src/service.rs` (937 → 936) shrank and is recorded in
`packages/tddy-host-service/docs/code-issues/oversized-file-service.md`.
