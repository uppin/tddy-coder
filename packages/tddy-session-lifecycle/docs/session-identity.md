# Session identity: the project's account at session start

Every session this daemon starts or resumes acts as **the GitHub account its project assigns** — for
its commits and for the agent's GitHub tools — from one resolution
([resolution rules](../../tddy-accounts/docs/github-identity-resolution.md)). This document is where
that resolution is taken, what it is bound to, and how the token reaches the agent.

Code: `src/connection_service/session_acting_identity.rs`, `host_session_socket.rs`,
`inherited_host_sockets.rs`.

## One lookup, two halves

`SessionAccountAccess::session_identity(session_id, Option<&[AccountAssignment]>)` returns a
`SessionIdentity { git_environment, github_credential }`. The caller reads the project's assignments
once; `LaunchHost::session_identity(os_user, session_id, project_id, session_token)` is the host-side
wrapper (project lookup, then this). The account is resolved **once** (`acting_identity`) against the
GitHub records of the caller's own vault (`SessionVaultAccountStore::list(session_token)`), and both
halves derive from that outcome:

- the `GIT_AUTHOR_*` / `GIT_COMMITTER_*` pairs, from the `ActingIdentity`;
- `SessionGithubCredential`, **pinned** to it.

`None` assignments (no project row — a client-supplied `repo_path`, or an unreadable project, the
latter logged) yield neither half. A project that does not resolve yields no pairs but **still binds a
handler**, which refuses each request with the resolver's own message.

### Pinned at the session's start

If the start resolved an account, the handler holds that account's **id**. Every later `github-token`
request lists the owner's vault (per call, never cached) and fetches **that** record by id, through
the same `acting_identity` function over a one-entry assignment naming the pinned account — so it can
never answer with another account's token, whatever the project's assignments or the vault's contents
become. A reassignment, or a second account appearing, takes effect on the next start or resume.

If the start was **refused**, the handler holds the reason and repeats it verbatim on every request
until a resume resolves again: a session whose commits are not attributed to an account is never
handed that account's token. A start refused only because the vault was locked stays refused after
the person unlocks it, until the session is resumed.

The handler holds the start's session token in daemon memory for the session's life; when it
expires, tools refuse with `NoSuchSession` until the session is resumed with a live token. The
token is returned per call and kept nowhere.

### A refused resolution does not stop the session

When resolution fails for any reason (`NotAssigned`, `UnknownOnThisHost`, `Ambiguous`, `Unusable`, a
locked / uninitialized / unavailable vault, no such session, no vaults configured) the session still
starts, with no `GIT_*` variables, and the refusal is logged at `warn`
(`tddy_daemon::connection_service`). This is the one place the identity half is not strictly
enforced: the agent's commits use the checkout's inherited `user.name` / `user.email`. It is not a
token fallback — no credential is delivered, so a push cannot succeed as the wrong GitHub account
through this seam.

## Which source a session uses

A project's sessions take their GitHub identity from the vault account it assigns (above) or, when
the project row has `local_gh`, from the host's own GitHub CLI. The source is decided at start and at
resume, in this order:

