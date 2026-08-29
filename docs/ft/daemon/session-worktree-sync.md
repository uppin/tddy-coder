# Session worktree sync (`tddy-session-sync`)

**Status:** 📝 Planned
**Product area:** Daemon
**Date:** 2026-08-15

## Summary

A standalone client, **`tddy-session-sync`**, joins a session's LiveKit room, watches the agent
work, and keeps a local directory as a **fully managed mirror** of that session's worktree —
committed history *and* the uncommitted edits the agent has made but not yet committed.

```bash
tddy-session-sync --session-id 1780828020298-abc --dest ~/mirrors/my-app
```

The session room already broadcasts *that* the checkout moved
([session-room.md](session-room.md)). This feature makes it broadcast **what the agent did** and
adds a way to fetch **the delta each edit produced**, so a participant can reconstruct the worktree
rather than merely learn that it changed.

Committed work rides the git transport that already exists
([remote-git-repo.md](remote-git-repo.md)) — a session's worktree is a `git worktree` of the
project's `main_repo_path`, so `git-upload-pack` there already advertises the session's branch. No
new bulk transport is invented. Uncommitted work is what this feature adds.

## User Story

As a developer whose agent is working on a daemon host I cannot SSH into, I want a local directory
that tracks that session's worktree as the agent edits it, so I can read the work in my own editor,
run my own tools against it, and see uncommitted changes without waiting for a commit or a push.

## Why the existing signals are not enough

Three gaps, each of which makes a mirror silently wrong rather than loudly broken:

1. **`WorktreeActivityEvent` carries no paths and no content.** By design — "an event says *that*
   the checkout moved" (`worktree_activity.proto:16`). A mirror needs the content.
2. **`AgentActivityRecord` carries no commit.** It has `tool_name` and the raw tool `input`, so a
   consumer knows an `Edit` happened but not *what the file looked like before*. Applying a change
   without knowing which commit it was cut from is how a mirror silently diverges.
3. **`ReadWorktreeFile` cannot carry the files.** It returns `string content_utf8` and hard-fails on
   any non-UTF-8 byte (`worktree_files.rs:165`), truncates at 1 MiB, and the `changed_paths` a
   consumer would feed it are git-C-quoted with `{old => new}` rename syntax — documented as
   display-only, "not for opening a file with" (`worktrees.rs:745`).

## Design

### The room is the whole interface

The syncer joins exactly one room, `session-{session_id}`, and everything it needs is there: the
activity broadcast, the commit broadcast, room metadata, and the `ConnectionService` RPCs the
facilitating daemon already serves in that room. The one exception is the git transport itself,
which lives on the daemon's common room and is reached through the existing
`tddy-remote-git-repo` shim.

### Deltas come from the poll loop, not from tool inputs

The session room's poll loop already measures the checkout every `session_room.poll_interval_ms`.
It gains one more measurement per tick: a **WIP tree**, built by staging the whole worktree into a
*temporary index* and writing a tree object.

```
GIT_INDEX_FILE=<tmp>  git add -A
                      git write-tree          → wip_tree
```

The temporary index is not optional: `git add -A` against the agent's real index would rewrite the
staging area out from under it.

A tick's **delta** is then `git diff --binary <prev_wip_tree> <wip_tree>` — an ordinary git patch,
which is why it carries deletions, renames, file modes and binary content for free, and why the
client applies it with `git apply` rather than with logic of its own.

This also closes a blind spot the current event model has by design: a `Write` that creates a new
file leaves it *untracked*, which produces no `FILES_CHANGED` event and appears in no
`git diff HEAD`. A `add -A` tree includes it.

### A delta is scoped to the files its call touched

Measuring per window is what catches every writer; serving per window would be useless. So a tick's
diff is **partitioned by path**:

- a call's delta is the tick's diff limited to that call's own `changed_paths` — the `file_path` an
  `Edit` or `Write` declared;
