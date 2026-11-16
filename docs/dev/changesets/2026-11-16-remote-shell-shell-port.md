# 2026-11-16 — One remote-shell port behind every SSH call

**Type:** Feature · `#remote-shell` 1/7, PR [#915](https://github.com/uppin/tddy-coder/pull/915), base `master`, wave 1 of 4

Every place the daemon runs something on a machine it does not serve goes through one port,
`RemoteShell`, in the new crate `tddy-remote-shell`. OpenSSH is its first transport. The four
existing SSH spawn sites move onto it and share one set of connection options and one typed
error. Behaviour on a healthy host is unchanged; an unreachable target now fails within the
connect timeout and names the cause.

## What changed

- **tddy-remote-shell** (new): `RemoteShell` with one-shot exec (stdin, deadline), streamed
  exec, and an interactive argv for a local PTY; `RemoteShellError::{Unreachable, TimedOut,
  Exited, Spawn}`; `SshShell` with a single `ssh_options()`; `InMemoryShell` and the contract
  suite behind `test-support`. Crate docs:
  [`packages/tddy-remote-shell/docs/architecture.md`](../../packages/tddy-remote-shell/docs/architecture.md).
- **tddy-tool-engine**: the exec catalog's `RemoteShell` wraps an `Arc<dyn RemoteShell>`;
  `session_shell` builds an `SshShell`.
- **tddy-git**: `ssh_batch_command` / `run_ssh_batch` deleted; worktree setup takes
  `&dyn RemoteShell`. The `ssh_exec` re-exports in **tddy-core** follow.
- **tddy-agent-launch**: `slot_file_over_ssh.rs` and `run_over_ssh` / `ssh_options` deleted;
  slot files and the session pointer are written through the port.
- **tddy-worktree-service**: `PackExecution::Remote` streams through `exec_streamed`;
  `ssh_batch_mode_argv` deleted.
- **Ratchet**: `one_ssh_spawn_site` — no crate other than `tddy-remote-shell` spawns `ssh`
  to run a command on a target.

## Verification

Scoped `./test -p` for the seven touched packages: 860 passed, 0 failed, 1 ignored
(2026-11-16). Scoped clippy with `-D warnings` and `cargo fmt` clean. New crate CRAP highest
8.0 (`SshShell::exec`, complexity 8, tested) — this PR adds no code issue.

## Code issues

- `packages/tddy-worktree-service/docs/code-issues/oversized-file-remote-git-service.md` —
  re-measured 1,004 → 1,111 production lines (worse: the streamed port path added more than
  the deleted spawn branch removed). **Not split here**: this PR's boundaries forbid it and
  successor PR #916 also touches the file, so splitting would cascade conflicts through the
  stack. The record carries the new measurement; the split is a follow-up after the
  `#remote-shell` stack lands.

## Backlog

No `docs/dev/todo/` entry is resolved here. The passphrase entry
`2026-10-17-daemon-git-over-ssh-beyond-the-clone-cannot-ask-for-a-passphrase.md` stays open:
`SshShell` keeps `BatchMode=yes`, and the port is the one place an askpass environment would
later be added.
