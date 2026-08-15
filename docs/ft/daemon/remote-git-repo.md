# Remote git repository over LiveKit (`tddy-remote-git-repo`)

**Status:** ✅ Implemented
**Product area:** Daemon
**Date:** 2026-08-15

## Summary

Every **project** a `tddy-daemon` serves becomes a **git remote** reachable from any machine that
can join the daemon's LiveKit room — no SSH daemon, no open port, no VPN. A new binary,
**`tddy-remote-git-repo`**, plays the role `ssh` plays in git's SSH transport: git execs it, it
carries `git-upload-pack` / `git-receive-pack` over a LiveKit bidi RPC stream to the daemon, and the
daemon runs the real git command against the project's checkout as the project's OS user.

```bash
export GIT_SSH_COMMAND="tddy-remote-git-repo"     # credentials via TDDY_* env, see Credentials
git clone udoo-1780828020298:my-app
git -C my-app fetch
git -C my-app push origin feat/my-branch
```

That first line assumes the binary is on `PATH`; `./install` and the `.deb` put it there (see
[Shipping](#shipping)).

`udoo-1780828020298` is the daemon's `daemon_instance_id` (its LiveKit participant is
`daemon-{instance_id}`); `my-app` is the project's `name` or `project_id` in that OS user's
`~/.tddy/projects/projects.yaml`.

## User Story

As a developer whose repositories live on daemon-managed hosts I cannot SSH into, I want to
`git clone`, `fetch` and `push` against a daemon project using ordinary git commands, so a
daemon host is a first-class remote rather than a place I can only reach through the web UI.

## Why this is not the terminal RPC

The obvious reuse — `TerminalSessionService` / `ConnectionService`'s terminal RPCs — **cannot carry
git**, for three independent reasons, each of which silently corrupts a packfile rather than
failing loudly:

1. **A prologue is injected before any payload byte.** `open_replay_ack_live`
   (`packages/tddy-terminal-rpc/src/bridge.rs:190`) emits the capture's mouse-tracking DECSET
   sequence as the first frame. Git would read those escape bytes as pkt-line data.
2. **Output is replayed from a lossy ring.** The live bridge reads a `broadcast::Receiver`
   (`bridge.rs:262`, `:324`) that drops on `Lagged`, and the capture ring evicts old bytes. A
   terminal tolerates a dropped repaint; a packfile does not.
3. **The transport is a PTY.** The line discipline rewrites `\n` → `\r\n`, interprets `^C`/`^D`,
   and echoes. `git-upload-pack` output is binary.

Beyond corruption, the terminal proto carries **no exit code** and **no separate stderr** — git
needs both: it reports the remote's failure status, and git sends progress ("Counting objects…")
on stderr while the pack goes down stdout.

So the wire format is new. What **is** reused is the client scaffolding: `run_livekit_terminal`
(`packages/tddy-tools/src/pty_relay.rs:343-478`) already joins a room, waits for a named
participant, opens a bidi stream and pumps local stdio, and `PtyRelayArgs` (`pty_relay.rs:40-129`)
is already the LiveKit CLI credential set.

## Acceptance Criteria

### Client — git's SSH argv contract

1. Invoked as git invokes an SSH command — `tddy-remote-git-repo <host> "git-upload-pack 'my-app'"` —
   the client resolves daemon instance `<host>`, verb `git-upload-pack`, project ref `my-app`.
2. The command argument is **shell-dequoted** the way `ssh` receives it: `'my app'` → `my app`,
   `'\''` → `'`. A leading `/` on the project ref is stripped, matching scp-style URL semantics
   (`host:/my-app` and `host:my-app` name the same project).
3. `<host>` may carry an ignored `user@` prefix (`git@udoo-1:my-app` resolves daemon `udoo-1`), so
   habitual `git@` remotes work. SSH options git may place ahead of the host are handled explicitly
   rather than mistaken for it:
   - `-o <setting>` is **accepted and ignored** — it configures ssh behavior with no analogue here.
     This is what makes the shim work whichever SSH variant git selects: git's protocol-v2 probe
     (`-o SendEnv=GIT_PROTOCOL`) is silently dropped, and the session negotiates v0/v1.
   - **Every other leading `-…` is rejected by name** — `-p <port>`, `-4`, `-i`, anything. A daemon
     instance id is the whole address, so silently dropping a port would connect somewhere the user
     did not ask for; and treating an unrecognised option as the host shifts every argument along,
     so the refusal would name the daemon as the command and never mention the option at all.
4. A command outside the whitelist — anything that is not `git-upload-pack`, `git-receive-pack`,
   `git upload-pack` or `git receive-pack` — is **refused before any network call**, exits `128`,
   and names the rejected command on stderr. This binary is a git shell, not a shell.
5. A missing daemon URL or credential token exits `255` (ssh's transport-failure code) and names the
   missing flag *and* its environment variable; so does a value that cannot be read as what it must
   be (an unparsable `TDDY_CONNECT_TIMEOUT_SECS` is refused, never replaced by the default). No
   partial connect, and no exit code git cannot interpret — a command line clap itself rejects also
   exits `255`, not clap's default `2`.

   The client holds **no LiveKit credential**. It asks the daemon for a room JWT
   (AC5a) and is told the URL and room along with it. `--help` prints the *name* of each
   credential's environment variable and never its value.

5a. **The daemon mints the LiveKit room token.** `auth.LiveKitTokenService/MintLiveKitToken` takes
   a `session_token` and nothing else:
   - The token is verified by the same resolver every other RPC uses (AC8); anything that is not a
     live access token is `UNAUTHENTICATED`.
   - A login with no `users:` mapping is `PERMISSION_DENIED`.
   - The room is the daemon's configured `livekit.common_room` — there is no request field for it.
   - The participant identity is generated server-side as `remote-git-<uuid>`. A client cannot ask
     for `daemon-<id>`, which is how a daemon's RPC-serving participant is addressed and would let
     a client be sent other participants' calls.
   - The JWT lives one hour, ample for one git operation.
   - A daemon with no `livekit.url` / `api_key` / `api_secret` / `common_room` answers
     `FAILED_PRECONDITION`. It does not mint against a default.
6. The remote command's exit code becomes the client's own exit code, so git sees the true remote
   status; transport, auth and resolution failures exit `255`.

### Server — authorization and resolution

7. `Serve`'s first frame must carry `open`; a first frame without it is `INVALID_ARGUMENT` and no
   process is spawned.
8. An absent, malformed, expired, or refresh-kind `session_token` is `UNAUTHENTICATED`; no process
   is spawned. (Same resolver as every other daemon RPC — `packages/tddy-daemon/src/auth.rs:117`.)
9. A valid token whose GitHub login has no `users:` mapping is `PERMISSION_DENIED`; no process is
   spawned.
10. `project_ref` resolves against **that OS user's own** `~/.tddy/projects/projects.yaml` — by
    `project_id` first, then by `name`. No match is `NOT_FOUND`; no process is spawned.
11. **The repository path is never taken from the client.** It is read from the resolved project's
    `main_repo_path`. An `open` whose `project_ref` is an absolute path (`/etc`, `../../etc`) is
    treated as a name lookup and yields `NOT_FOUND` — there is no path traversal surface.
12. The `verb` is **re-validated server-side** against the same closed whitelist; a verb outside it
    is `PERMISSION_DENIED` and no process is spawned. The client-side check in AC4 is a fast path,
    never the gate.
13. The git child runs **as the project's OS user**, dropping privilege via `setpriv` when the
    daemon's own identity differs (reusing `pty_runtime::wrap_argv_for_privilege_drop`,
    `pty_runtime.rs:181`). It is spawned directly — never through a shell — with `cwd =
    main_repo_path`.
14. A resolved project whose `main_repo_path` does not exist is `FAILED_PRECONDITION`; no process is
    spawned.

### Server — byte fidelity and lifecycle

15. Bytes are relayed **verbatim**: the client's stdin reaches the child's stdin and the child's
    stdout reaches the client's stdout with no prologue, no newline translation, and no loss. A
    clone of a repository containing binary content produces an identical object database.
16. The child's stderr is delivered in its **own** frame field and is written to the client's
    stderr, never interleaved into the stdout byte stream.
17. Output frames are chunked to at most 32 KB of payload so a frame never reaches
    `tddy_livekit::chunking::MAX_CHUNK_FRAME_BYTES` (60 000) and is never chunk-framed on the wire.
18. The child's exit status is delivered in a final frame carrying `done = true`, and that frame is
    the last thing the stream emits.
19. **When the connection ends, the child is dropped.** A client disconnect, a LiveKit participant
    departure, or a closed inbound stream terminates the git child (SIGTERM, then SIGKILL after a
    grace period) and reaps it. The signal goes to the child's **process group**, not just the
    process the daemon spawned: `git-upload-pack` forks `pack-objects`, and a grandchild that
    outlived the connection would hold the repository's object database open indefinitely. No
    orphan survives a dropped connection — this is the deliberate difference from the terminal
    path, which leaves its PTY running on disconnect by design
    (`packages/tddy-daemon/src/cli_session_manager.rs:1093`).

### End to end

20. `git clone` against a daemon project through `GIT_SSH_COMMAND` produces a working copy whose
    `HEAD` commit equals the project repository's.
21. `git fetch` after a new commit lands upstream brings that commit down.
22. `git push` of a **new** branch creates that branch in the daemon-side repository.
23. `git push` to the branch **currently checked out** in the daemon-side repository is refused with
    git's own `denyCurrentBranch` error, surfaced verbatim to the client. The daemon does not
    override `receive.denyCurrentBranch`, and there is no fallback that would let a push silently
    rewrite an agent's working tree.

## Wire contract

New service, registered alongside the daemon's existing entries in the common room (and therefore
also over the local socket and Connect-HTTP):