- the tick's **residual** delta is the diff limited to the paths *no* call claimed — what a `Bash`
  running a formatter or a codegen step changed without declaring it.

Every call's scope plus the residual reconstructs the tick exactly, because every path a tick
touched is claimed by some call or by none. That is the property that lets scoping be narrow
without being lossy, and it is pinned by a test.

The narrowing slices the **recorded** patch into its per-file sections rather than asking git for a
narrower diff again. A patch is a concatenation of self-contained file sections, and `git apply`
accepts any subset of them, so a scoped patch is a real patch and not a filtered rendering of one.

Two properties follow, and both matter:

- every call's slice plus the residual adds back up to **exactly** the bytes the tick produced;
- a lookup needs no subprocess and **no dependence on the trees still existing**. Only the newest
  WIP commit is named by a ref, so an older tick's trees are unreachable objects that `git gc` may
  reclaim — re-diffing them would be a lookup that works until it silently does not.

(`diff_between` itself does use git's pathspec limiting; that is how the tick's patch is produced,
not how it is later partitioned.)

A call that declared nothing gets an **empty** delta, not its neighbours' changes. Falling back to
the whole tick would credit a call with another's work and apply the same change twice.

### Reconciling is a git fetch, not a patch

The mirror is a clone of the same repository, so recovering does not need a whole-worktree diff —
git already knows how to move only the objects a clone is missing, delta-compressed. Each tick the
daemon wraps its WIP tree in a commit parented on `HEAD` and points **`refs/tddy/session/{id}/wip`**
at it. A client that has fallen behind fetches that ref and hard-resets to it.

```
daemon:   commit-tree <wip_tree> -p HEAD  →  refs/tddy/session/{id}/wip
mirror:   git fetch origin +refs/tddy/session/{id}/wip:refs/tddy/wip
          git reset --hard refs/tddy/wip^        # HEAD ← the session's own commit
          git read-tree -u --reset refs/tddy/wip # working tree ← the WIP tree, HEAD untouched
```

**Two commands, not one, and the reason is load-bearing.** Resetting straight onto the WIP commit
would leave the mirror's `HEAD` on *that* commit — but every delta names the session's real `HEAD`
as its `base_commit`, and the client compares that against its own `rev-parse HEAD`. A mirror parked
on the WIP commit therefore rejects every delta that follows, reconciles, parks there again, and
reconciles forever. The WIP commit's parent **is** the session's commit (`commit-tree … -p HEAD`),
so `refs/tddy/wip^` puts `HEAD` where the deltas expect it, and `read-tree` lays the uncommitted
state over it without moving `HEAD` again.

Under `refs/tddy/` rather than `refs/heads/` because it is not a branch: it must never appear in
`git branch`, never be a push target, and never be a name the agent working in that checkout has to
reason about. It is deleted when the session's room closes, so its objects become unreachable and
ordinary `git gc` reclaims them — left behind, every deleted session would pin a whole worktree's
worth of blobs forever.

### Divergence is reconciled from git and reported

Every failure mode — a rejected patch, a `seq` gap, a delta aged out of the daemon's ring, a record
whose `head_commit` is not the mirror's — resolves the same way: fetch the WIP ref, hard reset to
it, and log at `error` naming what diverged. Self-healing, never quiet.

## Acceptance Criteria

### Wire — what the room publishes

1. **`AgentActivityRecord` carries `head_commit`** — the worktree's HEAD at the moment the call was
   recorded, read from the filesystem rather than by spawning `git`, so stamping a record costs no
   subprocess. A record whose HEAD could not be read carries an empty `head_commit` and is never
   given a fabricated one.
2. **`AgentActivityRecord` carries `activity_seq` and `changed_paths`** — the poll tick its delta
   belongs to (`0` when no tick has covered it yet), and the worktree paths the call is credited
   with. The paths are plain and relative, **not** git's C-quoted display form: they are used to
   open files and to build pathspecs, and a quoted name would select nothing.
