# 2026-11-18 — A Coder command ends when its caller stops waiting, never pages, and streams byte-exact

**Type:** Fix · `#coder-start` 5/10, PR [#945](https://github.com/uppin/tddy-coder/pull/945)
**Packages:** `tddy-remote-shell`

`coder ssh` always gives the command a pty. #939 made one-shots pty-safe; three exposures stayed, all
found live on `mantasi/remote-dev` on 2026-11-18.

- **A one-shot past its deadline kept running on the workspace.** `CoderShell::exec` dropped the local
  `coder ssh` at the deadline, and the remote `sh` and its children were never signalled. The wrapper
  now runs the command as its own process group under a POSIX `sh` watchdog that sends `TERM` to the
  group after `ceil(deadline)` seconds and `KILL` 2 s later. It needs nothing beyond `sh`
  (coreutils `timeout` is not on every workspace). A command it ends exits 143.
- **A one-shot paged.** The wrapper exports `PAGER=cat`, `GIT_PAGER=cat` and `GIT_TERMINAL_PROMPT=0`.
- **A streamed command crossed the pty raw**, so closing stdin was never an EOF, output got `onlcr`
  and stderr merged. `exec_streamed` now runs OpenSSH with
  `ProxyCommand='<coder> ssh --stdio --disable-autostart <workspace>'` (plain pipes) and the host
  token `coder-workspace`. The deployment URL and token reach only the `ssh` child's environment,
  which the proxy inherits, and no argv. New surface: `CoderShell::with_ssh_binary`.

`exec`, `read_file`, `write_file`, `interactive`, `SshShell` and the `RemoteShell` trait are
unchanged.

**Found by the live run, not by the stand-ins.** Two defects passed every stand-in test and failed
against the real workspace:

- dash, the workspace's `/bin/sh`, rejects `kill -TERM -- -<pgid>` ("Illegal number: -"). The
  watchdog then stayed alive holding the connection, so every one-shot hung to its deadline. The
  wrapper uses `kill -s TERM -- -<pgid>`, which dash and bash both accept, and a unit test pins the
  form. The stand-ins run macOS's bash, which accepts both.
- `ssh`'s own `Warning: Permanently added 'coder-workspace' ... to the list of known hosts.` landed on
  the streamed command's stderr. `LogLevel=ERROR` keeps it off.

**Pinned by** five tests in `tests/coder_shell_acceptance.rs` that failed before the fix (deadline
kill, pager environment, the streamed contract against pty-faithful stand-ins, a 256-byte-value round
trip, the proxy argv and credential placement), and the unit test above.

**Live check on `mantasi/remote-dev`** (real `CoderShell` against `coder` v2.33.5, 2026-11-18):

1. `echo $$ > f; exec sleep 67` within 10 s returned `TimedOut` after 10.00 s; the process was gone
   (`kill -0` failed) 4 s later. (67 s, not 60, so a concurrent session's `sleep 60` on the shared
   workspace could not be mistaken for it.)
2. `git -C ~ log -1 || git --version` came back with no escape byte; `$PAGER|$GIT_PAGER|$GIT_TERMINAL_PROMPT`
   printed `cat|cat|0`.
3. A streamed `cat` returned bytes 0..=255 unchanged after stdin was shut, and exited 0.
4. A streamed `cat; printf done >&2; exit 4` echoed its input, wrote `done` on stderr alone, and exited 4.
5. A `ps` on the workspace afterwards listed no leftover `sleep 67` and no watchdog `sleep`.

Backlog entries cleared: `coder-one-shot-deadline-leaves-the-remote-process`,
`coder-one-shots-start-pagers`, `coder-exec-streamed-crosses-the-pty-raw`.
