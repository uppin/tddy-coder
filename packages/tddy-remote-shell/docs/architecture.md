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

## Test support

Behind the `test-support` feature: `InMemoryShell`, a scriptable in-memory double, and
`contract::assert_behaves_like_a_remote_shell`, the behaviour suite every transport is held
to. The suite runs against both `InMemoryShell` and `SshShell` driving a stand-in `ssh`
executable; no real SSH server and no network are involved.

## Consumers

- `tddy-tool-engine`: the exec catalog's `RemoteShell` wraps an `Arc<dyn RemoteShell>`;
  `session_shell` builds an `SshShell` for a set `ssh_config_host`.
- `tddy-git`: `setup_worktree_for_session_over_ssh` takes `&dyn RemoteShell`.
- `tddy-agent-launch`: slot files and the session pointer are written through the port.
- `tddy-worktree-service`: remote git pack verbs stream through `exec_streamed`.