3. All three fields are `#[serde(default)]` on the persisted JSONL row, so an `agent-activity.jsonl`
   written before this change still reads rather than every historical row being skipped as
   malformed.
4. **Activity records are broadcast in the session room** on a new `session.activity` data-channel
   topic, binary `connection.AgentActivityRecord`, published once with no
   `destination_identities` — the same broadcast discipline as `worktree.activity`. The topic is a
   named constant beside the payload's schema, for the reason `WORKTREE_ACTIVITY_TOPIC` already
   documents: publisher and receiver live in different crates, and a topic each spelled for itself
   fails as silence.
5. The daemon is the **only** broadcaster, because it is the only participant in that room.
   Coder-hosted sessions (tool, cursor-cli) report their records to it through the existing
   `ReportAgentActivity` RPC, so no session type is a silent blind spot.

### Wire — how a delta is fetched

6. `StreamAgentActivityDelta(session_id, call_id, scope)` **streams** a patch **scoped to the files
   the named call touched** — `DELTA_SCOPE_CALL`, the default. Every frame carries the patch slice,
   the tick `seq`, the `prev_seq` it follows, the `base_commit` it applies onto, the patch's
   `total_byte_size`, and the `scoped_paths` it was limited to, so a client can check the server
   scoped the way it asked rather than trusting it.
   **Streamed rather than unary because a patch has no useful upper bound**: a payload over
   `tddy_livekit::chunking::MAX_CHUNK_FRAME_BYTES` (60 000) is chunk-framed, where a lost chunk
   wedges the call with no error rather than failing it. Frames are capped at 48 KiB of payload, as
   `HOST_DOCUMENT_FRAME_BYTES` is.
7. `DELTA_SCOPE_RESIDUAL` streams the paths of that tick claimed by **no** call — what an undeclared
   writer changed. Every call's scope plus the residual reconstructs the tick exactly; without this
   scope a change no tool declared would be attributed to nobody and reach no one.
8. A call that declared no paths gets an **empty** patch, never its neighbours' changes.
9. A call that changed nothing is **one frame** with an empty `patch` and `total_byte_size` 0, so
   "nothing changed" is distinguishable from "the stream failed".
10. A `call_id` the daemon does not know is `NOT_FOUND`. A call whose delta has aged out of the
    daemon's bounded ring is `NOT_FOUND` **with a distinct message**, because the client's response
    differs: an unknown call is a bug, an aged-out delta is a reconcile.
11. The delta ring is **bounded** — by tick count and by total bytes — so a long session cannot grow
    it without limit. Eviction is oldest-first and is not an error.
12. **There is no whole-worktree delta.** Reconciling is a git fetch of the WIP ref (AC13); a
    cumulative patch would re-send the entire dirty tree over a data channel every time a client
    fell one tick behind.
13. **Each tick publishes `refs/tddy/session/{session_id}/wip`** — a commit wrapping the tick's WIP
    tree, parented on `HEAD`, so "which commit does this apply to" is answered by the object graph.
    It lives under `refs/tddy/` and therefore never appears in `git branch`. It is **deleted when
    the session's room closes**, so its objects stop being pinned.
14. Same authorization as every other `ConnectionService` RPC: `session_token` resolved by the same
    resolver, `UNAUTHENTICATED` / `PERMISSION_DENIED` as usual, and refused **before** any git
    subprocess runs.

### Wire — streaming worktree reads

15. **`StreamReadWorktreeFile(ReadWorktreeFileRequest) returns (stream WorktreeFileChunk)`** — the
    streaming sibling `ReadWorktreeFile` never had. Same request message, same `project_id` +
    `worktree_path` addressing, same `resolve_listed_worktree` gate.
16. It returns **`bytes`**, never a string: a PNG, a UTF-16 file and a file with a lone `0x80` byte
    all round-trip byte-identical. No UTF-8 validation exists on this path to fail.
17. There is **no 1 MiB truncation**. The bound is `max_attachment_bytes`, and a file over it is
    **refused** before the first frame rather than silently cut — a truncated mirror is a wrong
    mirror.