1. `StartSessionRequest.github_source`, when set (a forwarded start fills it from the forwarding
   host's row), is used and recorded in the session's directory as `github-source.yaml`;
2. otherwise, on resume, the recorded file if present;
3. otherwise this host's row (`local_gh` selects Local gh, else the vault assignment).

A recorded source that cannot be read resolves nothing, logged; the session does not silently take
the row's source instead. `LaunchHost::session_identity` implements this through
`SessionAccountAccess::session_identity_in`, and `DaemonSessionHost::with_github_cli(GithubCli)`
injects the `gh` it asks (production resolves `gh` on the spawned child's `PATH` per call, so one
installed after the daemon started is found).

### Local gh

The identity is read once per start or resume, as the session's OS user, about the GitHub host the
project's git URL names
([host derivation](../../tddy-accounts/docs/github-identity-resolution.md#local-gh)):

- `gh api user --hostname H` gives the login and id, from which the four commit pairs derive;
- `TDDY_GITHUB_SOURCE=local-gh:H` is added beside them: not a secret, it tells the session's
  processes which host's `gh` to ask;
- `github_credential` is a handler that **refuses** with a Local gh reason, so the host-session
  socket never answers a token. The token is read from `gh` by the process that needs it
  ([toolcall](../../tddy-toolcall/docs/architecture.md#the-github_source_env-variable-and-local-gh)).

When `gh` is missing, logged out for that host or failing, the session still starts, under the
checkout's own identity, with the source entry present and the reason logged at `warn`; a step that
later needs a token fails with `gh`'s own reason. This is the same consented exception as a refused
vault resolution. A Local gh identity is read as the identity `gh` reports at the start, so a
`gh auth login` as someone else takes effect at the next start or resume.

**Sandboxed sessions.** The jailed agent's commands run host-side under the relay's environment. When
the daemon runs as the session's OS user the identity is applied as for a vault account. When it
serves several OS users, the relay would reach the daemon user's `gh`, so the session carries no
identity there and the relay gets only `GH_CONFIG_DIR` pointing at an empty directory: `gh` in its
commands is logged out
([`relayed_by`](../../tddy-agent-launch/docs/module-layout.md#github-identity-of-a-session)). PR tools
inside a jail cannot reach GitHub, which has no network.

**Limit.** Child sessions (stack children and conversations) read this host's row, not the parent's
forwarded source, and carry no `github-source.yaml`; see
[the open TODO](../../../docs/dev/todo/2026-10-15-local-gh-child-sessions-and-telegram-gh.md).

## Which path carries what

| Path | Commit pairs | Token handler | Where the pairs go |
|---|---|---|---|
| co-located claude-cli start / resume | yes | yes | the agent's `env_extra` |
| sandboxed claude-cli start and relaunch | yes | yes | `session_env` |
| sandboxed cursor-cli start | yes | yes | `session_env` |
| plain cursor-cli start / resume | yes | none — runs no toolcall listener | the cursor process env |
| agent-spawned children (`spawn-child`, `spawn-conversation`) | yes | yes, via the child's own start | as the claude-cli start |
| tool session (`tddy-coder`) start / resume | yes | yes — over the per-OS-user host-session socket | the child's environment, over the spawn wire |
| split-agent | not applicable | none | — |

**Jails.** The jailed agent never mounts the checkout; it edits and commits through the host-side
Shell relay, whose commands run under the env `dial_and_bridge` is given (`session_env`). The pairs
therefore go there, not in the runner's own env.

**Split-agent.** The agent's working directory is a context dir, not a checkout, and every commit is
made by the tools running on the codebase daemon, so `GIT_*` does not apply.

**Agent-spawned children.** `StackChildSpawnHandler` and `GrillMeConversationSpawnHandler` hold the
orchestrator's `SessionAccountAccess` (vaults, subject resolver, the orchestrator's start token) and
hand it to the child's start. A child is a session of the same owner on the same project, so it acts
as the account that project assigns, resolved over the owner's vault the orchestrator already reads.
The token stays in daemon memory; the child gets pairs and, via its own handler, per-call tokens. A
child spawned after the orchestrator's token expired is refused with `NoSuchSession` and starts under
the checkout's identity.

**Not supported.** Sandboxed cursor-cli resume (`resume_cursor_cli_session` ignores `sandbox`), so
there is no relaunch to carry an identity.

## Tool sessions: the token over the per-OS-user host-session socket

Every tool session the daemon starts or resumes gets `--host-session-socket`, whichever recipe. What
stays recipe-specific is the *handler*: only a grill-me session registers a `spawn_conversation`
handler, so another recipe's request is refused (`this session's recipe does not spawn
conversations`).

**One long-lived socket per OS user**, bound lazily by the first session that needs it, kept for the
daemon's lifetime, and bound again by the next session after a restart. Path:
`<data dir>/run/<os user>/host.sock` (`host_session_socket_path`). `run/` is `0o711` (traversable, not
listable), `<os user>/` is `0o700`, `host.sock` is `0o600`. The path is under the data dir rather
than the session's directory because that nests too deep for AF_UNIX's ~104-byte limit; a data dir
deep enough that the path exceeds 100 bytes is refused with that reason.

**Permissions.** The directory is narrowed to `0o700` **before** the socket is created in it, so
there is no window with wider permissions. Ownership moves last (socket, then directory) to the
session's OS user when the daemon runs as someone else. A `chown` the daemon is not privileged to make
**refuses the bind and removes what it created** instead of widening anything; the session then
starts without the flag, logged. A stale file left by a dead daemon is replaced only after checking it
is a socket, owned by this daemon or the target user, with **nothing listening**; a live socket is
never taken over and a non-socket file is never removed.

**Trust.** The socket's filesystem permissions are the authentication boundary. One socket serves many
sessions, so every request names its `session_id`, but the id is **a label, not a secret**. The
daemon refuses an unknown or deleted id (`not_found`), refuses one registered for a different OS user
than the socket's owner (`permission_denied`, naming neither user, no token), and otherwise answers
from **that session's** handler. The residual trust: any process running as the socket's OS user can
ask for the token of any of *that user's* registered sessions — the boundary of the user's own vault,
since an agent already runs as that user. A hostile process of the same OS user is not defended
against.

**Registry.** `HostSessionSockets` holds the `HostSessionRegistry`
([host-session-service](../../tddy-host-service/docs/host-session-service.md)) and a map of per-user
servers; `ensure_bound` reuses a server whose task is alive and whose socket file is still the inode it
bound, and otherwise stops it and binds again. A session registers at start and again at resume
(replacing the entry with the refreshed assignments and token) and is unregistered at
`DeleteSession`. Dropping the sockets aborts their servers (`BoundServer: Drop`).

### Stopped sessions and daemon restarts

- **Stop hook.** After a start or resume spawns, `watch_until_stopped(session_id, pid)` records the
  spawn result's pid on the registration and polls it (`kill(pid, 0)` every two seconds;
  `ESRCH` is gone, `EPERM` — a process of another user — is still there). When it is gone the session
  is unregistered, so the daemon stops holding the start's session token for a session that is not
  running. A resume registers again, which forgets the earlier pid, so an old process stopping late
  never removes the newer registration. A spawn that fails unregisters. Polling, because the
  supervised backend's children are not the daemon's to `wait` on; a pid reused inside one interval
  keeps a dead session registered a little longer (memory only).
- **Restart.** The registry is daemon memory and **no token is persisted**. An unregistered session
  that exists on disk (`.session.yaml` under the socket owner's sessions base) is refused with
  `this session was started before the daemon restarted; resume it to re-enable GitHub tools`
  (`failed_precondition`), distinct from an unknown or deleted one (`not_found`). The probe asks
  about the socket's owner only, so a request cannot learn that another OS user's session exists.
- **No startup re-attach.** Re-registering needs the owner's session token, which is not durable by
  design. Until a session of that OS user is started or resumed after a restart, the socket is not
  bound and the coder reports that the socket could not be reached, with the hint to resume.

### Supervisor-declared sockets

Where the daemon cannot `chown` to the session's OS user (it is an unprivileged child of
`tddy-supervisor`), the supervisor creates the socket and hands it over; the daemon adopts it
(`inherited_host_sockets.rs`, `HostSessionSockets::adopt_inherited`) and never binds, replaces or
removes it. See [host sockets](../../tddy-supervisor/docs/host-sockets.md). An OS user that is not
declared keeps the explicit refusal, which names `host_sockets:` in `supervisor.yaml`.

The self-binding path is unchanged for a daemon that can `chown` (root, same user, embedded,
desktop).

## Not available

- A tool session started from Telegram gets no host-session socket, and no commit identity from a
  vault account; a Local gh project's start carries the `gh` identity — see the
  [telegram-control architecture](../../tddy-telegram-control/docs/architecture.md).
- Plain cursor-cli has no PR-tool token: it has no toolcall listener.
- A running tool session is not re-registered after a daemon restart; it must be resumed.

## Tests

`tests/tool_session_git_identity.rs` (the real `StartSession` / `ResumeSession` into a script that
records its environment), `tests/tool_session_host_socket.rs` (every recipe gets the flag, owner-only
socket, token over the real socket, two sessions two accounts, no cross-session token, verbatim
refusals, `GITHUB_TOKEN` rescues nothing, delete unregisters, stop and restart),
`src/connection_service/session_acting_identity_tests.rs` (pinning), `host_session_socket_tests.rs`,
`inherited_host_sockets_tests.rs`. Local gh: `tests/tool_session_local_gh.rs` (the real start and
resume into a recording tool, driven by the `a_gh_cli` stub: the `gh` login's commit pairs, an
Enterprise host, the source entry, a logged-out `gh`, a start naming the source on a host whose row was
never set, resume from the recorded source and from the row, and no daemon token variable reaching
`gh`).
