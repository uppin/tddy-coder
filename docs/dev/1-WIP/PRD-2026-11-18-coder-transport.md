# PRD: A Coder command ends when its caller stops waiting, never pages, and streams byte-exact

**Date**: 2026-11-18
**Status**: 🚧 In Progress
**Stack**: `#coder-start` 5/10 — branch `feature/coder-start/coder-transport`, base `feature/coder-start/sweep` (#944)

## Problem

`coder ssh` always gives the command a pty (`coder-ssh-pty-facts`). #939 made one-shots pty-safe;
three exposures stayed, each found live on `mantasi/remote-dev` on 2026-11-18:

1. **A one-shot past its deadline keeps running on the workspace.** `CoderShell::exec` drops the
   local `coder ssh` at the deadline (`kill_on_drop`); the remote `sh` and its children are never
   signalled. The smoke test's hung writes left two `sh`/`cat` pairs alive for hours.
2. **A one-shot pages.** Its stdout is a tty, so `git log -1` came back wrapped in `less`'s escapes,
   and a pager waiting for a key holds the command until its deadline.
3. **A streamed command crosses the pty raw.** `exec_streamed` hands the caller `coder ssh`'s own
   pipes: closing stdin is never an EOF, output gets `onlcr`, input passes canonical mode. The hook
   follower tails its log this way on every Coder target; git pack verbs would break the same way.

## Requirements

1. A one-shot that outlives its deadline is stopped **on the workspace** too, within a bound a little
   inside the local deadline, and the caller still gets `TimedOut`.
2. Every one-shot runs with `PAGER=cat`, `GIT_PAGER=cat` and `GIT_TERMINAL_PROMPT=0`.
3. `CoderShell::exec_streamed` runs OpenSSH with `ProxyCommand=<coder> ssh --stdio --disable-autostart
   <workspace>`: a real pipe with stdin EOF, raw bytes both ways, separate stderr and the command's
   exit status. The deployment URL and token still reach only the `coder` process, through its
   environment, never an argv.
4. `exec`, `read_file`, `write_file` and `interactive` keep their transport.

## Acceptance criteria

- [ ] A one-shot whose command outlives its deadline is `TimedOut`, and its command is no longer
      running on the stand-in workspace shortly after
- [ ] A one-shot sees `PAGER`, `GIT_PAGER` = `cat` and `GIT_TERMINAL_PROMPT` = `0`
- [ ] `CoderShell` passes the shell contract's streamed case against a stand-in `coder` whose `coder
      ssh` gives no EOF and translates newlines (as the real one does), and a binary round trip of
      all 256 byte values
- [ ] The streamed call names `coder ssh --stdio --disable-autostart <workspace>` as its proxy; the
      credentials are in the proxy's environment and on no argv
- [ ] Scoped gates: `./test -p tddy-remote-shell`, clippy `-D warnings`

## Out of scope

- `SshShell`'s deadline (no pty; its remote side gets a hangup) — unchanged.
- The agent's path, env and trust on a target (`#coder-start` 7/10).