18. A zero-byte file yields exactly one empty frame, so "the file is empty" and "the stream failed"
    are distinguishable.
19. Frames carry at most 48 KiB of payload, matching `HOST_DOCUMENT_FRAME_BYTES`, so a frame never
    approaches `tddy_livekit::chunking::MAX_CHUNK_FRAME_BYTES`.
20. The same guards as the unary read apply and are tested as such: `..` and absolute paths are
    `INVALID_ARGUMENT`, a path outside the git listing is `PERMISSION_DENIED`, a symlink resolving
    outside the worktree root is `PERMISSION_DENIED`, a missing file is `NOT_FOUND`.

### Client — attaching to a session

21. `--session-id` and `--dest` are required. The syncer resolves the session's `project_id`,
    `worktree_path` and `daemon_instance_id` from the daemon rather than taking them as flags; a
    session it cannot resolve is a hard error naming the session id.
22. LiveKit credentials come from flags or environment (`--livekit-url` / `LIVEKIT_URL`,
    `--livekit-api-key` / `LIVEKIT_API_KEY`, `--livekit-api-secret` / `LIVEKIT_API_SECRET`), with
    the repo-root `.env` read as a last resort under the house rule that **an already-set variable
    always wins**. The reader is the existing hand-rolled one, not a new `dotenv` dependency.
23. Daemon credentials are the existing set — `--daemon-url` / `TDDY_DAEMON_URL` plus exactly one of
    `--session-token` / `--refresh-token`. Both credential sets are required: LiveKit admits the
    syncer to the room, the daemon token authorizes the RPCs it makes there. `--help` prints the
    *name* of every credential's environment variable and never its value.
24. A missing or unparsable setting exits non-zero naming the flag *and* its environment variable.
    Nothing is replaced by a default it was not given.

### Client — the managed mirror

25. `--dest` is **owned** by the syncer. It writes a `.tddy-session-sync.json` marker recording the
    session id, daemon instance id, project, last applied `activity_seq` and last `head_commit`.
26. A `--dest` that exists, is non-empty, and has **no** marker is refused. So is one whose marker
    names a **different** session. Neither is overwritten and neither is adopted — the two ways a
    managed directory silently eats someone's work.
27. On first attach the syncer clones the project over the existing git transport
    (`GIT_SSH_COMMAND=tddy-remote-git-repo`) and restores the session's **WIP ref** as AC31
    describes — so attaching to an already-dirty session mid-flight produces a correct mirror, not
    one that is merely correct-so-far.
28. A `commit` activity event re-fetches and restores the session's WIP ref, whose parent is the
    new `head_commit` — so the mirror follows `HEAD` and the deltas that follow still apply.
29. An edit activity fetches its delta by `call_id` and applies it with `git apply`. A delta whose
    tick `seq` has already been applied is skipped — several calls sharing one tick apply it once.
30. Deltas are applied in `seq` order. A gap in `seq` is a lost broadcast and triggers a reconcile;
    it is never applied out of order and never skipped over.
31. **Reconcile** is `git fetch` of the WIP ref, then `reset --hard <wip>^` to put `HEAD` on the
    session's own commit and `read-tree -u --reset <wip>` to lay the uncommitted state over it —
    never a whole-worktree patch. Resetting onto the WIP commit itself would park `HEAD` where no
    delta's `base_commit` can match, and reconcile forever. A path that git cannot reconstruct (one
    the session's `.gitignore` excludes, so it is in no tree) is pulled whole with
    `StreamReadWorktreeFile`.
32. **Every divergence is logged at `error`** naming what diverged — the expected and actual
    commit, or the rejected path, or the aged-out `call_id`. A reconcile is never silent, and a
    sync that cannot complete exits non-zero rather than leaving a half-written mirror and
    reporting success.
33. Local edits inside `--dest` are **discarded** by the next sync without prompting. The marker
    file and this document are the only warning; the directory is a mirror, not a workspace.

