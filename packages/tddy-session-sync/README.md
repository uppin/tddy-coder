# tddy-session-sync

Mirror a **tddy session's worktree** locally by watching its LiveKit room — committed history *and*
the uncommitted edits the agent has made but not yet committed.

```bash
tddy-session-sync --session-id 1780828020298-abc --dest ~/mirrors/my-app
```

Feature: [docs/ft/daemon/session-worktree-sync.md](../../docs/ft/daemon/session-worktree-sync.md)

> ⚠️ **Not usable yet.** The credential and mirror layers are built and tested; the room attach and
> the sync loop are not, so the binary resolves its settings and then exits non-zero. See
> [Status](#status).

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

## Status

| Layer | State |
|---|---|
| `credentials` — flags, environment, `.env`, refusals | ✅ built and tested |
| `mirror` — ownership, sequence de-duplication, apply, reconcile reasons | ✅ built and tested |
| `attach` — resolve the session, join the room | ⛔ not built |
| the sync loop | ⛔ not built |

The daemon side it depends on is likewise partial: `StreamAgentActivityDelta` and
`StreamReadWorktreeFile` are registered but answer `unimplemented`, and nothing broadcasts
`session.activity` yet. See the changeset for what remains.

## Modules

| Module | Role |
|--------|------|
| `credentials` | Flags with per-parameter environment fallback, and the `.env` beneath both |
| `mirror` | The managed destination: marker, ownership refusal, apply |
| `apply` | What a delta is, and what applying one can conclude |
