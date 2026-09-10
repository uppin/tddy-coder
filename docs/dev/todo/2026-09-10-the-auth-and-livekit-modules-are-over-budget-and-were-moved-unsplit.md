# 2026-09-10 — The modules extracted into `tddy-daemon-auth` and `tddy-daemon-livekit` are over the file budget and were moved unsplit

**Category:** Deferred refactor
**Source:** `#unbundle` node 4, [#473](https://github.com/uppin/tddy-coder/pull/473), milestone M8

Five files stand over the repo's 500-line budget, and **every one is relocated code**:

| File | Lines |
|---|---:|
| `packages/tddy-daemon-livekit/src/session_room.rs` | 2,992 |
| `packages/tddy-daemon-livekit/src/livekit_peer_discovery.rs` | 2,060 |
| `packages/tddy-daemon-auth/src/auth.rs` | 1,200 |
| `packages/tddy-daemon-livekit/src/common_room_supervisor.rs` | 881 |
| `packages/tddy-daemon-livekit/src/livekit_rooms_stream.rs` | 779 |

All five were already over budget inside `tddy-daemon`; the move neither caused nor worsened it.
Everything the node **authored** is under budget: `livekit_service.rs` 127,
`tddy-daemon-livekit/src/lib.rs` 137, `tddy-daemon-auth/src/lib.rs` 153.

**Why they were not split here**, on two independent grounds:

- `session_room.rs` is reached by the session subsystem that `#unbundle` nodes 6–8 move, and
  `/pr-wrap` forbids restructuring a file another PR in the stack also touches — a rename cascade
  through their diffs turns each into a conflict.
- `move_module_to_crate` operates on a whole `<crate>/src/<module>.rs`, so a split has to happen
  either **before** the move — churning the diff a reviewer reads as *"did any logic change?"* — or
  **after** it, at a second 40-minute rust-analyzer index per file.

The seams are worth their own PR now that each subsystem is in one crate. Do it on a follow-up
branch after the stack lands, not inside it.