### End to end

34. An agent `Write` creating a new file appears in the mirror, with identical bytes, without a
    commit having happened.
35. An agent `Edit` to an existing file appears in the mirror, with identical bytes.
36. A file the agent **deletes** disappears from the mirror.
37. A **binary** file the agent writes appears byte-identical — the case `ReadWorktreeFile` cannot
    express at all.
38. A `git commit` in the session moves the mirror's `HEAD` to the same sha, and the mirror's
    working tree matches the session's.
39. A mirror corrupted by hand (a file edited locally, or reset to an older commit) is restored to
    match the session on the next activity, and the divergence is reported.

## Wire contract

```proto
// connection.proto — additions

message AgentActivityRecord {
  // … existing fields 1-9 …

  // The worktree HEAD when this call was recorded. A patch is only applicable against the commit
  // it was cut from, so a record that cannot name its base is a record a mirror must not act on.
  string head_commit = 10;

  // The poll tick whose delta covers this call; 0 when no tick has covered it yet. Several calls
  // in one window share a seq but NOT a patch — changed_paths is what separates them.
  uint64 activity_seq = 11;

  // The paths this call is credited with. The delta served for it is the tick's diff limited to
  // exactly these, so two calls in one window get two patches. Plain relative paths, never git's
  // C-quoted display form.
  repeated string changed_paths = 12;
}

service ConnectionService {
  // … existing …

  // The patch a call produced, scoped to that call's own files. The INCREMENTAL path only —
  // reconciling is a git fetch of the WIP ref. Streamed, because a patch has no useful upper
  // bound and an oversized frame wedges silently rather than failing.
  rpc StreamAgentActivityDelta(AgentActivityDeltaRequest) returns (stream AgentActivityDeltaChunk);

  // The streaming, byte-exact sibling of ReadWorktreeFile. Same addressing, same gates; bytes
  // rather than a UTF-8 string, and refused rather than truncated when oversized.
  rpc StreamReadWorktreeFile(ReadWorktreeFileRequest) returns (stream WorktreeFileChunk);
}

message AgentActivityDeltaRequest {
  string session_token      = 1;
  string session_id         = 2;
  string daemon_instance_id = 3;  // routing, as on ExecuteTool; empty means the one being called
  string call_id            = 4;  // required — there is no whole-worktree mode
  DeltaScope scope          = 5;
}

// Every path a tick touched is claimed by some call or by none, so CALL over every call plus
// RESIDUAL reconstructs TICK exactly — which is what lets scoping be narrow without being lossy.
enum DeltaScope {
  DELTA_SCOPE_CALL     = 0;  // the named call's own paths
  DELTA_SCOPE_RESIDUAL = 1;  // the paths of that tick no call claimed
  DELTA_SCOPE_TICK     = 2;  // the whole tick, unscoped
}

// The describing fields repeat on every frame, as HostDocumentChunk's total_byte_size does: a
// reader knows what it is receiving from the first frame, with no header frame to special-case.
message AgentActivityDeltaChunk {
  bytes  patch           = 1;  // one slice of `git diff --binary` output
  uint64 seq             = 2;  // the tick this patch belongs to
  uint64 prev_seq        = 3;  // the tick it follows; a gap means a lost broadcast
  string base_commit     = 4;  // the commit the patch applies onto
  reserved 5;                  // was `cumulative`, before reconciling became a git fetch
  uint64 total_byte_size = 6;  // the patch's full size; 0 when nothing changed
  repeated string scoped_paths = 7;  // what the patch was limited to, resolved
}

message WorktreeFileChunk {
  bytes  data            = 1;
  uint64 total_byte_size = 2;
}
```

Broadcast topic, beside `worktree.activity`:

```
session.activity  →  binary connection.AgentActivityRecord
```

And one git ref per session, which is the whole reconcile surface:

```
refs/tddy/session/{session_id}/wip  →  commit-tree <wip_tree> -p <head_commit>
```

