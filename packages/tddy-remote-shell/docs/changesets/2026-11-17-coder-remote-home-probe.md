# 2026-11-17 — A Coder workspace's home is read through its pty without the pty's words

**Type:** Fix · PR [#938](https://github.com/uppin/tddy-coder/pull/938)

The one-shot start marker is recognised on whichever stream it arrives on — `coder ssh` runs the
command in a pty, which merges stderr into stdout, so the marker arrives on stdout carrying the
pty's `\r\n` — and is stripped from the stream it arrived on. A command that ran keeps its own
status; only a non-zero exit with the marker on neither stream is `Unreachable`, its detail
falling back to stdout when stderr is empty. The marker is kept: `coder ssh` exits 1 both for a
stopped workspace and for a command exiting 1, and the marker is what tells them apart. Full
record:
[`docs/dev/changesets/2026-11-17-coder-remote-home-probe.md`](../../../../docs/dev/changesets/2026-11-17-coder-remote-home-probe.md).