```proto
package remote_git;

service RemoteGitService {
  rpc Serve(stream GitClientFrame) returns (stream GitServerFrame);
}

message GitOpen {
  string session_token = 1;  // daemon access token; same one every other RPC carries
  string project_ref   = 2;  // project_id or project name — NEVER a filesystem path
  string verb          = 3;  // "git-upload-pack" | "git-receive-pack"
}

message GitClientFrame {
  GitOpen open      = 1;  // first frame only; ignored on later frames
  bytes   stdin     = 2;
  bool    stdin_eof = 3;  // child's stdin is closed; git needs the EOF to finish
}

message GitServerFrame {
  bytes stdout    = 1;
  bytes stderr    = 2;
  int32 exit_code = 3;  // meaningful only on the final frame
  bool  done      = 4;  // final frame
}
```

## Credentials

Every parameter is a CLI flag with an environment-variable fallback, so one
`GIT_SSH_COMMAND` (or a plain exported environment) serves every remote.

| Flag | Environment | Default |
|------|-------------|---------|
| `--daemon-url` | `TDDY_DAEMON_URL` | — (required) |
| `--session-token` | `TDDY_SESSION_TOKEN` | — |
| `--refresh-token` | `TDDY_REFRESH_TOKEN` | — |
| `--connect-timeout-secs` | `TDDY_CONNECT_TIMEOUT_SECS` | `30` |