## Credentials

| Flag | Environment | Default |
|------|-------------|---------|
| `--session-id` | — | — (required) |
| `--dest` | — | — (required) |
| `--livekit-url` | `LIVEKIT_URL` | — (required) |
| `--livekit-api-key` | `LIVEKIT_API_KEY` | — (required) |
| `--livekit-api-secret` | `LIVEKIT_API_SECRET` | — (required) |
| `--daemon-url` | `TDDY_DAEMON_URL` | — (required) |
| `--session-token` | `TDDY_SESSION_TOKEN` | — |
| `--refresh-token` | `TDDY_REFRESH_TOKEN` | — |
| `--connect-timeout-secs` | `TDDY_CONNECT_TIMEOUT_SECS` | `30` |

**Both credential sets are required, and that is not redundancy.** LiveKit credentials buy room
*admission*; the daemon token buys *authorization*. Every RPC the syncer makes carries a
`session_token` verified server-side against the GitHub-login → OS-user mapping, so no LiveKit
credential can substitute for it.

Why raw LiveKit credentials here when `tddy-remote-git-repo` deliberately has none
([remote-git-repo.md](remote-git-repo.md) § Credentials): `MintLiveKitToken` grants the daemon's
**common room** and only that room. The signals this feature consumes are broadcast in
`session-{session_id}`, which no minted token admits. Until a mint exists that can grant a session
room to an authorized caller, a client that must be in that room has to mint for itself.

> ⚠️ **This is a real widening of the client trust surface, recorded rather than hidden.**
> `LIVEKIT_API_SECRET` is the same value a daemon signs session tokens with, so a host running
> `tddy-session-sync` holds a credential that could mint an access token for any GitHub user on the
> fleet. The syncer never does this — it takes a daemon token like every other client — but holding
> the secret makes it *possible*, which is precisely what `remote-git-repo.md` § Trust model
> refused for the git shim. Closing it means extending `MintLiveKitToken` to grant a session room
> to a caller authorized for that session; see `docs/dev/TODO.md`.

## Non-goals

- **Writing back.** The mirror is one-way. Edits in `--dest` are discarded, not pushed.
- **A whole-worktree patch.** Reconciling is a git fetch of the WIP ref; there is deliberately no
  RPC that returns the entire dirty tree as a diff.
- **Mirroring ignored files.** A WIP tree is `git add -A`, which respects `.gitignore`, so build
  output and a local `.env` never sync — and `StreamReadWorktreeFile` cannot reach them either: its
  listing gate exists to keep exactly those paths unreadable. Mirroring one would need a deliberate,
  separately-authorized opt-in.
- **Sub-call attribution.** A call's delta is scoped to the paths it *declared*. Two calls editing
  the same file in one poll window share that file's net change; the wire cannot separate them,
  because the measurement is per window.
- **Sub-poll-interval latency.** A delta exists at most `session_room.poll_interval_ms` after the
  edit. Lowering that is a daemon config change with a documented git-load cost.
- **Sessions with no room.** A `workspace` session has no facilitating daemon and no room
  ([session-room.md](session-room.md)); the syncer refuses it by name.
- **Reattaching across a daemon restart.** Rooms are not re-opened when the daemon restarts
  (a known limitation of session rooms); the syncer exits rather than waiting forever.

## Related documentation

- [Session rooms](session-room.md) — the room, its metadata, and the poll loop this extends
- [Remote git repository over LiveKit](remote-git-repo.md) — the git transport this reuses
- [Project concept](project-concept.md) — the registry the git leg resolves against
- [Cross-daemon session authentication](session-auth.md) — the token model
- [Session room module](../../../packages/tddy-daemon/docs/session-room.md) — how the WIP tree, the fetchable ref and the bounded delta ring are built
- [Session worktree mirroring](../../../packages/tddy-session-sync/docs/mirroring.md) — how the client consumes them and keeps a destination equal
