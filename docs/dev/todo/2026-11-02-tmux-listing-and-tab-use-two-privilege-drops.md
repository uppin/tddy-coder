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
