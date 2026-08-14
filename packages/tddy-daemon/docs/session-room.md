# Session room module (`tddy_daemon::session_room`)

## Role

Names a session's LiveKit room, measures the checkout behind it, turns consecutive measurements into
broadcast events, and owns the task that hosts the room. Product contract:
[session-room.md](../../../docs/ft/daemon/session-room.md).

The file is two layers with a one-way dependency: naming, snapshotting and the snapshot→event rules
are pure (or pure plus `git`), and the room lifecycle at the bottom is built on them. Nothing in the
lower layer knows about LiveKit.

## Public API (summary)

| Item | Role |
|------|------|
| **`session_room_name`** | `session-{session_id}`. Derived from an id the facilitating daemon already owns, so the room's name never travels as a field. |
| **`WORKTREE_ACTIVITY_TOPIC`** | Re-export of the topic constant from `tddy-service`, where the payload schema lives — `tddy-tools` receives on the same topic and depends on `tddy-service` unconditionally. |
| **`WorktreeActivityEvent`**, **`WorktreeActivityKind`** | Re-exports of the generated broadcast types, so one import reaches both the topic and its payload. |
| **`WorktreeSnapshot`** | `head_commit`, `branch`, `changed_paths`, `changed_files`, `lines_added`, `lines_removed`, `untracked_files`. |
| **`snapshot_worktree`** / **`snapshot_worktree_within`** | Measure a local checkout; the `_within` form bounds the whole sequence of git commands by one deadline, not each command separately. |
| **`activity_between`** | Two snapshots plus a starting `seq` → the events that announce the difference. Pure. |
| **`room_metadata_json`** | The room's metadata document, `changed_paths` capped at **`MAX_METADATA_CHANGED_PATHS`** (200) with `changed_paths_truncated` when it is. |
| **`WorktreeSource`** | One measurement, however obtained. Implemented by `LocalCheckout` and **`RemoteCheckout`**. |
| **`RemoteSnapshotSource`** | One `GetWorktreeSnapshot` call. A trait so this module does not depend on `connection_service`; the daemon supplies the implementation. |
| **`RemoteCheckout`** | A `WorktreeSource` that measures by asking the codebase daemon. |
| **`SessionRoomHost`** | Opens the room of a session whose agent is about to be spawned. A trait object, so the agent-start path does not have to name the daemon's concrete RPC server type. |
| **`DaemonRoomHosting`** | This daemon as a host of rooms; `for_worktree` (local) and `for_remote_worktree` (split) build a `SessionRoomHosting`. |
| **`SessionRoomRegistry`** | `open`, `open_measured_by`, `close`, `close_for_worktree`. Holds the live rooms, keyed by session. |
| **`OpenedSessionRoom`** | Room, url and server identity of a room that was opened. |

## Measurement

`snapshot_worktree_within` runs `git rev-parse HEAD`, `git rev-parse --abbrev-ref HEAD`,
`git diff --numstat HEAD` and `git status --porcelain` under a single deadline, killing any child
that overruns it. The numstat is parsed by `worktrees::parse_git_diff_numstat` — the same parser the
Worktrees screen reads — so a room and that screen can never quote different totals for one checkout.

A checkout git cannot read snapshots as empty rather than failing: this feeds a periodic poll whose
only recourse is the next tick.

`RemoteCheckout` returns `Measurement::Unavailable` when the peer cannot be reached, never an empty
snapshot. An empty `head_commit` differs from the previous one, so reporting it would broadcast a
`commit` to the empty sha on every network hiccup.

## Event rules

- HEAD changed → one `Commit`.
- Tracked diff changed (`changed_paths`, `changed_files`, `lines_added`, `lines_removed`) → one
  `FilesChanged`.
- Both → both, **commit first**, with consecutive `seq`.
- Identical snapshots → nothing.
- `branch` alone → nothing; it is state, carried in metadata.
- `untracked_files` alone → nothing, but metadata still refreshes. See
  `WorktreeSnapshot::tracked_diff_differs` for why.

## The hosting task

`open` (local) and `open_measured_by` (any source) take the first measurement *before* the room
exists, so its opening metadata already describes the checkout, then create the room, join it as
`daemon-{instance_id}`, and register two tasks under the session's id:

- **serve** — the RPC surface, so participants reach every file-access method without a second
  connection anywhere. When it ends on its own the daemon logs at `error` and flips the shared
  `stopped` flag; nothing reconnects, matching how the common-room participant behaves.
- **poll** — measure, and on any change write metadata *then* publish events, which is what makes
  "an event was observed" imply "the metadata already reflects it". A failed metadata write does not
  advance `previous`, so the next tick retries the whole announce; a failed publish does, and burns
  the `seq`, because the schema documents a gap as a lost event.

Both observe one `stopped` flag, so the poll loop can never outlive the connection it publishes on.
`SessionRoomTask` has a `Drop` that aborts both.

`SessionRoomHosting::worktree_root` is `Option<&Path>`: a split room has no local checkout, so no
local path names it and `close_for_worktree` correctly never matches one.

The registry holds `Arc`s and each serving task owns a clone of `ConnectionServiceImpl`, which holds
the registry. That cycle is deliberate and is broken by `close` — called from `DeleteSession`, from
the Telegram delete path, and from `RemoveWorktree` (by path).

## Configuration

`session_room.poll_interval_ms` (2000) and `session_room.git_timeout_ms` (5000), each overridable by
`TDDY_SESSION_ROOM_*`. Out-of-range values are **rejected at config load** rather than clamped: a
clamp would turn `poll_interval_ms: 0` into a 1 ms loop spawning git subprocesses per room.

## Tests

| Suite | Covers |
|---|---|
| `tests/worktree_activity.rs` | Naming, snapshotting a real checkout, every event rule, metadata shape, the log line. No LiveKit. |
| `tests/session_room_acceptance.rs` | One daemon, real LiveKit: first-joiner, file access, broadcast fan-out, idle silence, late-joiner metadata, attachments, no-credentials, and that a `workspace` session gets no room. |
| `tests/session_room_cross_host_acceptance.rs` | Two daemons, a real split session: a forwarded read is indistinguishable from a local one, and a commit on the codebase daemon is broadcast in the facilitating daemon's room. |

## Related

- [Session rooms (product)](../../../docs/ft/daemon/session-room.md)
- [Connection service](connection-service.md) — `GetWorktreeSnapshot` and the peer routing it reuses
- [Worktrees module](worktrees.md) — the shared numstat parser
