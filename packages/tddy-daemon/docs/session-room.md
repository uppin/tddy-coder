# Session room module (`tddy_daemon::session_room`)

## Role

Names a session's LiveKit room, measures the checkout behind it, turns consecutive measurements into
broadcast events, and owns the task that hosts the room. Product contract:
[session-room.md](../../../docs/ft/daemon/session-room.md).

Since the worktree-sync work the room is sufficient to **reconstruct** the checkout, not merely to
learn that it moved: each tick stages a WIP tree, publishes it as a fetchable ref, and keeps the
patch between consecutive ticks. See [Reconstructing the checkout](#reconstructing-the-checkout) and
[session-worktree-sync.md](../../../docs/ft/daemon/session-worktree-sync.md).

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
| **`SESSION_ACTIVITY_TOPIC`** | Re-export of `session.activity`, the topic each `AgentActivityRecord` is broadcast on. Distinct from `worktree.activity`, which still carries only *that* the checkout moved. |
| **`write_wip_tree_within`** | Stage the whole checkout into a **scratch index** and `write-tree` it. Returns `""` on any failure, since a poll's only recourse is the next tick. |
| **`tick_delta`** | Two snapshots plus a `seq` → the `ActivityDelta` between their WIP trees, or `None`. |
| **`wip_ref_name`** / **`publish_wip_ref`** / **`delete_wip_ref`** | `refs/tddy/session/{id}/wip`, the commit published under it, and its removal at close. |
| **`ActivityDelta`** | One patch plus what a client needs to place it: `seq`, `prev_seq`, `base_commit`, `patch`, `scoped_paths`. |
| **`SessionDeltaStore`** | The bounded ring of recent ticks and the `call_id → seq` index over it; `delta_for_call` narrows a tick to one call's files. |
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

## Reconstructing the checkout

Three things the poll loop does beyond measuring, all so a participant can rebuild the worktree
rather than merely notice it changed.

**A WIP tree per tick.** `write_wip_tree_within` copies the agent's index to a scratch index inside
the git directory, `add -A`s into that, and `write-tree`s it — so the agent's own index is never
touched. The scratch index is seeded from the agent's rather than started empty because `git add -A`
against an empty index re-hashes the whole checkout; the copy carries git's stat cache, so a tick
hashes only what moved. It lives inside the git dir deliberately: under the worktree it would stage
itself into the tree being measured, and in the system temp dir it would usually cross a filesystem
and copy a possibly-huge index byte for byte every tick.

**The tree is published as a fetchable ref.** `publish_wip_ref` wraps the tree in a commit parented
on the measured `HEAD` and points `refs/tddy/session/{id}/wip` at it. Under `refs/tddy/` rather than
`refs/heads/` so it is never a branch an agent sees, pushes to, or can `git checkout` by name. The
commit is authored by a fixed `tddy-daemon@tddy.invalid` identity — the object is a machine-made
snapshot, not signed work, and `commit-tree` refuses outright in a repository with no configured
identity. This is what makes reconciliation an ordinary `git fetch`: the mirror is a clone of the
same repository, so git moves only the objects it lacks, where a cumulative patch would resend the
whole dirty tree each time a client fell a tick behind. An unborn or unreadable `HEAD` yields a
parentless commit rather than no ref at all.

**A bounded ring of patches.** `tick_delta` diffs consecutive WIP trees; `SessionDeltaStore` retains
them, bounded on **both** axes — `SESSION_DELTA_RING_TICKS` (64, about two minutes at the shipped
2 s interval) and `SESSION_DELTA_RING_BYTES` (16 MiB). Neither bound alone suffices: a tick count
bounds a busy session only if its patches are small, and a byte budget bounds one enormous change
but not a million tiny ones. A client further behind than the ring reconciles by fetching the WIP
ref, which is cheaper than the longer ring would have been. `delta_for_call` narrows a tick to the
paths one call declared, and serves what no call declared as that tick's residual — so every call
scope plus the residual reconstructs the whole tick.

`tick_delta` returns `None`, never an empty delta, whenever a tick **cannot be described**: no
previous tree, no current tree, identical trees, an unreadable `HEAD`, or a diff git refused. The
distinction is the safety property — an empty patch means "this tick moved nothing" and lets a
client record the tick and advance its sequence, so an empty patch standing in for a failure is a
change the mirror never learns about and never reconciles.

**Records are stamped and broadcast.** An `AgentActivityRecord` reaching the daemon is stamped with
the commit the checkout was on and the worktree-relative paths the call declared, then broadcast on
`SESSION_ACTIVITY_TOPIC`. The coder does not push these over a transport of its own: the poll loop
tails the `agent-activity.jsonl` the coder already writes and the daemon already owns, which is why
no new coder credential or channel exists.

**Close releases the ref under an interlock.** Deletion happens in `close`, not `Drop`, and takes
`wip_ref_released` for the whole release — so a tick already measuring either published before it
and is deleted here, or finds the ref released and publishes nothing. The cost is that a close waits
out a publish in flight, bounded by that tick's git budget. A failure is logged at `error`: what is
left behind is a worktree of blobs pinned in a repository shared by every checkout of the project.

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
| `tests/session_room_livekit_acceptance.rs` | One daemon, real LiveKit: the tick ring, the WIP ref's parentage and its release at close, a call served the patch its tick produced, and a record broadcast into the room stamped with its tick. |
| `tests/session_activity_delta_acceptance.rs` | Real git repositories, ticks driven explicitly, no LiveKit: staging a WIP tree without touching the agent's index, delta scoping per call, the residual, and ring eviction. |
| `tests/session_activity_wiring_acceptance.rs` | `read_head_commit` checked against what git itself reports, and what one tick produces. |
| `tests/session_activity_attribution_acceptance.rs` | `tick_activity` as a pure function: which tick a call belongs to, decided without a room. |
| `tests/agent_activity_stamping_acceptance.rs` | The real `ReportAgentActivity` handler: head-commit stamping, and crediting a call with the worktree-relative paths it declared. |
| `tests/stream_agent_activity_delta_rpc_acceptance.rs` | The delta RPC across its scopes, including an unknown `call_id` distinguished from one that aged out. |

## Related

- [Session rooms (product)](../../../docs/ft/daemon/session-room.md)
- [Connection service](connection-service.md) — `GetWorktreeSnapshot` and the peer routing it reuses
- [Worktrees module](worktrees.md) — the shared numstat parser
- [Session worktree sync (product)](../../../docs/ft/daemon/session-worktree-sync.md) — what the WIP ref and the delta ring are for
- [`tddy-session-sync`](../../tddy-session-sync/docs/mirroring.md) — the client that consumes them
