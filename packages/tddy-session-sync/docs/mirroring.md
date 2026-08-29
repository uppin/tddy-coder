# Session worktree mirroring (`tddy-session-sync`)

## Purpose

Keep a local directory equal to a tddy session's worktree — the committed history **and** the edits
the agent has made but not yet committed — by watching the session's LiveKit room. Product
contract: [session-worktree-sync.md](../../../docs/ft/daemon/session-worktree-sync.md). Operator
guide: [README](../README.md).

The crate is split the way `tddy-remote-git-repo` is, and for the same reason: everything decidable
without I/O is a pure function over injected inputs, so the decisions are testable without a daemon,
a room, or a clock. Only `sync::run` and the `Mirror` methods touch the world.

## Modules

| Module | Role |
|--------|------|
| `credentials` | Flags with per-parameter environment fallback, and the repo-root `.env` beneath both. An already-set variable always wins — the rule `./web-dev` and `tddy_vm_testkit::env_file` implement. |
| `attach` | Resolve the session over HTTP, mint a room token, join `session-{id}`, subscribe to both topics. |
| `sync` | The loop: what each broadcast provokes, and the git that keeps the mirror equal. |
| `mirror` | The managed destination: marker, ownership refusal, sequence de-duplication, apply. |
| `apply` | What a delta is (`Delta`), and what offering one can conclude (`ApplyOutcome`, `ReconcileReason`). |

## What the room supplies

| Signal | Carries |
|---|---|
| `session.activity` broadcast | each `AgentActivityRecord`: the tool, the commit it ran upon, the tick it belongs to, and the paths it declared |
| `worktree.activity` broadcast | `commit` / `files_changed`, as before this feature |
| `StreamAgentActivityDelta` | the patch a tick produced, scoped to a call's own files or served whole |
| `refs/tddy/session/{id}/wip` | the entire uncommitted state, as an ordinary fetchable git ref |

Committed work rides the git transport that already exists
([`tddy-remote-git-repo`](../../tddy-remote-git-repo/README.md)) as `GIT_SSH_COMMAND`; a session
worktree is a `git worktree` of the project's `main_repo_path`, so `git-upload-pack` there already
advertises the session's branch. Nothing new was invented to move bytes.

## Where the mirror is parked, and why

`HEAD` sits on the session's **own commit**, with the agent's uncommitted edits present in the
working tree:

```
git fetch origin +refs/tddy/session/{id}/wip:refs/tddy/wip
git reset --hard refs/tddy/wip^        # HEAD ← the session's commit (the WIP commit's parent)
git read-tree -u --reset refs/tddy/wip # working tree ← the WIP tree, HEAD untouched
```

Every delta is cut from the session's `HEAD`, so a mirror parked anywhere else would refuse all of
them. `LOCAL_WIP_REF` (`refs/tddy/wip`) is where the fetched tip lands locally.

**Reconciling is a fetch, not a patch.** The mirror is a clone of the same repository, so recovering
from any divergence — a rejected patch, a lost broadcast, a hand-edited mirror — is fetching the WIP
ref and resetting onto it. Git moves only the objects the clone is missing, delta-compressed.

## Deltas are fetched per tick, not per call

`MIRROR_DELTA_SCOPE` is `DeltaScope::Tick`. Several tool calls land in one poll window and share a
tick; the mirror applies that tick's patch **once**. Asking for each call's own scope would apply
the first call of a window and silently drop the rest, along with the residual carrying whatever no
call declared.

`decide_record` therefore de-duplicates on `seq`, not `call_id`, and `ApplyOutcome` distinguishes
`AlreadyApplied` from `Applied` for the same reason: collapsing them would let a re-broadcast
advance the mirror's sequence past a tick it never saw.

## When the mirror resyncs

`ReconcileReason` has three variants, and each carries the values it saw because these are what gets
logged — a reconcile reported as "diverged" with no numbers is one nobody can debug.

| Reason | Meaning |
|---|---|
| `SequenceGap { expected, found }` | The delta does not follow the last one applied — a lost broadcast. |
| `BaseCommitMismatch { expected, found }` | The delta was cut from a different commit than the mirror is on. |
| `PatchRejected { detail }` | `git apply` refused it; carries git's own message. |

## The destination is owned

`--dest` belongs to the syncer. It carries a `MirrorMarker` at `.tddy-session-sync.json`, and local
edits under it are **discarded** by the next sync. Two directories are refused rather than adopted,
because each is a way a managed directory silently eats someone's work:

- non-empty with no marker, and
- marked for a different session.

## Credentials

Two sets, which is not redundancy: LiveKit buys room **admission**, the daemon token buys
**authorization**. Every RPC carries a `session_token` the daemon verifies against its GitHub-login
→ OS-user mapping. Exactly one of `--session-token` / `--refresh-token` is required — an access
token lives five minutes, too short for something configured once, so the seven-day refresh token is
accepted and exchanged.

**`LIVEKIT_API_SECRET` has no flag on purpose.** It is the key a daemon signs every session token
with, and an argv is world-readable through `/proc/<pid>/cmdline`. That this client holds it at all
is a real widening of the trust surface versus `tddy-remote-git-repo`, which holds none:
`MintLiveKitToken` grants the daemon's *common* room and only that, while these signals are
broadcast in `session-{id}`, which no minted token admits. Recorded in `docs/dev/TODO.md` rather
than hidden.

The full flag/environment table is in the [README](../README.md#credentials).

## Tests

| Suite | Covers |
|---|---|
| `tests/credentials_acceptance.rs` | Flag/environment/`.env` layering, precedence, and each refusal naming the variable it wanted. |
| `tests/mirror_acceptance.rs` | Ownership refusals, sequence de-duplication, apply outcomes, and every reconcile reason naming its expected and actual values. |
| `tests/attach_acceptance.rs` | Session resolution and the room/identity rules. Joining a real room is not covered here. |
| `tests/sync_acceptance.rs` | Every decision and every git command the loop issues, as data rather than execution. |
| `tests/cli_acceptance.rs` | The binary's surface, including that it never prints the value of a token it found in the environment. |

## Related

- [Session worktree sync (product)](../../../docs/ft/daemon/session-worktree-sync.md)
- [Session room module](../../tddy-daemon/docs/session-room.md) — the WIP tree, the delta ring, and the two RPCs this consumes
- [`tddy-remote-git-repo`](../../tddy-remote-git-repo/README.md) — the git transport committed work rides
