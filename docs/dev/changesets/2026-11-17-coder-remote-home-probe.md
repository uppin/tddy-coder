# 2026-11-17 — A Coder workspace's home is read through its pty without the pty's words

**Type:** Fix · PR [#938](https://github.com/uppin/tddy-coder/pull/938)

Root cause of every `mantasi/remote-dev` start failing with `remote worktree setup failed: coder
target … is unreachable: … Process exited with status 1` (diagnostics §8, verified):
`coder ssh` runs the remote command in a pty, which merges the command's stderr into its stdout.
The daemon's home probe therefore captured
`tddy-coder-shell: the command is running\r\n/home/coder` as the home; the next command,
`test -d '<marker>\r\n/home/coder/.tddy/repos/qape-hq/.git'`, exited 1; and because the one-shot
start marker was looked for on stderr alone, that exit was misreported as the workspace being
unreachable.

**The start marker is recognised on either stream** (`tddy-remote-shell`): present on stdout — the
pty-merged case, with the `\r\n` a pty writes — it is stripped from stdout and the command's
status is its own; only a non-zero exit with the marker on neither stream is `Unreachable`, and
its detail falls back to stdout when stderr is empty. The marker is kept: `coder ssh` exits 1 both
for a stopped workspace and for a command exiting 1, and the marker is what tells them apart.

**Value probes frame their value with sentinels** (`tddy-git`): `remote_checkout_root` prints
`TDDY_REMOTE_HOME_BEGIN:` / `:TDDY_REMOTE_HOME_END` around `$HOME` and parses the value out from
between them, so transport noise — a merged marker, a banner, carriage returns — never enters the
path. An answer without the sentinels is `HomeUnresolved` carrying the target's words, never a
guessed home.

**A remote command that ran and exited non-zero names its step** — `HomeUnresolved` / `Clone` /
`Fetch` / `Worktree`, the fetch case pinned by a test — and carries the target's stderr or output.
`Unreachable` is kept for connection failures only: no marker, non-zero exit.

**The `/home/dev/qape-hq` path-guess report** (diagnostics §1) is the same root, not a separate
defect: the `ssh_config_host` route resolves its checkout root through the same `$HOME` probe, and
no production source names `/home/dev` (pinned by
`tddy-git/tests/no_fixture_root_in_production.rs`), so the fixed probe covers it.

The pty merge itself stays open in
[`docs/dev/todo/2026-11-16-coder-hosts-never-run-against-a-real-deployment.md`](../todo/2026-11-16-coder-hosts-never-run-against-a-real-deployment.md):
it is now pinned by a stand-in (`a_coder_whose_workspaces_merge_stderr_into_stdout`) and by the
real deployment's diagnostics, but the four facts it lists still have no real-deployment run
behind them.
