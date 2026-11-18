# 2026-11-18 — A Coder command ends with its caller, never pages, and streams byte-exact

**Type:** Fix

- A one-shot runs as its own process group under a POSIX `sh` watchdog: past its deadline the
  workspace's command gets `TERM`, then `KILL` 2 s later (exit 143), instead of outliving the dropped
  `coder ssh`. The watchdog signals with `kill -s TERM -- -<pgid>`, the form dash accepts.
- A one-shot exports `PAGER=cat GIT_PAGER=cat GIT_TERMINAL_PROMPT=0`.
- `exec_streamed` runs OpenSSH over `ProxyCommand='<coder> ssh --stdio --disable-autostart <workspace>'`:
  plain pipes, so stdin's end is an EOF, bytes are exact and stderr is separate. `ssh` runs with
  `LogLevel=ERROR`, `StrictHostKeyChecking=no` and `UserKnownHostsFile=/dev/null`; credentials are on
  the `ssh` child's environment only. `CoderShell::with_ssh_binary` names the `ssh` to run.
- `exec`, `read_file`, `write_file` and `interactive` keep the pty.

Run against `mantasi/remote-dev`. See [architecture.md](../architecture.md). Full record:
[`docs/dev/changesets/2026-11-18-coder-transport.md`](../../../../docs/dev/changesets/2026-11-18-coder-transport.md).
