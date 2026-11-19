# tddy-remote-shell architecture

## Overview

The remote-shell port: how the daemon runs something on a machine it does not serve. One trait,
`RemoteShell`, with three shapes of "run this there", a typed error, and OpenSSH as its first
transport. Every place the daemon runs a command on an SSH target goes through this crate; no
other crate spawns `ssh` to run a command on a target (enforced by
`tests/one_ssh_spawn_site.rs`, with two listed exemptions: a local `ssh -V` version probe and
the QEMU guest path, neither of which runs a command on a session's target).

## The port

- **`RemoteShell`** — `address()`, `exec(ExecRequest)`, `exec_streamed(command)`,
  `interactive(command)`, and the provided `read_file` / `write_file` built on `exec`.
  A non-zero exit is an `Ok(ExecOutput)`; a caller that needs success asks
  `ExecOutput::success_or_exited()`.
- **`ShellAddress`** — a transport tag (a string, so a later transport adds its tag without
  editing the type) plus the target's name on that transport, for errors and logs.
- **`ExecRequest`** — a command line, optional stdin bytes, and a deadline
  (`DEFAULT_EXEC_DEADLINE`, 60 s) bounding the whole call, connecting included.
- **`StreamedExec`** — boxed `AsyncWrite` / `AsyncRead` pipes and an exit future, not
  `tokio::process` child handles, so a transport that is not a local child can produce one.
- **`InteractiveCommand`** — the argv and environment a **local** PTY runs to be the remote
  command's terminal. The port opens no PTY of its own.
- **`RemoteShellError`** — `Unreachable`, `TimedOut`, `Exited`, `Spawn`: which of four things
  happened, each carrying the address it happened on.

## The SSH transport

`SshShell::new(alias)` / `through(ssh_binary, alias)` runs the host's OpenSSH client, so
`~/.ssh/config` and ssh-agent apply. Every call carries one option set (`ssh_options()`):
`BatchMode=yes`, `ConnectTimeout=15`, `ServerAliveInterval=5`, `ServerAliveCountMax=3` — an
unloaded key is a hard failure, never a prompt, and a connection cannot hang. OpenSSH's exit
255 is `Unreachable`; a command past its deadline is stopped and reported `TimedOut`.
`interactive` is `ssh -tt <options> -- <alias> <command>`.

## The Coder transport

`CoderShell::new(credentials, workspace)` / `through(coder_binary, …)` runs the host's
`coder` CLI: `coder ssh --disable-autostart <owner>/<name>[.<agent>] -- sh -c '<command>'`,
with `CODER_URL` and `CODER_SESSION_TOKEN` set on that child alone and scrubbed from every
probe the daemon runs as another user. The workspace is never started; one that is not
running is `Unreachable` with what the CLI said, told from a failed command by a start
marker the one-shot wrapper (`coder/one_shot.rs`) prints to stderr before the command runs. `coder ssh` runs
the command in a pty, which merges its stderr into its stdout, so the marker is
recognised on either stream (with the pty's `\r\n`) and stripped from the stream it
arrived on; only a non-zero exit with the marker on neither stream is `Unreachable`.
Value-returning probes (the target's home, in `tddy-git`) frame their value with
sentinels and parse it out, so merged transport words never enter the value.

A one-shot never relies on that pty, which turns no end of the CLI's stdin into an EOF and
rewrites every `\n` the command prints as `\r\n` (`onlcr`):

- **Output** — the wrapper runs `stty -opost` first, so the command's bytes arrive as
  written.
- **No stdin** — the shell's stdin is `/dev/null`, so a command that reads it reads an
  empty one.
- **Stdin** — sent as base64 lines of 76 characters, each ending `\n`: no character a tty
  acts on (`^D`, `^C`, `\r`), every line far inside the canonical-mode limit. The workspace
  reads exactly that many bytes (`head -c`, so no EOF is needed), decodes them into a
  `mktemp` file, checks it holds the original length, makes it the shell's stdin and
  unlinks it before the command runs. A stdin that did not arrive whole runs nothing and
  exits 125, saying so. Echo is turned off before the start marker is printed and the
  lines are written only once the marker has arrived, so none is echoed into the output.
  The bytes travel on stdin, never on a command line: `write_file` carries the session
  pointer, and with it the hook token.

A one-shot is also bounded on the workspace and pager-free. The wrapper exports `PAGER=cat`,
`GIT_PAGER=cat` and `GIT_TERMINAL_PROMPT=0`, then runs the command as `sh -c` in a process group of
its own (`set -m`) under a POSIX `sh` watchdog. After `ceil(deadline)` seconds from the command's
start the watchdog sends `TERM` to the group, and `KILL` 2 s later. A command it ends exits 143, and
the watchdog never kills one that finished within the deadline. Dropping the local `coder` does not
stop what it started, so the caller's `TimedOut` and the workspace's cleanup come from different
clocks that agree to within the connect time. The watchdog needs nothing beyond `sh`: coreutils
`timeout` is not on every workspace. It signals with `kill -s TERM -- -<pgid>`, the one form both
dash (the usual workspace `/bin/sh`) and bash accept; `kill -TERM -- -<pgid>` fails in dash and
leaves the watchdog holding the connection open. Job-control notices go to `/dev/null`, with the
command keeping the original stderr as fd 3.

`exec_streamed` does **not** use the pty. It runs the host's OpenSSH client with
`-o ProxyCommand='<coder> ssh --stdio --disable-autostart <workspace>'` and the fixed host token
`coder-workspace` (the proxy reaches the workspace, the name resolves nothing). The CLI's `--stdio`
mode is a plain pipe, so the command's bytes arrive as written, its stderr is separate, closing
stdin is an EOF and its exit status is the command's. `ssh` carries the shared `ssh_options()` plus
`StrictHostKeyChecking=no`, `UserKnownHostsFile=/dev/null` (the CLI authenticates the workspace by
token, not by host key) and `LogLevel=ERROR` (so ssh's own known-hosts warning stays off the
command's stderr). `CODER_URL` and `CODER_SESSION_TOKEN` are on the `ssh` child's environment
alone, which the proxy inherits; neither is on any argv. `CoderShell::with_ssh_binary` names the
`ssh` to run (default: `ssh` on `PATH`). `exec`, `read_file`, `write_file` and `interactive` keep
the pty route.

`interactive` is `coder ssh --disable-autostart <workspace> -- <command>` for a local PTY. Credentials are
held in memory for the shell's life; `Debug` shows the URL and never the token.

## Test support

Behind the `test-support` feature: `InMemoryShell`, a scriptable in-memory double, and
`contract::assert_behaves_like_a_remote_shell`, the behaviour suite every transport is held
to. The suite runs against `InMemoryShell`, `SshShell` driving a stand-in `ssh`
executable, and `CoderShell` driving a stand-in `coder` executable; no real SSH server, no
Coder deployment and no network are involved. The Coder suite's stand-ins also play the
real CLI's pty: one merges stderr into stdout, one never ends the command's input (its
stdin is a FIFO the command holds open), and one ends every line with `\r\n` until a
stand-in `stty -opost` turns that off.

## Consumers

- `tddy-tool-engine`: the exec catalog's `RemoteShell` wraps an `Arc<dyn RemoteShell>`;
  `session_shell` builds an `SshShell` for a set `ssh_config_host`.
- `tddy-git`: `setup_remote_worktree` and `remote_checkout_root` take `&dyn RemoteShell`; a target's checkout is `$HOME/.tddy/repos/<repository>/.worktrees/<session>` on the target, resolved through its shell.
- `tddy-agent-launch`: slot files and the session pointer are written through the port.
- `tddy-worktree-service`: remote git pack verbs stream through `exec_streamed`.
