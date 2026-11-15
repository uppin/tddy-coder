# 2026-11-15 — A split session's agent either comes up or the start fails with its own words

**Type:** Fix

A split session's agent terminal is recorded in its session directory, every ended `main`
terminal is logged with its real outcome and lifetime, the PTY runtime reports the child's
real exit code (a signal as a failure naming it), and a split start whose agent exits inside
the daemon's startup window fails — quoting the agent's last output — with the codebase half
torn down. Stack `#split-alive` 1/4 ([#905](https://github.com/uppin/tddy-coder/pull/905));
the product statement is in
[remote-managed-worktree.md § Session start](../../ft/daemon/remote-managed-worktree.md#session-start).

This closes fixes 1 and 2 of the backlog entry `2026-11-15-split-session-agent-not-running`
(deleted at this wrap). What remains of that entry is recorded in
`docs/dev/todo/2026-11-15-split-agent-exit-cause-unknown.md`: why the agent exits (fix 4) is
still unknown and now self-reporting, and fixes 3, 5 and 6–7 land in #906, #907 and #908.

Code issues re-measured at this wrap: `oversized-file-runtime` 523 → 523 production lines and
`complexity-runtime-spawn-child-and-pump` 67 lines, both unchanged;
`complexity-svc-spawn-split-agent-spawn-split-agent` 133 → 152 lines — worse, kept open, its
split deferred until stack `#split-alive` lands because #906–#908 also modify that file.
