# 2026-11-02 — The tmux listing and the tmux tab drop privilege by two different mechanisms

**Category:** Privilege / spawn path
**Source:** #session-tabs 3/7, PR #806, 2026-11-02

- `HostTmux::list` runs `tmux list-sessions` through `tddy_daemon_kernel::spawn_as_user::run_output_as_user`
  (a `pre_exec` setgid/initgroups/setuid drop). The tab itself is spawned through `PtyRuntime`, which
  wraps argv in `setpriv`.
- The two can disagree: a host where one works and the other does not lists sessions the tab then cannot
  attach, or the reverse. Today the roster refuses before spawning with the kernel's
  `plan_privilege_change` (`CannotBecomeUser`), which covers the non-root case only.
- Needs one mechanism for "run this as the session's OS user", either a PTY variant of the kernel's
  drop or a listing through the PTY runtime. Related: `2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md`.

## Update 2026-11-03 — `#session-tabs` sweep (narrowed, still open)

Done: the PTY spawn (`tddy_terminal_rpc::pty_runtime`) now plans its change with the kernel's
`plan_privilege_change_from` and refuses with the same `CannotBecomeUser` text before a PTY opens, so
listing and tab agree on **whether** a host can become the user.

Open: the **mechanism**. The tab still drops through a `setpriv` wrapper, the listing through a
`pre_exec` drop. A `pre_exec` drop in the PTY child is not reachable through portable-pty 0.9
(`CommandBuilder::as_command` is `pub(crate)`, `SlavePty` has only `spawn_command`, its own `pre_exec`
is not extensible), and the only behaviour it would change is the Linux root-daemon drop, which was
not exercisable locally. Options: (a) vendor/patch portable-pty with a `pre_exec` hook; (b) own
`openpty` + `StagedCommand` in `tddy-pty`; (c) keep `setpriv` (today). Needs the developer's choice and
a Linux-as-root run.
