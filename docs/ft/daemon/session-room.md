# Session rooms

**Status:** Current
**Product area:** Daemon

## Summary

Every session that runs an agent has its own LiveKit room, `session-{session_id}`, hosted by that
session's **facilitating daemon** — the daemon running the agent. The facilitating daemon opens the
room before the agent process is spawned, serves its full RPC surface there as
`daemon-{instance_id}`, broadcasts worktree activity to every participant on the
**`worktree.activity`** data-channel topic, and keeps the room's *metadata* as the current
working-tree summary. Agents join as peers. A session with no agent — a standalone `workspace`
session — has no facilitating daemon and no room.

The room belongs to the **session**, not to the checkout. A session has exactly one agent-running
daemon, but its repo may live elsewhere; keying the room on the worktree would leave it homeless
whenever the two are split. Keying it on the session gives it one unambiguous host in every
placement, and lets the worktree be something the room reports on rather than the thing it belongs
to.

## Roles

| Role | Who it is | When it exists |
|---|---|---|
| **Facilitating daemon** | The daemon **running the session's agent**. Hosts the session room, is its first participant, serves file access to the other participants, publishes activity, owns the room's metadata. Session attachments are materialized here. | Always. |
| **Codebase daemon** | The daemon **holding the checkout** when it is not the facilitating daemon. Hosts no room; answers the facilitating daemon's worktree measurements and tool calls over `tddy-rpc`. | Only when the repo is remote ([split placement](remote-managed-worktree.md)). |

In the ordinary case these are the same process: one daemon runs the agent *and* holds the worktree,
so every measurement and every file read is local. Split placement separates them, and the
facilitating daemon then reaches the checkout the same way the agent already does — over `tddy-rpc`
to the codebase daemon. The room does not move; it stays with the agent it serves.

## Membership and identity

| Participant | Identity | Joins |
|---|---|---|
| Facilitating daemon | `daemon-{instance_id}` | At session start, **before the agent is spawned** |
| Split session's agent | `split-agent-{session_id}` | When its `tddy-tools --mcp` child connects |
| Further agents (fastcontext, discovery) | Their own | Minting a second token for the same room; no daemon-side registration |

First-joiner-ness is a consequence of ordering, not a race: the room is opened and joined while the
only thing that could join it is still unspawned.

## File access

The facilitating daemon serves `ExecuteTool`, `StreamExecuteTool`, `ListWorktreeDirectory`,
`ReadWorktreeFile`, `ReadHostDocument` and the rest of `ConnectionService` in the room, and every
participant addresses that one identity. When the repo is local it answers from its own filesystem;
when the repo is remote it forwards to the codebase daemon over the peer routing that already exists
(`classify_exec_tool_route` → `forward_to_peer`).

**Participants never learn which case they are in.** That is the point of putting the room on the
agent's daemon rather than on the checkout's — and it is what a split session's agent relies on: its
`TDDY_REMOTE_SERVER_IDENTITY` is the facilitating daemon, and its scoped join token admits it to this
room and no other.

## Activity

The facilitating daemon measures the checkout every `session_room.poll_interval_ms` and compares
consecutive snapshots:

- a changed HEAD is a **`commit`** event;
- a changed tracked-diff summary is a **`files_changed`** event;
- an identical snapshot publishes nothing.

A change in the **untracked** file count alone produces no event but does refresh room metadata. The
event schema carries numstat counts, which are all zero for an untracked path, so such an event would
be a notification with no content — and every write is untracked for the moment between the file
appearing and `git add` staging it, so treating it as activity would put an empty event in front of
the commit that actually describes the work.

**Where the measurement comes from depends only on placement.** A local checkout is measured by
shelling out to git (`rev-parse HEAD`, `status --porcelain`, `diff --numstat HEAD`); a remote one by a
single **`GetWorktreeSnapshot`** RPC to the codebase daemon, which runs exactly the same measurement
against its own filesystem. One poll is one round trip, not three — so a split placement costs
latency, never a different answer. A peer that cannot be reached costs the room a tick and nothing
else: an unreachable measurement is *unavailable*, never an empty snapshot, because an empty
`head_commit` would otherwise broadcast a commit to the empty sha on every network hiccup.

Events are published **once**, on the reliable data channel, with no `destination_identities` — a
true broadcast, independent of how many peers are connected. They carry `kind`, a monotonic `seq`, a
timestamp, the changed-file count, `+`/`-` line counts and the HEAD sha, encoded as a binary
`worktree_activity.WorktreeActivityEvent`. They carry **no file contents, no diff hunks and no file
paths**: an event says *that* the checkout moved, and reading it is what the file-access RPCs in the
same room are for.

