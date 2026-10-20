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

## Grown by #626 (`#satellite` 4/13, room tokens)

| File | Production lines | Grown by #626 | Record |
|---|---|---|---|
| `packages/tddy-daemon/src/runtime.rs` | 1,813 → 1,820 | +7 | `packages/tddy-daemon/docs/code-issues/oversized-file-runtime.md` |
| `packages/tddy-screen-sharing/src/screen_sharing_service.rs` | 967 → 983 | +16 | `packages/tddy-screen-sharing/docs/code-issues/oversized-file-screen-sharing-service.md` |

`packages/tddy-daemon-auth/src/auth.rs` (622 → 619) shrank and is recorded in
`packages/tddy-daemon-auth/docs/code-issues/oversized-file-auth.md`.

## Grown by #628 (`#satellite` 6/13, terminal exposure)

Deferred with the developer's consent (2026-10-19, "defer"): #629–#635 are open and edit the daemon's wiring,
so a split here would conflict them.

| File | Production lines | Grown by #628 | Record |
|---|---|---|---|
| `packages/tddy-daemon/src/runtime.rs` | 1,818 → 1,832 | +14 | `packages/tddy-daemon/docs/code-issues/oversized-file-runtime.md` |
| `packages/tddy-telegram-control/src/telegram_session_control/mod.rs` | 539 → 557 | +18 | `packages/tddy-telegram-control/docs/code-issues/oversized-file-telegram-session-control-mod.md` |
| `packages/tddy-telegram-control/src/telegram_session_control/session_start.rs` | 713 → 719 | +6 | `packages/tddy-telegram-control/docs/code-issues/oversized-file-telegram-session-control-session-start.md` |

The growth is the `terminal_exposure` wiring: the field and its builder in the harness, the port call
replacing `spawn_livekit_bridge` in the Telegram start, and the exposure built once in `build` and handed to
the session host and `build_telegram`.

Also open from this node, not a size issue: the Telegram start logs `warn!` ("terminal exposure failed")
when no exposure is wired, which the port defines as "not exposed" (`FailedPrecondition`). Distinguishing it
needs `tddy-telegram-control` to depend on `tddy-rpc` (for `Code`) — an external-dependency decision left to
the developer.

## Grown by #634 (`#satellite` 12/13, split-placement transport)

Deferred under the stack run's pre-approved consent (2026-10-20). `connection_service.rs` is touched
by #633 and #635 as well, so a split there would conflict them. `runtime.rs` is not touched by #635;
it is deferred because decomposing the composition root (1,858 production lines, `build` alone 1,071)
is a restructure-engine plan of its own, which this node's +26 does not change the shape of.

| File | Production lines | Grown by #634 | Record |
|---|---|---|---|
| `packages/tddy-daemon/src/runtime.rs` | 1,832 → 1,858 | +26 | `packages/tddy-daemon/docs/code-issues/oversized-file-runtime.md` |
| `packages/tddy-session-lifecycle/src/connection_service.rs` | +5 by `git diff --numstat` | +5 | `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-connection-service.md` |

The growth is the split-rooms port: `split_room_hosting` in the runtime (+19), its call and
`.with_split_rooms(..)` in `build` (+7), and the host's `split_rooms` field.
`packages/tddy-daemon-livekit/src/session_room.rs` shrank (2,885 → 2,857).
