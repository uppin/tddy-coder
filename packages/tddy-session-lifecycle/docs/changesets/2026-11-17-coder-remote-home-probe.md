# 2026-11-17 — A Coder workspace's home is read through its pty without the pty's words

**Type:** Fix · PR [#938](https://github.com/uppin/tddy-coder/pull/938)

The stub shell behind the CLI-spec harness answers the home probe the way a real shell answers the
new command: the probe's sentinels around the home, framed as a pty delivers them. Full record:
[`docs/dev/changesets/2026-11-17-coder-remote-home-probe.md`](../../../../docs/dev/changesets/2026-11-17-coder-remote-home-probe.md).