Today every receiver does exactly one thing with an event — emits a single `DEBUG` line through
`tddy_service::worktree_activity::format_worktree_activity_for_log`. Nothing derives state from one.

## Room metadata

The room's metadata is the current working-tree summary: `head_commit`, `branch`, `changed_paths`,
`changed_files`, `lines_added`, `lines_removed`, `untracked_files`, `attachments` and
`updated_at_unix_ms`. A participant joining mid-session reads it from `room.metadata()` without
waiting for the next event, so a second agent can start at any time and know where things stand.

JSON rather than protobuf: it is a LiveKit string field browsers read as well as daemons, and a
snapshot rather than a message on a schema-versioned channel.

Two properties worth knowing:

- **Metadata is written before the event that announces it.** "An event was observed" therefore
  implies "the metadata already reflects it", so a participant reacting to an event never reads a
  snapshot older than the event that woke it. A metadata write that fails holds the change back so
  the next tick retries the whole announce; a *publish* failure does not, and burns the `seq` —
  the schema documents a gap as a lost event.
- **`changed_paths` is capped** at 200 entries, with `changed_paths_truncated: true` added when it
  is. The count fields still carry the true totals. Without the cap a large refactor produces
  metadata the server rejects, and the retry-on-failure rule above would then retry the same
  oversized write every tick forever.

Note that `changed_paths` inherits git's presentation — C-quoted names for non-ASCII paths, and
`{old => new}` rename syntax — so it is display-only.

## Attachments

A session's attachments are materialized on the facilitating daemon, in every placement, and served
to that room's participants through `ReadHostDocument` / `StreamReadHostDocument` under
`scope = SESSION_ARTIFACT`, `relative_path = "attachments/{basename}"`. Their basenames are listed in
room metadata, so a joining agent learns what is shared with no extra round trip.

This is why a split session's forwarded `StartSession` clears `attachments` from the workspace
request: attachments are consumed by the coding agent, the agent runs on the facilitating daemon, and
a copy beside the checkout would move bytes across the network to a host nobody reads them from.

## Configuration

Requires the `livekit:` block. Without credentials no room is created and sessions start exactly as
they did before session rooms existed — the room is an addition, never a prerequisite.

| Key | Default | Role |
|---|---|---|
| `session_room.poll_interval_ms` | 2000 | How often the checkout is measured. Lower means fresher activity and heavier git load: every tick spawns git subprocesses per hosted room. Rejected at config load if out of range, rather than clamped — a `0` would otherwise become a silent git storm. |
| `session_room.git_timeout_ms` | 5000 | Ceiling on one measurement, enforced on the git child rather than only on the waiter. A repo that exceeds it (a stale `index.lock`, a stalled network filesystem) loses that tick's freshness and nothing else. |

Both are overridable by `TDDY_SESSION_ROOM_*`, following the daemon's usual
`defaults ← daemon.yaml ← TDDY_*` precedence.

## Lifecycle

A room is closed when its session is deleted — through the `DeleteSession` RPC, through Telegram's
delete, or when `RemoveWorktree` destroys the checkout it was measuring (matched by path). Closing
aborts both the serving connection and the poll loop; without it the room would keep shelling out to
git in a directory that no longer exists.

If the hosting connection ends on its own, the daemon logs at `error` and stops the poll loop rather
than publishing into a dead connection. It does not rejoin — the LiveKit SDK handles connection-level
reconnects underneath, and an SDK give-up is reported rather than papered over. This matches how the
daemon's common-room participant behaves.

## Known limitations

- **Rooms are not re-opened when the daemon restarts.** The registry starts empty and a room is only
  opened at session start, so a surviving session's checkout loses its host until the session is
  restarted. A split agent resumed against it times out on its ten-second wait for the participant.
  Tracked in `docs/dev/TODO.md`.
- **A claude-cli split agent cannot read its own attachments.** They are served in its room over
  `ReadHostDocument`, which a browser or a second agent can call, but that agent speaks only
  `ExecuteTool` — whose tools are worktree-rooted with traversal rejected. Tracked in
  `docs/dev/TODO.md`.
- **Attachments added after a session starts** are written only to the agent-side session and never
  reach the room's metadata listing.
- **`git diff --numstat HEAD` is HEAD-relative**, so committed-but-unpushed work counts as zero
  changed files. This matches what the Worktrees screen shows for the same checkout; the HEAD sha in
  metadata is what makes commits visible.

## Related documentation

- [Remote managed worktree](remote-managed-worktree.md) — the split placement this builds on
- [LiveKit common-room peer discovery](livekit-peer-discovery.md) — the room this one sits beside
- [Session room module](../../../packages/tddy-daemon/docs/session-room.md) — the implementation
- [Daemon changelog](changelog.md)
