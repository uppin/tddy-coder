# 2026-08-15 — Session worktree sync — deliberate gaps

**Category:** Future enhancement
**Source:** session-worktree-sync changeset, 2026-08-15

- **A `tool` session gets no session room, so it cannot be mirrored.** At spawn the daemon hands
  `tddy-coder` the *project main repo path*; the coder creates the session worktree itself, later,
  from a branch suggestion inside its workflow. So the worktree does not exist when the room would
  open, and `SessionRoomRegistry::open` fails the start on `Measurement::Gone`. Opening against the
  main repo would pin the poll loop to the wrong checkout for the session's life; opening after the
  spawn breaks the first-participant ordering. Closing it means the coder reporting its worktree
  back. Every other agent-running type — claude-cli, sandboxed claude-cli, cursor-cli, sandboxed
  cursor-cli — now opens one.
- **A split session's calls are broadcast but not attributed.** Its room records no deltas (the
  checkout is on another daemon), so a seq would point into an empty ring and send the client to
  fetch a WIP ref that lives elsewhere. Records go out with `activity_seq: 0`; serving an empty
  patch instead would be a lie. Feeding a split session's deltas through its room is a separate
  piece of work.
- **A sandboxed session's in-jail tool calls are not broadcast.** They reach the durable log and the
  live hub but not the room's `session.activity` topic — `sandbox_session.rs` has no
  `SessionRoomRegistry`. Marked `TODO(session-worktree-sync)` at the call site; closing it means
  threading the registry through `dial_and_bridge`.
- **`AgentActivityDeltaRequest.daemon_instance_id` is not honoured.** The field documents peer
  forwarding "as on ExecuteTool", but the handler only looks up the local store; a request naming
  another daemon gets a local `NOT_FOUND` rather than being forwarded.
- ~~**Three room-dependent behaviours are wired but unpinned.**~~ **Closed** by
  `packages/tddy-daemon/tests/session_room_livekit_acceptance.rs`, which opens a real room over a
  real LiveKit server. Kept here because the suite is container-backed and therefore slower than the
  rest: it is the only place the wiring is checked, and the reason it exists is that
  `SessionDeltaStore::attribute` shipped with no production caller while every isolated suite stayed
  green. Original note follows.
  **Three room-dependent behaviours were wired but unpinned.** The `session.activity` broadcast, the
  `delta_store` lookup after a room is registered, and the `close`→`delete_wip_ref` call site all
  need a live LiveKit room to exercise; `SessionRoomRegistry::register` is private and
  `BroadcastPublisher`'s constructors are `pub(crate)`, so no seam exists to inject one. The
  *behaviours* are tested in isolation — what is untested is the wiring. A container-backed suite
  using `tddy-livekit-testkit` (already a dev-dependency) would close this.

- **A split session mirrors committed history only.** The facilitating daemon reaches a remote
  checkout through `GetWorktreeSnapshot`, whose response carries counts and paths but **no tree**
  (`packages/tddy-service/proto/connection.proto`), so it cannot diff a worktree it does not hold —
  see the `FIXME(session-worktree-sync)` at `packages/tddy-daemon/src/connection_service.rs`
  (`remote_worktree_snapshot`). Closing it means a `wip_tree` field on
  `GetWorktreeSnapshotResponse` and the codebase daemon writing it on every measurement. Until then
  a split session must **say** it is committed-only rather than mirror silently stale content.
- **`MintLiveKitToken` cannot grant a session room.** It grants the daemon's `common_room` and only
  that (`packages/tddy-daemon/src/auth.rs`), so a client that must join `session-{id}` has to hold
  `LIVEKIT_API_SECRET` and mint for itself — which is the fleet's session-token signing key, and
  therefore a real widening of the client trust surface versus `tddy-remote-git-repo`. Closing it
  means a mint that takes a session id and grants that room to a caller authorized for that session,
  which needs the room-ownership model recorded under *Remote git repo over LiveKit* below.
- **`StreamReadWorktreeFile` duplicates `StreamReadHostDocument`'s `SESSION_WORKTREE` scope.** Two
  RPCs read the same bytes through two resolvers, differing only in addressing (`project_id` +
  `worktree_path` versus `session_id`). They share the byte reader and every guard, so they cannot
  drift on what they *allow* — but collapsing them onto one reader is the tidier end state.
- **Per-tick attribution is not per-call attribution.** A delta covers every writer in its poll
  window, so `activity_seq` identifies which patch to fetch, never what one call changed on its own.
  Genuine per-call deltas would mean diffing around each tool call, which costs a `git diff` per
  call including read-only ones.
- **Ignored files never sync, and no RPC can reach them either.** A WIP tree is `git add -A`, which
  respects `.gitignore`, so build output and a local `.env` are outside the mirror. That is not a
  gap to close with `StreamReadWorktreeFile`: its listing gate exists precisely to keep
  `.gitignore`'d paths unreadable (`worktree_files.rs`, `resolve_listed_worktree_file`), and
  loosening it would serve every session's `.env` over LiveKit. Mirroring an ignored file needs a
  deliberate, separately-authorized opt-in, not a wider read.