`--daemon-url` is the daemon's Connect-HTTP root (the one `/rpc/…` hangs off). It has no default:
a default would silently point a misconfigured remote at some other daemon, and that failure reads
as "no such project" rather than "you did not say which daemon".

Exactly one of `--session-token` / `--refresh-token` is required. A daemon **access** token lives
5 minutes ([session-auth.md](session-auth.md)) — too short for a credential you configure once — so
the client also accepts the **7-day refresh token** and exchanges it for an access token via
`auth.AuthService/RefreshSession` before anything else runs.

There is deliberately **no LiveKit setting here at all**. `LIVEKIT_API_SECRET` is the same value a
daemon signs session tokens with (`packages/tddy-daemon/src/auth.rs`, `tddy_github::
SessionTokenSigner`), so a client that held it could mint an access token for *any* GitHub user on
the fleet — which would reduce every `session_token` check in the daemon to decoration. The client
therefore asks the daemon for a room JWT:

```proto
service LiveKitTokenService {          // package auth
  rpc MintLiveKitToken(MintLiveKitTokenRequest) returns (MintLiveKitTokenResponse);
}

message MintLiveKitTokenRequest  { string session_token = 1; }
message MintLiveKitTokenResponse {
  string token = 1;  string url = 2;  string room = 3;  uint64 ttl_seconds = 4;
}
```

