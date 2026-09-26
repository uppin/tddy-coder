# 2026-09-26 — One contained spawn, and a `Read` that honours its window

**Type:** Bug Fix

`contained_shell` is the one way this crate starts a process — the blocking `Shell`, a background
`ShellTaskBody`, `LocalShell::run`, and `Grep`'s `rg`. Each child gets `/dev/null` for standard
input and its own process group, and a command that overruns its budget has the whole **group**
signalled (`SIGTERM`, then `SIGKILL`).

Both halves were a real cost: `tokio::process::Command::output()` leaves stdin inherited, unlike
its `std` counterpart, so a jailed command reading standard input became a rival reader on
`tddy-sandbox-runner --stdio`'s tool-IPC request pipe; and `tokio::time::timeout` only stops
waiting, so the orphan outlived the call that was told it timed out.

`Read` honours the `offset`/`limit` the catalog has advertised since it was written, returning
`{content, truncated, total_lines}`. No default cap here: a bare `Read` returns the whole file byte
for byte, trailing newline included. The 200-line cap that bounds a *subagent's* context stays one
layer up in `tddy_discovery::subagent`.

[README.md](../../README.md) §§ Tools, Every child process is contained · [../../../../docs/dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md](../../../../docs/dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md)
