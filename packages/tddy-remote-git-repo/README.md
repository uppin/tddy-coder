# tddy-remote-git-repo

Serve a **tddy-daemon project as a git remote** over LiveKit. No SSH daemon, no open port, no VPN —
if you can join the daemon's LiveKit room, you can clone from it.

Git already knows how to run an external command as its transport. This binary is that command: it
plays the role `ssh` plays in git's SSH transport, carrying `git-upload-pack` / `git-receive-pack`
over a `remote_git.RemoteGitService` bidi stream to the daemon, which runs the real git command
against the project's checkout as the project's OS user.

Feature: [docs/ft/daemon/remote-git-repo.md](../../docs/ft/daemon/remote-git-repo.md)

## Where the binary comes from

`GIT_SSH_COMMAND` is looked up on `PATH` like any other command, so the binary has to be installed
before the usage below works:

| How you got tddy | Where `tddy-remote-git-repo` lands |
|------------------|------------------------------------|
| `sudo ./install --systemd` from a checkout | `INSTALL_BIN_DIR`, default `/usr/local/bin` |
| `./install --systemd --user` | `INSTALL_BIN_DIR`, default `~/.local/bin` |
| the `tddy` `.deb` (`./publish.sh`) | `/usr/bin` |

Built it yourself instead? `./release` (or `cargo build --release -p tddy-remote-git-repo`) leaves
it at `target/release/tddy-remote-git-repo`, which is not on anyone's `PATH` — point
`GIT_SSH_COMMAND` at the absolute path, or copy it somewhere that is:

```bash
export GIT_SSH_COMMAND="$PWD/target/release/tddy-remote-git-repo"
```

## Usage

```bash
export GIT_SSH_COMMAND=tddy-remote-git-repo   # installed on PATH — see above
export TDDY_DAEMON_URL=http://udoo-1.example:8899
export TDDY_REFRESH_TOKEN=…          # from the web UI's localStorage.tddy_refresh_token

git clone udoo-1780828020298:my-app
git -C my-app fetch
git -C my-app push origin feat/my-branch
```

Two settings, both about the daemon: where it is, and who you are. **No LiveKit credential of any
kind.** The daemon mints the room JWT itself (`auth.LiveKitTokenService/MintLiveKitToken`) and
returns the LiveKit URL and room along with it. That is not a convenience — `LIVEKIT_API_SECRET`
is also the HMAC key every daemon signs session tokens with, so anyone holding it could mint an
access token for any GitHub user on the fleet.

The remote is `<daemon-instance-id>:<project>`:

- **`<daemon-instance-id>`** is the daemon's `daemon_instance_id`; its LiveKit participant is
  `daemon-{instance_id}`. A `user@` prefix is accepted and ignored, so `git@udoo-1:my-app` works.
- **`<project>`** is the project's `name` or `project_id` in that OS user's
  `~/.tddy/projects/projects.yaml`. It is **not** a filesystem path — the daemon resolves the
  repository location from its own registry, so nothing you put here can select a directory the
  registry does not already name.

Settings can equally be flags on the `GIT_SSH_COMMAND` string, which is useful when different
remotes live on different daemons:

```bash
git config --local core.sshCommand \
  "tddy-remote-git-repo --daemon-url http://udoo-1.example:8899 --refresh-token …"
```

## Options

| Flag | Environment | Default |
|------|-------------|---------|
| `--daemon-url` | `TDDY_DAEMON_URL` | — (required) |
| `--session-token` | `TDDY_SESSION_TOKEN` | — |
| `--refresh-token` | `TDDY_REFRESH_TOKEN` | — |
| `--connect-timeout-secs` | `TDDY_CONNECT_TIMEOUT_SECS` | `30` |

`--daemon-url` has no default on purpose. A default would point every misconfigured remote at some
other daemon and fail there, which reads as "that project does not exist" rather than "you did not
say which daemon".

Exactly one of `--session-token` / `--refresh-token` is required. A daemon **access** token lives
5 minutes, which is too short for a credential you configure once, so a **refresh** token (7 days)
is exchanged for one via `auth.AuthService/RefreshSession` first.

`--help` never prints the *value* of a token it found in the environment, only the variable's name.

## What it talks to

| Leg | Call | Why |
|-----|------|-----|
| HTTP | `auth.AuthService/RefreshSession` | refresh token → access token (skipped with `--session-token`) |
| HTTP | `auth.LiveKitTokenService/MintLiveKitToken` | access token → room JWT, LiveKit URL, room |
| LiveKit | `remote_git.RemoteGitService/Serve` | the git stdio stream itself |

## Exit codes

Mirrors `ssh`, because that is what git interprets:

| Code | Meaning |
|------|---------|
| *remote's own* | The git command ran; this is its exit status |
| `128` | The request was malformed or refused locally — a command outside the git pack whitelist, a missing host, an ssh option with no analogue here |
| `255` | Transport, credential or authentication failure — the remote was never reached. Also what an unusable command line exits with, rather than clap's `2`, which git reads as nothing in particular |

## What it will not do

Only `git-upload-pack` and `git-receive-pack` are servable, and the whitelist is enforced **on the
daemon**, not here. This is a git shell, not a shell — the name is the contract.

Pushing to the branch the daemon-side repository has checked out is refused by git's own
`receive.denyCurrentBranch`, surfaced verbatim. The daemon does not override it: an agent may be
working in that tree.

## Modules

| Module | Role |
|--------|------|
| `ssh_argv` | Git's SSH argv contract — host, shell-dequoted command, verb whitelist |
| `credentials` | Flags with per-parameter environment fallback |
| `daemon_rpc` | The daemon's Connect-HTTP leg — token exchange and room-token mint |
| `relay` | Join the room, open `Serve`, pump stdio, exit with the remote's status |