The whole flow is: (refresh token → `RefreshSession`) → `MintLiveKitToken` → join the room the
response names → open `Serve`. The first two legs are Connect-HTTP; only the third is LiveKit.

## Trust model

LiveKit room membership defines the peer group as in
[livekit-peer-discovery.md](livekit-peer-discovery.md) § Trust model — but **this client is not
given the credential that grants membership**, and that is the difference from every other peer on
the room. A daemon access token is the only thing it holds; room admission is derived from it, by
the daemon, per operation.

Four properties keep this from being a remote shell, or a way in:

- The `session_token` is the actual authorization, re-validated on every `Serve` open (AC8) *and*
  again on every mint (AC5a). Both use the same resolver, so they cannot drift apart.
- The room a minted JWT grants is the daemon's own `common_room`, and the identity it carries is
  server-generated `remote-git-<uuid>`. A client cannot choose either, so it cannot join as
  `daemon-<id>` and be handed another participant's calls (AC5a).
- The verb whitelist is **closed** and enforced server-side (AC12).
- The repository path comes from the daemon's own registry, never from the client (AC11).

The second property is fleet-wide, not local to this client. `token.TokenService/GenerateToken`
(`packages/tddy-service/src/token_service.rs`) is the *other* mint — the one the web UI joins
presenter, session and lobby rooms through, which takes room and identity from the request. It used
to be unauthenticated, which meant anything that could reach a daemon's `/rpc` could obtain a
`daemon-*` identity and be handed other participants' calls, undoing AC5a's guarantee for everyone
else on the room. Two changes close that:

- Its requests carry a `session_token`, and the daemon's registration
  (`tddy_daemon::auth::build_token_service_entry`) verifies it with the same resolver used above —
  so `MintLiveKitToken` and `GenerateToken` cannot drift apart on *who* may mint.
- The service itself refuses any identity beginning `daemon-`
  (`tddy_service::RESERVED_DAEMON_IDENTITY_PREFIX`, the constant `daemon_rpc_identity` composes
  from), on **every** registration — including the session coder's, which is unauthenticated by
  design because a coder holds no session-token signer.

> **Residual gap, recorded rather than closed.** An *authenticated* caller may still name any room
> on `GenerateToken`, and a session coder's own registration remains open to whatever can reach it.
> Authorizing which rooms a given user may join needs a room-ownership model that does not exist
> today; see `docs/dev/TODO.md` § Remote git repo over LiveKit for the reasoning and what closing
> it would take. Impersonating a daemon — the vector that defeated AC5a — is closed.

## Shipping

`tddy-remote-git-repo` is built by `./release`, installed by `./install` into `INSTALL_BIN_DIR`
(default `/usr/local/bin`, or `~/.local/bin` under `--user`), and packaged by `./publish.sh` into
the daemon's `.deb` at `/usr/bin/tddy-remote-git-repo`. `GIT_SSH_COMMAND=tddy-remote-git-repo`
therefore resolves on any host that has had one of those run.

