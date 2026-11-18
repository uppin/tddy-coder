# 2026-11-17 — A Coder workspace's home is read through its pty without the pty's words

**Type:** Fix · PR [#938](https://github.com/uppin/tddy-coder/pull/938)

`remote_checkout_root` frames the home probe's answer with sentinels (`TDDY_REMOTE_HOME_BEGIN:` /
`:TDDY_REMOTE_HOME_END`) and parses the value out from between them, so transport noise — a
merged marker, a banner, carriage returns — never enters the path; an answer without the
sentinels is `HomeUnresolved` carrying the target's words, never a guessed home. A remote command
that ran and exited non-zero names its step (`HomeUnresolved` / `Clone` / `Fetch` / `Worktree`)
and carries the target's words; `Unreachable` is for connection failures only. Full record:
[`docs/dev/changesets/2026-11-17-coder-remote-home-probe.md`](../../../../docs/dev/changesets/2026-11-17-coder-remote-home-probe.md).
