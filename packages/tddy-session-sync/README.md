# tddy-session-sync

Mirror a **tddy session's worktree** locally by watching its LiveKit room — committed history *and*
the uncommitted edits the agent has made but not yet committed.

```bash
tddy-session-sync --session-id 1780828020298-abc --dest ~/mirrors/my-app
```

Feature: [docs/ft/daemon/session-worktree-sync.md](../../docs/ft/daemon/session-worktree-sync.md)

Module documentation: [docs/mirroring.md](docs/mirroring.md).

## How it works

The session room already broadcasts *that* the checkout moved. This feature makes it broadcast
**what the agent did**, and adds a way to fetch the delta each edit produced — so a participant can
reconstruct the worktree rather than merely learn it changed.

| Signal | Carries |
|---|---|
| `session.activity` broadcast | each `AgentActivityRecord`: the tool, the commit it ran upon, and the paths it declared |
| `worktree.activity` broadcast | `commit` / `files_changed`, as it already did |
| `StreamAgentActivityDelta` | the patch a call produced, **scoped to that call's own files** |
| `refs/tddy/session/{id}/wip` | the whole uncommitted state, as an ordinary git ref |

Committed work rides the git transport that already exists
([`tddy-remote-git-repo`](../tddy-remote-git-repo/README.md)) — a session worktree is a
`git worktree` of the project's `main_repo_path`, so `git-upload-pack` there already advertises the
session's branch. Nothing new is invented to move bytes.

**Reconciling is a `git fetch`, not a patch.** The mirror is a clone of the same repository, so
recovering from any divergence — a rejected patch, a lost broadcast, a hand-edited mirror — is
fetching the WIP ref and hard-resetting onto it. Git moves only the objects the clone is missing.

## The destination is owned

`--dest` belongs to the syncer. It writes a `.tddy-session-sync.json` marker, and **local edits
under it are discarded** by the next sync. A directory that is non-empty and carries no marker is
refused rather than adopted; so is one marked for a different session. Those are the two ways a
managed directory silently eats someone's work.

## Credentials

Two sets, and that is not redundancy: LiveKit buys room **admission**, the daemon token buys
**authorization**. Every RPC carries a `session_token` the daemon verifies against its GitHub-login
→ OS-user mapping.

| Flag | Environment | Default |
|------|-------------|---------|
| `--session-id` | `TDDY_SESSION_ID` | — (required) |
| `--dest` | — | — (required) |
| `--livekit-url` | `LIVEKIT_URL` | — (required) |
| `--livekit-api-key` | `LIVEKIT_API_KEY` | — (required) |
| *(no flag)* | `LIVEKIT_API_SECRET` | — (required) |
| `--daemon-url` | `TDDY_DAEMON_URL` | — (required) |
| `--session-token` | `TDDY_SESSION_TOKEN` | — |
| `--refresh-token` | `TDDY_REFRESH_TOKEN` | — |
| `--connect-timeout-secs` | `TDDY_CONNECT_TIMEOUT_SECS` | `30` |

The repo-root `.env` is read beneath both, and **an already-set variable always wins** — the same
rule `./web-dev` and `tddy_vm_testkit::env_file` implement for that file.

**`LIVEKIT_API_SECRET` has no flag on purpose.** It is the key a daemon signs every session token
with, and an argv is world-readable through `/proc/<pid>/cmdline`.

> Why this client holds a LiveKit secret at all, when `tddy-remote-git-repo` deliberately holds
> none: `MintLiveKitToken` grants the daemon's **common room** and only that. The signals this tool
> consumes are broadcast in `session-{id}`, which no minted token admits. That is a real widening of
> the client trust surface, recorded in `docs/dev/TODO.md` rather than hidden.

Exactly one of `--session-token` / `--refresh-token` is required. An access token lives 5 minutes —
too short for something configured once — so the 7-day refresh token is accepted and exchanged.

## How a mirror stays equal

The mirror is a checkout of the same repository, so it is kept on the state the session's checkout
is actually in: **`HEAD` on the session's own commit, with the agent's uncommitted edits present in
the working tree**. That is what makes an incoming patch applicable — every delta is cut from the
session's `HEAD`, so a mirror parked anywhere else would refuse all of them.

```
git fetch origin +refs/tddy/session/{id}/wip:refs/tddy/wip
git reset --hard refs/tddy/wip^      # HEAD ← the session's commit (the WIP commit's parent)
git read-tree -u --reset refs/tddy/wip   # working tree ← the WIP tree, HEAD untouched
```

Each tick's delta is fetched **once per tick, whole** (`DELTA_SCOPE_TICK`) rather than once per
call. Several tool calls in one poll window share a tick, and the mirror applies a tick's patch
once; asking for each call's own scope would apply the first call of a window and silently drop
the rest, along with the residual that carries what no call declared.

## Status

Working end to end for daemon-hosted sessions. The poll loop stages a WIP tree, produces a scoped
delta and publishes `refs/tddy/session/{id}/wip`; records are stamped with the commit they ran upon
and broadcast on `session.activity`; both RPCs serve; and this client attaches, mirrors and
reconciles.

| Layer | State |
|---|---|
| `credentials` — flags, environment, `.env`, refusals | ✅ built and tested |
| `mirror` — ownership, sequence de-duplication, apply, reconcile reasons | ✅ built and tested |
| `attach` — resolve the session, join the room, subscribe | ✅ built; resolution and the room/identity rules tested, joining a real room is not |
| `sync` — first attach, deltas, reconcile | ✅ built; every decision and every git command tested |

**Known gap.** `StreamReadWorktreeFile` is served by the daemon but **not called** by this client:
the fallback for a path git cannot reconstruct — one the session's `.gitignore` excludes — is not
implemented, so such a file is absent from the mirror rather than fetched by hand.

## Modules

| Module | Role |
|--------|------|
| `credentials` | Flags with per-parameter environment fallback, and the `.env` beneath both |
| `attach` | Resolve the session over HTTP, mint a room token, join, subscribe to both topics |
| `sync` | The loop: what each broadcast provokes, and the git that keeps the mirror equal |
| `mirror` | The managed destination: marker, ownership refusal, apply |
| `apply` | What a delta is, and what applying one can conclude |