**Decision: it ships in the daemon's `.deb`, even though it is a client.** This is a judgement
call, and the argument against it is real — the binary runs on the developer's machine, which is by
definition *not* the host it dials, so bundling it with a daemon package installs a client on hosts
that may never invoke it. Three things settle it the other way:

- **A client with no shipping path is a client nobody has.** It is not an ACP-style plugin the
  daemon execs; nothing in the daemon references the path. If no script installs it, the only way to
  obtain it is a repo checkout plus a Rust toolchain — which is the whole audience the feature is
  meant to reach turned away at the door.
- **The daemon host is a plausible client of itself and of its peers.** An operator on a
  daemon-managed host wants `git clone` against the fleet as much as anyone; the projects are right
  there.
- **The cost is one static binary.** It carries no config, writes nothing, opens no port, and reads
  nothing `install` renders — an unused copy on a server is inert.

A separate client-only package (`tddy-client`, say) would be the tidier answer once there is a
second client-side binary to put in it. Until then, one more file in an existing package beats a
second package for a single executable.

## Throughput

**Measured: 150 MiB cloned in 59 s — 2.54 MiB/s — every byte intact.** That is the honest number to
plan against, not an estimate: `clones_a_large_repository_with_every_byte_intact` runs a real `git
clone` of incompressible content through the real binary over a real LiveKit server, and is
`#[ignore]`d so it is re-measurable on demand without loading the default suite. Size it with
`TDDY_REMOTE_GIT_THROUGHPUT_BYTES`.

The rate is **bound by the LiveKit data channel**, not by anything in this feature: raising
`GIT_FRAME_CHANNEL_CAPACITY` eightfold (8 → 64) moved it not at all (59.1 s vs 59.0 s). So the
bounded channel that keeps a large clone from becoming a large RSS costs nothing in speed, and
there is no cheap tuning win here — a materially faster transfer would need a different carrier,
not a bigger buffer.

What this means in practice: a typical source repository (tens of MB) clones in seconds to a
minute; a multi-gigabyte monorepo is measured in tens of minutes and is better served by cloning
once and fetching thereafter. This is roughly an order of magnitude slower than a LAN clone over
SSH — which is the trade the feature exists to make, since its whole premise is hosts you cannot
reach by SSH at all.

## Non-goals

- **An interactive shell.** Only the two git pack verbs are servable. `tddy-remote-git-repo` is
  named for what it serves.
- **Git protocol v2.** The shim ignores `-o SendEnv=GIT_PROTOCOL` rather than forwarding the
  variable, so the daemon never sees it and serves v0/v1. Functionally complete, just less
  efficient on ref advertisement. Supporting v2 means threading the client's `GIT_PROTOCOL` into
  the child's environment.
- **A port in the remote URL.** A daemon instance id is the address, not a host:port; `-p` is
  refused rather than ignored.
- **A CLI login flow.** The refresh token is obtained from the web UI (`localStorage`
  `tddy_refresh_token`). A `tddy-tools auth login` device-code flow is tracked in
  `docs/dev/TODO.md`.
- **Bare-repository projects.** A project's `main_repo_path` is a non-bare clone; AC23 is the
  consequence, not a limitation to work around.
- **Serving a project on a peer daemon.** The client addresses one daemon directly; there is no
  `StartSession`-style peer forwarding on this path.

## Related documentation

- [Project concept](project-concept.md) — the project registry this resolves against.
- [Cross-daemon session authentication](session-auth.md) — the token model and its TTLs.
- [LiveKit peer discovery](livekit-peer-discovery.md) — daemon identity, common room, trust model.
- [Multiple tools per session](terminal-sessions.md) — the terminal RPC surface this deliberately
  does not reuse, and why.
